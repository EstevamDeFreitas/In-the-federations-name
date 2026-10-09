use clap::Parser;
use federation_headless::{Environment, write_json};
use federation_sim::{
    POSITION_UNITS_PER_TILE, Route, RouteDiagnostics, RouteError, RouteRequest, RoutingWorkspace,
    Simulation, SimulationCommand, SimulationConfig, TransportMode, TransportNodeId,
    TransportPosition, rng::DeterministicRng,
};
use rayon::prelude::*;
use serde::Serialize;
use std::{error::Error, path::PathBuf, time::Instant};

#[derive(Parser)]
#[command(about = "Measure deterministic Dijkstra on a synthetic directed grid")]
struct Cli {
    #[arg(long, default_value_t = 128, value_parser = clap::value_parser!(u32).range(2..))]
    width: u32,
    #[arg(long, default_value_t = 128, value_parser = clap::value_parser!(u32).range(2..))]
    height: u32,
    #[arg(long, default_value_t = 512, value_parser = clap::value_parser!(u32).range(1..))]
    queries: u32,
    #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u32).range(1..))]
    runs: u32,
    #[arg(long, default_value_t = 16)]
    warmup: u32,
    #[arg(long, value_delimiter = ',', default_value = "1,2,4,8,12,16")]
    threads: Vec<usize>,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long, default_value = "target/route-bench-results.json")]
    output_json: PathBuf,
}

fn create_grid(
    width: u32,
    height: u32,
) -> Result<(Simulation, Vec<TransportNodeId>, usize), Box<dyn Error>> {
    let count = u64::from(width) * u64::from(height);
    // Temporary guards for manual benchmark resource consumption, not game limits.
    if count > 1_000_000 {
        return Err("grid exceeds temporary guard of 1,000,000 nodes".into());
    }
    let mut sim = Simulation::new(SimulationConfig {
        width: 1,
        height: 1,
        vehicles: 0,
        threads: 1,
        ..Default::default()
    })?;
    let mut nodes = Vec::new();
    nodes.try_reserve_exact(count as usize)?;
    for y in 0..height {
        for x in 0..width {
            let node_id = sim.reserve_transport_node_id()?;
            sim.submit_command(SimulationCommand::CreateTransportNode {
                node_id,
                position: TransportPosition {
                    x: i64::from(x) * POSITION_UNITS_PER_TILE,
                    y: i64::from(y) * POSITION_UNITS_PER_TILE,
                    z: 0,
                },
            })?;
            nodes.push(node_id);
        }
    }
    let mut edges = 0;
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) as usize;
            let neighbors = [
                (x + 1 < width).then_some(index + 1),
                (y + 1 < height).then_some(index + width as usize),
            ];
            for neighbor in neighbors.into_iter().flatten() {
                for (from, to) in [
                    (nodes[index], nodes[neighbor]),
                    (nodes[neighbor], nodes[index]),
                ] {
                    let edge_id = sim.reserve_transport_edge_id()?;
                    sim.submit_command(SimulationCommand::CreateTransportEdge {
                        edge_id,
                        from,
                        to,
                        mode: TransportMode::Road,
                        base_cost: 10,
                        enabled: true,
                    })?;
                    edges += 1;
                }
            }
        }
    }
    sim.tick()?;
    for result in sim.command_results() {
        result.result?;
    }
    Ok((sim, nodes, edges))
}

fn requests(nodes: &[TransportNodeId], count: u32, seed: u64) -> Vec<RouteRequest> {
    let mut rng = DeterministicRng::new(seed);
    (0..count)
        .map(|_| {
            let origin = (rng.next_u64() % nodes.len() as u64) as usize;
            let offset = 1 + (rng.next_u64() % (nodes.len() - 1) as u64) as usize;
            RouteRequest {
                origin: nodes[origin],
                destination: nodes[(origin + offset) % nodes.len()],
                mode: TransportMode::Road,
            }
        })
        .collect()
}

