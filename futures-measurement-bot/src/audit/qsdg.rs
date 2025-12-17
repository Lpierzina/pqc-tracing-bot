use crate::audit::{AuditEvent, AuditSink};
use pqcnet_qs_dag::icosuple::IcosupleLayer;
use pqcnet_qs_dag::state::{QsDag, StateDiff, StateOp};
use pqcnet_qs_dag::tuple::{
    PayloadProfile, QIPTag, TupleDomain, TupleEnvelope, TupleProof, TupleProofKind, TupleValidation,
};
use std::sync::Mutex;

/// Minimal QS-DAG anchoring sink.
///
/// This writes each audit event as a QS-DAG `StateDiff` with an attached `TupleEnvelope`
/// (domain=Finance), which gives you an append-only audit graph (in-memory here).
pub struct QsDagAuditSink {
    author: String,
    dag: Mutex<QsDag>,
}

impl QsDagAuditSink {
    pub fn new(author: impl Into<String>) -> anyhow::Result<Self> {
        let author = author.into();
        let genesis = StateDiff::genesis("genesis", &author);
        let dag = QsDag::new(genesis).map_err(|e| anyhow::anyhow!("init QS-DAG failed: {e}"))?;
        Ok(Self {
            author,
            dag: Mutex::new(dag),
        })
    }
}

impl AuditSink for QsDagAuditSink {
    fn emit(&self, event: AuditEvent) -> anyhow::Result<()> {
        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;

        let kind = match &event {
            AuditEvent::Intent { .. } => "intent",
            AuditEvent::Sent { .. } => "sent",
            AuditEvent::Ack { .. } => "ack",
            AuditEvent::Reject { .. } => "reject",
            AuditEvent::Cancel { .. } => "cancel",
            AuditEvent::Fill { .. } => "fill",
            AuditEvent::Timeout { .. } => "timeout",
        };

        let payload = serde_json::to_vec(&event)
            .map_err(|e| anyhow::anyhow!("serialize qsdg payload failed: {e}"))?;
        let mut qrng_seed = [0u8; 32];
        // Deterministic seed from payload for now; swap with real QRNG feed in production.
        let digest = blake3::hash(&payload);
        qrng_seed.copy_from_slice(digest.as_bytes());

        let validation = TupleValidation::new(
            "placeholder-signature",
            digest.as_bytes().to_vec(),
            self.author.as_bytes().to_vec(),
        );
        let proof = TupleProof::new(
            TupleProofKind::Custom("audit".into()),
            digest.as_bytes().to_vec(),
        );

        let tuple = TupleEnvelope::new(
            TupleDomain::Finance,
            IcosupleLayer::APPLICATION_TIER_15,
            PayloadProfile::MessageBus,
            format!("did:bot:{}", self.author),
            "did:venue:audit".to_string(),
            0,
            &payload,
            qrng_seed,
            now_ns,
            QIPTag::Native,
            Some(proof),
            validation,
        );

        let mut dag = self.dag.lock().unwrap();
        let parent = dag
            .canonical_head()
            .map(|d| d.id.clone())
            .unwrap_or_else(|| "genesis".into());

        let id = format!("{kind}:{}", digest.to_hex());
        let ops = vec![StateOp::upsert(format!("audit.{kind}.last"), id.clone())];
        let diff = tuple.into_state_diff(id, &self.author, vec![parent], now_ns, ops);

        let _ = dag
            .insert(diff)
            .map_err(|e| anyhow::anyhow!("insert qsdg diff failed: {e}"))?;
        Ok(())
    }
}
