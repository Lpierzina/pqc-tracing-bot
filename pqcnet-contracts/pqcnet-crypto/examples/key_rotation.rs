use pqcnet_crypto::{CryptoConfig, CryptoProvider};

fn main() {
    let provider =
        CryptoProvider::from_config(&CryptoConfig::sample("demo-sentry")).expect("valid config");

    for peer in ["watcher-a", "watcher-b"] {
        let derived = provider.derive_shared_key(peer);
        println!(
            "[pqcnet-crypto] {} shared key {:?} expires at {:?}",
            peer,
            &derived.material[..4],
            derived.expires_at
        );
    }

    let payload = b"demo-payload";
    let signature = provider.sign(payload);
    println!(
        "[pqcnet-crypto] signature {:?} verified={}",
        &signature.digest[..4],
        provider.verify(payload, &signature)
    );
}