struct Sample {
    index: usize,
    seconds: f64,
    route: Route,
    diagnostics: RouteDiagnostics,
}
fn query_batch(
    sim: &Simulation,
    queries: &[RouteRequest],
    workspaces: &mut [RoutingWorkspace],
    pool: &rayon::ThreadPool,
) -> Result<Vec<Sample>, RouteError> {
    let workers = workspaces.len();
    let batches: Vec<Result<Vec<Sample>, RouteError>> = pool.install(|| {
        workspaces
            .par_iter_mut()
            .enumerate()
            .map(|(worker, workspace)| {
                let mut samples = Vec::new();
                samples
                    .try_reserve_exact(queries.len().div_ceil(workers))
                    .map_err(|_| RouteError::Allocation)?;
                for index in (worker..queries.len()).step_by(workers) {
                    let start = Instant::now();
                    let route = sim.find_route(queries[index], workspace)?;
                    samples.push(Sample {
                        index,
                        seconds: start.elapsed().as_secs_f64(),
                        route,
                        diagnostics: workspace.diagnostics(),
                    });
                }
                Ok(samples)
            })
            .collect()
    });
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(queries.len())
        .map_err(|_| RouteError::Allocation)?;
    for batch in batches {
        samples.extend(batch?);
    }
    samples.sort_unstable_by_key(|s| s.index);
    Ok(samples)
}

