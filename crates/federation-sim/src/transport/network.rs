use super::{TransportEdgeId, TransportNodeId};
use crate::{CommandError, SimulationCommand, metrics::StableChecksum};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportMode {
    Road,
    Rail,
}

impl TransportMode {
    pub(crate) fn tag(self) -> u8 {
        match self {
            Self::Road => 0,
            Self::Rail => 1,
        }
    }
}

/// X/Y use position units per tile; Z is an independent provisional elevation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransportPosition {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

pub(crate) struct NodeRecord {
    pub id: TransportNodeId,
    pub position: TransportPosition,
    pub used: bool,
    pub alive: bool,
    pub outgoing: Vec<TransportEdgeId>,
    pub incident: u64,
}
pub(crate) struct EdgeRecord {
    pub id: TransportEdgeId,
    pub data: Option<EdgeData>,
    pub alive: bool,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct EdgeData {
    pub from: TransportNodeId,
    pub to: TransportNodeId,
    pub mode: TransportMode,
    pub base_cost: u64,
    pub physical_length: u64,
    pub enabled: bool,
}

#[derive(Default)]
pub(crate) struct TransportNetwork {
    pub nodes: Vec<NodeRecord>,
    pub edges: Vec<EdgeRecord>,
    pub revision: u64,
    next_node: u64,
    next_edge: u64,
}
impl TransportNetwork {
    pub fn reserve_node(&mut self) -> Result<TransportNodeId, CommandError> {
        let next = self
            .next_node
            .checked_add(1)
            .ok_or(CommandError::TransportIdOverflow)?;
        if self.nodes.len() >= 1_000_000 {
            return Err(CommandError::TransportSafetyGuard);
        }
        self.nodes
            .try_reserve(1)
            .map_err(|_| CommandError::Allocation)?;
        let id = TransportNodeId(self.next_node);
        self.nodes.push(NodeRecord {
            id,
            position: TransportPosition::default(),
            used: false,
            alive: false,
            outgoing: Vec::new(),
            incident: 0,
        });
        self.next_node = next;
        Ok(id)
    }
    pub fn reserve_edge(&mut self) -> Result<TransportEdgeId, CommandError> {
        let next = self
            .next_edge
            .checked_add(1)
            .ok_or(CommandError::TransportIdOverflow)?;
        if self.edges.len() >= 4_000_000 {
            return Err(CommandError::TransportSafetyGuard);
        }
        self.edges
            .try_reserve(1)
            .map_err(|_| CommandError::Allocation)?;
        let id = TransportEdgeId(self.next_edge);
        self.edges.push(EdgeRecord {
            id,
            data: None,
            alive: false,
        });
        self.next_edge = next;
        Ok(id)
    }
    // Direct ID-to-slot mapping is temporary and always resolved here.
    pub fn node_slot(&self, id: TransportNodeId) -> Option<usize> {
        let slot = usize::try_from(id.0).ok()?;
        self.nodes.get(slot).filter(|n| n.id == id).map(|_| slot)
    }
    pub fn edge_slot(&self, id: TransportEdgeId) -> Option<usize> {
        let slot = usize::try_from(id.0).ok()?;
        self.edges.get(slot).filter(|e| e.id == id).map(|_| slot)
    }
    pub fn live_node(&self, id: TransportNodeId) -> Result<usize, CommandError> {
        self.node_slot(id)
            .filter(|&i| self.nodes[i].alive)
            .ok_or(CommandError::InvalidTransportNode(id))
    }
    fn live_edge(&self, id: TransportEdgeId) -> Result<usize, CommandError> {
        self.edge_slot(id)
            .filter(|&i| self.edges[i].alive)
            .ok_or(CommandError::InvalidTransportEdge(id))
    }
    pub fn apply(&mut self, command: SimulationCommand) -> Result<(), CommandError> {
        use SimulationCommand::*;
        let changed = match command {
            CreateTransportNode { node_id, position } => {
                let slot = self
                    .node_slot(node_id)
                    .ok_or(CommandError::InvalidTransportNode(node_id))?;
                if self.nodes[slot].used {
                    return Err(CommandError::TransportNodeIdUsed(node_id));
                }
                const LIMIT: i64 = 1 << 50;
                if [position.x, position.y, position.z]
                    .iter()
                    .any(|v| !(-LIMIT..=LIMIT).contains(v))
                {
                    return Err(CommandError::InvalidTransportPosition);
                }
                let node = &mut self.nodes[slot];
                node.position = position;
                node.used = true;
                node.alive = true;
                true
            }
            CreateTransportEdge {
                edge_id,
                from,
                to,
                mode,
                base_cost,
                enabled,
            } => {
                let slot = self
                    .edge_slot(edge_id)
                    .ok_or(CommandError::InvalidTransportEdge(edge_id))?;
                if self.edges[slot].data.is_some() {
                    return Err(CommandError::TransportEdgeIdUsed(edge_id));
                }
                let source = self.live_node(from)?;
                let target = self.live_node(to)?;
                if from == to {
                    return Err(CommandError::TransportSelfLoop);
                }
                if base_cost == 0 {
                    return Err(CommandError::InvalidTransportCost);
                }
                self.nodes[source]
                    .outgoing
                    .try_reserve(1)
                    .map_err(|_| CommandError::Allocation)?;
                self.edges[slot].data = Some(EdgeData {
                    from,
                    to,
                    mode,
                    base_cost,
                    physical_length: physical_length(
                        self.nodes[source].position,
                        self.nodes[target].position,
                    )?,
                    enabled,
                });
                self.edges[slot].alive = true;
                self.nodes[source].outgoing.push(edge_id);
                self.nodes[source].incident += 1;
                self.nodes[target].incident += 1;
                true
            }
            RemoveTransportNode { node_id } => {
                let slot = self.live_node(node_id)?;
                if self.nodes[slot].incident != 0 {
                    return Err(CommandError::TransportNodeInUse(node_id));
                }
                self.nodes[slot].alive = false;
                true
            }
            RemoveTransportEdge { edge_id } => {
                let slot = self.live_edge(edge_id)?;
                let data = self.edges[slot].data.expect("live edge has data");
                let from = self.live_node(data.from)?;
                let to = self.live_node(data.to)?;
                self.edges[slot].alive = false;
                self.nodes[from].incident -= 1;
                self.nodes[to].incident -= 1;
                true
            }
            SetTransportEdgeEnabled { edge_id, enabled } => {
                let slot = self.live_edge(edge_id)?;
                let data = self.edges[slot].data.as_mut().expect("live edge has data");
                let changed = data.enabled != enabled;
                data.enabled = enabled;
                changed
            }
            SetTransportEdgeCost { edge_id, base_cost } => {
                let slot = self.live_edge(edge_id)?;
                if base_cost == 0 {
                    return Err(CommandError::InvalidTransportCost);
                }
                let data = self.edges[slot].data.as_mut().expect("live edge has data");
                let changed = data.base_cost != base_cost;
                data.base_cost = base_cost;
                changed
            }
            _ => unreachable!("only transport commands are dispatched to the network"),
        };
        if changed {
            self.revision = self.revision.saturating_add(1);
        }
        Ok(())
    }
    pub fn edge_data(&self, id: TransportEdgeId) -> Option<(bool, EdgeData)> {
        let edge = &self.edges[self.edge_slot(id)?];
        Some((edge.alive, edge.data?))
    }
    pub fn node_position(&self, id: TransportNodeId) -> Option<TransportPosition> {
        let node = &self.nodes[self.node_slot(id)?];
        node.used.then_some(node.position)
    }
    pub fn checksum(&self, hash: &mut StableChecksum) {
        hash.u64(self.next_node);
        hash.u64(self.next_edge);
        hash.u64(self.revision);
        hash.u64(self.nodes.len() as u64);
        for node in &self.nodes {
            hash.u64(node.id.0);
            hash.bytes(&[u8::from(node.used), u8::from(node.alive)]);
            for v in [node.position.x, node.position.y, node.position.z] {
                hash.bytes(&v.to_le_bytes());
            }
        }
        hash.u64(self.edges.len() as u64);
        for edge in &self.edges {
            hash.u64(edge.id.0);
            hash.bytes(&[u8::from(edge.alive), u8::from(edge.data.is_some())]);
            if let Some(data) = edge.data {
                hash.u64(data.from.0);
                hash.u64(data.to.0);
                hash.bytes(&[data.mode.tag()]);
                hash.u64(data.base_cost);
                hash.u64(data.physical_length);
                hash.bytes(&[u8::from(data.enabled)]);
            }
        }
    }
}

/// Manhattan length in X/Y/Z, minimum one for distinct colocated nodes.
fn physical_length(a: TransportPosition, b: TransportPosition) -> Result<u64, CommandError> {
    let length = (i128::from(a.x) - i128::from(b.x)).unsigned_abs()
        + (i128::from(a.y) - i128::from(b.y)).unsigned_abs()
        + (i128::from(a.z) - i128::from(b.z)).unsigned_abs();
    u64::try_from(length.max(1)).map_err(|_| CommandError::InvalidTransportLength)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn id_counter_overflow_does_not_create_reservations() {
        let mut network = TransportNetwork {
            next_node: u64::MAX,
            next_edge: u64::MAX,
            ..Default::default()
        };
        assert_eq!(
            network.reserve_node(),
            Err(CommandError::TransportIdOverflow)
        );
        assert_eq!(
            network.reserve_edge(),
            Err(CommandError::TransportIdOverflow)
        );
        assert!(network.nodes.is_empty());
        assert!(network.edges.is_empty());
    }
    #[test]
    fn global_revision_saturates_explicitly() {
        let mut network = TransportNetwork {
            revision: u64::MAX - 1,
            ..Default::default()
        };
        let a = network.reserve_node().unwrap();
        let b = network.reserve_node().unwrap();
        for node_id in [a, b] {
            network
                .apply(SimulationCommand::CreateTransportNode {
                    node_id,
                    position: TransportPosition::default(),
                })
                .unwrap();
        }
        assert_eq!(network.revision, u64::MAX);
        network
            .apply(SimulationCommand::RemoveTransportNode { node_id: a })
            .unwrap();
        assert_eq!(network.revision, u64::MAX);
    }
}
