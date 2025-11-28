use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// Minimal trait that abstracts over QRNG beacons or entropy pallets.
pub trait QrngCore {
    type Error;

    fn next_u64(&mut self) -> Result<u64, Self::Error>;
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct PoolId {
    pub tier: u8,
    pub slot: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PoolTier {
    pub tier: u8,
    pub pool_capacity: usize,
}

impl PoolTier {
    pub const fn new(tier: u8, pool_capacity: usize) -> Self {
        Self {
            tier,
            pool_capacity,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PoolCoordinator<ValidatorId> {
    pub pool_id: PoolId,
    pub validator: ValidatorId,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum VerificationVerdict {
    Approve,
    Reject,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VerificationOutcome<ValidatorId> {
    pub tuple_id: String,
    pub approvals: Vec<ValidatorId>,
    pub rejections: Vec<ValidatorId>,
    pub verdict: VerificationVerdict,
}

#[derive(Clone, Debug)]
struct VerificationPool<ValidatorId> {
    id: PoolId,
    members: Vec<ValidatorId>,
    coordinator: Option<ValidatorId>,
    capacity: usize,
}

impl<ValidatorId: Clone> VerificationPool<ValidatorId> {
    fn new(id: PoolId, capacity: usize) -> Self {
        Self {
            id,
            members: Vec::new(),
            coordinator: None,
            capacity,
        }
    }

    fn has_capacity(&self) -> bool {
        self.members.len() < self.capacity
    }

    fn add_member(&mut self, validator: ValidatorId) {
        self.members.push(validator);
    }

    fn elect_coordinator<R: QrngCore>(
        &mut self,
        qrng: &mut R,
    ) -> Result<Option<PoolCoordinator<ValidatorId>>, R::Error> {
        if self.members.is_empty() {
            self.coordinator = None;
            return Ok(None);
        }
        let idx = (qrng.next_u64()? as usize) % self.members.len();
        let validator = self.members[idx].clone();
        self.coordinator = Some(validator.clone());
        Ok(Some(PoolCoordinator {
            pool_id: self.id,
            validator,
        }))
    }
}

#[derive(Clone, Debug)]
struct TupleVote<ValidatorId> {
    approvals: BTreeSet<ValidatorId>,
    rejections: BTreeSet<ValidatorId>,
}

impl<ValidatorId: Ord> TupleVote<ValidatorId> {
    fn new() -> Self {
        Self {
            approvals: BTreeSet::new(),
            rejections: BTreeSet::new(),
        }
    }

    fn record_vote(&mut self, validator: ValidatorId, verdict: VerificationVerdict) {
        match verdict {
            VerificationVerdict::Approve => {
                self.rejections.remove(&validator);
                self.approvals.insert(validator);
            }
            VerificationVerdict::Reject => {
                self.approvals.remove(&validator);
                self.rejections.insert(validator);
            }
        }
    }

    fn approvals(&self) -> usize {
        self.approvals.len()
    }

    fn rejections(&self) -> usize {
        self.rejections.len()
    }

    fn into_outcome(
        self,
        tuple_id: String,
        verdict: VerificationVerdict,
    ) -> VerificationOutcome<ValidatorId>
    where
        ValidatorId: Clone,
    {
        VerificationOutcome {
            tuple_id,
            approvals: self.approvals.into_iter().collect(),
            rejections: self.rejections.into_iter().collect(),
            verdict,
        }
    }
}

#[derive(Clone, Debug)]
struct TierState<ValidatorId> {
    config: PoolTier,
    pools: Vec<VerificationPool<ValidatorId>>,
    next_slot: u16,
}

impl<ValidatorId: Clone> TierState<ValidatorId> {
    fn new(config: PoolTier) -> Self {
        Self {
            config,
            pools: Vec::new(),
            next_slot: 0,
        }
    }

    fn assign_member(&mut self, validator: ValidatorId) -> PoolId {
        if let Some(pool) = self.pools.iter_mut().find(|pool| pool.has_capacity()) {
            pool.add_member(validator);
            return pool.id;
        }
        let pool_id = PoolId {
            tier: self.config.tier,
            slot: self.next_slot,
        };
        self.next_slot = self.next_slot.wrapping_add(1);
        let mut pool = VerificationPool::new(pool_id, self.config.pool_capacity);
        pool.add_member(validator);
        self.pools.push(pool);
        pool_id
    }

    fn coordinators<R: QrngCore>(
        &mut self,
        qrng: &mut R,
    ) -> Result<Vec<PoolCoordinator<ValidatorId>>, R::Error> {
        let mut coordinators = Vec::new();
        for pool in self.pools.iter_mut() {
            if let Some(coordinator) = pool.elect_coordinator(qrng)? {
                coordinators.push(coordinator);
            }
        }
        Ok(coordinators)
    }
}

/// HierarchicalVerificationPools keeps validators organized across tiers and
/// supports QRNG-elected coordinators per pool. Votes roll up to a global
/// supermajority threshold (default 2/3).
pub struct HierarchicalVerificationPools<ValidatorId, R: QrngCore> {
    tiers: BTreeMap<u8, TierState<ValidatorId>>,
    validator_map: BTreeMap<ValidatorId, PoolId>,
    qrng: R,
    quorum_ratio: (u16, u16),
    votes: BTreeMap<String, TupleVote<ValidatorId>>,
}

impl<ValidatorId, R> HierarchicalVerificationPools<ValidatorId, R>
where
    ValidatorId: Ord + Clone,
    R: QrngCore,
{
    pub fn new(tier_configs: Vec<PoolTier>, qrng: R) -> Self {
        let tiers = tier_configs
            .into_iter()
            .map(|config| (config.tier, TierState::new(config)))
            .collect();
        Self {
            tiers,
            validator_map: BTreeMap::new(),
            qrng,
            quorum_ratio: (2, 3),
            votes: BTreeMap::new(),
        }
    }

    pub fn with_quorum_ratio(mut self, numerator: u16, denominator: u16) -> Self {
        assert!(denominator > 0, "denominator must be > 0");
        self.quorum_ratio = (numerator, denominator);
        self
    }

    pub fn register_validator(&mut self, tier: u8, validator: ValidatorId) -> Option<PoolId> {
        let tier_state = self.tiers.get_mut(&tier)?;
        let pool_id = tier_state.assign_member(validator.clone());
        self.validator_map.insert(validator, pool_id);
        Some(pool_id)
    }

    pub fn elect_coordinators(&mut self) -> Result<Vec<PoolCoordinator<ValidatorId>>, R::Error> {
        let mut coordinators = Vec::new();
        for state in self.tiers.values_mut() {
            coordinators.extend(state.coordinators(&mut self.qrng)?);
        }
        Ok(coordinators)
    }

    pub fn submit_vote(
        &mut self,
        tuple_id: impl Into<String>,
        validator: &ValidatorId,
        verdict: VerificationVerdict,
    ) -> Option<VerificationOutcome<ValidatorId>> {
        if !self.validator_map.contains_key(validator) {
            return None;
        }
        let tuple_id = tuple_id.into();
        let entry = self
            .votes
            .entry(tuple_id.clone())
            .or_insert_with(TupleVote::new);
        entry.record_vote(validator.clone(), verdict);
        let approvals = entry.approvals();
        let rejections = entry.rejections();
        let threshold = self.threshold();
        if approvals >= threshold {
            let vote = self.votes.remove(&tuple_id)?;
            return Some(vote.into_outcome(tuple_id, VerificationVerdict::Approve));
        }
        if rejections >= threshold {
            let vote = self.votes.remove(&tuple_id)?;
            return Some(vote.into_outcome(tuple_id, VerificationVerdict::Reject));
        }
        None
    }

    fn threshold(&self) -> usize {
        let (num, den) = self.quorum_ratio;
        let total = self.validator_map.len();
        if total == 0 {
            return usize::MAX;
        }
        ((total * num as usize) + (den as usize - 1)) / den as usize
    }

    pub fn total_validators(&self) -> usize {
        self.validator_map.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IncrementingQrng(u64);

    impl QrngCore for IncrementingQrng {
        type Error = core::convert::Infallible;

        fn next_u64(&mut self) -> Result<u64, Self::Error> {
            let value = self.0;
            self.0 = self.0.wrapping_add(1);
            Ok(value)
        }
    }

    #[test]
    fn elects_coordinators_per_pool() {
        let mut hvp = HierarchicalVerificationPools::new(
            vec![PoolTier::new(8, 2), PoolTier::new(9, 2)],
            IncrementingQrng(0),
        );
        hvp.register_validator(8, "v1".to_string());
        hvp.register_validator(8, "v2".to_string());
        hvp.register_validator(9, "v3".to_string());
        let coordinators = hvp.elect_coordinators().unwrap();
        assert_eq!(coordinators.len(), 2);
        assert!(coordinators.iter().any(|c| c.validator == "v1"));
    }

    #[test]
    fn reaches_quorum() {
        let mut hvp =
            HierarchicalVerificationPools::new(vec![PoolTier::new(8, 10)], IncrementingQrng(0));
        for idx in 0..9 {
            hvp.register_validator(8, format!("v{idx}"));
        }
        assert_eq!(hvp.total_validators(), 9);
        // threshold with 2/3 => 6
        for idx in 0..6 {
            let outcome =
                hvp.submit_vote("tuple-1", &format!("v{idx}"), VerificationVerdict::Approve);
            if idx < 5 {
                assert!(outcome.is_none());
            } else {
                let out = outcome.expect("quorum reached");
                assert_eq!(out.verdict, VerificationVerdict::Approve);
                assert_eq!(out.tuple_id, "tuple-1");
                break;
            }
        }
    }
}
