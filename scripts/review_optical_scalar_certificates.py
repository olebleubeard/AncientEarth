"""Compare encoded-coordinate certificates with independent SPD references.

References are produced by verify_optical_scalar_geometry.py. A skew input is
compared only with an explicitly recorded symmetric-part reference; the script
checks that transformation and retains its magnitude. This does not certify
GPU bound arithmetic, a globally coherent query field, or physical accuracy.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import mpmath as mp


def review(source: Path, references: list[Path]) -> dict:
    raw = json.loads(source.read_bytes())
    by_name = {}
    for path in references:
        for row in json.loads(path.read_bytes())["cases"]:
            if row["name"] in by_name:
                raise ValueError(f"Duplicate reference: {row['name']}")
            by_name[row["name"]] = row
    rows = []
    with mp.workdps(120):
        for case in raw["cases"]:
            if case["name"] not in by_name:
                continue
            row = by_name.pop(case["name"])
            inputs, reference = row["inputs"], row["reference"]
            a = mp.matrix(case["matrix_rows"])
            symmetric = (a + a.T) / 2
            if mp.matrix(inputs["matrix_rows"]) != symmetric:
                raise ValueError("Reference matrix is not the exact symmetric part")
            if mp.matrix(inputs["q"]) != mp.matrix(case["q"]) or mp.mpf(inputs["f0"]) != mp.mpf(case["f0"]):
                raise ValueError("Reference query differs from encoded input")
            skew = max(sum(abs((a[i, j] - a[j, i]) / 2) for j in range(3)) for i in range(3))
            h, n = mp.mpf(reference["height_m"]), mp.matrix(reference["normal"])
            errors = {}
            for label in ("candidate", "legacy"):
                value = case[label]
                errors[label] = {
                    "height_error_m": float(abs(mp.mpf(value["height"]) - h)),
                    "normal_error": float(mp.norm(mp.matrix(value["normal"]) - n)),
                }
            certificate = case["encoded_certificate"]
            height_error = abs(mp.mpf(case["candidate"]["height"]) - h)
            normal_error = mp.norm(mp.matrix(case["candidate"]["normal"]) - n)
            passed = height_error <= mp.mpf(certificate["height_error"]) and normal_error <= mp.mpf(certificate["normal_error"])
            rows.append({"name": case["name"], "explicit_symmetrization": a != a.T,
                         "skew_infinity_norm": float(skew), **errors,
                         "certificate": certificate, "passed": passed})
    if by_name:
        raise ValueError("Reference contains a query absent from the source")
    if not rows:
        raise ValueError("No matched reference queries")
    return {
        "scope": __doc__.strip(),
        "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
        "reference_sha256": {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in references},
        "case_count": len(rows),
        "symmetric_count": sum(not x["explicit_symmetrization"] for x in rows),
        "explicit_symmetric_part_count": sum(x["explicit_symmetrization"] for x in rows),
        "maximum_errors": {label: {key: max(x[label][key] for x in rows) for key in ("height_error_m", "normal_error")} for label in ("candidate", "legacy")},
        "maximum_bound_ratios": {
            "height": max(x["candidate"]["height_error_m"] / x["certificate"]["height_error"] for x in rows),
            "normal": max(x["candidate"]["normal_error"] / x["certificate"]["normal_error"] for x in rows),
        },
        "passed": all(x["passed"] for x in rows), "cases": rows,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--reference", type=Path, action="append", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    result = review(args.input, args.reference)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "cases"}, indent=2))
    if not result["passed"]:
        raise SystemExit("Certificate failure retained in output")


if __name__ == "__main__":
    main()
