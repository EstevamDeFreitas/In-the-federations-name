/// Stable identity; the internal slot mapping is not a public contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransportNodeId(pub(crate) u64);
impl TransportNodeId {
    /// Identity token for serialization; never interpret it as a storage slot.
    pub const fn to_raw(self) -> u64 {
        self.0
    }
    /// Reconstruct a token. Reservation and existence are checked when used.
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}

/// Identifies one directed edge, independently of its endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TransportEdgeId(pub(crate) u64);
impl TransportEdgeId {
    /// Identity token for serialization; never interpret it as a storage slot.
    pub const fn to_raw(self) -> u64 {
        self.0
    }
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}
