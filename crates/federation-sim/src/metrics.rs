/// Diagnostics are computed on request, outside the normal tick path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationMetrics {
    pub tick_count: u64,
    pub logical_time_ms: u128,
    pub vehicles: usize,
    pub dirty_chunks: usize,
    pub checksum: u64,
}

/// Versioned checksum v1: FNV-1a 64-bit over explicit little-endian fields.
/// This is a reproducibility fingerprint, not a cryptographic hash.
pub(crate) struct StableChecksum(u64);
impl StableChecksum {
    pub fn new() -> Self {
        Self(0xcbf29ce484222325)
    }
    pub fn bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 ^= u64::from(byte);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }
    pub fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    pub fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fnv1a_known_vector() {
        let mut hash = StableChecksum::new();
        hash.bytes(b"hello");
        assert_eq!(hash.finish(), 0xa430d84680aabd0b);
    }
}
