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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{PqcError, PqcResult};
    use crate::types::{Bytes, EdgeId, KeyId};
    use alloc::vec::Vec;
    use spin::Mutex;

    struct RecordingHost {
        payload: Bytes,
        attachments: Mutex<Vec<(EdgeId, KeyId, Bytes)>>,
    }

    impl RecordingHost {
        fn new(payload: Bytes) -> Self {
            Self {
                payload,
                attachments: Mutex::new(Vec::new()),
            }
        }

        fn attachments(&self) -> Vec<(EdgeId, KeyId, Bytes)> {
            self.attachments.lock().clone()
        }
    }

    impl QsDagHost for RecordingHost {
        fn attach_pqc_signature(
            &self,
            edge_id: &EdgeId,
            signer: &KeyId,
            signature: &[u8],
        ) -> PqcResult<()> {
            self.attachments
                .lock()
                .push((edge_id.clone(), signer.clone(), signature.to_vec()));
            Ok(())
        }

        fn get_edge_payload(&self, _edge_id: &EdgeId) -> PqcResult<Bytes> {
            Ok(self.payload.clone())
        }
    }

    #[test]
    fn verify_and_anchor_attaches_signature_on_success() {
        let host = RecordingHost::new(b"dag payload".to_vec());
        let facade = QsDagPqc::new(&host);
        let edge = EdgeId([0xAA; 32]);
        let signer = KeyId([0x55; 32]);
        let signature = b"dag payload".to_vec();

        facade
            .verify_and_anchor(&edge, &signer, &signature, |_id, msg, sig| {
                assert_eq!(msg, b"dag payload");
                assert_eq!(sig, signature.as_slice());
                Ok(())
            })
            .expect("verification succeeds");

        let attachments = host.attachments();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].0, edge);
        assert_eq!(attachments[0].1, signer);
        assert_eq!(attachments[0].2, signature);
    }

    #[test]
    fn verify_and_anchor_propagates_verifier_errors() {
        let host = RecordingHost::new(b"payload".to_vec());
        let facade = QsDagPqc::new(&host);
        let edge = EdgeId([0x01; 32]);
        let signer = KeyId([0x02; 32]);
        let err = facade
            .verify_and_anchor(&edge, &signer, b"sig", |_id, _msg, _sig| {
                Err(PqcError::VerifyFailed)
            })
            .unwrap_err();
        assert_eq!(err, PqcError::VerifyFailed);
        assert!(host.attachments().is_empty());
    }
}
