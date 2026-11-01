"""Summarize raw native application frame pacing; no GPU-time inference."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
from decimal import Decimal
import json
import math
from pathlib import Path
import re
import statistics
from typing import Any


def interval_statistics(intervals: list[float]) -> dict[str, Any]:
    """Statistics of already validated, nonempty, positive finite intervals."""
    count = len(intervals)
    duration = math.fsum(intervals)
    if not math.isfinite(duration):
        raise ValueError("recorded duration overflows")
    ordered = sorted(intervals)
    # Nearest-rank percentile: ceil(p*n), using exact integer ranks.
    percentile = lambda p: ordered[(p * count + 99) // 100 - 1] * 1000.0
    throughput = count / duration
    timing = {
        "minimum": ordered[0] * 1000.0,
        "mean": duration / count * 1000.0,
        "median": statistics.median(ordered) * 1000.0,
        "p95_nearest_rank": percentile(95),
        "p99_nearest_rank": percentile(99),
        "maximum": ordered[-1] * 1000.0,
    }
    if not math.isfinite(throughput) or any(not math.isfinite(v) for v in timing.values()):
        raise ValueError("derived frame statistics overflow")
    return {
        "sample_count": count,
        "recorded_duration_seconds": duration,
        "throughput_frames_per_second": throughput,
        "frame_interval_ms": timing,
    }


def view_phase_statistics(qa: Any, intervals: list[float], *, continuous: bool = False,
                          optical: bool = False) -> list[dict[str, Any]] | None:
    """Recompute phase statistics; never trust the recorder's derived summary.

    Every raw interval must appear once, in the same order, including recovery
    and the initial boundary frame. The phase labels refer to the prior action.
    """
    if qa is None:
        return None
    if not isinstance(qa, dict) or not isinstance(qa.get("frames"), list):
        raise ValueError("view_motion_qa requires its raw frames")
    frames = qa["frames"]
    if len(frames) != len(intervals):
        raise ValueError("view-motion frames do not cover all recorded intervals")
    by_phase: dict[str, list[float]] = {}
    allowed = ({"warmup_boundary", "ascent", "forward_pan", "slide_zoom_in", "slide_zoom_out", "recovery", "complete"}
               if optical else {"warmup_boundary", "ascent", "far_settle", "descent", "near_settle", "complete"}
               if continuous else {"warmup_boundary", "zoom_in", "zoom_out", "ascent", "recovery", "complete"})
    for frame, interval in zip(frames, intervals):
        if not isinstance(frame, dict):
            raise ValueError("view-motion frame must be an object")
        phase = frame.get("phase")
        value = frame.get("interval_seconds")
        if not isinstance(phase, str) or phase not in allowed:
            raise ValueError("unknown view-motion phase")
        if isinstance(value, bool) or not isinstance(value, (int, float)) or value != interval:
            raise ValueError("view-motion intervals disagree with the raw capture")
        by_phase.setdefault(phase, []).append(interval)
    return [{"phase": phase, **interval_statistics(values)} for phase, values in by_phase.items()]


def optical_phase_statistics(qa: Any, intervals: list[float]) -> tuple[Any, Any]:
    """Retain all intervals, including failed readiness/focus, for optical flight.

    Recorder pass flags and derived counters are not substitutes for raw frame
    evidence. Validate declared counters against the flags and recompute both
    overall and per-phase counts without filtering the timing samples.
    """
    phases = view_phase_statistics(qa, intervals, optical=True)
    if phases is None:
        return None, None
    fields = {
        "optical_active": "inactive_optical_intervals",
        "optical_ready": "optical_pending_intervals",
        "presentation_ready": "presentation_pending_intervals",
        "render_matches_observer": "stale_render_intervals",
        "focused": "unfocused_intervals",
    }
    totals = dict.fromkeys(fields.values(), 0)
    by_phase = {phase["phase"]: dict.fromkeys(fields.values(), 0) for phase in phases}
    for frame in qa["frames"]:
        for flag, counter in fields.items():
            if type(frame.get(flag)) is not bool:
                raise ValueError(f"optical-motion {flag} must be a Boolean")
            if not frame[flag]:
                totals[counter] += 1
                by_phase[frame["phase"]][counter] += 1
    for counter, total in totals.items():
        if type(qa.get(counter)) is not int or qa[counter] != total:
            raise ValueError(f"optical-motion {counter} disagrees with raw flags")
    for phase in phases:
        phase["state_interval_counts"] = by_phase[phase["phase"]]
    return phases, totals


def optical_header_statistics(readback: Any, qa: Any) -> dict[str, Any] | None:
    """Associate producer evidence by all four exact u32 words, never by order.

    Requests include warmup and may outlive the recording boundary. A matching
    header proves that producer packet completed; it says nothing about fragment
    consumption or presentation. Missing/duplicate requests remain explicit.
    """
    frames = qa.get("frames", []) if isinstance(qa, dict) else []
    if readback is None and not any("submitted_compute_control" in f for f in frames):
        return None

    def header(value: Any, *, requested: bool) -> tuple[int, int, int, int]:
        if (not isinstance(value, list) or len(value) != 4
                or any(type(v) is not int or not 0 <= v <= 0xffffffff for v in value)):
            raise ValueError("optical compute header must contain four u32 integers")
        if requested and (value[0] == 0 or value[1] == 0):
            raise ValueError("requested compute extent must be positive")
        return tuple(value)

    requests = []
    if readback is not None:
        if not isinstance(readback, dict) or not isinstance(readback.get("requests"), list):
            raise ValueError("optical compute readback requires its raw requests")
        requests = readback["requests"]
    counts = dict.fromkeys(("exact_match", "mismatch", "pending", "error"), 0)
    by_header: dict[tuple[int, int, int, int], list[int]] = {}
    states = []
    for i, request in enumerate(requests):
        if not isinstance(request, dict):
            raise ValueError("compute request must be an object")
        wanted = header(request.get("requested_extent_and_token"), requested=True)
        value = request.get("returned_extent_and_token")
        returned = None if value is None else header(value, requested=False)
        match = None if returned is None else wanted == returned
        if request.get("exact_match") is not match:
            raise ValueError("compute exact_match disagrees with raw header words")
        error = request.get("error")
        if error is not None and not isinstance(error, str):
            raise ValueError("compute readback error must be text or null")
        state = ("error" if error is not None else "pending" if returned is None
                 else "exact_match" if match else "mismatch")
        counts[state] += 1
        states.append(state)
        by_header.setdefault(wanted, []).append(i)

    associations = []
    used = set()
    association_counts: dict[str, int] = {}
    for i, frame in enumerate(frames):
        indices = []
        if "submitted_compute_control" not in frame:
            state = "legacy_token_unavailable"
        elif frame["submitted_compute_control"] is None:
            state = "no_submitted_packet"
        else:
            wanted = header(frame["submitted_compute_control"], requested=True)
            if "physical_size" in frame and list(wanted[:2]) != frame["physical_size"]:
                raise ValueError("submitted compute extent disagrees with motion viewport")
            indices = by_header.get(wanted, [])
            used.update(indices)
            state = ("readback_not_enabled" if readback is None else "not_requested"
                     if not indices else "ambiguous_requests" if len(indices) > 1
                     else states[indices[0]])
        association_counts[state] = association_counts.get(state, 0) + 1
        associations.append({"frame_index": i, "phase": frame["phase"],
                             "state": state, "request_indices": indices})
    return {
        "scope": "Exact producer header association only; not fragment consumption, presentation, GPU duration, or an inferred per-frame completion time. Unassociated requests include warmup and possible boundary requests; their timing is not inferred.",
        "readback_enabled": readback is not None,
        "request_count": len(requests), "request_counts": counts,
        "frame_association_counts": association_counts,
        "frame_associations": associations,
        "unassociated_request_indices": [i for i in range(len(requests)) if i not in used],
    }


def optical_stellar_header_statistics(readback: Any, qa: Any) -> dict[str, Any] | None:
    """Keep stellar producer evidence separate, using the compositor's token."""
    frames = qa.get("frames", []) if isinstance(qa, dict) else []
    associated = []
    for frame in frames:
        mapped = dict(frame)
        mapped.pop("submitted_compute_control", None)
        if "expected_stellar_control" in frame:
            expected = frame["expected_stellar_control"]
            if expected is not None and expected != frame.get("submitted_compute_control"):
                raise ValueError("expected stellar token differs from current primary submission")
            mapped["submitted_compute_control"] = expected
        associated.append(mapped)
    stellar_readback = None
    if readback is not None:
        if not isinstance(readback, dict):
            raise ValueError("optical compute readback must be an object")
        if "stellar_requests" in readback:
            stellar_readback = {"requests": readback["stellar_requests"]}
    result = optical_header_statistics(stellar_readback, {"frames": associated})
    if result is None:
        return None
    labels = {"no_submitted_packet": "table_not_selected",
              "legacy_token_unavailable": "legacy_table_selection_unavailable"}
    result["frame_association_counts"] = {labels.get(k, k): v for k, v in result["frame_association_counts"].items()}
    for row in result["frame_associations"]:
        row["state"] = labels.get(row["state"], row["state"])
    result["scope"] = "Exact stellar producer header association by the compositor's expected token, which must equal the current primary submission. Expected selection alone is not completion. Matching readback does not prove fragment consumption, presentation, or per-frame completion time. Warmup, unselected-table requests, mismatches and missing responses remain explicit."
    return result


