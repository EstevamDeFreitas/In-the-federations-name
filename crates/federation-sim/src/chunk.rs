/// Metadata only; vertex heights are stored once in the global heightmap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkMetadata {
    pub x: u32,
    pub y: u32,
    pub dirty: bool,
    pub revision: u64,
}

impl ChunkMetadata {
    pub(crate) fn invalidate(&mut self) {
        self.dirty = true;
        self.revision = self.revision.saturating_add(1);
    }
}
