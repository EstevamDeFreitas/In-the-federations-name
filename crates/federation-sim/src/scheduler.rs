use crate::{SimulationError, config::SimulationConfig, entities::Vehicles};
use crate::{commands::CommandQueue, terrain::Terrain};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    ApplyCommands,
    Movement,
    Finalize,
    Diagnostics,
}

pub const TICK_PHASES: [Phase; 3] = [Phase::ApplyCommands, Phase::Movement, Phase::Finalize];
pub const DIAGNOSTIC_PHASE: Phase = Phase::Diagnostics;

pub(crate) struct Scheduler {
    pool: rayon::ThreadPool,
}

impl Scheduler {
    pub fn new(threads: usize) -> Result<Self, SimulationError> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .thread_name(|i| format!("federation-{i}"))
            .build()?;
        Ok(Self { pool })
    }
    pub fn tick(
        &self,
        vehicles: &mut Vehicles,
        terrain: &mut Terrain,
        commands: &mut CommandQueue,
        config: &SimulationConfig,
        ticks: &mut u64,
    ) -> Result<(), SimulationError> {
        let next_tick = ticks.checked_add(1).ok_or(SimulationError::TickOverflow)?;
        for phase in TICK_PHASES {
            match phase {
                Phase::ApplyCommands => commands.apply(terrain, vehicles),
                Phase::Movement => self.pool.install(|| {
                    vehicles.move_parallel(config.width, config.height, config.tick_ms)
                }),
                Phase::Finalize => *ticks = next_tick,
                Phase::Diagnostics => unreachable!("diagnostics are requested separately"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn movement_runs_on_multiple_workers() {
        let scheduler = Scheduler::new(4).unwrap();
        let config = SimulationConfig {
            vehicles: 100_000,
            threads: 4,
            ..SimulationConfig::default()
        };
        let mut vehicles =
            Vehicles::new(config.vehicles, config.width, config.height, config.seed).unwrap();
        let mut ticks = 0;
        let mut terrain = Terrain::new(config.width, config.height, config.chunk_size).unwrap();
        let mut commands = CommandQueue::default();
        for _ in 0..4 {
            scheduler
                .tick(
                    &mut vehicles,
                    &mut terrain,
                    &mut commands,
                    &config,
                    &mut ticks,
                )
                .unwrap();
        }
        assert!(vehicles.observed_worker_count() > 1);
        assert_eq!(ticks, 4);
    }
    #[test]
    fn pool_has_requested_workers() {
        let scheduler = Scheduler::new(4).unwrap();
        let mut workers = scheduler.pool.broadcast(|ctx| ctx.index());
        workers.sort_unstable();
        assert_eq!(workers, vec![0, 1, 2, 3]);
        assert_eq!(
            TICK_PHASES,
            [Phase::ApplyCommands, Phase::Movement, Phase::Finalize]
        );
    }
}
