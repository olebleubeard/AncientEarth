"""Independent raw-buffer and exact-tick audit of a stellar consumer probe."""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
from decimal import Decimal
import hashlib
import json
from pathlib import Path
from statistics import median

import numpy as np


def chain_statistics(data: dict) -> dict:
    cached_names = ["clear_stellar_requests", "gather_stellar_requests", "requested_stellar_table", "requested_stellar_pixels"]
    previous = -1
    rows = []
    medians = defaultdict(list)
    for index, sample in enumerate(data["samples"]):
        assert sample["iteration"] == index and sample["warmup"] == (index < 3)
        assert sample["direct_first"] == (index % 2 == 0)
        names = [x["stage"] for x in sample["stages"]]
        expected = ["direct_stellar_pixels"] + cached_names if sample["direct_first"] else cached_names + ["direct_stellar_pixels"]
        assert names == expected
        elapsed = {}
        by_name = {}
        periods = set()
        for stage in sample["stages"]:
            begin, end = stage["raw_timestamps"]
            assert begin >= previous and end > begin
            previous = end
            period = Decimal(str(stage["timestamp_period_ns"])) / Decimal(10**9)
            assert period > 0
            periods.add(period)
            by_name[stage["stage"]] = (begin, end)
            elapsed[stage["stage"]] = Decimal(end - begin) * period
        assert len(periods) == 1
        period = periods.pop()
        values = {
            "original": elapsed["direct_stellar_pixels"],
            "cached_stage_sum": sum(elapsed[name] for name in cached_names),
            "cached_span": Decimal(by_name[cached_names[-1]][1] - by_name[cached_names[0]][0]) * period,
        }
        rows.append({"iteration": index, "warmup": sample["warmup"], "direct_first": sample["direct_first"], "seconds": {key: str(value) for key, value in values.items()}})
        if not sample["warmup"]:
            for key, value in values.items():
                medians[key].append(value)
    assert len(rows) == 8 and all(len(values) == 5 for values in medians.values())
    return {"five_sample_medians_seconds": {key: str(median(values)) for key, values in medians.items()}, "iterations": rows}


