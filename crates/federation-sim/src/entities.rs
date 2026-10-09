use crate::{
    CommandError, SimulationError, VehicleId, VehicleSnapshot, filled_vec, rng::DeterministicRng,
};
use crate::{
    MovementEvent, MovementEventKind, MovementMetrics, RouteExecutionMemoryEstimate, RoutePlanId,
    TransportEdgeId, TransportNodeId, TransportPosition, VehicleMovementState,
    metrics::StableChecksum,
    movement::{Execution, advance},
    route_plans::RoutePlans,
    transport::network::TransportNetwork,
};
use rayon::prelude::*;

pub const POSITION_UNITS_PER_TILE: i64 = 1024;

#[derive(Debug)]
pub struct Vehicles {
    #[cfg(test)]
    observed_workers: std::sync::atomic::AtomicU64,
    ids: Vec<VehicleId>,
    x: Vec<i64>,
    y: Vec<i64>,
    vx: Vec<i64>,
    vy: Vec<i64>,
    z: Vec<i64>,
    plan_ids: Vec<Option<RoutePlanId>>,
    edge_indices: Vec<usize>,
    progress: Vec<u64>,
    states: Vec<VehicleMovementState>,
    current_edges: Vec<Option<TransportEdgeId>>,
    current_nodes: Vec<Option<TransportNodeId>>,
    speeds: Vec<u64>,
    slot_events: Vec<Option<MovementEventKind>>,
    events: Vec<MovementEvent>,
    movement_metrics: MovementMetrics,
}

impl Vehicles {
    pub(crate) fn new(
        count: usize,
        width: u32,
        height: u32,
        seed: u64,
    ) -> Result<Self, SimulationError> {
        let mut result = Self {
            #[cfg(test)]
            observed_workers: std::sync::atomic::AtomicU64::new(0),
            ids: filled_vec(count, VehicleId(0))?,
            x: filled_vec(count, 0)?,
            y: filled_vec(count, 0)?,
            vx: filled_vec(count, 0)?,
            vy: filled_vec(count, 0)?,
            z: filled_vec(count, 0)?,
            plan_ids: filled_vec(count, None)?,
            edge_indices: filled_vec(count, 0)?,
            progress: filled_vec(count, 0)?,
            states: filled_vec(count, VehicleMovementState::Idle)?,
            current_edges: filled_vec(count, None)?,
            current_nodes: filled_vec(count, None)?,
            speeds: filled_vec(count, 8)?,
            slot_events: filled_vec(count, None)?,
            events: Vec::new(),
            movement_metrics: MovementMetrics::default(),
        };
        result.events.try_reserve_exact(count)?;
        let mut rng = DeterministicRng::new(seed);
        for i in 0..count {
            result.ids[i] = VehicleId(i as u64);
            result.x[i] =
                (rng.next_u64() % (u64::from(width) * POSITION_UNITS_PER_TILE as u64)) as i64;
            result.y[i] =
                (rng.next_u64() % (u64::from(height) * POSITION_UNITS_PER_TILE as u64)) as i64;
            result.vx[i] = (rng.next_u64() % 17) as i64 - 8;
            result.vy[i] = (rng.next_u64() % 17) as i64 - 8;
        }
        Ok(result)
    }
    #[cfg(test)]
    pub(crate) fn observed_worker_count(&self) -> u32 {
        self.observed_workers
            .load(std::sync::atomic::Ordering::Relaxed)
            .count_ones()
    }
    pub fn len(&self) -> usize {
        self.ids.len()
    }
    pub fn ids(&self) -> &[VehicleId] {
        &self.ids
    }
    pub fn x(&self) -> &[i64] {
        &self.x
    }
    pub fn y(&self) -> &[i64] {
        &self.y
    }
    pub fn vx(&self) -> &[i64] {
        &self.vx
    }
    pub fn vy(&self) -> &[i64] {
        &self.vy
    }

