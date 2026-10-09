/// Public identity, independent of storage slots. No removal or reuse yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VehicleId(pub(crate) u64);

impl VehicleId {
    /// Reconstruct an identity token; existence is checked when used.
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}
