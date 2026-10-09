use crate::{
    CommandError, Route, TransportEdgeId, TransportMode, TransportNodeId, metrics::StableChecksum,
    transport::network::TransportNetwork,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RoutePlanId(pub(crate) u64);
impl RoutePlanId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }
}
/// Owned public snapshot; the stored record is immutable and centrally shared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutePlan {
    pub id: RoutePlanId,
    pub origin: TransportNodeId,
    pub destination: TransportNodeId,
    pub mode: TransportMode,
    pub network_revision_when_created: u64,
    pub total_routing_cost: u128,
    pub edges: Vec<TransportEdgeId>,
}
#[derive(Debug, Clone, Copy, Default)]
pub enum RoutePlanSelection<'a> {
    #[default]
    None,
    Ids(&'a [RoutePlanId]),
}

#[derive(Default)]
pub(crate) struct RoutePlans {
    next: u64,
    records: Vec<Option<RoutePlan>>,
}
impl RoutePlans {
    pub fn reserve(&mut self) -> Result<RoutePlanId, CommandError> {
        let next = self
            .next
            .checked_add(1)
            .ok_or(CommandError::RoutePlanIdOverflow)?;
        if self.records.len() >= 1_000_000 {
            return Err(CommandError::RoutePlanSafetyGuard);
        }
        self.records
            .try_reserve(1)
            .map_err(|_| CommandError::Allocation)?;
        let id = RoutePlanId(self.next);
        self.records.push(None);
        self.next = next;
        Ok(id)
    }
    pub fn get(&self, id: RoutePlanId) -> Option<&RoutePlan> {
        self.records
            .get(usize::try_from(id.0).ok()?)?
            .as_ref()
            .filter(|p| p.id == id)
    }
    pub fn store(
        &mut self,
        id: RoutePlanId,
        route: Route,
        network: &TransportNetwork,
    ) -> Result<(), CommandError> {
        let slot = usize::try_from(id.0)
            .ok()
            .filter(|&i| i < self.records.len())
            .ok_or(CommandError::InvalidRoutePlan(id))?;
        if self.records[slot].is_some() {
            return Err(CommandError::RoutePlanIdUsed(id));
        }
        if route.edges.len() > 1_000_000 {
            return Err(CommandError::RoutePlanSafetyGuard);
        }
        network.live_node(route.origin)?;
        network.live_node(route.destination)?;
        let mut current = route.origin;
        let mut cost = 0_u128;
        for &edge_id in &route.edges {
            let (alive, edge) = network
                .edge_data(edge_id)
                .ok_or(CommandError::InvalidTransportEdge(edge_id))?;
            if !alive
                || !edge.enabled
                || edge.mode != route.mode
                || edge.from != current
                || edge.physical_length == 0
            {
                return Err(CommandError::InvalidRoute);
            }
            current = edge.to;
            cost += u128::from(edge.base_cost);
        }
        if current != route.destination || cost != route.total_cost {
            return Err(CommandError::InvalidRoute);
        }
        self.records[slot] = Some(RoutePlan {
            id,
            origin: route.origin,
            destination: route.destination,
            mode: route.mode,
            network_revision_when_created: route.network_revision,
            total_routing_cost: route.total_cost,
            edges: route.edges,
        });
        Ok(())
    }
    pub fn checksum(&self, hash: &mut StableChecksum) {
        hash.u64(self.next);
        hash.u64(self.records.len() as u64);
        for record in &self.records {
            hash.bytes(&[u8::from(record.is_some())]);
            if let Some(plan) = record {
                hash.u64(plan.id.0);
                hash.u64(plan.origin.0);
                hash.u64(plan.destination.0);
                hash.bytes(&[plan.mode.tag()]);
                hash.u64(plan.network_revision_when_created);
                hash.bytes(&plan.total_routing_cost.to_le_bytes());
                hash.u64(plan.edges.len() as u64);
                for edge in &plan.edges {
                    hash.u64(edge.0);
                }
            }
        }
    }
    pub fn memory_bytes(&self) -> (u64, u64) {
        (
            (self.records.capacity() * std::mem::size_of::<Option<RoutePlan>>()) as u64,
            self.records
                .iter()
                .flatten()
                .map(|p| (p.edges.capacity() * std::mem::size_of::<TransportEdgeId>()) as u64)
                .sum(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan_id_overflow_preserves_storage() {
        let mut plans = RoutePlans {
            next: u64::MAX,
            ..Default::default()
        };
        assert_eq!(plans.reserve(), Err(CommandError::RoutePlanIdOverflow));
        assert!(plans.records.is_empty());
    }
}
