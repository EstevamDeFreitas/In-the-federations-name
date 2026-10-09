use super::{
    TransportEdgeId, TransportMode, TransportNodeId, TransportPosition, network::TransportNetwork,
};
use crate::SimulationError;

#[derive(Debug, Clone, Copy, Default)]
pub struct TransportSnapshotRequest {
    pub include_nodes: bool,
    pub include_edges: bool,
    pub known_revision: Option<u64>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportNodeSnapshot {
    pub id: TransportNodeId,
    pub position: TransportPosition,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransportEdgeSnapshot {
    pub id: TransportEdgeId,
    pub from: TransportNodeId,
    pub to: TransportNodeId,
    pub mode: TransportMode,
    pub base_cost: u64,
    pub enabled: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportSnapshot {
    pub network_revision: u64,
    /// Distinguishes omitted unchanged data from an empty network.
    pub unchanged: bool,
    pub nodes: Vec<TransportNodeSnapshot>,
    pub edges: Vec<TransportEdgeSnapshot>,
}
impl TransportNetwork {
    pub fn snapshot(
        &self,
        request: TransportSnapshotRequest,
    ) -> Result<TransportSnapshot, SimulationError> {
        let unchanged = request.known_revision == Some(self.revision);
        let mut snapshot = TransportSnapshot {
            network_revision: self.revision,
            unchanged,
            nodes: Vec::new(),
            edges: Vec::new(),
        };
        if unchanged {
            return Ok(snapshot);
        }
        if request.include_nodes {
            snapshot
                .nodes
                .try_reserve_exact(self.nodes.iter().filter(|n| n.alive).count())?;
            for node in self.nodes.iter().filter(|n| n.alive) {
                snapshot.nodes.push(TransportNodeSnapshot {
                    id: node.id,
                    position: node.position,
                });
            }
        }
        if request.include_edges {
            snapshot
                .edges
                .try_reserve_exact(self.edges.iter().filter(|e| e.alive).count())?;
            for record in self.edges.iter().filter(|e| e.alive) {
                let data = record.data.expect("live edge has data");
                snapshot.edges.push(TransportEdgeSnapshot {
                    id: record.id,
                    from: data.from,
                    to: data.to,
                    mode: data.mode,
                    base_cost: data.base_cost,
                    enabled: data.enabled,
                });
            }
        }
        Ok(snapshot)
    }
}
