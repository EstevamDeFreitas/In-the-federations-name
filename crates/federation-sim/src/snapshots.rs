use crate::{Simulation, SimulationError, VehicleId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VehicleSnapshot {
    pub id: VehicleId,
    pub x: i64,
    pub y: i64,
    pub vx: i64,
    pub vy: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerrainChunkSnapshot {
    pub x: u32,
    pub y: u32,
    pub revision: u64,
    pub origin_x: u32,
    pub origin_y: u32,
    pub vertices_x: u32,
    pub vertices_y: u32,
    /// Row-major heights, including shared border vertices and partial chunks.
    pub heights: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRevisionQuery {
    pub x: u32,
    pub y: u32,
    /// None requests an initial copy. Any unequal revision requests an update.
    pub known_revision: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum VehicleSelection<'a> {
    #[default]
    None,
    All,
    Ids(&'a [VehicleId]),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SnapshotRequest<'a> {
    pub vehicles: VehicleSelection<'a>,
    pub terrain_chunks: &'a [ChunkRevisionQuery],
}

/// Owned copy of a tick. Editing this value cannot change authoritative state.
/// Snapshot representation is independent of simulation storage representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationSnapshot {
    pub tick: u64,
    pub vehicles: Vec<VehicleSnapshot>,
    pub terrain_chunks: Vec<TerrainChunkSnapshot>,
}

impl Simulation {
    pub fn snapshot(
        &self,
        request: SnapshotRequest<'_>,
    ) -> Result<SimulationSnapshot, SimulationError> {
        let mut vehicles = Vec::new();
        match request.vehicles {
            VehicleSelection::None => {}
            VehicleSelection::All => {
                vehicles.try_reserve_exact(self.vehicles().len())?;
                for &id in self.vehicles().ids() {
                    vehicles.push(self.vehicles().get(id).expect("stored identity exists"));
                }
            }
            VehicleSelection::Ids(ids) => {
                vehicles.try_reserve_exact(ids.len())?;
                for &id in ids {
                    vehicles.push(
                        self.vehicles()
                            .get(id)
                            .ok_or(SimulationError::InvalidVehicle(id))?,
                    );
                }
            }
        }
        let terrain = self.terrain();
        let mut terrain_chunks = Vec::new();
        for query in request.terrain_chunks {
            let chunk = &terrain.chunks()[terrain.chunk_index(query.x, query.y)?];
            if query.known_revision == Some(chunk.revision) {
                continue;
            }
            let origin_x = query.x * terrain.chunk_size();
            let origin_y = query.y * terrain.chunk_size();
            let vertices_x = terrain.chunk_size().min(terrain.width() - origin_x) + 1;
            let vertices_y = terrain.chunk_size().min(terrain.height() - origin_y) + 1;
            let mut heights = Vec::new();
            heights.try_reserve_exact(vertices_x as usize * vertices_y as usize)?;
            for y in origin_y..origin_y + vertices_y {
                let start = terrain.vertex_index(origin_x, y)?;
                heights.extend_from_slice(&terrain.heights()[start..start + vertices_x as usize]);
            }
            terrain_chunks.try_reserve(1)?;
            terrain_chunks.push(TerrainChunkSnapshot {
                x: query.x,
                y: query.y,
                revision: chunk.revision,
                origin_x,
                origin_y,
                vertices_x,
                vertices_y,
                heights,
            });
        }
        Ok(SimulationSnapshot {
            tick: self.tick_count(),
            vehicles,
            terrain_chunks,
        })
    }
}
