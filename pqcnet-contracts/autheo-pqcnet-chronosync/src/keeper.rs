use autheo_pqcnet_5dqeh::{
    HypergraphModule, Icosuple, ModuleError, ModuleStorageLayout, MsgAnchorEdge,
    MsgAnchorEdgeResponse, PqcBinding, PqcScheme, VertexId, VertexReceipt,
};
use blake3::Hasher;
use pqcnet_networking::AnchorEdgeEndpoint;
use pqcnet_qs_dag::{DagError, QsDag, StateDiff, StateOp};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::{ChronosyncConfig, DagNode, EpochReport};

/// Chronosync keeper that hydrates 5D-QEH vertices from QS-DAG elections.
pub struct ChronosyncKeeper {
    config: ChronosyncConfig,
    module: HypergraphModule,
    dag: QsDag,
    vertex_index: BTreeMap<String, VertexId>,
}

impl ChronosyncKeeper {
    /// Instantiate a keeper with the provided Chronosync + hypergraph config.
    pub fn new(config: ChronosyncConfig, module: HypergraphModule) -> Self {
        let genesis = StateDiff::genesis("chronosync/genesis", "chronosync");
        let dag = QsDag::new(genesis).expect("chronosync genesis diff must be valid");
        Self {
            config,
            module,
            dag,
            vertex_index: BTreeMap::new(),
        }
    }

    /// Access the underlying hypergraph module (mostly for diagnostics/tests).
    pub fn module(&self) -> &HypergraphModule {
        &self.module
    }

    /// Mutable access to the underlying hypergraph module.
    pub fn module_mut(&mut self) -> &mut HypergraphModule {
        &mut self.module
    }

    /// Apply the DAG witness from a Chronosync epoch to the hypergraph module.
    pub fn ingest_epoch_report(
        &mut self,
        report: &EpochReport,
    ) -> Result<ChronosyncKeeperReport, ChronosyncKeeperError> {
        let mut receipts = Vec::new();
        let mut missing_parents = Vec::new();

        for node in &report.dag_witness.nodes {
            let (parents, missing) = self.lookup_parents(&node.parents);
            missing_parents.extend(missing);
            let msg = self.build_anchor_message(node, parents, report);
            let receipt = self.module.apply_anchor_edge(msg)?;
            self.vertex_index
                .insert(node.node_id.clone(), receipt.vertex_id);
            self.record_qs_dag(node, &receipt)?;
            receipts.push(receipt);
        }

        let dag_head = self.dag.canonical_head().map(|diff| diff.id.clone());
        let storage_layout = self.module.storage_layout().clone();

        Ok(ChronosyncKeeperReport {
            epoch_index: report.epoch_index,
            applied_vertices: receipts,
            missing_parents,
            storage_layout,
            dag_head,
        })
    }

    fn lookup_parents(&self, parent_ids: &[String]) -> (Vec<VertexId>, Vec<String>) {
        let mut vertices = Vec::with_capacity(parent_ids.len());
        let mut missing = Vec::new();
        for parent in parent_ids {
            if let Some(id) = self.vertex_index.get(parent) {
                vertices.push(*id);
            } else {
                missing.push(parent.clone());
            }
        }
        (vertices, missing)
    }

    fn build_anchor_message(
        &self,
        node: &DagNode,
        parents: Vec<VertexId>,
        report: &EpochReport,
    ) -> MsgAnchorEdge {
        let parent_coherence = if parents.is_empty() || self.config.max_parents == 0 {
            0.1
        } else {
            (parents.len() as f64 / self.config.max_parents as f64).min(1.0)
        };
        let ann_similarity = (1.0_f32 - report.fairness_gini as f32).clamp(0.0, 1.0);
        let denom = if report.aggregated_tps <= 0.0 {
            1.0
        } else {
            report.aggregated_tps
        };
        let contribution_score = (node.transactions_carried as f64 / denom).clamp(0.0, 1.0);
        let icosuple = Icosuple::synthesize(
            self.module.config(),
            node.node_id.clone(),
            node.payload_bytes,
            ann_similarity,
        );

        MsgAnchorEdge {
            request_id: derive_request_id(&node.node_id, report.epoch_index),
            chain_epoch: report.epoch_index,
            parents,
            parent_coherence,
            lamport: node.transactions_carried,
            contribution_score,
            ann_similarity,
            qrng_entropy_bits: self.config.qrng_entropy_bits,
            pqc_binding: PqcBinding::new(node.leader.clone(), PqcScheme::Dilithium),
            icosuple,
        }
    }

    fn record_qs_dag(
        &mut self,
        node: &DagNode,
        receipt: &VertexReceipt,
    ) -> Result<(), ChronosyncKeeperError> {
        let parents = if node.parents.is_empty() {
            vec!["chronosync/genesis".into()]
        } else {
            node.parents.clone()
        };
        let diff = StateDiff::new(
            node.node_id.clone(),
            node.leader.clone(),
            parents,
            node.transactions_carried,
            vec![StateOp::upsert(
                format!("vertex/{}", node.node_id),
                receipt.vertex_id.to_string(),
            )],
        );
        self.dag.insert(diff)?;
        Ok(())
    }
}

/// Outcome of feeding a Chronosync epoch into the keeper.
#[derive(Clone, Debug)]
pub struct ChronosyncKeeperReport {
    pub epoch_index: u64,
    pub applied_vertices: Vec<VertexReceipt>,
    pub missing_parents: Vec<String>,
    pub storage_layout: ModuleStorageLayout,
    pub dag_head: Option<String>,
}

/// Errors produced while syncing VG-DAG elections into 5D-QEH.
#[derive(Error, Debug)]
pub enum ChronosyncKeeperError {
    #[error(transparent)]
    Module(#[from] ModuleError),
    #[error("qs-dag error: {0}")]
    Dag(DagError),
}

impl From<DagError> for ChronosyncKeeperError {
    fn from(err: DagError) -> Self {
        ChronosyncKeeperError::Dag(err)
    }
}

fn derive_request_id(node_id: &str, epoch: u64) -> u64 {
    let mut hasher = Hasher::new();
    hasher.update(node_id.as_bytes());
    hasher.update(&epoch.to_le_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest.as_bytes()[..8]);
    u64::from_le_bytes(bytes)
}

impl AnchorEdgeEndpoint for ChronosyncKeeper {
    fn submit_anchor_edge(
        &mut self,
        msg: MsgAnchorEdge,
    ) -> Result<MsgAnchorEdgeResponse, ModuleError> {
        let receipt = self.module.apply_anchor_edge(msg)?;
        Ok(MsgAnchorEdgeResponse {
            storage: self.module.storage_layout().clone(),
            receipt,
        })
    }
}
