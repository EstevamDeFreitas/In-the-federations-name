use crate::{VehicleId, entities::Vehicles, metrics::StableChecksum, terrain::Terrain};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CommandSequence(pub(crate) u64);
impl CommandSequence {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimulationCommand {
    SetTerrainVertexHeight {
        x: u32,
        y: u32,
        height: i32,
    },
    SetVehicleVelocity {
        vehicle_id: VehicleId,
        vx: i64,
        vy: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandEnvelope {
    pub sequence: CommandSequence,
    pub command: SimulationCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("terrain vertex ({x}, {y}) is outside the map")]
    InvalidVertex { x: u32, y: u32 },
    #[error("vehicle identity does not exist: {0:?}")]
    InvalidVehicle(VehicleId),
    #[error("velocity exceeds the temporary safety guard")]
    InvalidVelocity,
    #[error("command sequence exhausted")]
    SequenceOverflow,
    #[error("cannot reserve command queue storage")]
    Allocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandResult {
    pub sequence: CommandSequence,
    pub result: Result<(), CommandError>,
}

/// Serial FIFO with simulation-issued unique sequences; buffers are reused.
pub(crate) struct CommandQueue {
    next: u64,
    pending: Vec<CommandEnvelope>,
    results: Vec<CommandResult>,
}
impl Default for CommandQueue {
    fn default() -> Self {
        Self {
            next: 1,
            pending: Vec::new(),
            results: Vec::new(),
        }
    }
}
impl CommandQueue {
    pub fn submit(&mut self, command: SimulationCommand) -> Result<CommandSequence, CommandError> {
        let next = self
            .next
            .checked_add(1)
            .ok_or(CommandError::SequenceOverflow)?;
        self.pending
            .try_reserve(1)
            .map_err(|_| CommandError::Allocation)?;
        // Reserve results at submission so ApplyCommands cannot fail midway or allocate.
        let required = self.pending.len() + 1;
        if self.results.capacity() < required {
            self.results
                .try_reserve(required.saturating_sub(self.results.len()))
                .map_err(|_| CommandError::Allocation)?;
        }
        let sequence = CommandSequence(self.next);
        self.pending.push(CommandEnvelope { sequence, command });
        self.next = next;
        Ok(sequence)
    }
    pub fn results(&self) -> &[CommandResult] {
        &self.results
    }
    pub fn apply(&mut self, terrain: &mut Terrain, vehicles: &mut Vehicles) {
        self.results.clear();
        for envelope in self.pending.drain(..) {
            let result = match envelope.command {
                SimulationCommand::SetTerrainVertexHeight { x, y, height } => terrain
                    .set_height(x, y, height)
                    .map_err(|_| CommandError::InvalidVertex { x, y }),
                SimulationCommand::SetVehicleVelocity { vehicle_id, vx, vy } => {
                    vehicles.set_velocity(vehicle_id, vx, vy)
                }
            };
            self.results.push(CommandResult {
                sequence: envelope.sequence,
                result,
            });
        }
    }
    pub fn checksum(&self, hash: &mut StableChecksum) {
        hash.u64(self.next);
        hash.u64(self.pending.len() as u64);
        for envelope in &self.pending {
            hash.u64(envelope.sequence.0);
            match envelope.command {
                SimulationCommand::SetTerrainVertexHeight { x, y, height } => {
                    hash.bytes(&[0]);
                    hash.u64(u64::from(x));
                    hash.u64(u64::from(y));
                    hash.bytes(&height.to_le_bytes());
                }
                SimulationCommand::SetVehicleVelocity { vehicle_id, vx, vy } => {
                    hash.bytes(&[1]);
                    hash.u64(vehicle_id.0);
                    hash.bytes(&vx.to_le_bytes());
                    hash.bytes(&vy.to_le_bytes());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sequence_exhaustion_preserves_queue() {
        let mut queue = CommandQueue {
            next: u64::MAX - 1,
            ..CommandQueue::default()
        };
        let command = SimulationCommand::SetTerrainVertexHeight {
            x: 0,
            y: 0,
            height: 1,
        };
        assert_eq!(queue.submit(command).unwrap().value(), u64::MAX - 1);
        assert_eq!(queue.submit(command), Err(CommandError::SequenceOverflow));
        assert_eq!(queue.pending.len(), 1);
        assert_eq!(queue.next, u64::MAX);
    }
}
