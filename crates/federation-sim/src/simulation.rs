use crate::{
    SimulationError,
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
        })
    }
    pub fn tick(&mut self) -> Result<(), SimulationError> {
        self.scheduler
            .tick(&mut self.vehicles, &self.config, &mut self.ticks)
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
    pub fn set_height(&mut self, x: u32, y: u32, height: i32) -> Result<(), SimulationError> {
        self.terrain.set_height(x, y, height)
    }
    pub fn clear_terrain_dirty(&mut self) {
        self.terrain.clear_dirty();
    }

    pub fn checksum(&self) -> u64 {
        let mut hash = StableChecksum::new();
        hash.bytes(b"federation-state-v1");
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
            hash.u64(self.vehicles.ids()[i]);
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
