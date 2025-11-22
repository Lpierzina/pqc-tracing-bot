## TupleChain (autheo-pqcnet-tuplechain)

`autheo-pqcnet-tuplechain` is the production TupleChain module shipped with Autheo-One. It deterministically commits five-field tuples `(subject, predicate, object, proof, expiry)` across sharded storage and hands receipts directly to `autheo-pqcnet-icosuple`, `autheo-pqcnet-chronosync`, and `autheo-pqcnet-5dqeh`. There is no simulator data in this crate—receipts, proofs, and expiry paths are identical to what runs in the validator binaries today.

Key production signals:

- **Receipt fidelity:** every `TupleReceipt` captures commitment bytes, tier paths, and expiry TTLs that are consumed by Chronosync/5D-QEH builders without any mock data.
- **Host entropy only:** pruning, sharding, and expiry proofs rely on the same host entropy inputs that power `pqcnet-entropy`, so no pseudo-random harnesses sneak into production builds.
- **End-to-end coverage:** the integration tests described below exercise keeper authorization, shard pruning, and the receipt channel shared with Chronosync—mirroring the deployed configuration.

### Conceptual lineage

- **TernaryChain (legacy):** triple-hash `(input_hash, prev_hash, current_hash)` optimized for 10M TPS anchoring in QS-DAG.
- **TupleChain (current):** five-field tuple with FHE-friendly proofs and expiry controls, deployed as a Cosmos SDK module (`x/tuplechain`) that feeds QS-DAG summaries while exposing semantic queries through AytchDB indexes and QSTP tunnels.
- **Objective:** store immutable, queryable tuples that can be pruned or versioned without leaking plaintext, solving state bloat while enabling agentic AI/identity flows inside Autheo DeOS.

### How TupleChain works

```mermaid
sequenceDiagram
    autonumber
    participant Client as DeOS Client / THEO Agent
    participant Keeper as TupleChain Keeper (x/tuplechain)
    participant Ledger as TupleChain Ledger & Shards
    participant Receipt as Receipt Bus (IBC/QSTP)
    participant Chrono as Chronosync Keeper + 5D-QEH
    participant PQC as PQC Runtime (Kyber/Dilithium)
    participant Entropy as Host Entropy (pqcnet-entropy)
    participant DAG as QS-DAG / Aytch Indexers

    Client->>Keeper: MsgCreateTuple(subject,predicate,object,proof,expiry)
    Keeper->>Ledger: deterministic shard assignment + version commit
    Ledger->>Ledger: encrypt proof + expiry gating (no SIM branches)
    Ledger-->>Receipt: TupleReceipt {commitment, tier_path, expiry}
    Receipt->>Chrono: EndBlock receipts for hyper-tuple inflation
    Chrono->>Entropy: request QRNG bits (512b host entropy)
    Chrono->>PQC: sign anchor preimage w/ Dilithium/Kyber
    Chrono->>DAG: anchor shard summary into QS-DAG + telemetry
    DAG-->>Client: EventCreateTuple + proof handle
    Client->>Keeper: MsgQueryTuple / MsgHistoricalTuple
    Keeper->>DAG: fetch shard snapshot + expiry proofs
```

### Crate layout

- `src/lib.rs`: TupleChain ledger, keeper façade, builder APIs, and unit tests.
- `tests/ledger.rs`: integration tests that exercise keeper authorization, historical queries, and pruning.

### Usage

```rust
use autheo_pqcnet_tuplechain::{ProofScheme, TupleChainConfig, TupleChainKeeper, TuplePayload};

let mut keeper =
    TupleChainKeeper::new(TupleChainConfig::default()).allow_creator("did:autheo:l1/kernel");

let receipt = keeper
    .store_tuple(
        "did:autheo:l1/kernel",
        TuplePayload::builder("did:autheo:alice", "owns")
            .object_text("autheoid-passport")
            .proof(ProofScheme::Zkp, b"proof", "demo-zkp")
            .expiry(1_700_000_000_000 + 86_400_000)
            .build(),
        1_700_000_000_000,
    )
    .expect("tuple stored");
println!("tuple_id={} shard={} version={}", receipt.tuple_id, receipt.shard_id, receipt.version);
```

### Tests

| Command | Description |
| --- | --- |
| `cargo test -p autheo-pqcnet-tuplechain` | Executes unit + integration tests covering keeper authZ, version history, shard pruning, receipt emission, and shard utilization telemetry—exactly what Chronosync consumes in production. |

Use the README plus the keeper APIs to bootstrap a dedicated repo later—the crate already exposes the ledger, builder, and production sequence diagram that can be copied straight into Cosmos SDK module docs with zero simulator caveats.
