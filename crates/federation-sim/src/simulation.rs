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
        })
    }
    pub fn tick(&mut self) -> Result<(), SimulationError> {
        self.scheduler.tick(
            &mut self.vehicles,
            &mut self.terrain,
            &mut self.commands,
            &self.config,
            &mut self.ticks,
        )
    }
    pub fn tick_count(&self) -> u64 {
        self.ticks
    }
    pub fn config(&self) -> &SimulationConfig {
        &self.config
    }
    pub fn terrain(&self) -> &Terrain {
        &self.terrain
    }
    pub fn vehicles(&self) -> &Vehicles {
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

    pub fn checksum(&self) -> u64 {
        let mut hash = StableChecksum::new();
        hash.bytes(b"federation-state-v2");
        self.commands.checksum(&mut hash);
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
            hash.bytes(&[u8::from(chunk.dirty)]);
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
        sim.terrain.clear_dirty();
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
