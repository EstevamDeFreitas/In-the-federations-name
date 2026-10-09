use clap::Parser;
use federation_headless::{Environment, MapArgs, Scenario, write_json};
use federation_sim::Simulation;
use serde::Serialize;
use std::{error::Error, path::PathBuf, time::Instant};

#[derive(Parser)]
#[command(about = "Benchmark synthetic integer vehicle movement across Rayon pools")]
struct Cli {
    #[command(flatten)]
    map: MapArgs,
    #[arg(long, default_value_t = 200, value_parser = clap::value_parser!(u32).range(1..))]
    ticks: u32,
    #[arg(long, default_value_t = 30)]
    warmup: u32,
    #[arg(long, value_delimiter = ',', default_value = "1,2,4,8,16")]
    threads: Vec<usize>,
    #[arg(long, default_value = "target/bench-results.json")]
    output_json: PathBuf,
    #[arg(long, default_value_t = 5, value_parser = clap::value_parser!(u32).range(1..))]
    runs: u32,
    #[arg(long)]
    shuffle_thread_order: bool,
    #[arg(long, default_value_t = 42)]
    order_seed: u64,
}

#[derive(Debug, Serialize)]
struct Timing {
    mean_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    tick_seconds: f64,
    ticks_per_second: f64,
}
fn summarize(samples: &mut [f64]) -> Timing {
    samples.sort_by(f64::total_cmp);
    let total: f64 = samples.iter().sum();
    let n = samples.len();
    let median_ms = if n % 2 == 0 {
        (samples[n / 2 - 1] + samples[n / 2]) / 2.0
    } else {
        samples[n / 2]
    };
    // Nearest-rank percentiles; samples are seconds, reports are milliseconds.
    let percentile = |p: usize| samples[(n * p).div_ceil(100) - 1] * 1000.0;
    Timing {
        mean_ms: total / n as f64 * 1000.0,
        median_ms: median_ms * 1000.0,
        p95_ms: percentile(95),
        p99_ms: percentile(99),
        tick_seconds: total,
        ticks_per_second: if total > 0.0 { n as f64 / total } else { 0.0 },
    }
}

