"""Prepare an unused, explicit-stop canonical numerical acceptance matrix.

No GPU or reference integration occurs here. The retained independent Bouguer
numbers stay unchanged; new initial states and analytic descending stops are
separately reviewed before a shader consumes them.
"""
from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import sys

A = 6_378_137.0
ROOT = Path(__file__).resolve().parents[1]


def owner_at_exact_height(height, knots, vertical):
    """Explicit one-sided ownership at a known node, no proximity selection."""
    index = knots.index(height)
    return index + (vertical >= 0)


def prepare():
    original_path = ROOT / "artifacts/optical-bouguer-reference.json"
    original = json.loads(original_path.read_text())
    adapted_path = ROOT / "artifacts/optical-normal-bouguer-input.json"
    adapted = json.loads(adapted_path.read_text())
    cases = []
    for row in original["cases"]:
        inputs = {key: row[key] for key in (
            "origin_ecef_m", "initial_direction", "layers", "profile_base_height_m"
        )}
        if "target_path_m" in row:
            inputs["target_path_m"] = row["target_path_m"]
            stop = {"kind": "fixed_path", "path_m": row["target_path_m"]}
        else:
            stop = {"kind": "profile_top"}
        reference = {key: row[key] for key in (
            "endpoint_ecef_m", "exit_direction", "geometric_path_m", "optical_path_m"
        )}
        cases.append({"name": row["name"], "family": "retained_bouguer", "inputs": inputs,
                      "stop": stop, "reference": reference,
                      "reference_scope": "Original retained high-precision Bouguer quadrature."})
    tangent = next(row for row in adapted["cases"] if row["name"] == "vacuum tangent exactly on source knot")
    inputs = dict(tangent["inputs"], target_path_m=tangent["reference"]["geometric_path_m"])
    cases.append({"name": tangent["name"], "family": "retained_analytic",
                  "inputs": inputs, "initial_owner": 1,
                  "stop": {"kind": "fixed_path", "path_m": inputs["target_path_m"]},
                  "reference": tangent["reference"], "reference_scope": "Retained exact straight tangent fixture."})

    # The six exact boundary poses are deliberately source-side tests, not six
    # nearby heights silently snapped onto a knot. Their non-vacuum references
    # must be generated independently from these retained inputs.
    knots = [0.0, 1000.0, 3000.0]
    layers = [{"height_m": h, "refractivity": n} for h, n in zip(knots, [0.0003, 0.0002, 0.0])]
    for node, height in enumerate(knots):
        for vertical, label in ((0.5, "up"), (-0.5, "down")):
            inputs = {"origin_ecef_m": [A + height, 0.0, 0.0],
                      "initial_direction": [vertical, math.sqrt(1.0 - vertical * vertical), 0.0],
                      "layers": layers, "profile_base_height_m": 0.0}
            top_exit = node == len(knots) - 1 and vertical > 0
            if not top_exit:
                inputs["target_path_m"] = 100.0
            reference = None
            if top_exit:
                reference = {"endpoint_ecef_m": inputs["origin_ecef_m"],
                             "exit_direction": inputs["initial_direction"],
                             "geometric_path_m": 0.0, "optical_path_m": 0.0}
            cases.append({"name": f"exact source node {node} {label}",
                          "family": "exact_source_ownership", "inputs": inputs,
                          "initial_owner": owner_at_exact_height(height, knots, vertical),
                          "stop": {"kind": "profile_top"} if top_exit else {"kind": "fixed_path", "path_m": 100.0},
                          "reference": reference,
                          "reference_scope": "Immediate top exit" if top_exit else "Pending independent one-sided integration; null is not acceptance."})

    # Positive first intersection of an exact equatorial vacuum line with the
    # lower concentric circle. Rationalization avoids cancellation in the near
    # root. Directions are the stored binary64 values; normalize before use.
    for vertical in (-0.1, -0.9):
        direction = [vertical, math.sqrt(1.0 - vertical * vertical), 0.0]
        length = math.sqrt(sum(x * x for x in direction))
        direction = [x / length for x in direction]
        r0, r1 = A + 2000.0, A + 1000.0
        b = r0 * direction[0]
        c = (r0 - r1) * (r0 + r1)
        discriminant = b * b - c
        assert discriminant > 0
        distance = c / (-b + math.sqrt(discriminant))
        inputs = {"origin_ecef_m": [r0, 0.0, 0.0], "initial_direction": direction,
                  "layers": [{"height_m": 0.0, "refractivity": 0.0},
                             {"height_m": 120000.0, "refractivity": 0.0}],
                  "profile_base_height_m": 0.0, "target_path_m": distance}
        reference = {"endpoint_ecef_m": [r0 + direction[0] * distance, direction[1] * distance, 0.0],
                     "exit_direction": direction, "geometric_path_m": distance, "optical_path_m": distance}
        cases.append({"name": f"vacuum descending vertical {vertical}", "family": "descending_analytic",
                      "inputs": inputs, "initial_owner": 1,
                      "stop": {"kind": "fixed_path", "path_m": distance},
                      "reference": reference, "reference_scope": "First positive equatorial line/circle intersection at h=1000m; binary64 preparation, independently check at higher precision."})
    assert len(cases) == 17 and len({case["name"] for case in cases}) == 17
    return {
        "schema": "ancientearth.optical-numerical-matrix.diagnostic.v1",
        "scope": "Unused canonical inputs, explicit stopping semantics and retained/analytic references. No GPU result, scene acceptance or performance assertion. Fixed-path targets preserve returning trajectories; they must not be replaced by a profile-top stop. The six ownership cases need exact one-sided initialization. Near tangent/turning states retain the physical arclength solver.",
        "source_sha256": {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in (original_path, adapted_path)},
        "terminal_contract": "Retain the complete current per-ray raw record, actual normalized launch bits and source/profile packet. Report fixed-path completion distinctly from atmosphere exit and finite-budget failure; a failed prefix is never compared with a final target as a resolved ray.",
        "cases": cases,
    }


if __name__ == "__main__":
    destination = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "artifacts/optical-solver-matrix-input.json"
    destination.write_text(json.dumps(prepare(), indent=2, allow_nan=False) + "\n")
    print(f"Prepared 17 unused canonical cases: {destination}")
