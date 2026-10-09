//! Deterministic, frontend-independent simulation foundation.
#![forbid(unsafe_code)]

pub mod chunk;
pub mod commands;
pub mod config;
mod entities;
pub mod ids;
pub mod metrics;
pub mod movement;
pub mod rng;
pub mod route_plans;
pub mod scheduler;
pub mod simulation;
pub mod snapshots;
mod terrain;
pub mod transport;

pub use commands::{
    CommandEnvelope, CommandError, CommandResult, CommandSequence, SimulationCommand,
};
pub use config::SimulationConfig;
pub use entities::POSITION_UNITS_PER_TILE;
pub use ids::VehicleId;
pub use simulation::Simulation;
pub use snapshots::{
    ChunkRevisionQuery, SimulationSnapshot, SnapshotRequest, TerrainChunkSnapshot,
    VehicleSelection, VehicleSnapshot,
};

#[derive(Debug, thiserror::Error)]
pub enum SimulationError {
    #[error("route plan does not exist: {0:?}")]
    InvalidRoutePlan(RoutePlanId),
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

pub use snapshots::RegionSnapshotMetadata;
pub use transport::{
    Route, RouteDiagnostics, RouteError, RouteRequest, RoutingWorkspace, TransportEdgeId,
    TransportEdgeSnapshot, TransportMode, TransportNodeId, TransportNodeSnapshot,
    TransportPosition, TransportSnapshot, TransportSnapshotRequest,
};

pub use movement::{
    MovementBlockReason, MovementEvent, MovementEventKind, MovementMetrics,
    RouteExecutionMemoryEstimate, VehicleMovementState,
};
pub use route_plans::{RoutePlan, RoutePlanId, RoutePlanSelection};
