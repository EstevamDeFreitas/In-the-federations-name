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
    #[cfg(test)]
    pub fn chunk_dimensions(&self) -> (u32, u32) {
        (self.chunks_x, self.chunks_y)
    }

    #[cfg(test)]
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

    #[cfg(test)]
    pub fn height_at(&self, x: u32, y: u32) -> Result<i32, SimulationError> {
        Ok(self.heights[self.vertex_index(x, y)?])
    }

    #[cfg(test)]
    pub(crate) fn clear_dirty(&mut self) {
        for chunk in &mut self.chunks {
            chunk.dirty = false;
        }
    }

    pub(crate) fn set_height(
        &mut self,
        x: u32,
        y: u32,
        height: i32,
    ) -> Result<(), SimulationError> {
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

#[cfg(test)]
mod tests {
    #[test]
    fn rectangular_map_indexing_and_bounds() {
        let t = Terrain::new(65, 35, 32).unwrap();
        assert_eq!(t.chunk_dimensions(), (3, 2));
        assert_eq!(t.tile_index(64, 34).unwrap(), 65 * 35 - 1);
        assert_eq!(t.vertex_index(65, 35).unwrap(), 66 * 36 - 1);
        assert_eq!(t.chunk_index(2, 1).unwrap(), 5);
        assert!(t.tile_index(65, 0).is_err());
        assert!(t.tile_index(0, 35).is_err());
        assert!(t.vertex_index(66, 0).is_err());
        assert!(t.vertex_index(0, 36).is_err());
        assert!(t.chunk_index(3, 0).is_err());
        assert!(t.chunk_index(0, 2).is_err());
    }

    use super::*;
    #[test]
    fn shared_vertices_invalidate_exactly_adjacent_chunks() {
        let cases = [
            ((10, 10), vec![0]),
            ((32, 10), vec![0, 1]),
            ((10, 32), vec![0, 3]),
            ((32, 32), vec![0, 1, 3, 4]),
            ((64, 32), vec![1, 2, 4, 5]),
            ((0, 0), vec![0]),
            ((65, 35), vec![5]),
            ((0, 32), vec![0, 3]),
            ((65, 32), vec![2, 5]),
        ];
        for ((x, y), expected) in cases {
            let mut t = Terrain::new(65, 35, 32).unwrap();
            t.clear_dirty();
            t.set_height(x, y, -7).unwrap();
            assert_eq!(t.height_at(x, y).unwrap(), -7);
            let dirty: Vec<_> = t
                .chunks()
                .iter()
                .enumerate()
                .filter(|(_, c)| c.dirty)
                .map(|(i, _)| i)
                .collect();
            assert_eq!(dirty, expected, "vertex ({x},{y})");
            for c in t.chunks() {
                assert_eq!(c.revision, u64::from(c.dirty));
            }
            t.set_height(x, y, -7).unwrap();
            assert!(t.chunks().iter().all(|c| c.revision <= 1));
            t.set_height(x, y, 9).unwrap();
            for i in expected {
                assert_eq!(t.chunks()[i].revision, 2);
            }
        }
    }

    #[test]
    fn aligned_map_outer_corners_and_invalid_edit() {
        let mut t = Terrain::new(64, 32, 32).unwrap();
        t.clear_dirty();
        t.set_height(64, 32, 1).unwrap();
        assert!(!t.chunks()[0].dirty);
        assert!(t.chunks()[1].dirty);
        let heights = t.heights().to_vec();
        let chunks = t.chunks().to_vec();
        assert!(t.set_height(65, 32, 4).is_err());
        assert_eq!(t.heights(), heights);
        assert_eq!(t.chunks(), chunks);
        let mut tiny = Terrain::new(1, 1, 32).unwrap();
        for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            tiny.set_height(x, y, 2).unwrap();
        }
        assert_eq!(tiny.chunks()[0].revision, 4);
    }
}
