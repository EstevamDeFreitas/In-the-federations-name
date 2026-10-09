use crate::transport::{
    Route, RouteError, RouteRequest, RoutingWorkspace, TransportEdgeId, TransportNodeId,
    network::TransportNetwork,
};
use crate::{
    MovementEvent, MovementMetrics, RouteExecutionMemoryEstimate, RoutePlanId,
    route_plans::RoutePlans, scheduler::TickState,
};
use crate::{
    SimulationError,
    commands::{CommandError, CommandQueue, CommandResult, CommandSequence, SimulationCommand},
    config::SimulationConfig,
    entities::Vehicles,
    metrics::{SimulationMetrics, StableChecksum},
    scheduler::Scheduler,
    terrain::Terrain,
};

pub struct Simulation {
    config: SimulationConfig,
    terrain: Terrain,
    vehicles: Vehicles,
    scheduler: Scheduler,
    ticks: u64,
    commands: CommandQueue,
    pub(crate) transport: TransportNetwork,
    pub(crate) route_plans: RoutePlans,
}

impl Simulation {
    pub fn new(config: SimulationConfig) -> Result<Self, SimulationError> {
        config.validate()?;
        let terrain = Terrain::new(config.width, config.height, config.chunk_size)?;
        let vehicles = Vehicles::new(config.vehicles, config.width, config.height, config.seed)?;
        let scheduler = Scheduler::new(config.threads)?;
        Ok(Self {
            config,
            terrain,
            vehicles,
            scheduler,
            ticks: 0,
            commands: CommandQueue::default(),
            transport: TransportNetwork::default(),
            route_plans: RoutePlans::default(),
        })
    }
    pub fn tick(&mut self) -> Result<(), SimulationError> {
        self.scheduler.tick(
            TickState {
                vehicles: &mut self.vehicles,
                terrain: &mut self.terrain,
                commands: &mut self.commands,
                network: &mut self.transport,
                plans: &mut self.route_plans,
                ticks: &mut self.ticks,
            },
            &self.config,
        )
    }

    pub fn tick_count(&self) -> u64 {
        self.ticks
    }
    pub fn config(&self) -> &SimulationConfig {
        &self.config
    }
    pub(crate) fn terrain(&self) -> &Terrain {
        &self.terrain
    }
    pub(crate) fn vehicles(&self) -> &Vehicles {
        &self.vehicles
    }
    pub fn submit_command(
        &mut self,
        command: SimulationCommand,
    ) -> Result<CommandSequence, CommandError> {
        self.commands.submit(command)
    }
    /// Results of the last successful tick; replaced even by a tick with no commands.
    pub fn command_results(&self) -> &[CommandResult] {
        self.commands.results()
    }

