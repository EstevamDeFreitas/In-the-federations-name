use clap::{Parser, ValueEnum};
use federation_headless::{Environment, write_json};
use federation_sim::*;
use serde::Serialize;
use std::{error::Error, path::PathBuf, time::Instant};

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InvalidationAction {
    Disable,
    Remove,
}
#[derive(Parser)]
#[command(about = "Measure route execution with prepared, shared plans; no timed Dijkstra")]
struct Cli {
    #[arg(long,default_value_t=128,value_parser=clap::value_parser!(u32).range(2..))]
    width: u32,
    #[arg(long,default_value_t=128,value_parser=clap::value_parser!(u32).range(2..))]
    height: u32,
    #[arg(long,default_value_t=100_000,value_parser=clap::value_parser!(u32).range(1..))]
    vehicles: u32,
    #[arg(long,default_value_t=1000,value_parser=clap::value_parser!(u32).range(1..))]
    plans: u32,
    #[arg(long,default_value_t=200,value_parser=clap::value_parser!(u32).range(1..))]
    ticks: u32,
    #[arg(long, default_value_t = 20)]
    warmup: u32,
    #[arg(long,default_value_t=3,value_parser=clap::value_parser!(u32).range(1..))]
    runs: u32,
    #[arg(long, value_delimiter = ',', default_value = "1,2,4,8,12,16")]
    threads: Vec<usize>,
    #[arg(long, default_value_t = 4)]
    speed: u64,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    #[arg(long)]
    unique_plan_per_vehicle: bool,
    #[arg(long)]
    invalidate_at: Option<u32>,
    #[arg(long, default_value_t = 8)]
    invalidate_count: usize,
    #[arg(long, value_enum, default_value = "disable")]
    invalidation_action: InvalidationAction,
    #[arg(long, default_value = "target/routed-movement-results.json")]
    output_json: PathBuf,
}
fn grid(
    cli: &Cli,
    threads: usize,
    vehicles: usize,
) -> Result<(Simulation, Vec<TransportNodeId>), Box<dyn Error>> {
    let count = u64::from(cli.width) * u64::from(cli.height);
    if count > 1_000_000 {
        return Err("grid exceeds temporary safety guard of 1,000,000 nodes".into());
    }
    let mut sim = Simulation::new(SimulationConfig {
        width: cli.width,
        height: cli.height,
        vehicles,
        threads,
        seed: cli.seed,
        ..Default::default()
    })?;
    let mut nodes = Vec::new();
    nodes.try_reserve_exact(count as usize)?;
    for y in 0..cli.height {
        for x in 0..cli.width {
            nodes.push(
                sim.submit_create_transport_node(TransportPosition {
                    x: i64::from(x) * POSITION_UNITS_PER_TILE,
                    y: i64::from(y) * POSITION_UNITS_PER_TILE,
                    z: 0,
                })?
                .0,
            );
        }
    }
    for y in 0..cli.height {
        for x in 0..cli.width {
            let index = (y * cli.width + x) as usize;
            let adjacent = [
                (x + 1 < cli.width).then_some(index + 1),
                (y + 1 < cli.height).then_some(index + cli.width as usize),
            ];
            for neighbor in adjacent.into_iter().flatten() {
                sim.submit_create_transport_edge(
                    nodes[index],
                    nodes[neighbor],
                    TransportMode::Road,
                    10,
                    true,
                )?;
                sim.submit_create_transport_edge(
                    nodes[neighbor],
                    nodes[index],
                    TransportMode::Road,
                    10,
                    true,
                )?;
            }
        }
    }
    sim.tick()?;
    for result in sim.command_results() {
        result.result?;
    }
    Ok((sim, nodes))
}
fn prepare_routes(cli: &Cli) -> Result<Vec<Route>, Box<dyn Error>> {
    let (sim, nodes) = grid(cli, 1, 0)?;
    let mut rng = federation_sim::rng::DeterministicRng::new(cli.seed);
    let mut workspace = RoutingWorkspace::new(); // One routing worker, not one per vehicle.
    let mut routes = Vec::new();
    routes.try_reserve_exact(cli.plans as usize)?;
    for _ in 0..cli.plans {
        let origin = (rng.next_u64() % nodes.len() as u64) as usize;
        let offset = 1 + (rng.next_u64() % (nodes.len() - 1) as u64) as usize;
        routes.push(sim.find_route(
            RouteRequest {
                origin: nodes[origin],
                destination: nodes[(origin + offset) % nodes.len()],
                mode: TransportMode::Road,
            },
            &mut workspace,
        )?);
    }
    Ok(routes)
}
fn prepare_simulation(
    cli: &Cli,
    threads: usize,
    routes: &[Route],
) -> Result<Simulation, Box<dyn Error>> {
    let (mut sim, _) = grid(cli, threads, cli.vehicles as usize)?;
    let count = if cli.unique_plan_per_vehicle {
        cli.vehicles as usize
    } else {
        routes.len()
    };
    let mut plans = Vec::new();
    plans.try_reserve_exact(count)?;
    for i in 0..count {
        plans.push(
            sim.submit_store_route_plan(routes[i % routes.len()].clone())?
                .0,
        );
    }
    let snapshot = sim.snapshot(SnapshotRequest {
        vehicles: VehicleSelection::All,
        ..Default::default()
    })?;
    for (index, vehicle) in snapshot.vehicles.iter().enumerate() {
        sim.submit_command(SimulationCommand::SetVehicleNetworkSpeed {
            vehicle_id: vehicle.id,
            speed_units_per_ms: cli.speed,
        })?;
        sim.submit_command(SimulationCommand::AssignVehicleRoute {
            vehicle_id: vehicle.id,
            route_plan_id: plans[index % plans.len()],
        })?;
    }
    sim.tick()?;
    for result in sim.command_results() {
        result.result?;
    }
    for _ in 0..cli.warmup {
        sim.tick()?;
    }
    Ok(sim)
}
fn invalidated_edges(cli: &Cli, routes: &[Route]) -> Vec<TransportEdgeId> {
    let Some(tick) = cli.invalidate_at else {
        return Vec::new();
    };
    let distance = (u128::from(cli.warmup) + u128::from(tick) + 1) * u128::from(cli.speed) * 20;
    let index = (distance / POSITION_UNITS_PER_TILE as u128 + 1).min(usize::MAX as u128) as usize;
    let mut ids = Vec::new();
    for route in routes {
        if route.edges.is_empty() {
            continue;
        }
        let id = route.edges[index.min(route.edges.len() - 1)];
        if !ids.contains(&id) {
            ids.push(id);
        }
        if ids.len() >= cli.invalidate_count {
            break;
        }
    }
    ids
}

