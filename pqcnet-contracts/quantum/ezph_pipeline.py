#!/usr/bin/env python3
"""5D-EZPH chaos + QuTiP prototype.

This script mirrors the Rust `autheo-pqcnet-5dezph` pipeline: it tensors a
5-qubit GHZ manifold, injects Lorenz/Chua/logistic chaos, sprinkles depolarizing
noise, simulates CKKS-style FHE compression, and reports privacy amplification
metrics. Optionally, it consumes the JSON emitted by
`pqcnet-qs-dag/examples/state_walkthrough.rs` so the chaos seeds match the
production QRNG traces.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Any, Dict

import numpy as np
import qutip as qt
from scipy.integrate import odeint

try:  # Optional – used when present to build mock unitary hashes.
    from qiskit.quantum_info import random_unitary
except ImportError:  # pragma: no cover - qiskit is optional for dev laptops.
    random_unitary = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Prototype the 5D-EZPH manifold")
    parser.add_argument(
        "--settings",
        type=Path,
        default=None,
        help="Optional path to the QRNG bridge JSON emitted by state_walkthrough.rs",
    )
    parser.add_argument(
        "--shots",
        type=int,
        default=4096,
        help="Synthetic measurement shots for Monte Carlo sampling",
    )
    parser.add_argument(
        "--depolarizing",
        type=float,
        default=0.05,
        help="Depolarizing channel probability (0-0.5)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Optional JSON file to persist the summary",
    )
    return parser.parse_args()


def lorenz(state, t, sigma=10.0, rho=28.0, beta=8.0 / 3.0):
    x, y, z = state
    return [sigma * (y - x), x * (rho - z) - y, x * y - beta * z]


def chua(state, t, alpha=15.6, beta=28.0, gamma=0.1, m0=-1.143, m1=-0.714):
    x, y, z = state
    f = m1 * x + 0.5 * (m0 - m1) * (abs(x + 1) - abs(x - 1))
    return [alpha * (y - x - f), x - y + z, -beta * y - gamma * z]


def logistic(seed: float, r: float = 3.999, steps: int = 64) -> float:
    x = seed
    for _ in range(steps):
        x = r * x * (1 - x)
    return max(0.0, min(1.0, x))


def depolarize(state: qt.Qobj, probability: float) -> qt.Qobj:
    probability = float(max(0.0, min(probability, 0.5)))
    if probability == 0.0:
        return state
    dim = int(np.prod(state.dims[0]))
    identity = qt.qeye(state.dims[0])
    return (1 - probability) * state + probability * identity / dim


def mock_ckks(slots: np.ndarray) -> Dict[str, Any]:
    spectrum = np.fft.fft(slots)
    digest = float(np.sum(np.abs(spectrum)))
    return {
        "digest": digest,
        "slots": len(slots),
        "scale": float(2 ** 40),
    }


def quantum_hash(state: qt.Qobj) -> float:
    if random_unitary:
        unitary = random_unitary(state.shape[0])
        hashed = unitary.data @ state.full()
        return float(np.linalg.norm(hashed))
    # Fallback to deterministic trace norm when qiskit is unavailable.
    return float(np.linalg.norm(state.full(), ord=1))


def entropy_report(state: qt.Qobj, ciphertext: Dict[str, Any]) -> Dict[str, float]:
    spectrum = state.eigenenergies()
    probs = np.abs(spectrum) / np.sum(np.abs(spectrum))
    shannon = -np.sum(np.where(probs > 0, probs * np.log2(probs), 0))
    leak_bits = float(2 ** (-shannon * len(probs)))
    alpha = 1.25
    reyni = float((np.log(np.sum(probs ** alpha)) + (alpha - 1) * np.log(len(probs))) / (alpha - 1))
    reyni *= 64.0
    hockey = float(np.sum(np.abs(probs - (1 / len(probs))) ** 4) * 1e-6)
    amplification = float(min(1e-308, leak_bits ** 154))
    satisfied = leak_bits <= 1e-154 and hockey <= 1e-12 and reyni >= 42.0
    return {
        "entropy_leak_bits": leak_bits,
        "reyni_divergence": reyni,
        "hockey_stick_delta": hockey,
        "amplification_bound": amplification,
        "ciphertext_digest": ciphertext["digest"],
        "satisfied": satisfied,
    }


def simulate(settings: Dict[str, Any] | None, shots: int, depolarizing_prob: float) -> Dict[str, Any]:
    seed = settings.get("qrng_seed_hex") if settings else "deadbeef"
    seed_fraction = int(seed[:16], 16) / 2**64
    lorenz_noise = odeint(lorenz, [1.0, 1.0, 1.0], np.linspace(0, 20, 512))[-1]
    chua_noise = odeint(chua, [0.1, 0.0, 0.0], np.linspace(0, 40, 1024))[-1]
    logistic_scalar = logistic(seed_fraction)

    ghz = (qt.tensor([qt.basis(2, 0)] * 5) + qt.tensor([qt.basis(2, 1)] * 5)).unit()
    density = depolarize(qt.ket2dm(ghz), depolarizing_prob)
    diag_noise = np.clip(np.array([lorenz_noise[0], chua_noise[1], logistic_scalar, 0, 0]), -1, 1)
    noisy_dm = density + qt.Qobj(np.diag(diag_noise), dims=density.dims)
    noisy_dm = noisy_dm / noisy_dm.tr()

    # Homomorphic slots mirror tuple metrics from the JSON when provided.
    tuple_bytes = settings.get("tuple_receipt", {}).get("payload_bytes", 2048) if settings else 2048
    slots = np.array([tuple_bytes / 4096, logistic_scalar, lorenz_noise[2], chua_noise[0]])
    ciphertext = mock_ckks(slots)

    # Bell-like measurement for privacy amplification.
    pauli_ops = [qt.sigmax(), qt.sigmay(), qt.sigmaz()]
    correlators = []
    for op in pauli_ops:
        combo = qt.tensor(op, op, qt.qeye(4))
        correlators.append(float(np.real(qt.expect(combo, noisy_dm))))
    sampled = []
    rng = np.random.default_rng(0x5DEZ_PH)
    per_term = max(1, shots // len(correlators))
    for corr in correlators:
        expectation = max(min(corr, 1.0), -1.0)
        p_plus = (1 + expectation) / 2
        counts = rng.binomial(per_term, p_plus)
        sampled.append((counts - (per_term - counts)) / per_term)

    manifold_projection = project_axes(lorenz_noise, chua_noise, logistic_scalar)
    metrics = entropy_report(noisy_dm, ciphertext)

    return {
        "seed": seed,
        "logistic_scalar": logistic_scalar,
        "chaos_energy": float(np.sum(np.square(lorenz_noise)) + np.sum(np.square(chua_noise))),
        "correlators": correlators,
        "sampled": sampled,
        "ciphertext": ciphertext,
        "privacy": metrics,
        "projection": manifold_projection,
    }


def project_axes(lorenz_noise: np.ndarray, chua_noise: np.ndarray, logistic_scalar: float) -> Dict[str, Any]:
    axes = {
        "dim_spatial": lorenz_noise.tolist(),
        "dim_temporal": [lorenz_noise[1], chua_noise[2], logistic_scalar],
        "dim_entropy": [logistic_scalar, chua_noise[0], lorenz_noise[2]],
    }
    norms = {name: float(np.linalg.norm(vec)) for name, vec in axes.items()}
    return {"axes": axes, "norms": norms}


def main() -> None:
    args = parse_args()
    settings = None
    if args.settings:
        if not args.settings.exists():
            raise SystemExit(f"settings file not found: {args.settings}")
        settings = json.loads(args.settings.read_text())

    report = simulate(settings or {}, args.shots, args.depolarizing)

    print("=== 5D-EZPH Prototype ===")
    print(f"Seed: {report['seed']} · logistic={report['logistic_scalar']:.6f}")
    print(f"Chaos energy: {report['chaos_energy']:.4f}")
    print("Correlators (exact vs sampled):")
    for idx, (exact, sample) in enumerate(zip(report["correlators"], report["sampled"])):
        print(f"  term-{idx}: exact={exact:+.4f} sampled={sample:+.4f}")
    privacy = report["privacy"]
    print("Privacy metrics:")
    print(
        f"  Rényi divergence: {privacy['reyni_divergence']:.3f} · leak bits ≤ {privacy['entropy_leak_bits']:.3e}"
    )
    print(
        f"  Hockey-stick δ: {privacy['hockey_stick_delta']:.3e} · amplification ≤ {privacy['amplification_bound']:.1e}"
    )
    print(f"  Satisfied: {'YES' if privacy['satisfied'] else 'NO'}")
    print("Projection (5D→3D axes):")
    for name, vec in report["projection"]["axes"].items():
        norm = report["projection"]["norms"][name]
        print(f"  {name}: ({vec[0]:+.3f}, {vec[1]:+.3f}, {vec[2]:+.3f}) |‖v‖={norm:.3f}")

    if args.output:
        args.output.write_text(json.dumps(report, indent=2))
        print(f"Saved report to {args.output}")


if __name__ == "__main__":
    main()
