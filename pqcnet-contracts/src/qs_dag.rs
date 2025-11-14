//! QS-DAG integration shims for anchoring PQC signatures.

use crate::error::PqcResult;
use crate::types::{Bytes, EdgeId, KeyId};

/// Host-provided interface to QS-DAG consensus.
pub trait QsDagHost {
    /// Attach a PQC signature to a DAG edge.
    fn attach_pqc_signature(
        &self,
        edge_id: &EdgeId,
        signer: &KeyId,
        signature: &[u8],
    ) -> PqcResult<()>;

    /// Fetch the DAG payload for signing/verifying.
    fn get_edge_payload(&self, edge_id: &EdgeId) -> PqcResult<Bytes>;
}

/// Simple façade used by contracts.
pub struct QsDagPqc<'a> {
    host: &'a dyn QsDagHost,
}

impl<'a> QsDagPqc<'a> {
    /// Create a new QS-DAG façade.
    pub fn new(host: &'a dyn QsDagHost) -> Self {
        Self { host }
    }

    /// Verify a PQC signature against the DAG payload and anchor it.
    pub fn verify_and_anchor<FVerify>(
        &self,
        edge_id: &EdgeId,
        signer: &KeyId,
        signature: &[u8],
        verify_fn: FVerify,
    ) -> PqcResult<()>
    where
        FVerify: Fn(&KeyId, &[u8], &[u8]) -> PqcResult<()>,
    {
        let payload = self.host.get_edge_payload(edge_id)?;
        verify_fn(signer, &payload, signature)?;
        self.host.attach_pqc_signature(edge_id, signer, signature)
    }
}