    // Temporary direct identity-to-slot mapping, never a public contract.
    fn slot(&self, id: VehicleId) -> Option<usize> {
        let slot = usize::try_from(id.0).ok()?;
        (self.ids.get(slot) == Some(&id)).then_some(slot)
    }
    pub fn get(&self, id: VehicleId) -> Option<VehicleSnapshot> {
        let i = self.slot(id)?;
        Some(VehicleSnapshot {
            id,
            x: self.x[i],
            y: self.y[i],
            vx: self.vx[i],
            vy: self.vy[i],
            z: self.z[i],
            movement_state: self.states[i],
            route_plan_id: self.plan_ids[i],
            route_edge_index: self.edge_indices[i],
            current_edge: self.current_edges[i],
            edge_progress: self.progress[i],
            current_node: self.current_nodes[i],
            speed_units_per_ms: self.speeds[i],
        })
    }
    pub(crate) fn set_velocity(
        &mut self,
        id: VehicleId,
        vx: i64,
        vy: i64,
    ) -> Result<(), CommandError> {
        let i = self.slot(id).ok_or(CommandError::InvalidVehicle(id))?;
        // Temporary guard safe for any u32 timestep and validated map extent.
        const MAX_VELOCITY: i64 = 1_000_000;
        if !(-MAX_VELOCITY..=MAX_VELOCITY).contains(&vx)
            || !(-MAX_VELOCITY..=MAX_VELOCITY).contains(&vy)
        {
            return Err(CommandError::InvalidVelocity);
        }
        self.vx[i] = vx;
        self.vy[i] = vy;
        Ok(())
    }

