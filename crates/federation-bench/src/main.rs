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
struct ResultRow {
    threads: usize,
    oversubscribed: bool,
    timing: Timing,
    speedup: f64,
    checksum: String,
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
    let mut baseline = None;
    let mut reference_checksum = None;
    let mut checksums_match = true;
    for threads in counts {
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
        if threads == 1 {
            baseline = Some(timing.mean_ms);
        }
        let checksum = sim.checksum();
        if let Some(reference) = reference_checksum {
            checksums_match &= reference == checksum;
        } else {
            reference_checksum = Some(checksum);
        }
        let speedup = baseline.unwrap_or(timing.mean_ms) / timing.mean_ms;
        results.push(ResultRow {
            threads,
            oversubscribed: threads > environment.logical_cpus,
            timing,
            speedup,
            checksum: format!("{checksum:016x}"),
        });
    }
    let report = Report {
        environment,
        scenario: Scenario::from(&cli.map),
        warmup_ticks: cli.warmup,
        measured_ticks: cli.ticks,
        total_simulated_ticks: u64::from(cli.warmup) + u64::from(cli.ticks),
        checksum_algorithm: "FNV-1a-64 / federation-state-v1",
        checksums_match,
        results,
    };
    println!(
        "Synthetic movement benchmark | {} logical CPUs | {} / {}",
        report.environment.logical_cpus, report.environment.os, report.environment.architecture
    );
    println!(
        "threads | mean ms | median ms | p95 ms | p99 ms | ticks/s | speedup | oversubscribed"
    );
    for row in &report.results {
        println!(
            "{} | {:.4} | {:.4} | {:.4} | {:.4} | {:.2} | {:.2}x | {}",
            row.threads,
            row.timing.mean_ms,
            row.timing.median_ms,
            row.timing.p95_ms,
            row.timing.p99_ms,
            row.timing.ticks_per_second,
            row.speedup,
            row.oversubscribed
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
