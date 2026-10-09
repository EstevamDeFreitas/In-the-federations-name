use crate::SimulationError;

/// Provisional units: 1024 position units per tile, integer velocity units/ms.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationConfig {
    pub width: u32,
    pub height: u32,
    pub chunk_size: u32,
    pub vehicles: usize,
    pub threads: usize,
    pub seed: u64,
    pub tick_ms: u32,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            chunk_size: 32,
            vehicles: 100_000,
            threads: std::thread::available_parallelism().map_or(1, usize::from),
            seed: 42,
            tick_ms: 20,
        }
    }
}

impl SimulationConfig {
    pub fn validate(&self) -> Result<(), SimulationError> {
        validate_geometry(self.width, self.height, self.chunk_size)?;
        if self.threads == 0 || self.threads > 1024 {
            return Err(SimulationError::InvalidConfig(
                "threads must be in 1..=1024",
            ));
        }
        if self.tick_ms == 0 {
            return Err(SimulationError::InvalidConfig(
                "tick_ms must be greater than zero",
            ));
        }
        if self.vehicles > 10_000_000 {
            return Err(SimulationError::InvalidConfig(
                "vehicles exceeds the initial limit of 10,000,000",
            ));
        }
        Ok(())
    }
}

pub(crate) fn validate_geometry(
    width: u32,
    height: u32,
    chunk_size: u32,
) -> Result<(), SimulationError> {
    if width == 0 || height == 0 || chunk_size == 0 {
        return Err(SimulationError::InvalidConfig(
            "width, height and chunk_size must be greater than zero",
        ));
    }
    let vertices = (u64::from(width) + 1).checked_mul(u64::from(height) + 1);
    if !matches!(vertices, Some(count) if count <= 16_777_216) {
        return Err(SimulationError::InvalidConfig(
            "heightmap exceeds the initial limit of 16,777,216 vertices",
        ));
    }
    Ok(())
}
