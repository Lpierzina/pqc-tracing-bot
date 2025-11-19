use alloc::vec::Vec;

/// Host-provided interface to QS-DAG consensus.
pub trait QsDagHost {
    /// Identifier type of a DAG edge.
    type EdgeId;
    /// Identifier type of the signer/validator attaching PQC data.
    type KeyId;
    /// Error type emitted by host operations.
    type Error;

    /// Attach a PQC signature to a DAG edge.
    fn attach_pqc_signature(
        &self,
        edge_id: &Self::EdgeId,
        signer: &Self::KeyId,
        signature: &[u8],
    ) -> Result<(), Self::Error>;

    /// Fetch the DAG payload for signing or verifying.
    fn get_edge_payload(&self, edge_id: &Self::EdgeId) -> Result<Vec<u8>, Self::Error>;
}

/// Thin façade that runs signature verification before anchoring into QS-DAG.
pub struct QsDagPqc<'a, H: QsDagHost + ?Sized> {
    host: &'a H,
}

impl<'a, H: QsDagHost + ?Sized> QsDagPqc<'a, H> {
    /// Create a new QS-DAG façade.
    pub fn new(host: &'a H) -> Self {
        Self { host }
    }

    /// Verify a PQC signature against the DAG payload and anchor it.
    pub fn verify_and_anchor<FVerify>(
        &self,
        edge_id: &H::EdgeId,
        signer: &H::KeyId,
        signature: &[u8],
        verify_fn: FVerify,
    ) -> Result<(), H::Error>
    where
        FVerify: Fn(&H::KeyId, &[u8], &[u8]) -> Result<(), H::Error>,
    {
        let payload = self.host.get_edge_payload(edge_id)?;
        verify_fn(signer, &payload, signature)?;
        self.host.attach_pqc_signature(edge_id, signer, signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::String, vec::Vec};
    use spin::Mutex;

    #[derive(Debug, PartialEq)]
    enum TestError {
        VerifyFailed,
    }

    struct RecordingHost {
        payload: Vec<u8>,
        attachments: Mutex<Vec<(String, String, Vec<u8>)>>,
    }

    impl RecordingHost {
        fn new(payload: &[u8]) -> Self {
            Self {
                payload: payload.to_vec(),
                attachments: Mutex::new(Vec::new()),
            }
        }

        fn attachments(&self) -> Vec<(String, String, Vec<u8>)> {
            self.attachments.lock().clone()
        }
    }

    impl QsDagHost for RecordingHost {
        type EdgeId = String;
        type KeyId = String;
        type Error = TestError;

        fn attach_pqc_signature(
            &self,
            edge_id: &Self::EdgeId,
            signer: &Self::KeyId,
            signature: &[u8],
        ) -> Result<(), Self::Error> {
            self.attachments
                .lock()
                .push((edge_id.clone(), signer.clone(), signature.to_vec()));
            Ok(())
        }

        fn get_edge_payload(&self, _edge_id: &Self::EdgeId) -> Result<Vec<u8>, Self::Error> {
            Ok(self.payload.clone())
        }
    }

    #[test]
    fn verify_and_anchor_attaches_signature_on_success() {
        let host = RecordingHost::new(b"dag payload");
        let facade = QsDagPqc::new(&host);
        let id = "edge-1".to_string();
        let signer = "pk-123".to_string();
        let signature = b"dag payload".to_vec();

        facade
            .verify_and_anchor(&id, &signer, &signature, |_id, msg, sig| {
                if msg == sig {
                    Ok(())
                } else {
                    Err(TestError::VerifyFailed)
                }
            })
            .expect("verification succeeds");

        let attachments = host.attachments();
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].0, id);
        assert_eq!(attachments[0].1, signer);
        assert_eq!(attachments[0].2, signature);
    }

    #[test]
    fn verify_and_anchor_propagates_verifier_errors() {
        let host = RecordingHost::new(b"payload");
        let facade = QsDagPqc::new(&host);
        let id = "edge-1".to_string();
        let signer = "pk-123".to_string();

        let err = facade
            .verify_and_anchor(&id, &signer, b"sig", |_id, _msg, _sig| {
                Err(TestError::VerifyFailed)
            })
            .unwrap_err();
        assert_eq!(err, TestError::VerifyFailed);
        assert!(host.attachments().is_empty());
    }
}
