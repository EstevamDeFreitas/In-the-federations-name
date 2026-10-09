//! Deterministic, frontend-independent simulation foundation.
#![forbid(unsafe_code)]

pub mod chunk;
pub mod commands;
pub mod config;
pub mod entities;
pub mod ids;
pub mod metrics;
pub mod rng;
pub mod scheduler;
pub mod simulation;
pub mod snapshots;
pub mod terrain;

pub use commands::{
    CommandEnvelope, CommandError, CommandResult, CommandSequence, SimulationCommand,
};
pub use config::SimulationConfig;
pub use ids::VehicleId;
pub use simulation::Simulation;
pub use snapshots::{
    ChunkRevisionQuery, SimulationSnapshot, SnapshotRequest, TerrainChunkSnapshot,
    VehicleSelection, VehicleSnapshot,
};

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("vehicle identity does not exist: {0:?}")]
    InvalidVehicle(VehicleId),
    #[error("invalid configuration: {0}")]
    InvalidConfig(&'static str),
    #[error("{kind} coordinate ({x}, {y}) is outside the map")]
    OutOfBounds { kind: &'static str, x: u32, y: u32 },
    #[error("allocation failed: {0}")]
    Allocation(#[from] std::collections::TryReserveError),
    #[error("cannot create simulation thread pool: {0}")]
    ThreadPool(#[from] rayon::ThreadPoolBuildError),
    #[error("simulation tick counter exhausted")]
    TickOverflow,
}

pub(crate) fn filled_vec<T: Clone>(len: usize, value: T) -> Result<Vec<T>, SimulationError> {
    let mut values = Vec::new();
    values.try_reserve_exact(len)?;
    values.resize(len, value);
    Ok(values)
}
