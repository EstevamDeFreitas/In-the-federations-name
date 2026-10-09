use crate::{
    CommandError, SimulationError, VehicleId, VehicleSnapshot, filled_vec, rng::DeterministicRng,
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
        };
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
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
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

    pub(crate) fn move_parallel(&mut self, width: u32, height: u32, tick_ms: u32) {
        let extent_x = i64::from(width) * POSITION_UNITS_PER_TILE;
        let extent_y = i64::from(height) * POSITION_UNITS_PER_TILE;
        let dt = i64::from(tick_ms);
        #[cfg(test)]
        let observed_workers = &self.observed_workers;
        // Rayon recursively splits indexed slices; every writer owns disjoint positions.
        self.x
            .par_iter_mut()
            .zip(self.y.par_iter_mut())
            .zip(self.vx.par_iter())
            .zip(self.vy.par_iter())
            .with_min_len(1024)
            .for_each(|(((x, y), vx), vy)| {
                #[cfg(test)]
                if let Some(worker) = rayon::current_thread_index().filter(|&i| i < 64) {
                    observed_workers.fetch_or(1 << worker, std::sync::atomic::Ordering::Relaxed);
                }
                *x = (*x + vx * dt).rem_euclid(extent_x);
                *y = (*y + vy * dt).rem_euclid(extent_y);
            });
    }
}