#[derive(Serialize)]
struct RunStatistics {
    median_mean_ms: f64,
    min_mean_ms: f64,
    max_mean_ms: f64,
    stddev_mean_ms: f64,
}
fn run_statistics(means: &mut [f64]) -> RunStatistics {
    means.sort_by(f64::total_cmp);
    let mean = means.iter().sum::<f64>() / means.len() as f64;
    RunStatistics {
        median_mean_ms: summarize(&mut means.iter().map(|v| v / 1000.0).collect::<Vec<_>>())
            .median_ms,
        min_mean_ms: means[0],
        max_mean_ms: means[means.len() - 1],
        stddev_mean_ms: (means.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
            / means.len() as f64)
            .sqrt(),
    }
}
fn thread_order(
    counts: &[usize],
    shuffle: bool,
    rng: &mut federation_sim::rng::DeterministicRng,
) -> Vec<usize> {
    let mut order = counts.to_vec();
    if shuffle {
        for i in (1..order.len()).rev() {
            let j = (rng.next_u64() % (i as u64 + 1)) as usize;
            order.swap(i, j);
        }
    }
    order
}
#[derive(Serialize)]
struct ResultRow {
    threads: usize,
    oversubscribed: bool,
    timing: Timing,
    speedup: f64,
    checksum: String,
    runs: Vec<Timing>,
    run_statistics: RunStatistics,
}
#[derive(Serialize)]
struct Report {
    environment: Environment,
    scenario: Scenario,
    warmup_ticks: u32,
    measured_ticks: u32,
    total_simulated_ticks: u64,
    checksum_algorithm: &'static str,
    checksums_match: bool,
    results: Vec<ResultRow>,
    runs: u32,
    order_seed: u64,
    shuffle_thread_order: bool,
    execution_orders: Vec<Vec<usize>>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    if !cli.threads.contains(&1) {
        return Err("--threads must include 1 for the speedup baseline".into());
    }
    let mut counts = cli.threads.clone();
    counts.sort_unstable();
    counts.dedup();
    // Validate every scenario before beginning the benchmark.
    for &threads in &counts {
        cli.map.config(threads).validate()?;
    }
    let environment = Environment::default();
    let mut results = Vec::new();
    let mut reference_checksum = None;
    let mut checksums_match = true;
    let mut rng = federation_sim::rng::DeterministicRng::new(cli.order_seed);
    let mut execution_orders = Vec::new();
    let mut samples_by_count: Vec<Vec<f64>> = counts.iter().map(|_| Vec::new()).collect();
    let mut runs_by_count: Vec<Vec<Timing>> = counts.iter().map(|_| Vec::new()).collect();
    let mut checksums = vec![0; counts.len()];
    for _ in 0..cli.runs {
        let order = thread_order(&counts, cli.shuffle_thread_order, &mut rng);
        for &threads in &order {
            let index = counts.binary_search(&threads).expect("known thread count");
            let mut sim = Simulation::new(cli.map.config(threads))?;
            for _ in 0..cli.warmup {
                sim.tick()?;
            }
            let mut samples = Vec::new();
            samples.try_reserve_exact(cli.ticks as usize)?;
            for _ in 0..cli.ticks {
                let start = Instant::now();
                sim.tick()?;
                samples.push(start.elapsed().as_secs_f64());
            }
            let timing = summarize(&mut samples);
            samples_by_count[index].try_reserve(samples.len())?;
            samples_by_count[index].extend_from_slice(&samples);
            runs_by_count[index].push(timing);
            let checksum = sim.checksum();
            if let Some(reference) = reference_checksum {
                checksums_match &= reference == checksum;
            } else {
                reference_checksum = Some(checksum);
            }
            checksums[index] = checksum;
        }
        execution_orders.push(order);
    }
    let baseline_index = counts.binary_search(&1).expect("baseline validated");
    let baseline = samples_by_count[baseline_index].iter().sum::<f64>()
        / samples_by_count[baseline_index].len() as f64;
    for (index, &threads) in counts.iter().enumerate() {
        let timing = summarize(&mut samples_by_count[index]);
        let statistics = run_statistics(
            &mut runs_by_count[index]
                .iter()
                .map(|t| t.mean_ms)
                .collect::<Vec<_>>(),
        );
        let speedup = baseline * 1000.0 / timing.mean_ms;
        results.push(ResultRow {
            threads,
            oversubscribed: threads > environment.logical_cpus,
            timing,
            speedup,
            checksum: format!("{:016x}", checksums[index]),
            runs: std::mem::take(&mut runs_by_count[index]),
            run_statistics: statistics,
        });
    }
    let report = Report {
        environment,
        scenario: Scenario::from(&cli.map),
        warmup_ticks: cli.warmup,
        measured_ticks: cli.ticks,
        total_simulated_ticks: u64::from(cli.warmup) + u64::from(cli.ticks),
        checksum_algorithm: "FNV-1a-64 / federation-state-v4",
        checksums_match,
        results,
        runs: cli.runs,
        order_seed: cli.order_seed,
        shuffle_thread_order: cli.shuffle_thread_order,
        execution_orders,
    };
    println!(
        "Synthetic movement benchmark | {} logical CPUs | {} / {}",
        report.environment.logical_cpus, report.environment.os, report.environment.architecture
    );
    println!(
        "threads | mean ms | median ms | p95 ms | p99 ms | ticks/s | speedup | oversubscribed | median/min/max run mean ms"
    );
    for row in &report.results {
        println!(
            "{} | {:.4} | {:.4} | {:.4} | {:.4} | {:.2} | {:.2}x | {} | {:.4}/{:.4}/{:.4}",
            row.threads,
            row.timing.mean_ms,
            row.timing.median_ms,
            row.timing.p95_ms,
            row.timing.p99_ms,
            row.timing.ticks_per_second,
            row.speedup,
            row.oversubscribed,
            row.run_statistics.median_mean_ms,
            row.run_statistics.min_mean_ms,
            row.run_statistics.max_mean_ms
        );
    }
    println!("Checksums match: {}", report.checksums_match);
    write_json(&cli.output_json, &report)?;
    println!("JSON: {}", cli.output_json.display());
    if !report.checksums_match {
        return Err("determinism check failed; see exported report".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn run_statistics_measure_spread_between_runs() {
        let stats = run_statistics(&mut [4.0, 1.0, 3.0, 2.0]);
        assert_eq!(stats.median_mean_ms, 2.5);
        assert_eq!(stats.min_mean_ms, 1.0);
        assert_eq!(stats.max_mean_ms, 4.0);
        assert!((stats.stddev_mean_ms - 1.25_f64.sqrt()).abs() < 1e-10);
    }
    #[test]
    fn shuffled_orders_are_reproducible_permutations() {
        let counts = [1, 2, 4, 8, 16];
        let mut a = federation_sim::rng::DeterministicRng::new(42);
        let mut b = federation_sim::rng::DeterministicRng::new(42);
        let mut orders = Vec::new();
        for _ in 0..5 {
            let order = thread_order(&counts, true, &mut a);
            assert_eq!(order, thread_order(&counts, true, &mut b));
            let mut sorted = order.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, counts);
            orders.push(order);
        }
        assert!(orders.windows(2).any(|pair| pair[0] != pair[1]));
        assert_eq!(thread_order(&counts, false, &mut a), counts);
    }
    #[test]
    fn zero_runs_and_ticks_are_rejected() {
        assert!(Cli::try_parse_from(["bench", "--runs", "0"]).is_err());
        assert!(Cli::try_parse_from(["bench", "--ticks", "0"]).is_err());
    }
    #[test]
    fn timing_statistics_use_median_and_nearest_rank() {
        let timing = summarize(&mut [0.004, 0.001, 0.003, 0.002]);
        assert!((timing.mean_ms - 2.5).abs() < 1e-10);
        assert!((timing.median_ms - 2.5).abs() < 1e-10);
        assert!((timing.p95_ms - 4.0).abs() < 1e-10);
        assert!((timing.p99_ms - 4.0).abs() < 1e-10);
        assert!((timing.ticks_per_second - 400.0).abs() < 1e-10);
        assert_eq!(summarize(&mut [0.001]).median_ms, 1.0);
    }
}
