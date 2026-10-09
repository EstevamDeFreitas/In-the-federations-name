use clap::Parser;
use federation_headless::{Environment, MapArgs, Scenario, write_json};
use federation_sim::{Simulation, SimulationConfig};
use serde::Serialize;
use std::{error::Error, path::PathBuf, time::Instant};

#[derive(Parser)]
#[command(about = "Run the deterministic simulation without a renderer")]
struct Cli {
    #[command(flatten)]
    map: MapArgs,
    #[arg(long, default_value_t = 200)]
    ticks: u64,
    #[arg(long, default_value_t = SimulationConfig::default().threads)]
    threads: usize,
    #[arg(long)]
    output_json: Option<PathBuf>,
}

#[derive(Serialize)]
struct Report {
    environment: Environment,
    scenario: Scenario,
    threads: usize,
    ticks: u64,
    initialization_seconds: f64,
    tick_seconds: f64,
    ticks_per_second: f64,
    logical_time_ms: String,
    dirty_chunks: usize,
    checksum_algorithm: &'static str,
    checksum: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let start = Instant::now();
    let mut sim = Simulation::new(cli.map.config(cli.threads))?;
    let initialization_seconds = start.elapsed().as_secs_f64();
    let start = Instant::now();
    for _ in 0..cli.ticks {
        sim.tick()?;
    }
    let tick_seconds = start.elapsed().as_secs_f64();
    let metrics = sim.diagnostics();
    let report = Report {
        environment: Environment::default(),
        scenario: Scenario::from(&cli.map),
        threads: cli.threads,
        ticks: sim.tick_count(),
        initialization_seconds,
        tick_seconds,
        ticks_per_second: if tick_seconds > 0.0 {
            cli.ticks as f64 / tick_seconds
        } else {
            0.0
        },
        logical_time_ms: metrics.logical_time_ms.to_string(),
        dirty_chunks: metrics.dirty_chunks,
        checksum_algorithm: "FNV-1a-64 / federation-state-v3",
        checksum: format!("{:016x}", metrics.checksum),
    };
    println!("In The Federation's Name — headless");
    println!(
        "Map: {} x {} tiles | Vehicles: {} | Threads: {}",
        cli.map.width, cli.map.height, cli.map.vehicles, cli.threads
    );
    println!(
        "Ticks: {} | Tick time: {:.6}s | Ticks/s: {:.2}",
        report.ticks, report.tick_seconds, report.ticks_per_second
    );
    println!(
        "Initialization: {:.6}s | Logical time: {}ms",
        report.initialization_seconds, report.logical_time_ms
    );
    println!("Checksum: {}", report.checksum);
    if let Some(path) = cli.output_json {
        write_json(&path, &report)?;
        println!("JSON: {}", path.display());
    }
    Ok(())
}
