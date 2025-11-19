use pqcnet_qs_dag::{QsDagHost, QsDagPqc};
use std::{cell::RefCell, collections::HashMap};

#[derive(Debug)]
enum HostError {
    UnknownEdge(String),
    VerifyFailed,
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::UnknownEdge(id) => write!(f, "missing edge: {id}"),
            HostError::VerifyFailed => write!(f, "signature mismatch"),
        }
    }
}

impl std::error::Error for HostError {}

struct InMemoryHost {
    payloads: HashMap<String, Vec<u8>>,
    anchors: RefCell<Vec<(String, String, Vec<u8>)>>,
}

impl InMemoryHost {
    fn new() -> Self {
        Self {
            payloads: HashMap::new(),
            anchors: RefCell::new(Vec::new()),
        }
    }

    fn with_edge(mut self, id: &str, payload: &[u8]) -> Self {
        self.payloads.insert(id.into(), payload.to_vec());
        self
    }
}

impl QsDagHost for InMemoryHost {
    type EdgeId = String;
    type KeyId = String;
    type Error = HostError;

    fn attach_pqc_signature(
        &self,
        edge_id: &Self::EdgeId,
        signer: &Self::KeyId,
        signature: &[u8],
    ) -> Result<(), Self::Error> {
        println!(
            "anchoring edge={} signer={} sig={:?}",
            edge_id, signer, signature
        );
        self.anchors
            .borrow_mut()
            .push((edge_id.clone(), signer.clone(), signature.to_vec()));
        Ok(())
    }

    fn get_edge_payload(&self, edge_id: &Self::EdgeId) -> Result<Vec<u8>, Self::Error> {
        self.payloads
            .get(edge_id)
            .cloned()
            .ok_or_else(|| HostError::UnknownEdge(edge_id.clone()))
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = InMemoryHost::new().with_edge("edge-1", b"qs-dag-payload");
    let qs_dag = QsDagPqc::new(&host);
    let edge_id = "edge-1".to_string();
    let signer = "dilithium-node".to_string();

    qs_dag.verify_and_anchor(&edge_id, &signer, b"qs-dag-payload", |_id, msg, sig| {
        if msg == sig {
            println!("verified {} bytes", sig.len());
            Ok(())
        } else {
            Err(HostError::VerifyFailed)
        }
    })?;

    println!(
        "Anchors recorded: {:?}",
        host.anchors.borrow().iter().collect::<Vec<_>>()
    );
    Ok(())
}
