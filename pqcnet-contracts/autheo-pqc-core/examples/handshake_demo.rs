use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use autheo_pqc_core::adapters::{DemoMlDsa, DemoMlKem};
use autheo_pqc_core::dsa::MlDsaEngine;
use autheo_pqc_core::handshake::{
    derive_session_keys, DeriveSessionInput, InitHandshakeConfig, RespondHandshakeConfig,
};
use autheo_pqc_core::kem::MlKemEngine;
use autheo_pqc_core::types::KeyId;
use blake2::Blake2s256;
use digest::Digest;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let initiator_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
    let responder_kem = MlKemEngine::new(Box::new(DemoMlKem::new()));
    let initiator_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));
    let responder_dsa = MlDsaEngine::new(Box::new(DemoMlDsa::new()));

    let server_kem = responder_kem.keygen()?;
    let server_kem_id = key_id(&server_kem.public_key, 1);
    let client_sign = initiator_dsa.keygen()?;
    let client_sign_id = key_id(&client_sign.public_key, 2);
    let server_sign = responder_dsa.keygen()?;
    let server_sign_id = key_id(&server_sign.public_key, 3);

    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let application_data = format!("client=handshake-demo&ts={now_ms}").into_bytes();
    let route_hash = route_digest("waku/mesh/demo");

    let (request, init_state) = autheo_pqc_core::handshake::init_handshake(InitHandshakeConfig {
        kem: &initiator_kem,
        dsa: &initiator_dsa,
        server_kem_public_key: &server_kem.public_key,
        server_signing_public_key: &server_sign.public_key,
        server_signing_key_id: server_sign_id.clone(),
        client_signing_secret_key: &client_sign.secret_key,
        client_signing_public_key: &client_sign.public_key,
        client_signing_key_id: client_sign_id.clone(),
        route_hash,
        application_data: &application_data,
    })?;

    println!(
        "initiator -> responder ciphertext bytes: {}",
        request.ciphertext.len()
    );

    let (response, responder_state) = autheo_pqc_core::handshake::respond_handshake(
        RespondHandshakeConfig {
            kem: &responder_kem,
            dsa: &responder_dsa,
            server_kem_secret_key: &server_kem.secret_key,
            server_kem_public_key: &server_kem.public_key,
            server_kem_key_id: server_kem_id.clone(),
            server_signing_secret_key: &server_sign.secret_key,
            server_signing_public_key: &server_sign.public_key,
            server_signing_key_id: server_sign_id.clone(),
        },
        &request,
    )?;

    println!("responder session id: {}", hex(&response.session_id));

    let initiator_keys = derive_session_keys(DeriveSessionInput::Initiator {
        state: init_state,
        request: &request,
        response: &response,
        dsa: &initiator_dsa,
    })?;
    let responder_keys = derive_session_keys(DeriveSessionInput::Responder {
        state: responder_state,
    })?;

    println!("shared session id: {}", hex(&initiator_keys.session_id));

    let sender = Aes256Gcm::new_from_slice(&initiator_keys.send_key)?;
    let ciphertext = sender.encrypt(
        Nonce::from_slice(&initiator_keys.send_nonce),
        Payload {
            msg: b"post-quantum tunnels are live",
            aad: &response.route_hash,
        },
    )?;
    let receiver = Aes256Gcm::new_from_slice(&responder_keys.recv_key)?;
    let cleartext = receiver.decrypt(
        Nonce::from_slice(&responder_keys.recv_nonce),
        Payload {
            msg: &ciphertext,
            aad: &response.route_hash,
        },
    )?;

    println!(
        "responder decrypted payload: {}",
        String::from_utf8_lossy(&cleartext)
    );
    println!("tuple key (hex): {}", hex(&initiator_keys.tuple_key));
    println!("hint: run with `--features liboqs` to back these calls with liboqs Kyber/Dilithium");

    Ok(())
}

fn key_id(seed: &[u8], salt: u64) -> KeyId {
    let mut hasher = Blake2s256::new();
    hasher.update(seed);
    hasher.update(salt.to_le_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    KeyId(out)
}

fn route_digest(label: &str) -> [u8; 32] {
    let mut hasher = Blake2s256::new();
    hasher.update(label.as_bytes());
    let digest = hasher.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_slice());
    out
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}
