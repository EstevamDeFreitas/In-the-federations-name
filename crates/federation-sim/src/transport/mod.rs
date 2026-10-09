//! Logical directed multigraph. Coordinates do not imply connectivity.
#![doc = include_str!("../../../../docs/transport-network.md")]
mod ids;
pub(crate) mod network;
mod routing;
mod snapshot;

pub use ids::{TransportEdgeId, TransportNodeId};
pub use network::{TransportMode, TransportPosition};
pub use routing::{Route, RouteDiagnostics, RouteError, RouteRequest, RoutingWorkspace};
pub use snapshot::{
    TransportEdgeSnapshot, TransportNodeSnapshot, TransportSnapshot, TransportSnapshotRequest,
};
