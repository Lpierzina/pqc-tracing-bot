use pqcnet_crypto::{CryptoConfig, CryptoProvider};

fn main() {
    let mut provider =
        CryptoProvider::from_config(&CryptoConfig::sample("demo-sentry")).expect("valid config");

    for peer in ["watcher-a", "watcher-b"] {
        let derived = provider.derive_shared_key(peer).expect("derive shared key");
        println!(
            "[pqcnet-crypto] peer={} key[0..4]={:?} expires_at={:?} ciphertext[0..4]={:?}",
            peer,
            &derived.material[..4],
            derived.expires_at,
            &derived.ciphertext[..4]
        );
    }

    let payload = b"demo-payload";
    let signature = provider.sign(payload).expect("sign payload");
    let verified = provider
        .verify(payload, &signature)
        .expect("verify payload");
    println!(
        "[pqcnet-crypto] signature-bytes={} verified={}",
        signature.bytes.len(),
        verified
    );
}
