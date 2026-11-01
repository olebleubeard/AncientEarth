"""Recompute full-frame work distributions from retained integer GPU buffers.

These are counted operations on the selected diagnostic shader, not timings,
universal complexity bounds, or physical accuracy evidence.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

import numpy as np


def review(prefix: Path) -> dict:
    report = json.loads(prefix.with_suffix(".json").read_bytes())
    sources = json.loads(Path(str(prefix) + "-sources.json").read_text(encoding="utf-8-sig"))
    outputs = json.loads(Path(str(prefix) + "-outputs.json").read_text(encoding="utf-8-sig"))
    for entry in [*sources["sources"], *outputs]:
        if hashlib.sha256(Path(entry["path"]).read_bytes()).hexdigest() != entry["sha256"]:
            raise ValueError(f"Changed retained source/output: {entry['path']}")
    retained = np.fromfile(str(prefix) + "-retained.bin", dtype="<u4").reshape(-1, 12)
    instrumented = np.fromfile(str(prefix) + "-instrumented.bin", dtype="<u4").reshape(-1, 12)
    counters = np.fromfile(str(prefix) + "-counts.bin", dtype="<u4").reshape(-1, 4)
    if retained.shape != (1920 * 1080, 12) or instrumented.shape != retained.shape or counters.shape != (len(retained), 4):
        raise ValueError("Unexpected native-pixel buffer size")
    differing = np.flatnonzero(np.any(retained != instrumented, axis=1))
    if len(differing) != report["terminal_difference_pixels"]:
        raise ValueError("Stored terminal difference count disagrees")
    status = instrumented[:, 7]
    if np.any(status > 3):
        raise ValueError("Invalid terminal status")
    results = {}
    for index, label in enumerate(("sky", "surface", "unresolved", "aperture")):
        data = counters[status == index]
        if len(data) != report["status_counts"][index]:
            raise ValueError("Stored status count disagrees")
        rows = []
        for k, name in enumerate(("attempts", "dp_evaluations", "inverse_evaluations", "face_events")):
            unique, frequency = np.unique(data[:, k], return_counts=True)
            histogram = {str(int(v)): int(n) for v, n in zip(unique, frequency)}
            stored = report["counter_summaries_by_status"][index][k]
            if histogram != stored["histogram"]:
                raise ValueError("Stored work histogram disagrees")
            ordered = np.sort(data[:, k])
            nearest = lambda num, den: int(ordered[(len(ordered) * num + den - 1) // den - 1]) if len(ordered) else 0
            row = {"counter": name, "count": len(data), "sum": int(data[:, k].sum(dtype=np.uint64)),
                   "p50_nearest_rank": nearest(1, 2), "p90_nearest_rank": nearest(9, 10),
                   "p99_nearest_rank": nearest(99, 100), "p999_nearest_rank": nearest(999, 1000),
                   "maximum": int(ordered[-1]) if len(ordered) else 0}
            for actual, field in (("sum", "sum"), ("p50_nearest_rank", "median"),
                                  ("p90_nearest_rank", "p90"), ("p99_nearest_rank", "p99"),
                                  ("p999_nearest_rank", "p999"), ("maximum", "max")):
                if row[actual] != stored[field]:
                    raise ValueError("Stored work statistic disagrees")
            rows.append(row)
        results[label] = rows
    exhausted = np.flatnonzero(counters[:, 0] >= 4096)
    if len(exhausted) != len(report["attempt_budget_exhausted"]):
        raise ValueError("Stored budget exhaustion count disagrees")
    return {"scope": __doc__.strip(), "source_and_output_hashes_match": True,
            "native_pixel_count": len(retained), "terminal_difference_pixels": len(differing),
            "maximum_stored_counter": int(counters.max()),
            "counter_scope": "Exact stored u32 counts; the harness is finite and does not prove that arbitrary inputs cannot overflow counters.",
            "attempt_budget_exhausted_pixels": [[int(i % 1920), int(i // 1920)] for i in exhausted],
            "statistics": results}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    args = parser.parse_args()
    result = review(args.prefix)
    Path(str(args.prefix) + "-independent-review.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
