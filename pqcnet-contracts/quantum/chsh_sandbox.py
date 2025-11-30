#!/usr/bin/env python3
"""
QuTiP-backed CHSH sandbox that ingests the QRNG-derived plan emitted by
`pqcnet-qs-dag/examples/state_walkthrough.rs` and evaluates both the classic
two-qubit CHSH violation plus the 5D hypergraph extension used by the 5D-QEH
modules. The script prints the exact expectation values, a Monte Carlo estimate
based on the requested shot count, and highlights whether the observed values
surpass the classical and 5D thresholds.
"""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
from typing import Dict, List, Tuple

import numpy as np
import qutip as qt


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Evaluate CHSH violations with QRNG settings.")
    parser.add_argument(
        "--settings",
        type=Path,
        default=Path("pqcnet-contracts/target/chsh_bridge_state.json"),
        help="Path to the JSON payload exported by state_walkthrough.rs",
    )
    parser.add_argument(
        "--shots",
        type=int,
        default=4096,
        help="Number of synthetic measurement shots per correlator for Monte Carlo estimates.",
    )
    parser.add_argument(
        "--depolarizing",
        type=float,
        default=0.0,
        help="Optional depolarizing noise probability applied before measurements (0.0 - 0.5).",
    )
    parser.add_argument(
        "--save",
        type=Path,
        default=None,
        help="Optional path to persist the result payload as JSON.",
    )
    return parser.parse_args()


def bell_state_dm() -> qt.Qobj:
    zero_zero = qt.tensor(qt.basis(2, 0), qt.basis(2, 0))
    one_one = qt.tensor(qt.basis(2, 1), qt.basis(2, 1))
    return qt.ket2dm((zero_zero + one_one).unit())


def ghz_state_dm(dims: int) -> qt.Qobj:
    zeros = qt.tensor([qt.basis(2, 0)] * dims)
    ones = qt.tensor([qt.basis(2, 1)] * dims)
    return qt.ket2dm((zeros + ones).unit())


def apply_depolarizing(state: qt.Qobj, probability: float) -> qt.Qobj:
    if probability <= 0.0:
        return state
    probability = min(max(probability, 0.0), 0.5)
    dim = int(np.prod(state.dims[0]))
    identity = qt.qeye(state.dims[0])
    return (1 - probability) * state + probability * identity / dim


def bloch_operator(theta: float, phi: float) -> qt.Qobj:
    return (
        math.cos(theta) * qt.sigmaz()
        + math.sin(theta)
        * (math.cos(phi) * qt.sigmax() + math.sin(phi) * qt.sigmay())
    )


def planar_pauli(theta: float) -> qt.Qobj:
    return math.cos(theta) * qt.sigmaz() + math.sin(theta) * qt.sigmax()


def sample_from_expectation(expectation: float, shots: int, rng: np.random.Generator) -> float:
    expectation = max(min(expectation, 1.0), -1.0)
    prob_plus = (1 + expectation) / 2
    counts_plus = rng.binomial(shots, prob_plus)
    counts_minus = shots - counts_plus
    return (counts_plus - counts_minus) / shots


