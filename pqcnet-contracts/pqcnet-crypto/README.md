# pqcnet-crypto

Deterministic crypto helpers that pqcnet binaries rely on for deriving shared
keys and producing demo-friendly signatures. The crate is intentionally small so
it can be split into its own repository later without pulling in the rest of the
workspace.

## Example / Demo

```
cargo run -p pqcnet-crypto --example key_rotation
```

The example derives shared keys for two peers, prints the expiration time, and
signs/verifies a payload so you can show end-to-end behavior to stakeholders.

## Config schema

Reference snippet (TOML) that works with both the relayer and sentry configs:

```toml
[crypto]
node-id = "sentry-a"
secret-seed = "1111111111111111111111111111111111111111111111111111111111111111"
key-ttl-secs = 3600
```

`node-id` participates in key derivation, `secret-seed` is a hex-encoded 32-byte
value (omit to auto-generate), and `key-ttl-secs` controls the derived key
expiration window.

## Tests

- Unit + doc tests: `cargo test -p pqcnet-crypto`
- Run only the quickstart doctest: `cargo test -p pqcnet-crypto --doc`

These tests exercise `CryptoProvider` plus the new doctest that mirrors the
example so `cargo test --doc` no longer reports zero cases.
