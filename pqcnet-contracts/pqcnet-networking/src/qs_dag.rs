use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use thiserror::Error;

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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Error)]
pub enum DagError {
    #[error("diff {0} already exists")]
    Duplicate(String),
    #[error("unknown parent {0}")]
    UnknownParent(String),
    #[error("genesis diff must not have parents")]
    InvalidGenesis,
}

#[derive(Clone, Debug)]
struct DiffNode {
    diff: StateDiff,
    height: u64,
    score: u64,
}

#[derive(Clone, Debug)]
pub struct QsDag {
    nodes: HashMap<String, DiffNode>,
}

impl QsDag {
    pub fn new(genesis: StateDiff) -> Result<Self, DagError> {
        if !genesis.parents.is_empty() {
            return Err(DagError::InvalidGenesis);
        }
        let diff_id = genesis.id.clone();
        let node = DiffNode {
            diff: genesis,
            height: 0,
            score: 1,
        };
        let mut nodes = HashMap::new();
        nodes.insert(diff_id, node);
        Ok(Self { nodes })
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
            score: max_parent_score + diff_weight(&diff),
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

    fn reachable_from(&self, head_id: &str) -> HashSet<String> {
        let mut visited = HashSet::new();
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

fn diff_weight(diff: &StateDiff) -> u64 {
    (diff.ops.len() as u64).max(1)
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
}
