use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use blake3::Hasher;
use serde::{Deserialize, Serialize};

use crate::state::StateDiff;
use crate::tuple::TupleDomain;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct ShardId(pub u32);

impl ShardId {
    pub fn to_bytes(self) -> [u8; 4] {
        self.0.to_be_bytes()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShardPolicy {
    pub max_tuples: usize,
}

impl ShardPolicy {
    pub const fn new(max_tuples: usize) -> Self {
        Self { max_tuples }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchiveReceipt {
    pub shard_id: ShardId,
    pub tuple_ids: Vec<String>,
    pub merkle_root: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShardAssignment {
    pub shard_id: ShardId,
    pub tuple_id: String,
    pub global_anchor: [u8; 32],
    pub archive_receipt: Option<ArchiveReceipt>,
}

#[derive(Debug)]
pub enum ShardError {
    MissingTupleMetadata(String),
    UnknownShard(ShardId),
}

#[derive(Clone, Debug)]
pub struct TupleShard {
    pub id: ShardId,
    pub domain: TupleDomain,
    pub tuples: Vec<StateDiff>,
    pub archive_history: Vec<ArchiveReceipt>,
    pub archived: bool,
}

impl TupleShard {
    fn new(id: ShardId, domain: TupleDomain) -> Self {
        Self {
            id,
            domain,
            tuples: Vec::new(),
            archive_history: Vec::new(),
            archived: false,
        }
    }
}

pub struct ShardManager {
    policy: ShardPolicy,
    next_id: u32,
    active_by_domain: BTreeMap<String, ShardId>,
    shards: BTreeMap<ShardId, TupleShard>,
}

impl ShardManager {
    pub fn new(policy: ShardPolicy) -> Self {
        Self {
            policy,
            next_id: 0,
            active_by_domain: BTreeMap::new(),
            shards: BTreeMap::new(),
        }
    }

    pub fn assign(&mut self, diff: StateDiff) -> Result<ShardAssignment, ShardError> {
        let tuple = diff
            .tuple
            .as_ref()
            .ok_or_else(|| ShardError::MissingTupleMetadata(diff.id.clone()))?;
        let domain_label = tuple.domain.label().to_string();
        let shard_id = match self.active_by_domain.get(&domain_label) {
            Some(id) => *id,
            None => self.spawn_shard(tuple.domain.clone(), &domain_label),
        };
        let shard = self
            .shards
            .get_mut(&shard_id)
            .ok_or(ShardError::UnknownShard(shard_id))?;
        shard.tuples.push(diff.clone());
        let mut archive_receipt = None;
        if shard.tuples.len() >= self.policy.max_tuples {
            archive_receipt = Some(self.archive_shard(shard_id)?);
            self.spawn_shard(tuple.domain.clone(), &domain_label);
        }
        let global_anchor = coordinator_anchor(shard_id, &diff.id);
        Ok(ShardAssignment {
            shard_id,
            tuple_id: diff.id.clone(),
            global_anchor,
            archive_receipt,
        })
    }

    pub fn shard(&self, shard_id: ShardId) -> Option<&TupleShard> {
        self.shards.get(&shard_id)
    }

    fn spawn_shard(&mut self, domain: TupleDomain, label: &str) -> ShardId {
        let shard_id = ShardId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        let shard = TupleShard::new(shard_id, domain);
        self.active_by_domain.insert(label.to_string(), shard_id);
        self.shards.insert(shard_id, shard);
        shard_id
    }

    fn archive_shard(&mut self, shard_id: ShardId) -> Result<ArchiveReceipt, ShardError> {
        let shard = self
            .shards
            .get_mut(&shard_id)
            .ok_or(ShardError::UnknownShard(shard_id))?;
        let tuple_ids: Vec<String> = shard.tuples.iter().map(|diff| diff.id.clone()).collect();
        let merkle_root = merkle_root(&tuple_ids);
        let receipt = ArchiveReceipt {
            shard_id,
            tuple_ids,
            merkle_root,
        };
        shard.archive_history.push(receipt.clone());
        shard.tuples.clear();
        shard.archived = true;
        Ok(receipt)
    }
}

fn merkle_root(ids: &[String]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    for id in ids {
        hasher.update(id.as_bytes());
    }
    hasher.finalize().into()
}

fn coordinator_anchor(shard_id: ShardId, tuple_id: &str) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(&shard_id.to_bytes());
    hasher.update(tuple_id.as_bytes());
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icosuple::IcosupleLayer;
    use crate::state::{StateDiff, StateOp};
    use crate::tuple::{PayloadProfile, QIPTag, TupleDomain, TupleEnvelope, TupleValidation};

    fn sample_diff(id: &str, domain: TupleDomain) -> StateDiff {
        let envelope = TupleEnvelope::new(
            domain,
            IcosupleLayer::CONSENSUS_TIER_9,
            PayloadProfile::AssetTransfer,
            "did:a",
            "did:b",
            1,
            id.as_bytes(),
            [1; 32],
            100,
            QIPTag::Native,
            None,
            TupleValidation::new("Dilithium5", vec![0; 0], vec![1; 0]),
        );
        StateDiff::with_tuple(
            id,
            "node",
            vec!["genesis".into()],
            1,
            vec![StateOp::upsert("k", "v")],
            envelope,
        )
    }

    #[test]
    fn archives_after_threshold() {
        let mut manager = ShardManager::new(ShardPolicy::new(2));
        let diff_a = sample_diff("tuple-a", TupleDomain::Finance);
        let diff_b = sample_diff("tuple-b", TupleDomain::Finance);
        let diff_c = sample_diff("tuple-c", TupleDomain::Finance);
        let assign_a = manager.assign(diff_a).unwrap();
        assert!(assign_a.archive_receipt.is_none());
        let assign_b = manager.assign(diff_b).unwrap();
        assert!(assign_b.archive_receipt.is_some());
        let assign_c = manager.assign(diff_c).unwrap();
        assert!(manager.shard(assign_c.shard_id).is_some());
    }
}
