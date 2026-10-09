/// Metadata only; vertex heights are stored once in the global heightmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMetadata {
    pub x: u32,
    pub y: u32,
    pub dirty: bool,
    pub revision: u64,
}

impl ChunkMetadata {
    /// Saturates at u64::MAX; subsequent changes cannot be detected by revision alone.
    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
        self.revision = self.revision.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revision_saturates_without_wrapping() {
        let mut chunk = ChunkMetadata {
            x: 0,
            y: 0,
            dirty: false,
            revision: u64::MAX - 1,
        };
        chunk.invalidate();
        chunk.invalidate();
        assert_eq!(chunk.revision, u64::MAX);
        assert!(chunk.dirty);
    }
}
