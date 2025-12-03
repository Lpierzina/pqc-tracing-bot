use shell_words::split;

use crate::{
    error::{OverlayError, OverlayResult},
    rpc::{
        AnonymizeQueryParams, Dw3bOverlayRpc, EntropyRequestParams, ObfuscateRouteParams,
        PolicyConfigureParams, QtaidProveParams, SyncStateParams,
    },
};

pub fn parse_statement(statement: &str) -> OverlayResult<Dw3bOverlayRpc> {
    let tokens = split(statement).map_err(|err| OverlayError::grapplang(err.to_string()))?;
    if tokens.is_empty() {
        return Err(OverlayError::grapplang("empty statement"));
    }
    match tokens[0].as_str() {
        "dw3b-anonymize" => parse_anonymize(&tokens),
        "dw3b-obfuscate" => parse_obfuscate(&tokens),
        "dw3b-policy" => parse_policy(&tokens),
        "dw3b-entropy" => parse_entropy(&tokens),
        "dw3b-sync" => parse_sync(&tokens),
        "qtaid-prove" => parse_qtaid(&tokens),
        other => Err(OverlayError::grapplang(format!(
            "unsupported Grapplang command: {other}"
        ))),
    }
}

fn parse_anonymize(tokens: &[String]) -> OverlayResult<Dw3bOverlayRpc> {
    let mut params = AnonymizeQueryParams {
        did: "did:autheo:demo".into(),
        attribute: "dw3b::attribute".into(),
        payload: "dw3b::payload".into(),
        epsilon: 1e-6,
        delta: 2f64.powi(-40),
        route_layers: 5,
        bloom_capacity: None,
        bloom_fp_rate: None,
        stake_threshold: None,
        public_inputs: vec![],
        lamport_hint: None,
    };
    let mut index = 1;
    if tokens.len() > 1 && !tokens[1].starts_with("--") {
        params.attribute = tokens[1].clone();
        params.payload = tokens[1].clone();
        index = 2;
    }
    while index < tokens.len() {
        match tokens[index].as_str() {
            "--did" => {
                params.did = expect_value(tokens, index)?;
                index += 2;
            }
            "--payload" => {
                params.payload = expect_value(tokens, index)?;
                index += 2;
            }
            "--epsilon" => {
                params.epsilon = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("epsilon must be a float"))?;
                index += 2;
            }
            "--delta" => {
                params.delta = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("delta must be a float"))?;
                index += 2;
            }
            "--route-layers" => {
                params.route_layers = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("route-layers must be an integer"))?;
                index += 2;
            }
            "--bloom-capacity" => {
                let value = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("bloom-capacity must be integer"))?;
                params.bloom_capacity = Some(value);
                index += 2;
            }
            "--fp-rate" => {
                let value = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("fp-rate must be float"))?;
                params.bloom_fp_rate = Some(value);
                index += 2;
            }
            "--stake-threshold" => {
                let value = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("stake-threshold must be integer"))?;
                params.stake_threshold = Some(value);
                index += 2;
            }
            "--lamport" => {
                let value = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("lamport must be integer"))?;
                params.lamport_hint = Some(value);
                index += 2;
            }
            other => {
                return Err(OverlayError::grapplang(format!(
                    "unknown flag {other} in dw3b-anonymize"
                )));
            }
        }
    }
    Ok(Dw3bOverlayRpc::AnonymizeQuery(params))
}

fn parse_obfuscate(tokens: &[String]) -> OverlayResult<Dw3bOverlayRpc> {
    if tokens.len() < 2 {
        return Err(OverlayError::grapplang(
            "dw3b-obfuscate requires data argument",
        ));
    }
    let mut params = ObfuscateRouteParams {
        data: tokens[1].clone(),
        layers: 3,
        k_anonymity: 0.9,
    };
    let mut index = 2;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "--layers" => {
                params.layers = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("layers must be integer"))?;
                index += 2;
            }
            "--k" | "--k-anon" => {
                params.k_anonymity = expect_value(tokens, index)?
                    .parse()
                    .map_err(|_| OverlayError::grapplang("k must be float"))?;
                index += 2;
            }
            other => {
                return Err(OverlayError::grapplang(format!(
                    "unknown flag {other} in dw3b-obfuscate"
                )));
            }
        }
    }
    Ok(Dw3bOverlayRpc::ObfuscateRoute(params))
}

fn parse_policy(tokens: &[String]) -> OverlayResult<Dw3bOverlayRpc> {
    if tokens.len() < 2 {
        return Err(OverlayError::grapplang(
            "dw3b-policy requires a YAML blob or string",
        ));
    }
    let mut params = PolicyConfigureParams {
        policy_yaml: tokens[1].clone(),
        zkp_circuit: None,
    };
    let mut index = 2;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "--zkp" | "--circuit" => {
                params.zkp_circuit = Some(expect_value(tokens, index)?);
                index += 2;
            }
            other => {
                return Err(OverlayError::grapplang(format!(
                    "unknown flag {other} in dw3b-policy"
                )));
            }
        }
    }
    Ok(Dw3bOverlayRpc::PolicyConfigure(params))
}

fn parse_entropy(tokens: &[String]) -> OverlayResult<Dw3bOverlayRpc> {
    if tokens.len() < 2 {
        return Err(OverlayError::grapplang(
            "dw3b-entropy requires <samples> argument",
        ));
    }
    let samples: u32 = tokens[1]
        .parse()
        .map_err(|_| OverlayError::grapplang("samples must be integer"))?;
    let mut params = EntropyRequestParams {
        samples,
        dimension5: true,
    };
    if tokens.len() > 2 {
        params.dimension5 = tokens[2] == "--5d" || tokens[2] == "--dimension5";
    }
    Ok(Dw3bOverlayRpc::EntropyRequest(params))
}

fn parse_sync(_: &[String]) -> OverlayResult<Dw3bOverlayRpc> {
    Ok(Dw3bOverlayRpc::SyncState(SyncStateParams {
        causal_graph: None,
        force: false,
    }))
}

fn parse_qtaid(tokens: &[String]) -> OverlayResult<Dw3bOverlayRpc> {
    if tokens.len() < 2 {
        return Err(OverlayError::grapplang(
            "qtaid-prove <trait> [--owner DID] [--genome SEGMENT]",
        ));
    }
    let mut owner = "did:autheo:demo".to_string();
    let mut genome = tokens[1].trim_matches('"').to_string();
    let trait_name = tokens[1].trim_matches('"').to_string();
    let mut bits = None;
    let mut index = 2;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "--owner" => {
                owner = expect_value(tokens, index)?;
                index += 2;
            }
            "--genome" => {
                genome = expect_value(tokens, index)?;
                index += 2;
            }
            "--bits" => {
                bits = Some(
                    expect_value(tokens, index)?
                        .parse()
                        .map_err(|_| OverlayError::grapplang("bits must be integer"))?,
                );
                index += 2;
            }
            other => {
                return Err(OverlayError::grapplang(format!(
                    "unknown flag {other} in qtaid-prove"
                )));
            }
        }
    }
    Ok(Dw3bOverlayRpc::QtaidProve(QtaidProveParams {
        owner_did: owner,
        trait_name,
        genome_segment: genome,
        bits_per_snp: bits,
    }))
}

fn expect_value(tokens: &[String], index: usize) -> OverlayResult<String> {
    tokens
        .get(index + 1)
        .cloned()
        .ok_or_else(|| OverlayError::grapplang("flag missing value"))
}
