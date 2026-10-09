use crate::{SimulationError, chunk::ChunkMetadata, config::validate_geometry, filled_vec};

#[derive(Debug)]
pub struct Terrain {
    width: u32,
    height: u32,
    chunk_size: u32,
    chunks_x: u32,
    chunks_y: u32,
    heights: Vec<i32>,
    chunks: Vec<ChunkMetadata>,
}

impl Terrain {
    pub fn new(width: u32, height: u32, chunk_size: u32) -> Result<Self, SimulationError> {
        validate_geometry(width, height, chunk_size)?;
        let chunks_x = width.div_ceil(chunk_size);
        let chunks_y = height.div_ceil(chunk_size);
        let heights = filled_vec((width as usize + 1) * (height as usize + 1), 0)?;
        let mut chunks = Vec::new();
        chunks.try_reserve_exact(chunks_x as usize * chunks_y as usize)?;
        for y in 0..chunks_y {
            for x in 0..chunks_x {
                chunks.push(ChunkMetadata {
                    x,
                    y,
                    dirty: true,
                    revision: 0,
                });
            }
        }
        Ok(Self {
            width,
            height,
            chunk_size,
            chunks_x,
            chunks_y,
            heights,
            chunks,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn chunk_size(&self) -> u32 {
        self.chunk_size
    }
    pub fn chunks(&self) -> &[ChunkMetadata] {
        &self.chunks
    }
    pub fn heights(&self) -> &[i32] {
        &self.heights
    }
    pub fn chunk_dimensions(&self) -> (u32, u32) {
        (self.chunks_x, self.chunks_y)
    }

    pub fn tile_index(&self, x: u32, y: u32) -> Result<usize, SimulationError> {
        if x >= self.width || y >= self.height {
            return Err(SimulationError::OutOfBounds { kind: "tile", x, y });
        }
        Ok(y as usize * self.width as usize + x as usize)
    }

    pub fn vertex_index(&self, x: u32, y: u32) -> Result<usize, SimulationError> {
        if x > self.width || y > self.height {
            return Err(SimulationError::OutOfBounds {
                kind: "vertex",
                x,
                y,
            });
        }
        Ok(y as usize * (self.width as usize + 1) + x as usize)
    }

    pub fn chunk_index(&self, x: u32, y: u32) -> Result<usize, SimulationError> {
        if x >= self.chunks_x || y >= self.chunks_y {
            return Err(SimulationError::OutOfBounds {
                kind: "chunk",
                x,
                y,
            });
        }
        Ok(y as usize * self.chunks_x as usize + x as usize)
    }

    pub fn height_at(&self, x: u32, y: u32) -> Result<i32, SimulationError> {
        Ok(self.heights[self.vertex_index(x, y)?])
    }

    pub fn clear_dirty(&mut self) {
        for chunk in &mut self.chunks {
            chunk.dirty = false;
        }
    }

    pub fn set_height(&mut self, x: u32, y: u32, height: i32) -> Result<(), SimulationError> {
        let index = self.vertex_index(x, y)?;
        if self.heights[index] == height {
            return Ok(());
        }
        self.heights[index] = height;
        // A vertex touches the tiles immediately before and after it on each axis.
        // Mapping the inclusive tile rectangle to chunks handles outer edges too.
        let min_x = x.saturating_sub(1) / self.chunk_size;
        let max_x = x.min(self.width - 1) / self.chunk_size;
        let min_y = y.saturating_sub(1) / self.chunk_size;
        let max_y = y.min(self.height - 1) / self.chunk_size;
        for cy in min_y..=max_y {
            for cx in min_x..=max_x {
                self.chunks[cy as usize * self.chunks_x as usize + cx as usize].invalidate();
            }
        }
        Ok(())
    }
}
