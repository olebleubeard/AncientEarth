"""Audit the unused height-parameter replay against retained independent DOP853.

This compares finite trajectories; local acceptance estimates are not global
error bounds. It neither runs graphics nor accepts a renderer implementation.
"""
from __future__ import annotations

import hashlib
import json
import math
from pathlib import Path
import struct
import sys


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copied_values(actual, original, name: str, differences: list) -> None:
    """Keep the Rust JSON roundtrip visible instead of silently calling it exact."""
    if isinstance(original, dict):
        assert actual.keys() == original.keys(), name
        for key in original:
            copied_values(actual[key], original[key], f"{name}/{key}", differences)
    elif isinstance(original, list):
        assert len(actual) == len(original), name
        for index, (left, right) in enumerate(zip(actual, original)):
            copied_values(left, right, f"{name}/{index}", differences)
    elif actual != original:
        assert isinstance(actual, float) and isinstance(original, float), name
        assert math.isfinite(actual) and math.isfinite(original)
        bits = lambda value: int.from_bytes(struct.pack(">d", value), "big")
        distance = abs(bits(actual) - bits(original))
        assert distance <= 2, (name, actual, original, distance)
        differences.append({"field": name, "replayed": actual, "original": original,
                            "ulp_distance": distance, "absolute_difference": abs(actual-original)})


def review(path: Path, reference_path: Path) -> dict:
    report = json.loads(path.read_text(encoding="utf-8"))
    references = json.loads(reference_path.read_text(encoding="utf-8"))
    by_name = {case["name"]: case for case in references["cases"]}
    assert len(by_name) == len(references["cases"]) == 7
    assert len(report["cases"]) == 21
    expected = {(name, limit) for name in by_name for limit in (2000, 20000, 100000)}
    rows = []
    input_roundtrips, reference_roundtrips = [], []
    for case in report["cases"]:
        identity = (case["name"], case["maximum_step_m"])
        assert identity in expected, identity
        expected.remove(identity)
        reference_case = by_name[case["name"]]
        # serde_json's compiled parsing/serialization configuration can change
        # the last binary digit. Preserve every difference; comparisons below
        # always use the original independent Python reference values.
        copied_values(case["inputs"], reference_case["inputs"], str(identity), input_roundtrips)
        copied_values(case["reference"], reference_case["reference"], str(identity), reference_roundtrips)
        assert case["error"] is None, case
        actual, reference = case["result"], reference_case["reference"]
        assert actual is not None
        assert actual["height_root_calls"] == 0
        assert actual["accepted_steps"] <= actual["attempts"]
        assert actual["minimum_certified_vertical"] > 0.01
        assert actual["tolerance_m"] == 0.001
        assert 0 <= actual["maximum_local_error_metric_m"] <= actual["tolerance_m"]
        inputs = case["inputs"]
        assert "target_path_m" not in inputs
        assert actual["target_height_m"] == (
            inputs.get("profile_base_height_m", 0) + inputs["layers"][-1]["height_m"]
        )
        errors = {
            "endpoint_error_m": math.dist(actual["endpoint_ecef_m"], reference["endpoint_ecef_m"]),
            "direction_vector_error": math.dist(actual["exit_direction"], reference["exit_direction"]),
            "geometric_path_error_m": abs(actual["geometric_path_m"] - reference["geometric_path_m"]),
            "optical_path_error_m": abs(actual["optical_path_m"] - reference["optical_path_m"]),
        }
        assert all(math.isfinite(value) for value in errors.values())
        rows.append({"name": case["name"], "maximum_path_step_m": case["maximum_step_m"],
                     "attempts": actual["attempts"], "accepted_steps": actual["accepted_steps"],
                     "source_contacts": actual["layer_contacts"],
                     "minimum_certified_vertical": actual["minimum_certified_vertical"],
                     "maximum_local_error_metric_m": actual["maximum_local_error_metric_m"], **errors})
    assert not expected
    maxima = {}
    for limit in (2000, 20000, 100000):
        selected = [row for row in rows if row["maximum_path_step_m"] == limit]
        maxima[str(limit)] = {
            key: max(row[key] for row in selected)
            for key in ("endpoint_error_m", "direction_vector_error", "geometric_path_error_m", "optical_path_error_m")
        }
        maxima[str(limit)]["attempt_range"] = [min(row["attempts"] for row in selected), max(row["attempts"] for row in selected)]
    return {
        "scope": "All 21 unused f64 height-parameter clear trajectories compared with the retained independent DOP853 references at declared profile top. Four independent fixture groups separately test descending vacuum paths, principal curvatures, physical dense contacts, and turning refusal. No GPU, full-scene, global-error or performance acceptance.",
        "report_sha256": digest(path), "reference_sha256": digest(reference_path),
        "source_fingerprints_at_review": {
            str(source): digest(source) for source in (
                Path("scripts/optical_height_parameter.rs"), Path("scripts/optical_height_parameter_review.rs"),
                Path("scripts/optical_normal_coordinates.rs"),
            )
        },
        "resolved_cases": len(rows), "by_maximum_path_step_m": maxima, "cases": rows,
        "input_json_roundtrip_differences": input_roundtrips,
        "embedded_reference_json_roundtrip_differences": reference_roundtrips,
        "roundtrip_scope": "All retained Rust/Python JSON value differences are at most two binary64 ULPs and recorded here; original independent reference numbers are used for the error calculation. This is not bit-identical input decoding across the two languages.",
    }


if __name__ == "__main__":
    report_path = Path(sys.argv[1])
    reference_path = Path(sys.argv[2]) if len(sys.argv) > 2 else Path("artifacts/optical-dop853-reference.json")
    result = review(report_path, reference_path)
    output = report_path.with_name(report_path.stem + "-independent-review.json")
    output.write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"resolved_cases": result["resolved_cases"], "maxima": result["by_maximum_path_step_m"], "output": str(output)}, indent=2))