#[derive(Debug, Serialize)]
struct Memory {
    vehicle_base_arrays_bytes: u64,
    movement_arrays_bytes: u64,
    event_buffers_bytes: u64,
    route_plan_records_bytes: u64,
    route_plan_edges_bytes: u64,
}
impl From<RouteExecutionMemoryEstimate> for Memory {
    fn from(m: RouteExecutionMemoryEstimate) -> Self {
        Self {
            vehicle_base_arrays_bytes: m.vehicle_base_arrays_bytes,
            movement_arrays_bytes: m.movement_arrays_bytes,
            event_buffers_bytes: m.event_buffers_bytes,
            route_plan_records_bytes: m.route_plan_records_bytes,
            route_plan_edges_bytes: m.route_plan_edges_bytes,
        }
    }
}
#[derive(Serialize)]
struct Row {
    threads: usize,
    oversubscribed: bool,
    mean_tick_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    speedup: f64,
    vehicles_updated_per_second: f64,
    edge_transitions_per_second: f64,
    arrivals_per_second: f64,
    measured_routed_updates: u64,
    edge_transitions: u64,
    arrivals: u64,
    vehicles_needing_route: u64,
    safety_guard_failures: u64,
    checksum: String,
    memory: Memory,
    run_mean_tick_ms: Vec<f64>,
}
#[derive(Serialize)]
struct Report {
    environment: Environment,
    width: u32,
    height: u32,
    vehicles: u32,
    route_plans: usize,
    route_templates: u32,
    unique_plan_per_vehicle: bool,
    average_edges_per_route: f64,
    speed_units_per_ms: u64,
    tick_ms: u32,
    seed: u64,
    warmup_ticks: u32,
    measured_ticks: u32,
    runs: u32,
    invalidate_at: Option<u32>,
    invalidated_edge_count: usize,
    invalidation_action: String,
    checksum_algorithm: &'static str,
    checksums_match: bool,
    execution_orders: Vec<Vec<usize>>,
    results: Vec<Row>,
}
fn validate(cli: &Cli) -> Result<(), Box<dyn Error>> {
    if !cli.threads.contains(&1) || cli.threads.iter().any(|&n| n == 0 || n > 1024) {
        return Err("threads must include 1 and fit temporary guard 1..=1024".into());
    }
    if cli.speed > 1_000_000
        || cli.plans > 1_000_000
        || cli.vehicles > 10_000_000
        || cli.runs > 1000
    {
        return Err("speed/plans/vehicles/runs exceeds a temporary safety guard".into());
    }
    if cli.unique_plan_per_vehicle && cli.vehicles > 1_000_000 {
        return Err("unique plans exceeds temporary plan guard".into());
    }
    if u64::from(cli.ticks) * u64::from(cli.runs) > 10_000_000 {
        return Err("sample count exceeds temporary benchmark guard".into());
    }
    if cli.invalidate_at.is_some_and(|tick| tick >= cli.ticks) || cli.invalidate_count == 0 {
        return Err("invalid invalidation tick/count".into());
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    validate(&cli)?;
    let routes = prepare_routes(&cli)?;
    let invalidated = invalidated_edges(&cli, &routes);
    let mut counts = cli.threads.clone();
    counts.sort_unstable();
    counts.dedup();
    let mut samples: Vec<Vec<f64>> = counts.iter().map(|_| Vec::new()).collect();
    let mut run_means: Vec<Vec<f64>> = counts.iter().map(|_| Vec::new()).collect();
    let mut totals = vec![MovementMetrics::default(); counts.len()];
    let mut memories: Vec<Option<Memory>> = counts.iter().map(|_| None).collect();
    let mut checksums = vec![0; counts.len()];
    let mut reference = None;
    let mut checksums_match = true;
    let mut rng = federation_sim::rng::DeterministicRng::new(cli.seed);
    let mut execution_orders = Vec::new();
    for _ in 0..cli.runs {
        let mut order = counts.clone();
        for i in (1..order.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
        for &threads in &order {
            let index = counts.binary_search(&threads).expect("known count");
            let mut sim = prepare_simulation(&cli, threads, &routes)?;
            memories[index] = Some(sim.route_execution_memory_estimate().into());
            let mut run_total = 0.0;
            samples[index].try_reserve_exact(cli.ticks as usize)?;
            for tick in 0..cli.ticks {
                if cli.invalidate_at == Some(tick) {
                    for &edge_id in &invalidated {
                        let command = match cli.invalidation_action {
                            InvalidationAction::Disable => {
                                SimulationCommand::SetTransportEdgeEnabled {
                                    edge_id,
                                    enabled: false,
                                }
                            }
                            InvalidationAction::Remove => {
                                SimulationCommand::RemoveTransportEdge { edge_id }
                            }
                        };
                        sim.submit_command(command)?;
                    }
                }
                let start = Instant::now();
                sim.tick()?;
                let seconds = start.elapsed().as_secs_f64();
                if cli.invalidate_at == Some(tick) {
                    for result in sim.command_results() {
                        result.result?;
                    }
                }
                samples[index].push(seconds);
                run_total += seconds;
                let metrics = sim.movement_metrics();
                totals[index].routed_vehicles_updated += metrics.routed_vehicles_updated;
                totals[index].edge_transitions += metrics.edge_transitions;
                totals[index].arrivals += metrics.arrivals;
                totals[index].needs_route += metrics.needs_route;
                totals[index].safety_guard_failures += metrics.safety_guard_failures;
            }
            run_means[index].push(run_total / f64::from(cli.ticks) * 1000.0);
            let checksum = sim.checksum();
            if let Some(expected) = reference {
                checksums_match &= expected == checksum;
            } else {
                reference = Some(checksum);
            }
            checksums[index] = checksum;
        }
        execution_orders.push(order);
    }
    let environment = Environment::default();
    let baseline_index = counts.binary_search(&1).expect("baseline");
    let baseline =
        samples[baseline_index].iter().sum::<f64>() / samples[baseline_index].len() as f64;
    let mut results = Vec::new();
    for (index, &threads) in counts.iter().enumerate() {
        let values = &mut samples[index];
        values.sort_by(f64::total_cmp);
        let n = values.len();
        let time = values.iter().sum::<f64>();
        let mean = time / n as f64;
        let p = |rank: usize| values[(n * rank).div_ceil(100) - 1] * 1000.0;
        let p50 = if n % 2 == 0 {
            (values[n / 2 - 1] + values[n / 2]) * 500.0
        } else {
            values[n / 2] * 1000.0
        };
        let metrics = totals[index];
        results.push(Row {
            threads,
            oversubscribed: threads > environment.logical_cpus,
            mean_tick_ms: mean * 1000.0,
            p50_ms: p50,
            p95_ms: p(95),
            p99_ms: p(99),
            speedup: baseline / mean,
            vehicles_updated_per_second: metrics.routed_vehicles_updated as f64 / time,
            edge_transitions_per_second: metrics.edge_transitions as f64 / time,
            arrivals_per_second: metrics.arrivals as f64 / time,
            measured_routed_updates: metrics.routed_vehicles_updated,
            edge_transitions: metrics.edge_transitions,
            arrivals: metrics.arrivals,
            vehicles_needing_route: metrics.needs_route,
            safety_guard_failures: metrics.safety_guard_failures,
            checksum: format!("{:016x}", checksums[index]),
            memory: memories[index].take().expect("measured memory"),
            run_mean_tick_ms: std::mem::take(&mut run_means[index]),
        });
    }
    let plan_count = if cli.unique_plan_per_vehicle {
        cli.vehicles as usize
    } else {
        routes.len()
    };
    let average_edges = (0..plan_count)
        .map(|i| routes[i % routes.len()].edges.len())
        .sum::<usize>() as f64
        / plan_count as f64;
    let report = Report {
        environment,
        width: cli.width,
        height: cli.height,
        vehicles: cli.vehicles,
        route_plans: plan_count,
        route_templates: cli.plans,
        unique_plan_per_vehicle: cli.unique_plan_per_vehicle,
        average_edges_per_route: average_edges,
        speed_units_per_ms: cli.speed,
        tick_ms: 20,
        seed: cli.seed,
        warmup_ticks: cli.warmup,
        measured_ticks: cli.ticks,
        runs: cli.runs,
        invalidate_at: cli.invalidate_at,
        invalidated_edge_count: invalidated.len(),
        invalidation_action: format!("{:?}", cli.invalidation_action),
        checksum_algorithm: "FNV-1a-64 / federation-state-v4",
        checksums_match,
        execution_orders,
        results,
    };
    println!(
        "Routed movement: {} vehicles, {} plans, {:.2} edges/plan",
        report.vehicles, report.route_plans, report.average_edges_per_route
    );
    println!(
        "threads | mean ms | p50 | p95 | p99 | vehicles/s | transitions/s | arrivals/s | NeedsRoute"
    );
    for row in &report.results {
        println!(
            "{} | {:.4} | {:.4} | {:.4} | {:.4} | {:.0} | {:.0} | {:.0} | {}",
            row.threads,
            row.mean_tick_ms,
            row.p50_ms,
            row.p95_ms,
            row.p99_ms,
            row.vehicles_updated_per_second,
            row.edge_transitions_per_second,
            row.arrivals_per_second,
            row.vehicles_needing_route
        );
    }
    write_json(&cli.output_json, &report)?;
    println!(
        "Checksums match: {} | JSON: {}",
        report.checksums_match,
        cli.output_json.display()
    );
    if !checksums_match {
        return Err("routed movement determinism failed".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prepared_shared_plans_and_invalidation_are_deterministic() {
        let cli = Cli::parse_from([
            "routed-bench",
            "--width",
            "4",
            "--height",
            "4",
            "--vehicles",
            "2048",
            "--plans",
            "16",
            "--warmup",
            "0",
            "--ticks",
            "30",
            "--speed",
            "30",
            "--invalidate-at",
            "1",
        ]);
        let routes = prepare_routes(&cli).unwrap();
        let invalidated = invalidated_edges(&cli, &routes);
        let mut reference = None;
        for threads in [1, 2, 4, 8] {
            let mut sim = prepare_simulation(&cli, threads, &routes).unwrap();
            let mut needs = 0;
            for tick in 0..cli.ticks {
                if cli.invalidate_at == Some(tick) {
                    for &edge_id in &invalidated {
                        sim.submit_command(SimulationCommand::SetTransportEdgeEnabled {
                            edge_id,
                            enabled: false,
                        })
                        .unwrap();
                    }
                }
                sim.tick().unwrap();
                needs += sim.movement_metrics().needs_route;
            }
            assert!(needs > 0);
            let result = (sim.checksum(), needs);
            if let Some(expected) = reference {
                assert_eq!(result, expected);
            } else {
                reference = Some(result);
            }
        }
    }
    #[test]
    fn unique_plan_mode_stores_one_plan_per_vehicle() {
        let cli = Cli::parse_from([
            "routed-bench",
            "--width",
            "3",
            "--height",
            "3",
            "--vehicles",
            "8",
            "--plans",
            "2",
            "--warmup",
            "0",
            "--unique-plan-per-vehicle",
        ]);
        let routes = prepare_routes(&cli).unwrap();
        let sim = prepare_simulation(&cli, 1, &routes).unwrap();
        let snapshots = sim
            .snapshot(SnapshotRequest {
                vehicles: VehicleSelection::All,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(
            snapshots
                .vehicles
                .iter()
                .map(|v| v.route_plan_id)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            8
        );
    }
}