fn output_checksum(samples: &[Sample]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    let mut bytes = |data: &[u8]| {
        for &byte in data {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    bytes(b"federation-route-output-v1");
    bytes(&(samples.len() as u64).to_le_bytes());
    for sample in samples {
        bytes(&(sample.index as u64).to_le_bytes());
        let route = &sample.route;
        bytes(&route.network_revision.to_le_bytes());
        bytes(&route.origin.to_raw().to_le_bytes());
        bytes(&route.destination.to_raw().to_le_bytes());
        bytes(&[match route.mode {
            TransportMode::Road => 0,
            TransportMode::Rail => 1,
        }]);
        bytes(&route.total_cost.to_le_bytes());
        bytes(&(route.edges.len() as u64).to_le_bytes());
        for edge in &route.edges {
            bytes(&edge.to_raw().to_le_bytes());
        }
    }
    hash
}

#[derive(Serialize)]
struct Row {
    threads: usize,
    oversubscribed: bool,
    runs: u32,
    mean_ms_per_route: f64,
    median_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    routes_per_second: f64,
    speedup: f64,
    batch_seconds: Vec<f64>,
    route_output_checksum: String,
    average_visited_nodes: f64,
    average_relaxed_edges: f64,
    average_heap_pushes: f64,
}
#[derive(Serialize)]
struct Report {
    environment: Environment,
    width: u32,
    height: u32,
    nodes: usize,
    edges: usize,
    queries: u32,
    runs: u32,
    warmup_queries_per_workspace: u32,
    seed: u64,
    network_checksum: String,
    checksums_match: bool,
    execution_orders: Vec<Vec<usize>>,
    results: Vec<Row>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if !cli.threads.contains(&1) || cli.threads.iter().any(|&n| n == 0 || n > 1024) {
        return Err("--threads must include 1 and use the temporary guard 1..=1024".into());
    }
    if cli.queries > 1_000_000 || cli.runs > 1000 {
        return Err("queries/runs exceeds temporary benchmark guard".into());
    }
    let (sim, nodes, edges) = create_grid(cli.width, cli.height)?;
    let queries = requests(&nodes, cli.queries, cli.seed);
    let mut counts = cli.threads.clone();
    counts.sort_unstable();
    counts.dedup();
    let environment = Environment::default();
    let mut latencies: Vec<Vec<f64>> = counts.iter().map(|_| Vec::new()).collect();
    let mut durations: Vec<Vec<f64>> = counts.iter().map(|_| Vec::new()).collect();
    let mut totals = vec![RouteDiagnostics::default(); counts.len()];
    let mut checksums = vec![0; counts.len()];
    let mut reference = None;
    let mut checksums_match = true;
    let mut order_rng = DeterministicRng::new(cli.seed);
    let mut execution_orders = Vec::new();
    for _ in 0..cli.runs {
        let mut order = counts.clone();
        for i in (1..order.len()).rev() {
            let j = (order_rng.next_u64() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        for &threads in &order {
            let index = counts.binary_search(&threads).expect("known count");
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()?;
            let mut workspaces: Vec<_> = (0..threads).map(|_| RoutingWorkspace::new()).collect();
            // Warm every workspace on the same pool, outside the measured batch.
            pool.install(|| {
                workspaces.par_iter_mut().try_for_each(|workspace| {
                    for i in 0..cli.warmup as usize {
                        sim.find_route(queries[i % queries.len()], workspace)?;
                    }
                    Ok::<_, RouteError>(())
                })
            })?;
            let start = Instant::now();
            let samples = query_batch(&sim, &queries, &mut workspaces, &pool)?;
            durations[index].push(start.elapsed().as_secs_f64());
            let checksum = output_checksum(&samples);
            if let Some(expected) = reference {
                checksums_match &= expected == checksum;
            } else {
                reference = Some(checksum);
            }
            checksums[index] = checksum;
            for sample in samples {
                latencies[index].push(sample.seconds);
                totals[index].visited_nodes += sample.diagnostics.visited_nodes;
                totals[index].relaxed_edges += sample.diagnostics.relaxed_edges;
                totals[index].heap_pushes += sample.diagnostics.heap_pushes;
            }
        }
        execution_orders.push(order);
    }
    let baseline = durations[counts.binary_search(&1).expect("baseline")]
        .iter()
        .sum::<f64>();
    let mut results = Vec::new();
    for (index, &threads) in counts.iter().enumerate() {
        let samples = &mut latencies[index];
        samples.sort_by(f64::total_cmp);
        let n = samples.len();
        let time = durations[index].iter().sum::<f64>();
        let percentile = |p: usize| samples[(n * p).div_ceil(100) - 1] * 1000.0;
        let median_ms = if n % 2 == 0 {
            (samples[n / 2 - 1] + samples[n / 2]) * 500.0
        } else {
            samples[n / 2] * 1000.0
        };
        results.push(Row {
            threads,
            oversubscribed: threads > environment.logical_cpus,
            runs: cli.runs,
            mean_ms_per_route: samples.iter().sum::<f64>() / n as f64 * 1000.0,
            median_ms,
            p95_ms: percentile(95),
            p99_ms: percentile(99),
            routes_per_second: n as f64 / time,
            speedup: baseline / time,
            batch_seconds: std::mem::take(&mut durations[index]),
            route_output_checksum: format!("{:016x}", checksums[index]),
            average_visited_nodes: totals[index].visited_nodes as f64 / n as f64,
            average_relaxed_edges: totals[index].relaxed_edges as f64 / n as f64,
            average_heap_pushes: totals[index].heap_pushes as f64 / n as f64,
        });
    }
    let report = Report {
        environment,
        width: cli.width,
        height: cli.height,
        nodes: nodes.len(),
        edges,
        queries: cli.queries,
        runs: cli.runs,
        warmup_queries_per_workspace: cli.warmup,
        seed: cli.seed,
        network_checksum: format!("{:016x}", sim.checksum()),
        checksums_match,
        execution_orders,
        results,
    };
    println!(
        "Routing grid: {} nodes, {} directed edges, {} queries/run, {} runs",
        report.nodes, report.edges, report.queries, report.runs
    );
    println!(
        "threads | mean ms | p50 ms | p95 ms | p99 ms | routes/s | speedup | visited | relaxed"
    );
    for row in &report.results {
        println!(
            "{} | {:.4} | {:.4} | {:.4} | {:.4} | {:.2} | {:.2}x | {:.1} | {:.1}",
            row.threads,
            row.mean_ms_per_route,
            row.median_ms,
            row.p95_ms,
            row.p99_ms,
            row.routes_per_second,
            row.speedup,
            row.average_visited_nodes,
            row.average_relaxed_edges
        );
    }
    write_json(&cli.output_json, &report)?;
    println!(
        "Checksums match: {} | JSON: {}",
        report.checksums_match,
        cli.output_json.display()
    );
    if !checksums_match {
        return Err("routing determinism check failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_and_parallel_query_outputs_match_in_original_order() {
        let (sim, nodes, edges) = create_grid(4, 3).unwrap();
        assert_eq!(nodes.len(), 12);
        assert_eq!(edges, 34);
        let queries = requests(&nodes, 24, 42);
        let mut reference = None;
        for threads in [1, 2, 4, 8] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .unwrap();
            let mut workspaces: Vec<_> = (0..threads).map(|_| RoutingWorkspace::new()).collect();
            let samples = query_batch(&sim, &queries, &mut workspaces, &pool).unwrap();
            for (i, sample) in samples.iter().enumerate() {
                assert_eq!(sample.index, i);
                assert_eq!(sample.route.origin, queries[i].origin);
                assert_eq!(sample.route.destination, queries[i].destination);
            }
            let checksum = output_checksum(&samples);
            if let Some(expected) = reference {
                assert_eq!(checksum, expected);
            } else {
                reference = Some(checksum);
            }
        }
    }
    #[test]
    fn benchmark_rejects_empty_queries_runs_and_small_grids() {
        for args in [
            ["route-bench", "--queries", "0"],
            ["route-bench", "--runs", "0"],
            ["route-bench", "--width", "1"],
        ] {
            assert!(Cli::try_parse_from(args).is_err());
        }
        assert!(create_grid(u32::MAX, u32::MAX).is_err());
    }
}