def evaluate_two_qubit(
    plan: Dict[str, Dict[str, List[float]]],
    shots: int,
    depolarizing: float,
    rng: np.random.Generator,
) -> Dict[str, float]:
    state = apply_depolarizing(bell_state_dm(), depolarizing)
    alice_angles = plan["alice"]["angles"]
    bob_angles = plan["bob"]["angles"]

    ops = {
        "A": planar_pauli(alice_angles[0]),
        "A'": planar_pauli(alice_angles[1]),
        "B": planar_pauli(bob_angles[0]),
        "B'": planar_pauli(bob_angles[1]),
    }
    correlators = [
        (ops["A"], ops["B"], 1.0),
        (ops["A'"], ops["B"], 1.0),
        (ops["A"], ops["B'"], 1.0),
        (ops["A'"], ops["B'"], -1.0),
    ]

    exact = 0.0
    sampled = 0.0
    shots_per = max(1, shots // len(correlators))
    for op_a, op_b, sign in correlators:
        combo = qt.tensor(op_a, op_b)
        value = float(np.real(qt.expect(combo, state)))
        exact += sign * value
        sampled += sign * sample_from_expectation(value, shots_per, rng)

    return {
        "exact": exact,
        "sampled": sampled,
        "classical_limit": plan["classical_limit"],
        "tsirelson_bound": plan["tsirelson_bound"],
    }


def build_axis_operator(axis: Dict[str, Dict[str, float]], selector: str) -> qt.Qobj:
    pair = axis[selector]
    return bloch_operator(pair["theta"], pair["phi"])


def evaluate_five_d(
    axes: List[Dict[str, Dict[str, float]]],
    hyperedges: List[Dict[str, object]],
    target: float,
    shots: int,
    depolarizing: float,
    rng: np.random.Generator,
) -> Dict[str, float]:
    dims = len(axes)
    state = apply_depolarizing(ghz_state_dm(dims), depolarizing)
    exact = 0.0
    sampled = 0.0
    shots_per = max(1, shots // max(len(hyperedges), 1))

    for edge in hyperedges:
        participants: List[int] = edge["participants"]  # type: ignore[assignment]
        basis: List[str] = edge["basis"]  # type: ignore[assignment]
        sign = float(edge["sign"])  # type: ignore[arg-type]
        selector_map = {p: basis[idx] for idx, p in enumerate(participants)}
        operators = []
        for idx in range(dims):
            if idx in selector_map:
                operators.append(build_axis_operator(axes[idx], selector_map[idx]))
            else:
                operators.append(qt.qeye(2))
        combo = qt.tensor(*operators)
        value = float(np.real(qt.expect(combo, state)))
        exact += sign * value
        sampled += sign * sample_from_expectation(value, shots_per, rng)

    projection = project_axes_to_3d(axes)
    return {
        "exact": exact,
        "sampled": sampled,
        "target": target,
        "dims": dims,
        "projected_axes": projection,
    }


def project_axes_to_3d(axes: List[Dict[str, Dict[str, float]]]) -> List[Tuple[float, float, float]]:
    projected = []
    for axis in axes:
        theta = axis["primary"]["theta"]
        phi = axis["primary"]["phi"]
        projected.append(
            (
                math.sin(theta) * math.cos(phi),
                math.sin(theta) * math.sin(phi),
                math.cos(theta),
            )
        )
    return projected


def main() -> None:
    args = parse_args()
    if not args.settings.exists():
        raise SystemExit(f"Settings file not found: {args.settings}")

    payload = json.loads(args.settings.read_text())
    rng = np.random.default_rng(0x5D5D_C45C)

    two_qubit = evaluate_two_qubit(
        payload["two_qubit"],
        args.shots,
        args.depolarizing,
        rng,
    )
    five_d = evaluate_five_d(
        payload["five_d"]["axes"],
        payload["hyperedges"],
        payload["five_d"]["target_violation"],
        args.shots,
        args.depolarizing,
        rng,
    )

    print("=== QRNG-Seeded CHSH Sandbox ===")
    print(f"QRNG epoch: {payload['qrng_epoch']} · seed: {payload['qrng_seed_hex']}...")
    print(f"Tuple ID: {payload['tuple_receipt']['tuple_id']}")
    print()
    print("Two-qubit CHSH:")
    print(f"  Exact S     : {two_qubit['exact']:.4f}")
    print(f"  Sampled S   : {two_qubit['sampled']:.4f} (shots/term ~ {max(1, args.shots // 4)})")
    print(f"  Classical   : {two_qubit['classical_limit']:.4f}")
    print(f"  Tsirelson   : {two_qubit['tsirelson_bound']:.4f}")
    print(
        "  Status      : "
        + ("VIOLATION ✅" if two_qubit["exact"] > two_qubit["classical_limit"] else "classical ✖")
    )
    print()
    print("5D Hypergraph Correlator:")
    print(f"  Exact S_5D  : {five_d['exact']:.4f}")
    print(f"  Sampled S_5D: {five_d['sampled']:.4f} (shots/edge ~ {max(1, args.shots // len(payload['hyperedges']))})")
    print(f"  Target      : {five_d['target']:.4f}")
    print(
        "  Status      : "
        + ("VIOLATION ✅" if five_d["exact"] > five_d["target"] else "below target ✖")
    )
    print()
    print("Projected (5D → 3D) axis preview:")
    for idx, coords in enumerate(five_d["projected_axes"]):
        print(f"  axis-{idx}: ({coords[0]:+.3f}, {coords[1]:+.3f}, {coords[2]:+.3f})")

    result = {
        "two_qubit": two_qubit,
        "five_d": five_d,
        "shots": args.shots,
        "depolarizing": args.depolarizing,
    }
    if args.save:
        args.save.write_text(json.dumps(result, indent=2))
        print(f"\nSaved summary to {args.save}")


if __name__ == "__main__":
    main()
