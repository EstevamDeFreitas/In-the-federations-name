//! Shared CLI configuration and report I/O for the two executable hosts.
use clap::Args;
use federation_sim::SimulationConfig;
use serde::Serialize;
use std::{error::Error, path::Path};

#[derive(Debug, Args, Clone)]
pub struct MapArgs {
    #[arg(long, default_value_t = 512)]
    pub width: u32,
    #[arg(long, default_value_t = 512)]
    pub height: u32,
    #[arg(long, default_value_t = 32)]
    pub chunk_size: u32,
    #[arg(long, default_value_t = 100_000)]
    pub vehicles: usize,
    #[arg(long, default_value_t = 42)]
    pub seed: u64,
    #[arg(long, default_value_t = 20)]
    pub tick_ms: u32,
}
impl MapArgs {
    pub fn config(&self, threads: usize) -> SimulationConfig {
        SimulationConfig {
            width: self.width,
            height: self.height,
            chunk_size: self.chunk_size,
            vehicles: self.vehicles,
            threads,
            seed: self.seed,
            tick_ms: self.tick_ms,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Environment {
    pub os: &'static str,
    pub architecture: &'static str,
    pub logical_cpus: usize,
    pub build_mode: &'static str,
}
impl Default for Environment {
    fn default() -> Self {
        Self {
            os: std::env::consts::OS,
            architecture: std::env::consts::ARCH,
            logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
            build_mode: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Scenario {
    pub width: u32,
    pub height: u32,
    pub chunk_size: u32,
    pub vehicles: usize,
    pub seed: u64,
    pub tick_ms: u32,
}
impl From<&MapArgs> for Scenario {
    fn from(args: &MapArgs) -> Self {
        Self {
            width: args.width,
            height: args.height,
            chunk_size: args.chunk_size,
            vehicles: args.vehicles,
            seed: args.seed,
            tick_ms: args.tick_ms,
        }
    }
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(&mut writer, value)?;
    std::io::Write::flush(&mut writer)?;
    Ok(())
}
