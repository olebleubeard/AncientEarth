"""Audit actual GPU scalar-query certificates against independent SPD roots.

The exact detailed GPU query is the reference input. Tiny matrix skew is
explicitly symmetrized, not silently removed. This is a finite per-query audit,
not a certificate of a trajectory, GPU enclosure arithmetic, or speedup.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct

from review_optical_scalar_certificates import review


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--enclosures", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    inputs = json.loads(args.input.read_bytes())
    enclosures = json.loads(args.enclosures.read_bytes())
    references = json.loads(args.reference.read_bytes())
    if not (len(inputs["cases"]) == len(enclosures["cases"]) == len(references["cases"])):
        raise ValueError("GPU input, enclosure and reference counts differ")
    cases = []
    serialized_f64_differences = 0
    def same_f32(left, right):
        nonlocal serialized_f64_differences
        if len(left) != len(right):
            return False
        serialized_f64_differences += sum(a != b for a, b in zip(left, right))
        return all(struct.pack("<f", a) == struct.pack("<f", b) for a, b in zip(left, right))
    for index, (query, enclosure) in enumerate(zip(inputs["cases"], enclosures["cases"])):
        if query["name"] != str(index):
            raise ValueError("GPU query order is not the original contiguous order")
        if not same_f32([*query["q"], query["f0"]], enclosure["gpu_query"]):
            raise ValueError("Enclosure uses another GPU query")
        for label in ("candidate", "legacy"):
            if not same_f32([*query[label]["normal"], query[label]["height"]], enclosure[label]):
                raise ValueError("Enclosure uses another recorded output")
        cases.append({**query, "matrix_rows": query["original_encoded_matrix_rows"],
                      "encoded_certificate": enclosure["encoded_certificate"]})
    adapter = args.output.with_name(args.output.stem + "-inputs.json")
    adapter.write_text(json.dumps({"scope": __doc__.strip(), "cases": cases}, indent=2) + "\n")
    result = review(adapter, [args.reference])
    if result["case_count"] != len(cases):
        raise ValueError("Not every GPU query was reviewed")
    for row, enclosure in zip(result["cases"], enclosures["cases"]):
        rounded = enclosure["rounded_enclosure"]
        row["rounded_enclosure"] = rounded
        row["guard_accepts"] = enclosure["guard_accepts"]
        row["rounded_passed"] = (
            row["candidate"]["height_error_m"] <= rounded["height_error"]
            and row["candidate"]["normal_error"] <= rounded["normal_error"]
        )
    result["scope"] = __doc__.strip()
    result["original_inputs_sha256"] = hashlib.sha256(args.input.read_bytes()).hexdigest()
    result["enclosures_sha256"] = hashlib.sha256(args.enclosures.read_bytes()).hexdigest()
    result["serialized_f64_differences_with_identical_f32_bits"] = serialized_f64_differences
    result["serialization_scope"] = "Enclosure JSON decimal roundtrips differ in some binary64 low bits. All query/output components must recover identical binary32 bits; the reference and errors use the original raw-GPU binary32 values. This does not replace the enclosure's arithmetic audit."
    result["guard_accepts"] = sum(row["guard_accepts"] for row in result["cases"])
    result["guard_fallback"] = len(cases) - result["guard_accepts"]
    result["rounded_passed"] = all(row["rounded_passed"] for row in result["cases"])
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({k: v for k, v in result.items() if k != "cases"}, indent=2))
    if not (result["passed"] and result["rounded_passed"]):
        raise SystemExit("Finite certificate failure retained in output")


if __name__ == "__main__":
    main()