def external_foreground_statistics(report: dict[str, Any], launch: Any) -> dict[str, Any]:
    """Associate sampled OS observations with the nominal recording window.

    Exact decimal fractions preserve sub-microsecond boundary cases. A wall
    clock observation is not an interval-wide focus certificate or a GPU fence.
    """
    qa = report.get("optical_motion_qa")
    unfocused_policy = qa.get("unfocused_diagnostic_policy", False) if isinstance(qa, dict) else False
    if type(unfocused_policy) is not bool:
        raise ValueError("unfocused diagnostic policy must be Boolean")
    result = {
        "scope": "Sampled OS foreground evidence over the nominal application recording window, not continuous focus between samples, physical input delivery, or GPU timing. Scheduling alignment near recording boundaries is not hardware synchronized; exact boundary samples remain separate.",
        "eligible_for_focused_timing": False,
        "application_unfocused_recorded_frames": report.get("unfocused_recorded_frames"),
        "unfocused_diagnostic_policy": unfocused_policy,
        "reasons": ["explicit_unfocused_optical_diagnostic"] if unfocused_policy else [],
        "observations": [],
    }
    if launch is None:
        result["reasons"].append("launch_sidecar_missing")
        return result
    if not isinstance(launch, dict) or not isinstance(launch.get("focus_observations"), list):
        raise ValueError("launch sidecar requires raw focus_observations")
    metadata = report.get("metadata")
    start_value = metadata.get("recording_boundary_observed_unix_seconds") if isinstance(metadata, dict) else None
    if start_value is None:
        result["reasons"].append("recording_boundary_unavailable")
        return result
    if isinstance(start_value, bool) or not isinstance(start_value, (int, float)) or not math.isfinite(start_value):
        raise ValueError("recording boundary must be finite Unix seconds")
    intervals = report.get("interval_seconds")
    if not isinstance(intervals, list) or not intervals or any(
        isinstance(x, bool) or not isinstance(x, (int, float)) or not math.isfinite(x) or x <= 0
        for x in intervals
    ):
        raise ValueError("OS foreground association requires positive raw intervals")
    start = Decimal(str(start_value))
    duration = sum((Decimal(str(x)) for x in intervals), Decimal(0))
    end = start + duration
    result["nominal_recording_window_unix_seconds"] = [str(start), str(end)]
    result["raw_interval_duration_seconds"] = str(duration)
    counts = {part: {"foreground": 0, "not_foreground": 0}
              for part in ("before", "inside", "boundary", "after")}
    inside_times = []
    unavailable_inside = 0
    previous = None
    for i, observation in enumerate(launch["focus_observations"]):
        if not isinstance(observation, dict) or type(observation.get("own_window_is_foreground")) is not bool:
            raise ValueError("OS foreground observation must contain a Boolean flag")
        unavailable = observation.get("foreground_window_is_null")
        if unavailable is not None and type(unavailable) is not bool:
            raise ValueError("optional foreground-window-null flag must be Boolean")
        if unavailable and observation["own_window_is_foreground"]:
            raise ValueError("a null foreground window cannot be the application window")
        stamp = observation.get("utc")
        match = re.fullmatch(r"(\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2})(\.\d+)?(Z|[+-]\d{2}:\d{2})", stamp or "")
        if not match:
            raise ValueError("OS foreground timestamp must include an explicit UTC offset")
        whole = datetime.fromisoformat(match[1] + ("+00:00" if match[3] == "Z" else match[3]))
        delta = whole.astimezone(timezone.utc) - datetime(1970, 1, 1, tzinfo=timezone.utc)
        instant = Decimal(delta.days * 86400 + delta.seconds) + Decimal(match[2] or "0")
        if previous is not None and instant <= previous:
            raise ValueError("OS foreground timestamps must be strictly ordered")
        previous = instant
        part = "before" if instant < start else "after" if instant > end else "boundary" if instant in (start, end) else "inside"
        flag = observation["own_window_is_foreground"]
        counts[part]["foreground" if flag else "not_foreground"] += 1
        result["observations"].append({"index": i, "utc": stamp,
            "own_window_is_foreground": flag, "foreground_window_is_null": unavailable,
            "recording_association": part})
        if part == "inside":
            inside_times.append(instant)
            unavailable_inside += unavailable is True
    result["association_counts"] = counts
    result["foreground_unavailable_inside_count"] = unavailable_inside
    points = [start, *inside_times, end]
    result["maximum_unsampled_span_seconds"] = float(max(b - a for a, b in zip(points, points[1:])))
    if not inside_times:
        result["reasons"].append("no_os_samples_strictly_inside_recording")
    if counts["inside"]["not_foreground"]:
        result["reasons"].append("os_not_foreground_during_recording")
    if unavailable_inside:
        result["reasons"].append("foreground_window_unavailable_during_recording")
    if counts["boundary"]["not_foreground"]:
        result["reasons"].append("os_not_foreground_at_uncertain_recording_boundary")
    if report.get("unfocused_recorded_frames") != 0 or type(report.get("unfocused_recorded_frames")) is not int:
        result["reasons"].append("application_focus_not_verified")
    result["eligible_for_focused_timing"] = not result["reasons"]
    return result


