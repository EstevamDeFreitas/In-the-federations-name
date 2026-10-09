use super::{TransportEdgeId, TransportMode, TransportNodeId, network::TransportNetwork};
use std::{cmp::Reverse, collections::BinaryHeap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteRequest {
    pub origin: TransportNodeId,
    pub destination: TransportNodeId,
    pub mode: TransportMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub network_revision: u64,
    pub origin: TransportNodeId,
    pub destination: TransportNodeId,
    pub mode: TransportMode,
    /// u128 permits a simple path of u64 edge costs without overflow.
    pub total_cost: u128,
    pub edges: Vec<TransportEdgeId>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RouteDiagnostics {
    pub visited_nodes: u64,
    pub relaxed_edges: u64,
    pub heap_pushes: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RouteError {
    #[error("origin does not exist: {0:?}")]
    InvalidOrigin(TransportNodeId),
    #[error("destination does not exist: {0:?}")]
    InvalidDestination(TransportNodeId),
    #[error("destination is unreachable in the requested mode")]
    NoRoute,
    #[error("cannot allocate routing storage")]
    Allocation,
}
#[derive(Default)]
pub struct RoutingWorkspace {
    distance: Vec<u128>,
    previous: Vec<Option<TransportEdgeId>>,
    seen: Vec<u64>,
    settled: Vec<u64>,
    generation: u64,
    heap: BinaryHeap<Reverse<(u128, TransportNodeId)>>,
    diagnostics: RouteDiagnostics,
}
impl RoutingWorkspace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn diagnostics(&self) -> RouteDiagnostics {
        self.diagnostics
    }
    fn prepare(&mut self, size: usize) -> Result<(), RouteError> {
        macro_rules! grow {
            ($field:ident, $value:expr) => {
                if self.$field.len() < size {
                    self.$field
                        .try_reserve(size - self.$field.len())
                        .map_err(|_| RouteError::Allocation)?;
                    self.$field.resize(size, $value);
                }
            };
        }
        grow!(distance, 0);
        grow!(previous, None);
        grow!(seen, 0);
        grow!(settled, 0);
        if self.generation == u64::MAX {
            self.seen.fill(0);
            self.settled.fill(0);
            self.generation = 0;
        }
        self.generation += 1;
        self.heap.clear();
        Ok(())
    }
    fn push(&mut self, cost: u128, id: TransportNodeId) -> Result<(), RouteError> {
        self.heap
            .try_reserve(1)
            .map_err(|_| RouteError::Allocation)?;
        self.heap.push(Reverse((cost, id)));
        self.diagnostics.heap_pushes += 1;
        Ok(())
    }
}
impl TransportNetwork {
    pub fn find_route(
        &self,
        request: RouteRequest,
        workspace: &mut RoutingWorkspace,
    ) -> Result<Route, RouteError> {
        workspace.diagnostics = RouteDiagnostics::default();
        let origin = self
            .live_node(request.origin)
            .map_err(|_| RouteError::InvalidOrigin(request.origin))?;
        let destination = self
            .live_node(request.destination)
            .map_err(|_| RouteError::InvalidDestination(request.destination))?;
        let mut route = Route {
            network_revision: self.revision,
            origin: request.origin,
            destination: request.destination,
            mode: request.mode,
            total_cost: 0,
            edges: Vec::new(),
        };
        if origin == destination {
            return Ok(route);
        }
        workspace.prepare(self.nodes.len())?;
        let generation = workspace.generation;
        workspace.seen[origin] = generation;
        workspace.distance[origin] = 0;
        workspace.previous[origin] = None;
        workspace.push(0, request.origin)?;
        while let Some(Reverse((cost, id))) = workspace.heap.pop() {
            let slot = self.node_slot(id).expect("heap contains existing node");
            if workspace.settled[slot] == generation || workspace.distance[slot] != cost {
                continue;
            }
            workspace.settled[slot] = generation;
            workspace.diagnostics.visited_nodes += 1;
            if slot == destination {
                route.total_cost = cost;
                let mut current = destination;
                while current != origin {
                    let edge_id =
                        workspace.previous[current].expect("reachable node has predecessor");
                    route
                        .edges
                        .try_reserve(1)
                        .map_err(|_| RouteError::Allocation)?;
                    route.edges.push(edge_id);
                    let edge = self.edges[self.edge_slot(edge_id).expect("predecessor exists")]
                        .data
                        .expect("predecessor has data");
                    current = self.node_slot(edge.from).expect("source exists");
                }
                route.edges.reverse();
                return Ok(route);
            }
            for &edge_id in &self.nodes[slot].outgoing {
                let record =
                    &self.edges[self.edge_slot(edge_id).expect("adjacency identity exists")];
                if !record.alive {
                    continue;
                }
                let edge = record.data.expect("live edge has data");
                if !edge.enabled || edge.mode != request.mode {
                    continue;
                }
                workspace.diagnostics.relaxed_edges += 1;
                let to = self.node_slot(edge.to).expect("live endpoint exists");
                if workspace.settled[to] == generation {
                    continue;
                }
                let candidate = cost + u128::from(edge.base_cost);
                if workspace.seen[to] != generation || candidate < workspace.distance[to] {
                    workspace.seen[to] = generation;
                    workspace.distance[to] = candidate;
                    workspace.previous[to] = Some(edge_id);
                    workspace.push(candidate, edge.to)?;
                } else if candidate == workspace.distance[to]
                    && workspace.previous[to].is_some_and(|prev| edge_id < prev)
                {
                    // Equal-distance predecessor is the smallest edge identity.
                    workspace.previous[to] = Some(edge_id);
                }
            }
        }
        Err(RouteError::NoRoute)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SimulationCommand, TransportPosition};
    #[test]
    fn workspace_reuses_capacity_and_clears_generation_markers_on_wrap() {
        let mut network = TransportNetwork::default();
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
        let edge_id = network.reserve_edge().unwrap();
        network
            .apply(SimulationCommand::CreateTransportEdge {
                edge_id,
                from: a,
                to: b,
                mode: TransportMode::Road,
                base_cost: 1,
                enabled: true,
            })
            .unwrap();
        let request = RouteRequest {
            origin: a,
            destination: b,
            mode: TransportMode::Road,
        };
        let mut workspace = RoutingWorkspace::new();
        let first = network.find_route(request, &mut workspace).unwrap();
        let capacities = (
            workspace.distance.capacity(),
            workspace.previous.capacity(),
            workspace.seen.capacity(),
            workspace.settled.capacity(),
            workspace.heap.capacity(),
        );
        workspace.generation = u64::MAX;
        workspace.seen.fill(1);
        workspace.settled.fill(1);
        assert_eq!(network.find_route(request, &mut workspace).unwrap(), first);
        assert_eq!(workspace.generation, 1);
        assert_eq!(
            (
                workspace.distance.capacity(),
                workspace.previous.capacity(),
                workspace.seen.capacity(),
                workspace.settled.capacity(),
                workspace.heap.capacity()
            ),
            capacities
        );
    }
}
