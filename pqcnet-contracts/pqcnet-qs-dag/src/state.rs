use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

const MAX_PARENT_REFERENCES: usize = 10;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateOp {
    pub key: String,
    pub value: Option<String>,
}

impl StateOp {
    pub fn upsert(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: Some(value.into()),
        }
    }

    pub fn delete(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct StateDiff {
    pub id: String,
    pub author: String,
    pub parents: Vec<String>,
    pub lamport: u64,
    pub ops: Vec<StateOp>,
}

impl StateDiff {
    pub fn genesis(id: impl Into<String>, author: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            author: author.into(),
            parents: Vec::new(),
            lamport: 0,
            ops: Vec::new(),
        }
    }

    pub fn new(
        id: impl Into<String>,
        author: impl Into<String>,
        parents: Vec<String>,
        lamport: u64,
        ops: Vec<StateOp>,
    ) -> Self {
        Self {
            id: id.into(),
            author: author.into(),
            parents,
            lamport,
            ops,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSnapshot {
    pub head_id: String,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DagError {
    Duplicate(String),
    UnknownParent(String),
    InvalidGenesis,
    TooManyParents { diff_id: String, count: usize },
}

impl fmt::Display for DagError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DagError::Duplicate(id) => write!(f, "diff {id} already exists"),
            DagError::UnknownParent(parent) => write!(f, "unknown parent {parent}"),
            DagError::InvalidGenesis => write!(f, "genesis diff must not have parents"),
            DagError::TooManyParents { diff_id, count } => write!(
                f,
                "diff {diff_id} references {count} parents which exceeds the {MAX_PARENT_REFERENCES} limit"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TemporalWeight {
    /// Scaling factor applied to the fan-out (number of parents).
    pub alpha: u64,
}

impl TemporalWeight {
    pub const fn new(alpha: u64) -> Self {
        Self { alpha }
    }

    pub fn weight(&self, diff: &StateDiff) -> u64 {
        let timestamp = diff.lamport;
        let fan_out = diff.parents.len() as u64;
        timestamp
            .saturating_add(self.alpha.saturating_mul(fan_out))
            .max(1)
    }
}

impl Default for TemporalWeight {
    fn default() -> Self {
        // Tuned for control-plane convergence; governance can override as needed.
        Self { alpha: 8 }
    }
}

#[derive(Clone, Debug)]
struct DiffNode {
    diff: StateDiff,
    height: u64,
    score: u64,
}

#[derive(Clone, Debug)]
pub struct QsDag {
    nodes: BTreeMap<String, DiffNode>,
    temporal_weight: TemporalWeight,
}

impl QsDag {
    pub fn new(genesis: StateDiff) -> Result<Self, DagError> {
        Self::with_temporal_weight(genesis, TemporalWeight::default())
    }

    pub fn with_temporal_weight(
        genesis: StateDiff,
        temporal_weight: TemporalWeight,
    ) -> Result<Self, DagError> {
        if !genesis.parents.is_empty() {
            return Err(DagError::InvalidGenesis);
        }
        let diff_id = genesis.id.clone();
        let node = DiffNode {
            diff: genesis,
            height: 0,
            score: 1,
        };
        let mut nodes = BTreeMap::new();
        nodes.insert(diff_id, node);
        Ok(Self {
            nodes,
            temporal_weight,
        })
    }

    pub fn contains(&self, diff_id: &str) -> bool {
        self.nodes.contains_key(diff_id)
    }

    pub fn missing_parents(&self, diff: &StateDiff) -> Vec<String> {
        diff.parents
            .iter()
            .filter(|parent| !self.nodes.contains_key(*parent))
            .cloned()
            .collect()
    }

    pub fn insert(&mut self, diff: StateDiff) -> Result<bool, DagError> {
        if self.nodes.contains_key(&diff.id) {
            return Err(DagError::Duplicate(diff.id));
        }
        if diff.parents.is_empty() {
            return Err(DagError::InvalidGenesis);
        }
        if diff.parents.len() > MAX_PARENT_REFERENCES {
            return Err(DagError::TooManyParents {
                diff_id: diff.id.clone(),
                count: diff.parents.len(),
            });
        }
        let mut max_parent_height = 0u64;
        let mut max_parent_score = 0u64;
        for parent in &diff.parents {
            let parent_node = self
                .nodes
                .get(parent)
                .ok_or_else(|| DagError::UnknownParent(parent.clone()))?;
            max_parent_height = max_parent_height.max(parent_node.height);
            max_parent_score = max_parent_score.max(parent_node.score);
        }
        let node = DiffNode {
            diff: diff.clone(),
            height: max_parent_height + 1,
            score: max_parent_score.saturating_add(self.temporal_weight.weight(&diff)),
        };
        self.nodes.insert(diff.id.clone(), node);
        Ok(true)
    }

    pub fn canonical_head(&self) -> Option<&StateDiff> {
        self.nodes
            .values()
            .max_by(|a, b| {
                a.score
                    .cmp(&b.score)
                    .then_with(|| a.height.cmp(&b.height))
                    .then_with(|| a.diff.id.cmp(&b.diff.id))
            })
            .map(|node| &node.diff)
    }

    pub fn snapshot(&self) -> Option<StateSnapshot> {
        let head = self.canonical_head()?;
        let reachable = self.reachable_from(&head.id);
        let mut ordered: Vec<&DiffNode> = reachable
            .into_iter()
            .filter_map(|id| self.nodes.get(&id))
            .collect();
        ordered.sort_by(|a, b| {
            a.height
                .cmp(&b.height)
                .then_with(|| a.diff.lamport.cmp(&b.diff.lamport))
                .then_with(|| a.diff.id.cmp(&b.diff.id))
        });
        let mut values = BTreeMap::new();
        for node in ordered {
            for op in &node.diff.ops {
                match &op.value {
                    Some(value) => {
                        values.insert(op.key.clone(), value.clone());
                    }
                    None => {
                        values.remove(&op.key);
                    }
                }
            }
        }
        Some(StateSnapshot {
            head_id: head.id.clone(),
            values,
        })
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    fn reachable_from(&self, head_id: &str) -> BTreeSet<String> {
        let mut visited = BTreeSet::new();
        let mut stack = vec![head_id.to_string()];
        while let Some(id) = stack.pop() {
            if visited.insert(id.clone()) {
                if let Some(node) = self.nodes.get(&id) {
                    for parent in &node.diff.parents {
                        stack.push(parent.clone());
                    }
                }
            }
        }
        visited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dag() -> QsDag {
        QsDag::new(StateDiff::genesis("genesis", "system")).unwrap()
    }

    #[test]
    fn rejects_duplicate_ids() {
        let mut dag = dag();
        let diff = StateDiff::new(
            "diff-1",
            "node-a",
            vec!["genesis".into()],
            1,
            vec![StateOp::upsert("foo", "bar")],
        );
        dag.insert(diff.clone()).unwrap();
        assert!(matches!(dag.insert(diff), Err(DagError::Duplicate(_))));
    }

    #[test]
    fn snapshot_reflects_latest_values() {
        let mut dag = dag();
        let diff_a = StateDiff::new(
            "diff-a",
            "node-a",
            vec!["genesis".into()],
            1,
            vec![StateOp::upsert("foo", "a")],
        );
        dag.insert(diff_a).unwrap();
        let diff_b = StateDiff::new(
            "diff-b",
            "node-b",
            vec!["genesis".into()],
            2,
            vec![StateOp::upsert("foo", "b")],
        );
        dag.insert(diff_b).unwrap();
        let snapshot = dag.snapshot().unwrap();
        assert_eq!(snapshot.values["foo"], "b");
    }

    #[test]
    fn rejects_more_than_ten_parents() {
        let mut dag = dag();
        let mut parent_ids = Vec::new();
        for idx in 0..MAX_PARENT_REFERENCES {
            let id = format!("diff-{idx}");
            let diff = StateDiff::new(
                id.clone(),
                format!("node-{idx}"),
                vec!["genesis".into()],
                idx as u64 + 1,
                vec![StateOp::upsert(format!("key-{idx}"), "value")],
            );
            dag.insert(diff).unwrap();
            parent_ids.push(id);
        }
        let overflow_diff = StateDiff::new(
            "overflow",
            "node-overflow",
            parent_ids
                .iter()
                .cloned()
                .chain(core::iter::once("genesis".into()))
                .collect(),
            42,
            vec![StateOp::upsert("extra", "value")],
        );
        let err = dag.insert(overflow_diff).unwrap_err();
        assert!(matches!(err, DagError::TooManyParents { .. }));
    }

    #[test]
    fn temporal_weight_rewards_fan_out_and_recency() {
        let mut dag = dag();
        let early = StateDiff::new(
            "early",
            "node-a",
            vec!["genesis".into()],
            5,
            vec![StateOp::upsert("foo", "early")],
        );
        dag.insert(early).unwrap();
        let later = StateDiff::new(
            "later",
            "node-b",
            vec!["genesis".into(), "early".into()],
            50,
            vec![StateOp::upsert("foo", "later")],
        );
        dag.insert(later).unwrap();

        let early_score = dag.nodes.get("early").unwrap().score;
        let later_score = dag.nodes.get("later").unwrap().score;
        assert!(later_score > early_score);
    }
}