    pub fn set_network_speed(&mut self, id: VehicleId, speed: u64) -> Result<(), CommandError> {
        let slot = self.slot(id).ok_or(CommandError::InvalidVehicle(id))?;
        if speed > 1_000_000 {
            return Err(CommandError::InvalidNetworkSpeed);
        }
        self.speeds[slot] = speed;
        Ok(())
    }
    pub fn assign_route(
        &mut self,
        id: VehicleId,
        plan_id: RoutePlanId,
        plans: &RoutePlans,
        network: &TransportNetwork,
    ) -> Result<(), CommandError> {
        let slot = self.slot(id).ok_or(CommandError::InvalidVehicle(id))?;
        let plan = plans
            .get(plan_id)
            .ok_or(CommandError::InvalidRoutePlan(plan_id))?;
        if self.current_edges[slot].is_some()
            || self.current_nodes[slot].is_some_and(|n| n != plan.origin)
        {
            return Err(CommandError::InvalidRouteAssignment);
        }
        network.live_node(plan.origin)?;
        let origin = network
            .node_position(plan.origin)
            .ok_or(CommandError::InvalidTransportNode(plan.origin))?;
        self.x[slot] = origin.x;
        self.y[slot] = origin.y;
        self.z[slot] = origin.z;
        self.plan_ids[slot] = Some(plan_id);
        self.edge_indices[slot] = 0;
        self.progress[slot] = 0;
        self.current_edges[slot] = None;
        self.current_nodes[slot] = Some(plan.origin);
        self.slot_events[slot] = None;
        self.states[slot] = if plan.edges.is_empty() {
            self.slot_events[slot] = Some(MovementEventKind::VehicleArrived);
            VehicleMovementState::Arrived
        } else {
            VehicleMovementState::FollowingRoute
        };
        Ok(())
    }
    pub fn begin_tick(&mut self) {
        self.events.clear();
        self.slot_events.fill(None);
        self.movement_metrics = MovementMetrics::default();
    }
    pub fn events(&self) -> &[MovementEvent] {
        &self.events
    }
    pub fn movement_metrics(&self) -> MovementMetrics {
        self.movement_metrics
    }
    pub fn memory_estimate(&self) -> RouteExecutionMemoryEstimate {
        fn bytes<T>(v: &Vec<T>) -> u64 {
            (v.capacity() * std::mem::size_of::<T>()) as u64
        }
        RouteExecutionMemoryEstimate {
            vehicle_base_arrays_bytes: bytes(&self.ids)
                + bytes(&self.x)
                + bytes(&self.y)
                + bytes(&self.vx)
                + bytes(&self.vy),
            movement_arrays_bytes: bytes(&self.z)
                + bytes(&self.plan_ids)
                + bytes(&self.edge_indices)
                + bytes(&self.progress)
                + bytes(&self.states)
                + bytes(&self.current_edges)
                + bytes(&self.current_nodes)
                + bytes(&self.speeds),
            event_buffers_bytes: bytes(&self.slot_events) + bytes(&self.events),
            ..Default::default()
        }
    }
    pub fn checksum_movement(&self, hash: &mut StableChecksum) {
        for i in 0..self.len() {
            hash.bytes(&self.z[i].to_le_bytes());
            hash.bytes(&[self.states[i].tag()]);
            hash.bytes(&[u8::from(self.plan_ids[i].is_some())]);
            if let Some(id) = self.plan_ids[i] {
                hash.u64(id.0);
            }
            hash.u64(self.edge_indices[i] as u64);
            hash.u64(self.progress[i]);
            hash.u64(self.speeds[i]);
            hash.bytes(&[u8::from(self.current_edges[i].is_some())]);
            if let Some(id) = self.current_edges[i] {
                hash.u64(id.0);
            }
            hash.bytes(&[u8::from(self.current_nodes[i].is_some())]);
            if let Some(id) = self.current_nodes[i] {
                hash.u64(id.0);
            }
        }
    }
    pub(crate) fn move_parallel(
        &mut self,
        width: u32,
        height: u32,
        tick_ms: u32,
        network: &TransportNetwork,
        plans: &RoutePlans,
    ) {
        let extent_x = i64::from(width) * POSITION_UNITS_PER_TILE;
        let extent_y = i64::from(height) * POSITION_UNITS_PER_TILE;
        let dt = i64::from(tick_ms);
        #[cfg(test)]
        let observed_workers = &self.observed_workers;
        let totals = (&mut self.x, &mut self.y, &mut self.z)
            .into_par_iter()
            .zip(
                (
                    &mut self.edge_indices,
                    &mut self.progress,
                    &mut self.states,
                    &mut self.current_edges,
                    &mut self.current_nodes,
                    &mut self.slot_events,
                )
                    .into_par_iter(),
            )
            .zip((&self.plan_ids, &self.speeds, &self.vx, &self.vy).into_par_iter())
            .with_min_len(1024)
            .map(
                |(
                    ((x, y, z), (index, progress, state, current_edge, current_node, event)),
                    (plan_id, speed, vx, vy),
                )| {
                    #[cfg(test)]
                    if let Some(worker) = rayon::current_thread_index().filter(|&i| i < 64) {
                        observed_workers
                            .fetch_or(1 << worker, std::sync::atomic::Ordering::Relaxed);
                    }
                    if plan_id.is_none() && *state == VehicleMovementState::Idle {
                        *x = (*x + vx * dt).rem_euclid(extent_x);
                        *y = (*y + vy * dt).rem_euclid(extent_y);
                        return (0_u64, 0_u64);
                    }
                    if *state != VehicleMovementState::FollowingRoute {
                        return (0, 0);
                    }
                    let mut execution = Execution {
                        state: *state,
                        edge_index: *index,
                        progress: *progress,
                        edge: *current_edge,
                        node: *current_node,
                    };
                    let mut position = TransportPosition {
                        x: *x,
                        y: *y,
                        z: *z,
                    };
                    let (transitions, terminal) = advance(
                        *plan_id,
                        &mut execution,
                        &mut position,
                        u128::from(*speed) * u128::from(tick_ms),
                        network,
                        plans,
                    );
                    *x = position.x;
                    *y = position.y;
                    *z = position.z;
                    *index = execution.edge_index;
                    *progress = execution.progress;
                    *state = execution.state;
                    *current_edge = execution.edge;
                    *current_node = execution.node;
                    if terminal.is_some() {
                        *event = terminal;
                    }
                    (1, transitions)
                },
            )
            .reduce(|| (0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
        self.movement_metrics.routed_vehicles_updated = totals.0;
        self.movement_metrics.edge_transitions = totals.1;
        // Stable per-vehicle merge, including arrivals produced during command assignment.
        for (slot, event) in self.slot_events.iter().enumerate() {
            if let Some(kind) = event {
                self.events.push(MovementEvent {
                    vehicle_id: self.ids[slot],
                    kind: *kind,
                    current_node: self.current_nodes[slot],
                });
                match kind {
                    MovementEventKind::VehicleArrived => self.movement_metrics.arrivals += 1,
                    MovementEventKind::VehicleNeedsRoute(reason) => {
                        self.movement_metrics.needs_route += 1;
                        if *reason == crate::MovementBlockReason::TransitionSafetyGuard {
                            self.movement_metrics.safety_guard_failures += 1;
                        }
                    }
                }
            }
        }
    }
}
