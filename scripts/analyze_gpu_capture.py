"""Summarize valid per-process, per-engine PDH intervals inside a capture window.

These are Windows engine-busy counters, not shader execution times, power, or a
sum-able GPU percentage. A persistent query and consecutive samples are required.
See https://learn.microsoft.com/en-us/windows/win32/perfctrs/displaying-performance-data
The default two-second interior guard separates observations from startup/end
and pipelined frame-clock boundaries; it is an explicit QA policy, not clock sync.
"""
from __future__ import annotations
import argparse
from datetime import datetime
import json
import math
from pathlib import Path
import re


def timestamp(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00")).timestamp()


def log_boundary(text, message):
    for line in text.splitlines():
        if message in line:
            found = re.search(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z", line)
            if found:
                return timestamp(found.group())
    return None


def analyze(frame, counters, log="", guard_seconds=2.0):
    if not math.isfinite(guard_seconds) or guard_seconds < 0:
        raise ValueError("Guard must be finite and nonnegative")
    if not str(counters.get("collection", "")).startswith("persistent PDH query"):
        raise ValueError("Fresh-query or unspecified counter collection is not utilization evidence")
    start = frame.get("metadata", {}).get("recording_boundary_observed_unix_seconds")
    end = frame.get("completion_boundary_observed_unix_seconds")
    source = "main-world wall-clock boundary observations"
    if start is None:
        start = log_boundary(log, "Frame capture warmup complete; recording application intervals")
        source = "UTC application log boundaries"
    if end is None:
        end = log_boundary(log, "Saved raw frame capture to")
    if not all(isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)
               for value in (start, end)):
        raise ValueError("Missing finite UTC recording boundaries")
    intervals = frame.get("interval_seconds")
    if not isinstance(intervals, list) or not intervals or any(
            not isinstance(value, (int, float)) or isinstance(value, bool)
            or not math.isfinite(value) or value <= 0 for value in intervals):
        raise ValueError("Application intervals must be finite and positive")
    duration = math.fsum(intervals)
    if not math.isfinite(duration) or duration <= 0:
        raise ValueError("Invalid application interval duration")
    # Do not extend the GPU selection merely because JSON writing/logging took
    # extra time. Neither endpoint is claimed to be a hardware timestamp.
    lower, upper = start + guard_seconds, min(end, start + duration) - guard_seconds
    if upper <= lower:
        raise ValueError("Interior counter window is empty")
    prefix = f"pid_{counters['processId']}_"
    previous = {}
    records = {}
    rejected = {"first_or_discontinuous": 0, "invalid_counter": 0, "outside_window": 0}
    last_poll = None
    for sample in counters["samples"]:
        stop = timestamp(sample["timestampUtc"])
        begin_text = sample.get("intervalStartUtc")
        begin = timestamp(begin_text) if begin_text else None
        if last_poll is not None and stop <= last_poll:
            raise ValueError("Non-increasing counter timestamps")
        current = {}
        for engine in sample["engines"]:
            name = engine["InstanceName"]
            if not name.startswith(prefix):
                continue
            if name in current:
                raise ValueError("Duplicate engine identity within one poll")
            status, value = engine.get("Status"), engine.get("CookedValue")
            valid = isinstance(status, int) and not isinstance(status, bool) and status in (0, 1) and isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value) and value >= 0
            current[name] = valid
            if not valid:
                rejected["invalid_counter"] += 1
                continue
            if begin is None or begin != last_poll or not previous.get(name, False):
                rejected["first_or_discontinuous"] += 1
                continue
            if begin < lower or stop > upper:
                rejected["outside_window"] += 1
                continue
            records.setdefault(name[len(prefix):], []).append((stop - begin, float(value)))
        previous, last_poll = current, stop
    engines = {}
    for name, values in records.items():
        covered = math.fsum(duration for duration, _ in values)
        engines[name] = dict(valid_intervals=len(values), covered_seconds=covered,
                             window_coverage_fraction=covered / (upper - lower),
                             mean_busy_percent=math.fsum(duration * value for duration, value in values) / covered,
                             maximum_interval_busy_percent=max(value for _, value in values),
                             above_nominal_100_percent_intervals=sum(value > 100 for _, value in values),
                             zero_intervals=sum(value == 0 for _, value in values))
    return dict(measurement="per-process per-engine Windows busy counters; never summed across engines",
                boundary_source=source, interior_guard_seconds=guard_seconds,
                window_start_unix_seconds=lower, window_end_unix_seconds=upper,
                selected_window_seconds=upper - lower, rejected_intervals=rejected,
                counter_errors=counters.get("counterErrors", []), engines=engines)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("frame", type=Path)
    parser.add_argument("counters", type=Path)
    parser.add_argument("--log", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--guard-seconds", type=float, default=2.0)
    args = parser.parse_args()
    result = analyze(json.loads(args.frame.read_text(encoding="utf-8-sig")),
                     json.loads(args.counters.read_text(encoding="utf-8-sig")),
                     args.log.read_text(encoding="utf-8-sig") if args.log else "", args.guard_seconds)
    text = json.dumps(result, indent=2) + "\n"
    if args.output:
        args.output.write_text(text)
    print(text, end="")
