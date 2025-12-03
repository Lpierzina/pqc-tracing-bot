use crate::{
    error::{OverlayError, OverlayResult},
    rpc::{
        OverlayRpc, ProveAttributeParams, QtaidTokenizeParams, RevokeCredentialParams,
        VerifyProofParams,
    },
};

/// Parse a Zer0veil Grapplang statement into an Overlay RPC command.
pub fn parse_statement(statement: &str) -> OverlayResult<OverlayRpc> {
    let trimmed = statement.trim();
    let lowered = trimmed.to_lowercase();
    if lowered.starts_with("prove ") {
        return parse_prove(trimmed, &lowered);
    }
    if lowered.starts_with("verify proof") {
        return parse_verify(trimmed, &lowered);
    }
    if lowered.starts_with("revoke credential") {
        return parse_revoke(trimmed, &lowered);
    }
    if lowered.starts_with("qtaid prove") {
        return parse_qtaid(trimmed, &lowered);
    }
    Err(OverlayError::grapplang(format!(
        "unrecognized Grapplang statement: {trimmed}"
    )))
}

fn parse_prove(original: &str, lowered: &str) -> OverlayResult<OverlayRpc> {
    let from_idx = lowered
        .find(" from ")
        .ok_or_else(|| OverlayError::grapplang("expected `from` segment"))?;
    let predicate = original[6..from_idx].trim();
    let using_idx = lowered.find(" using ").unwrap_or(lowered.len());
    let did_segment = if using_idx < lowered.len() {
        &original[(from_idx + 6)..using_idx]
    } else {
        &original[(from_idx + 6)..]
    };
    let did = did_segment.trim().to_string();
    let manifold = if using_idx < lowered.len() {
        let tail = original[(using_idx + 7)..].trim();
        tail.strip_prefix("manifold=")
            .map(|value| value.trim().to_string())
    } else {
        None
    };
    Ok(OverlayRpc::ProveAttribute(ProveAttributeParams {
        did,
        attribute: predicate.to_string(),
        witness: predicate.to_string(),
        manifold,
    }))
}

fn parse_verify(original: &str, lowered: &str) -> OverlayResult<OverlayRpc> {
    let on_idx = lowered
        .find(" on vertex ")
        .ok_or_else(|| OverlayError::grapplang("verify statement missing `on vertex`"))?;
    let proof_id = original[12..on_idx]
        .trim()
        .trim_start_matches('$')
        .to_string();
    let vertex = original[(on_idx + 11)..]
        .trim()
        .trim_start_matches('$')
        .to_string();
    Ok(OverlayRpc::VerifyProof(VerifyProofParams {
        proof_id: Some(proof_id),
        proof_object: None,
        vertex: Some(vertex),
        include_telemetry: true,
    }))
}

fn parse_revoke(original: &str, lowered: &str) -> OverlayResult<OverlayRpc> {
    let before_idx = lowered
        .find(" before ")
        .ok_or_else(|| OverlayError::grapplang("revoke statement missing `before` clause"))?;
    let credential = original[17..before_idx]
        .trim()
        .trim_start_matches('$')
        .to_string();
    let reason = format!(
        "expires {}",
        original[(before_idx + 8)..].trim().trim_matches('"')
    );
    Ok(OverlayRpc::RevokeCredential(RevokeCredentialParams {
        credential_id: credential,
        reason: Some(reason),
    }))
}

fn parse_qtaid(original: &str, lowered: &str) -> OverlayResult<OverlayRpc> {
    let first_quote = original
        .find('"')
        .ok_or_else(|| OverlayError::grapplang("qtaid statement missing quoted trait"))?;
    let rest = &original[(first_quote + 1)..];
    let second_quote = rest
        .find('"')
        .ok_or_else(|| OverlayError::grapplang("unterminated quoted trait"))?;
    let trait_statement = &rest[..second_quote];
    let marker = "\" from genome ";
    let from_idx = lowered
        .find(marker)
        .ok_or_else(|| OverlayError::grapplang("qtaid statement missing `from genome` clause"))?;
    let genome = original[(from_idx + marker.len())..].trim();
    Ok(OverlayRpc::QtaidTokenize(QtaidTokenizeParams {
        genome_segment: trait_statement.to_string(),
        owner_did: genome.trim_start_matches('$').to_string(),
        bits_per_snp: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prove() {
        let rpc = parse_statement("prove age > 18 from did:autheo:abc123 using manifold=5d-chaos")
            .unwrap();
        match rpc {
            OverlayRpc::ProveAttribute(params) => {
                assert_eq!(params.did, "did:autheo:abc123");
                assert_eq!(params.manifold.unwrap(), "5d-chaos");
                assert_eq!(params.attribute, "age > 18");
            }
            _ => panic!("unexpected rpc"),
        }
    }

    #[test]
    fn parses_verify() {
        let rpc = parse_statement("verify proof $PR1 on vertex $VX1").unwrap();
        match rpc {
            OverlayRpc::VerifyProof(params) => {
                assert_eq!(params.proof_id.unwrap(), "PR1");
                assert_eq!(params.vertex.unwrap(), "VX1");
            }
            _ => panic!("unexpected rpc"),
        }
    }

    #[test]
    fn parses_revoke() {
        let rpc = parse_statement("revoke credential $CRED before 2030-01-01").unwrap();
        match rpc {
            OverlayRpc::RevokeCredential(params) => {
                assert_eq!(params.credential_id, "CRED");
                assert!(params.reason.unwrap().contains("2030"));
            }
            _ => panic!("unexpected rpc"),
        }
    }

    #[test]
    fn parses_qtaid() {
        let rpc = parse_statement("qtaid prove \"BRCA1=negative\" from genome $GENOME").unwrap();
        match rpc {
            OverlayRpc::QtaidTokenize(params) => {
                assert_eq!(params.genome_segment, "BRCA1=negative");
                assert_eq!(params.owner_did, "GENOME");
            }
            _ => panic!("unexpected rpc"),
        }
    }
}