def summarize(report: dict[str, Any], launch: Any = None) -> dict[str, Any]:
    if not isinstance(report, dict):
        raise ValueError("frame capture must be a JSON object")
    if report.get("format") != "ancient-earth-frame-capture-v1":
        raise ValueError("unsupported frame-capture format")
    intervals = report.get("interval_seconds")
    if not isinstance(intervals, list) or not intervals:
        raise ValueError("a nonempty interval_seconds array is required")
    if any(
        isinstance(value, bool)
        or not isinstance(value, (int, float))
        or not math.isfinite(value)
        or value <= 0
        for value in intervals
    ):
        raise ValueError("all intervals must be finite positive seconds")
    count = len(intervals)
    if type(report.get("sample_count")) is not int or report["sample_count"] != count:
        raise ValueError("sample_count disagrees with the raw data")
    stats = interval_statistics(intervals)
    duration = stats["recorded_duration_seconds"]
    declared = report.get("recorded_duration_seconds")
    if (
        isinstance(declared, bool)
        or not isinstance(declared, (int, float))
        or not math.isfinite(declared)
        or not math.isclose(declared, duration, rel_tol=1e-10, abs_tol=1e-9)
    ):
        raise ValueError("recorded duration disagrees with raw intervals")
    metadata = report.get("metadata")
    capture_mode = metadata.get("capture_mode") if isinstance(metadata, dict) else None
    optical_phases, optical_counts = optical_phase_statistics(report.get("optical_motion_qa"), intervals)
    foreground = external_foreground_statistics(report, launch)
    application_stationary = (
        report.get("motion_qa") is None
        and report.get("view_motion_qa") is None
        and report.get("continuous_motion_qa") is None
        and report.get("optical_motion_qa") is None
        and report.get("optical_motion_error") is None
        and capture_mode in (None, "stationary")
        and report.get("view_changed_during_capture") is False
        and type(report.get("incomplete_scene_frames")) is int
        and report["incomplete_scene_frames"] == 0
        and type(report.get("unfocused_recorded_frames")) is int
        and report["unfocused_recorded_frames"] == 0
    )
    return {
        "measurement": "application frame pacing on the recorded machine/configuration",
        "not_measured": "GPU execution time, latency, or hardware-independent performance",
        "metadata": report.get("metadata"),
        "view_changed_during_capture": report.get("view_changed_during_capture"),
        "incomplete_scene_frames": report.get("incomplete_scene_frames"),
        "motion_qa": report.get("motion_qa"),
        "view_motion_qa": report.get("view_motion_qa"),
        "view_motion_phase_statistics": view_phase_statistics(report.get("view_motion_qa"), intervals),
        "continuous_motion_qa": report.get("continuous_motion_qa"),
        "continuous_motion_phase_statistics": view_phase_statistics(report.get("continuous_motion_qa"), intervals, continuous=True),
        "optical_motion_qa": report.get("optical_motion_qa"),
        "optical_motion_error": report.get("optical_motion_error"),
        "optical_motion_phase_statistics": optical_phases,
        "optical_motion_interval_counts": optical_counts,
        "optical_compute_header_statistics": optical_header_statistics(
            report.get("optical_compute_readback"), report.get("optical_motion_qa")),
        "optical_stellar_header_statistics": optical_stellar_header_statistics(
            report.get("optical_compute_readback"), report.get("optical_motion_qa")),
        "external_foreground_statistics": foreground,
        "application_stationary_conditions_satisfied": application_stationary,
        "eligible_as_stationary_scene_baseline": application_stationary and foreground["eligible_for_focused_timing"],
        "unfocused_recorded_frames": report.get("unfocused_recorded_frames"),
        **stats,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("captures", type=Path, nargs="+")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    summaries = []
    try:
        for path in args.captures:
            report = json.loads(path.read_text(encoding="utf-8"))
            launch_path = path.with_suffix(".launch.json")
            launch = json.loads(launch_path.read_text(encoding="utf-8-sig")) if launch_path.exists() else None
            summaries.append({"capture": str(path), "launch_sidecar": str(launch_path) if launch is not None else None,
                              **summarize(report, launch)})
    except (OSError, ValueError, TypeError, OverflowError) as error:
        parser.error(str(error))
    text = json.dumps(summaries, indent=2, allow_nan=False) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text, encoding="utf-8")
    print(text, end="")


if __name__ == "__main__":
    main()
