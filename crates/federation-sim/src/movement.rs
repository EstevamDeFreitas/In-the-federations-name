#![doc = include_str!("../../../docs/vehicle-routing.md")]
use crate::{
    RoutePlanId, TransportEdgeId, TransportNodeId, TransportPosition, route_plans::RoutePlans,
    transport::network::TransportNetwork,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum VehicleMovementState {
    #[default]
    Idle,
    FollowingRoute,
    Arrived,
    NeedsRoute,
}
impl VehicleMovementState {
    pub(crate) fn tag(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::FollowingRoute => 1,
            Self::Arrived => 2,
            Self::NeedsRoute => 3,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementBlockReason {
    InvalidPlan,
    InvalidEdge,
    InconsistentEdge,
    TransitionSafetyGuard,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementEventKind {
    VehicleArrived,
    VehicleNeedsRoute(MovementBlockReason),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementEvent {
    pub vehicle_id: crate::VehicleId,
    pub kind: MovementEventKind,
    pub current_node: Option<TransportNodeId>,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MovementMetrics {
    pub routed_vehicles_updated: u64,
    pub edge_transitions: u64,
    pub arrivals: u64,
    pub needs_route: u64,
    pub safety_guard_failures: u64,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RouteExecutionMemoryEstimate {
    pub vehicle_base_arrays_bytes: u64,
    pub movement_arrays_bytes: u64,
    pub event_buffers_bytes: u64,
    pub route_plan_records_bytes: u64,
    pub route_plan_edges_bytes: u64,
}
pub(crate) struct Execution {
    pub state: VehicleMovementState,
    pub edge_index: usize,
    pub progress: u64,
    pub edge: Option<TransportEdgeId>,
    pub node: Option<TransportNodeId>,
}
pub(crate) const MAX_EDGE_TRANSITIONS_PER_TICK: u64 = 10_000;

fn blocked(execution: &mut Execution, reason: MovementBlockReason) -> Option<MovementEventKind> {
    execution.state = VehicleMovementState::NeedsRoute;
    Some(MovementEventKind::VehicleNeedsRoute(reason))
}
fn interpolate(a: i64, b: i64, progress: u64, length: u64) -> i64 {
    // Coordinate guards bound |delta| to 2^51; product remains within i128.
    (i128::from(a) + (i128::from(b) - i128::from(a)) * i128::from(progress) / i128::from(length))
        as i64
}
pub(crate) fn advance(
    plan_id: Option<RoutePlanId>,
    execution: &mut Execution,
    position: &mut TransportPosition,
    travel_budget: u128,
    network: &TransportNetwork,
    plans: &RoutePlans,
) -> (u64, Option<MovementEventKind>) {
    let Some(plan) = plan_id.and_then(|id| plans.get(id)) else {
        return (0, blocked(execution, MovementBlockReason::InvalidPlan));
    };
    let mut budget = travel_budget;
    let mut transitions = 0;
    loop {
        if execution.edge_index >= plan.edges.len() {
            if execution.edge_index == plan.edges.len() && execution.node == Some(plan.destination)
            {
                execution.state = VehicleMovementState::Arrived;
                return (transitions, Some(MovementEventKind::VehicleArrived));
            }
            return (
                transitions,
                blocked(execution, MovementBlockReason::InvalidPlan),
            );
        }
        if transitions >= MAX_EDGE_TRANSITIONS_PER_TICK {
            return (
                transitions,
                blocked(execution, MovementBlockReason::TransitionSafetyGuard),
            );
        }
        let id = plan.edges[execution.edge_index];
        let Some((alive, edge)) = network.edge_data(id) else {
            return (
                transitions,
                blocked(execution, MovementBlockReason::InvalidEdge),
            );
        };
        if execution.edge.is_none() {
            if execution.node != Some(edge.from) {
                return (
                    transitions,
                    blocked(execution, MovementBlockReason::InconsistentEdge),
                );
            }
            if !alive
                || !edge.enabled
                || edge.mode != plan.mode
                || network.live_node(edge.from).is_err()
                || network.live_node(edge.to).is_err()
            {
                return (
                    transitions,
                    blocked(execution, MovementBlockReason::InvalidEdge),
                );
            }
            execution.edge = Some(id);
        } else if execution.edge != Some(id) {
            return (
                transitions,
                blocked(execution, MovementBlockReason::InconsistentEdge),
            );
        }
        let Some(from) = network.node_position(edge.from) else {
            return (
                transitions,
                blocked(execution, MovementBlockReason::InvalidEdge),
            );
        };
        let Some(to) = network.node_position(edge.to) else {
            return (
                transitions,
                blocked(execution, MovementBlockReason::InvalidEdge),
            );
        };
        if edge.physical_length == 0 || execution.progress > edge.physical_length {
            return (
                transitions,
                blocked(execution, MovementBlockReason::InvalidEdge),
            );
        }
        let remaining = edge.physical_length - execution.progress;
        if budget < u128::from(remaining) {
            execution.progress += budget as u64;
            *position = TransportPosition {
                x: interpolate(from.x, to.x, execution.progress, edge.physical_length),
                y: interpolate(from.y, to.y, execution.progress, edge.physical_length),
                z: interpolate(from.z, to.z, execution.progress, edge.physical_length),
            };
            return (transitions, None);
        }
        budget -= u128::from(remaining);
        *position = to;
        execution.node = Some(edge.to);
        execution.edge = None;
        execution.progress = 0;
        execution.edge_index += 1;
        transitions += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Route, SimulationCommand, TransportMode};
    fn setup(
        cycle: bool,
    ) -> (
        TransportNetwork,
        RoutePlans,
        RoutePlanId,
        TransportNodeId,
        TransportNodeId,
    ) {
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
        let ab = network.reserve_edge().unwrap();
        let ba = network.reserve_edge().unwrap();
        for (edge_id, from, to) in [(ab, a, b), (ba, b, a)] {
            network
                .apply(SimulationCommand::CreateTransportEdge {
                    edge_id,
                    from,
                    to,
                    mode: TransportMode::Road,
                    base_cost: 1,
                    enabled: true,
                })
                .unwrap();
        }
        let edges = if cycle {
            (0..10_002)
                .map(|i| if i % 2 == 0 { ab } else { ba })
                .collect()
        } else {
            vec![ab]
        };
        let route = Route {
            network_revision: network.revision,
            origin: a,
            destination: if cycle { a } else { b },
            mode: TransportMode::Road,
            total_cost: if cycle { 10_002 } else { 1 },
            edges,
        };
        let mut plans = RoutePlans::default();
        let id = plans.reserve().unwrap();
        plans.store(id, route, &network).unwrap();
        (network, plans, id, a, b)
    }
    #[test]
    fn inconsistent_next_edge_stops_without_teleport() {
        let (network, plans, id, _, b) = setup(false);
        let mut execution = Execution {
            state: VehicleMovementState::FollowingRoute,
            edge_index: 0,
            progress: 0,
            edge: None,
            node: Some(b),
        };
        let mut position = TransportPosition {
            x: 17,
            y: 23,
            z: -9,
        };
        let before = position;
        let result = advance(
            Some(id),
            &mut execution,
            &mut position,
            100,
            &network,
            &plans,
        );
        assert_eq!(
            result,
            (
                0,
                Some(MovementEventKind::VehicleNeedsRoute(
                    MovementBlockReason::InconsistentEdge
                ))
            )
        );
        assert_eq!(position, before);
        assert_eq!(execution.node, Some(b));
        assert_eq!(execution.state, VehicleMovementState::NeedsRoute);
    }
    #[test]
    fn defensive_transition_guard_records_deterministic_failure() {
        let (network, plans, id, a, _) = setup(true);
        let mut execution = Execution {
            state: VehicleMovementState::FollowingRoute,
            edge_index: 0,
            progress: 0,
            edge: None,
            node: Some(a),
        };
        let mut position = TransportPosition::default();
        let result = advance(
            Some(id),
            &mut execution,
            &mut position,
            1_000_000,
            &network,
            &plans,
        );
        assert_eq!(
            result,
            (
                MAX_EDGE_TRANSITIONS_PER_TICK,
                Some(MovementEventKind::VehicleNeedsRoute(
                    MovementBlockReason::TransitionSafetyGuard
                ))
            )
        );
        assert_eq!(execution.state, VehicleMovementState::NeedsRoute);
        assert_eq!(execution.progress, 0);
        assert_eq!(execution.edge, None);
    }
}