    /// Reserves a unique identity; does not create a live node or change revision.
    pub fn reserve_transport_node_id(&mut self) -> Result<TransportNodeId, CommandError> {
        self.transport.reserve_node()
    }
    pub fn reserve_transport_edge_id(&mut self) -> Result<TransportEdgeId, CommandError> {
        self.transport.reserve_edge()
    }
    /// Allocate identity and enqueue creation in submission-call order.
    /// An identity reserved before a submission error is deliberately not reused.
    pub fn submit_create_transport_node(
        &mut self,
        position: crate::TransportPosition,
    ) -> Result<(TransportNodeId, CommandSequence), CommandError> {
        let node_id = self.reserve_transport_node_id()?;
        let sequence =
            self.submit_command(SimulationCommand::CreateTransportNode { node_id, position })?;
        Ok((node_id, sequence))
    }
    pub fn submit_create_transport_edge(
        &mut self,
        from: TransportNodeId,
        to: TransportNodeId,
        mode: crate::TransportMode,
        base_cost: u64,
        enabled: bool,
    ) -> Result<(TransportEdgeId, CommandSequence), CommandError> {
        let edge_id = self.reserve_transport_edge_id()?;
        let sequence = self.submit_command(SimulationCommand::CreateTransportEdge {
            edge_id,
            from,
            to,
            mode,
            base_cost,
            enabled,
        })?;
        Ok((edge_id, sequence))
    }
    pub fn reserve_route_plan_id(&mut self) -> Result<RoutePlanId, CommandError> {
        self.route_plans.reserve()
    }
    pub fn submit_store_route_plan(
        &mut self,
        route: Route,
    ) -> Result<(RoutePlanId, CommandSequence), CommandError> {
        let route_plan_id = self.reserve_route_plan_id()?;
        let sequence = self.submit_command(SimulationCommand::StoreRoutePlan {
            route_plan_id,
            route,
        })?;
        Ok((route_plan_id, sequence))
    }
    pub fn movement_events(&self) -> &[MovementEvent] {
        self.vehicles.events()
    }
    pub fn movement_metrics(&self) -> MovementMetrics {
        self.vehicles.movement_metrics()
    }
    /// Allocated payload capacities only; excludes allocator overhead, terrain and network.
    pub fn route_execution_memory_estimate(&self) -> RouteExecutionMemoryEstimate {
        let mut estimate = self.vehicles.memory_estimate();
        (
            estimate.route_plan_records_bytes,
            estimate.route_plan_edges_bytes,
        ) = self.route_plans.memory_bytes();
        estimate
    }
    pub fn find_route(
        &self,
        request: RouteRequest,
        workspace: &mut RoutingWorkspace,
    ) -> Result<Route, RouteError> {
        self.transport.find_route(request, workspace)
    }
    pub fn checksum(&self) -> u64 {
        let mut hash = StableChecksum::new();
        hash.bytes(b"federation-state-v4");
        self.commands.checksum(&mut hash);
        self.transport.checksum(&mut hash);
        self.route_plans.checksum(&mut hash);
        for value in [
            u64::from(self.config.width),
            u64::from(self.config.height),
            u64::from(self.config.chunk_size),
            u64::from(self.config.tick_ms),
            self.config.seed,
            self.ticks,
            self.vehicles.len() as u64,
        ] {
            hash.u64(value);
        }
        for &height in self.terrain.heights() {
            hash.bytes(&height.to_le_bytes());
        }
        for chunk in self.terrain.chunks() {
            hash.u64(chunk.revision);
        }
        for i in 0..self.vehicles.len() {
            hash.u64(self.vehicles.ids()[i].0);
            for value in [
                self.vehicles.x()[i],
                self.vehicles.y()[i],
                self.vehicles.vx()[i],
                self.vehicles.vy()[i],
            ] {
                hash.bytes(&value.to_le_bytes());
            }
        }
        self.vehicles.checksum_movement(&mut hash);
        hash.finish()
    }

    pub fn diagnostics(&self) -> SimulationMetrics {
        SimulationMetrics {
            tick_count: self.ticks,
            logical_time_ms: u128::from(self.ticks) * u128::from(self.config.tick_ms),
            vehicles: self.vehicles.len(),
            dirty_chunks: self.terrain.chunks().iter().filter(|c| c.dirty).count(),
            checksum: self.checksum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChunkRevisionQuery, SnapshotRequest};

    #[test]
    fn clearing_internal_dirty_flags_does_not_change_revision_queries() {
        let mut sim = Simulation::new(SimulationConfig {
            width: 2,
            height: 2,
            vehicles: 0,
            threads: 1,
            ..Default::default()
        })
        .unwrap();
        sim.submit_command(SimulationCommand::SetTerrainVertexHeight {
            x: 1,
            y: 1,
            height: 3,
        })
        .unwrap();
        sim.tick().unwrap();
        let queries = [ChunkRevisionQuery {
            x: 0,
            y: 0,
            known_revision: Some(0),
        }];
        let request = SnapshotRequest {
            terrain_chunks: &queries,
            ..Default::default()
        };
        let before = sim.snapshot(request).unwrap();
        let checksum = sim.checksum();
        sim.terrain.clear_dirty();
        assert_eq!(sim.checksum(), checksum);
        assert!(!sim.terrain.chunks()[0].dirty);
        assert_eq!(sim.snapshot(request).unwrap(), before);
    }

    #[test]
    fn tick_overflow_keeps_pending_commands_and_world_unchanged() {
        let mut sim = Simulation::new(SimulationConfig {
            width: 1,
            height: 1,
            vehicles: 0,
            threads: 1,
            ..Default::default()
        })
        .unwrap();
        sim.ticks = u64::MAX;
        sim.submit_command(SimulationCommand::SetTerrainVertexHeight {
            x: 0,
            y: 0,
            height: 9,
        })
        .unwrap();
        let checksum = sim.checksum();
        assert!(matches!(sim.tick(), Err(SimulationError::TickOverflow)));
        assert_eq!(sim.checksum(), checksum);
        assert_eq!(sim.terrain.height_at(0, 0).unwrap(), 0);
    }
}
