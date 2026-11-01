"""Fill only pending numerical matrix references using independent DOP853.

Retained Bouguer/analytic records remain unchanged. Descending line references
are separately checked with 80-digit arithmetic. No GPU execution or acceptance.
"""
from copy import deepcopy
import hashlib
import json
from pathlib import Path

import mpmath as mp

from verify_optical_dop853 import integrate, differences


def main():
    source = Path("artifacts/optical-solver-matrix-input.json")
    original = json.loads(source.read_text(encoding="utf-8"))
    result = deepcopy(original)
    assert len(result["cases"]) == 17
    numeric_checks, analytic_checks = [], []
    retained = json.loads(Path("artifacts/optical-bouguer-reference.json").read_text())
    retained = {case["name"]: case for case in retained["cases"]}
    for case in result["cases"]:
        if case["family"] == "retained_bouguer":
            expected = retained[case["name"]]
            for key, value in case["reference"].items():
                assert value == expected[key]
            if "target_path_m" in expected:
                assert case["stop"] == {"kind": "fixed_path", "path_m": expected["target_path_m"]}
                assert case["inputs"]["target_path_m"] == expected["target_path_m"]
        if case["reference"] is None:
            assert case["family"] == "exact_source_ownership"
            inputs = case["inputs"]
            # These five new poses are exactly authored equatorial source
            # nodes, not nearby values assigned to a knot by a tolerance.
            h = inputs["origin_ecef_m"][0] - 6378137.0
            assert inputs["origin_ecef_m"][1:] == [0., 0.]
            nodes = [row["height_m"] for row in inputs["layers"]]
            assert h in nodes
            expected_owner = nodes.index(h) + (inputs["initial_direction"][0] > 0)
            assert case["initial_owner"] == expected_owner
            assert case["stop"] == {"kind": "fixed_path", "path_m": 100.0}
            coarse, tight = integrate(inputs, False), integrate(inputs, True)
            assert coarse["geometric_path_m"] == tight["geometric_path_m"] == 100.0
            error = differences(coarse, tight)
            assert error["endpoint_m"] < 1e-7 and error["direction"] < 1e-12
            assert error["optical_path_m"] < 1e-7
            case["reference"] = tight
            case["reference_scope"] = "Independent ellipsoid-Lagrange geometry plus one-sided DOP853; exact authored-node ownership; coarse/tight convergence retained, not a universal global error bound."
            numeric_checks.append({"name": case["name"], "convergence": error,
                                   "coarse_reference": coarse})
        if case["family"] == "descending_analytic":
            with mp.workdps(80):
                inputs = case["inputs"]
                u = [mp.mpf(value) for value in inputs["initial_direction"]]
                length = mp.sqrt(mp.fsum(x*x for x in u))
                u = [x/length for x in u]
                r0, r1 = mp.mpf(6380137), mp.mpf(6379137)
                b = r0*u[0]
                c = (r0-r1)*(r0+r1)
                distance = c/(-b+mp.sqrt(b*b-c))
                exact = [r0+u[0]*distance, u[1]*distance, mp.mpf(0)]
                stored = case["reference"]
                point_error = mp.sqrt(mp.fsum((mp.mpf(a)-b)**2 for a,b in zip(stored["endpoint_ecef_m"],exact)))
                path_error = abs(mp.mpf(stored["geometric_path_m"])-distance)
                assert point_error < mp.mpf("1e-8") and path_error < mp.mpf("1e-8")
                analytic_checks.append({"name": case["name"], "endpoint_discrepancy_m": float(point_error),
                                        "path_discrepancy_m": float(path_error),
                                        "path_80_digit_m": mp.nstr(distance, 80)})
    assert len(numeric_checks) == 5 and len(analytic_checks) == 2
    assert all(case["reference"] is not None for case in result["cases"])
    result["independent_review"] = {
        "input_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "new_exact_node_references": numeric_checks, "descending_analytic_checks": analytic_checks,
        "scope": "Seventeen explicit-stop canonical cases. Original retained references are preserved, five missing 100m path references independently integrated, two descending analytic stops checked at80digits. GPU launch quantization requires its own readback-launch reference. No scene, hardware or performance acceptance.",
    }
    output = Path("artifacts/optical-solver-matrix-reference.json")
    output.write_text(json.dumps(result, indent=2, allow_nan=False)+"\n", encoding="utf-8")
    print(json.dumps({"output": str(output), "new_numeric_references": len(numeric_checks),
        "analytic_checks": analytic_checks,
        "maximum_coarse_tight_difference": {key:max(row["convergence"][key] for row in numeric_checks)
            for key in numeric_checks[0]["convergence"]}}, indent=2))


if __name__ == "__main__":
    main()