def review(prefix: Path) -> dict:
    def path(suffix):
        return Path(str(prefix) + suffix)

    data = json.loads(path(".json").read_text())
    for item in json.loads(path("-outputs.json").read_text()):
        assert hashlib.sha256(Path(item["path"]).read_bytes()).hexdigest() == item["sha256"]
    arrivals = np.fromfile(path("-arrivals.bin"), dtype="<u4").reshape(-1, 4)
    requests = np.fromfile(path("-requests.bin"), dtype="<u4")
    assert arrivals.shape == (3001, 4)
    assert arrivals[0].tolist() == data["control"]
    assert set(map(int, requests[:3000])) <= {0, 1}
    reference = json.loads(Path("artifacts/optical-stellar-table-probe.json").read_text())
    for i, row in enumerate(arrivals[1:]):
        original = reference["arrivals"][i]
        assert original["stable_id"] == i
        prior = original["raw_words"]
        assert row.tolist() == data["records"][i]["raw_words"]
        if requests[i] == 0:
            assert row.tolist() == [0, 0, 0, 0]
        else:
            assert row[:3].tolist() == prior[:3]
            assert row[3] == (2 if prior[3] == 0x3F800000 else 1)
    queue = data.get("compact_queue")
    if queue:
        count = int(requests[3000])
        offsets = requests[3001:3001 + count]
        catalogue = np.fromfile(path("-catalogue.bin"), dtype="<f4").reshape(-1, 4)
        assert count == int(requests[:3000].sum()) == queue["count"]
        assert offsets.tolist() == queue["packed_catalogue_offsets"]
        assert len(set(map(int, offsets))) == count
        ids = []
        for offset in offsets:
            assert 6144 <= offset < len(catalogue) - 1 and (offset - 6144) % 2 == 0
            value = float(catalogue[int(offset) + 1, 3])
            assert value.is_integer()
            ids.append(int(value))
        assert sorted(ids) == np.flatnonzero(requests[:3000]).tolist() == queue["unique_ids"]
    consumers = np.fromfile(path("-consumer.bin"), dtype="<u4").reshape(3000, 12)
    assert np.array_equal(consumers[:, :4], consumers[:, 4:8])
    assert set(map(int, consumers[:, 11])) == {0, 1, 2}
    for suffix in ("-consumer-wrong-arrival-header.bin", "-consumer-wrong-primary-header.bin"):
        wrong = np.fromfile(path(suffix), dtype="<u4").reshape(3000, 12)
        assert np.all(wrong[:, :8] == 0xFFFFFFFF)
    direct = np.fromfile(path("-direct-pixels.bin"), dtype="<u4").reshape(-1, 4)
    cached = np.fromfile(path("-requested-pixels.bin"), dtype="<u4").reshape(-1, 4)
    assert direct.shape == (1920 * 1080, 4) and np.array_equal(direct, cached)
    colors = direct.view("<f4")
    assert np.isfinite(colors).all()
    lit = int(np.any(colors[:, :3] != 0, axis=1).sum())
    assert lit > 0, "An all-black result does not exercise stellar radiance"
    duration = defaultdict(list)
    previous = -1
    for row in data["timestamp_samples"]:
        values = []
        for stage in row["stages"]:
            begin, end = stage["raw_timestamps"]
            assert begin >= previous and end > begin
            previous = end
            seconds = Decimal(end - begin) * Decimal(str(stage["timestamp_period_ns"])) / Decimal(10**9)
            assert abs(seconds - Decimal(str(stage["gpu_seconds"]))) < Decimal("1e-15")
            values.append(seconds)
            if not row["warmup"]:
                duration[stage["stage"]].append(seconds)
        if not row["warmup"]:
            duration["sum_of_stages"].append(sum(values))
            period = Decimal(str(row["stages"][0]["timestamp_period_ns"]))
            duration["first_begin_to_last_end"].append(Decimal(row["stages"][-1]["raw_timestamps"][1] - row["stages"][0]["raw_timestamps"][0]) * period / Decimal(10**9))
    assert all(len(values) == 5 for values in duration.values())
    check = data["consumer_check"]
    ticks = check["pixel_warmup_raw_timestamps"]
    assert ticks[0] < ticks[1] <= ticks[2] < ticks[3]
    period = Decimal(str(check["timestamp_period_ns"])) / Decimal(10**9)
    chain = None
    if check.get("chain_timing"):
        chain = chain_statistics(check["chain_timing"])
        final_direct = np.fromfile(path("-chain-direct-pixels.bin"), dtype="<u4").reshape(-1, 4)
        final_cached = np.fromfile(path("-chain-requested-pixels.bin"), dtype="<u4").reshape(-1, 4)
        final_arrivals = np.fromfile(path("-chain-arrivals.bin"), dtype="<u4").reshape(-1, 4)
        assert np.array_equal(final_direct, direct) and np.array_equal(final_cached, direct)
        assert np.array_equal(final_arrivals, arrivals)
        chain["final_raw_pixels_and_arrivals_unchanged"] = True
        chain["scope"] = "Alternating whole-path order, original pixel composition versus full clear/gather/solve/cached-pixel chain. Same fixed primary terminals. Final stored buffers checked; no per-iteration output readback, primary-trace cost, native-frame or changing-sky claim."
    return {
        "scope": "Independent raw bytes, current headers, all arrival states, nonzero pixel radiance and exact timestamp conversion. Fixed identity-rotation/no-body fixture; single pixel pass is warmup evidence, not native throughput or steady-state savings.",
        "report_sha256": hashlib.sha256(path(".json").read_bytes()).hexdigest(),
        "arrival_states": dict(Counter(map(int, arrivals[1:, 3]))),
        "consumer_states": dict(Counter(map(int, consumers[:, 11]))),
        "requested_ids": int(requests[:3000].sum()),
        "compact_queue_checked": bool(queue),
        "consumer_return_differences": 0, "both_wrong_headers_all_pending": True,
        "equal_pixels": len(direct), "nonzero_rgb_pixels": lit,
        "five_sample_stage_medians_seconds": {key: str(median(values)) for key, values in duration.items()},
        "single_pixel_warmup_seconds": {"original": str(Decimal(ticks[1] - ticks[0]) * period), "requested": str(Decimal(ticks[3] - ticks[2]) * period)},
        "repeated_chain": chain,
        "passed": True,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    args = parser.parse_args()
    result = review(args.prefix)
    Path(str(args.prefix) + "-independent-review.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
