"""Closed-form checks of frame-pacing statistics and malformed input rejection."""
import unittest
from copy import deepcopy

from analyze_frame_capture import (
    external_foreground_statistics, optical_header_statistics,
    optical_stellar_header_statistics, summarize,
)


def capture(values):
    return {
        "format": "ancient-earth-frame-capture-v1",
        "sample_count": len(values),
        "recorded_duration_seconds": sum(values),
        "interval_seconds": values,
        "view_changed_during_capture": False,
        "incomplete_scene_frames": 0,
        "unfocused_recorded_frames": 0,
    }


class FrameCaptureAnalysisTests(unittest.TestCase):
    def test_stellar_headers_use_separate_requests_and_current_selected_token(self):
        tokens = [[1920, 1080, 0x1000001 + i, 9] for i in range(4)]
        def request(wanted, returned):
            return {"requested_extent_and_token": wanted,
                    "returned_extent_and_token": returned,
                    "exact_match": None if returned is None else wanted == returned,
                    "error": None}
        frames = [{"phase": "forward_pan", "physical_size": [1920, 1080],
                   "submitted_compute_control": token, "expected_stellar_control": token}
                  for token in tokens[:3]]
        frames += [{"phase": "recovery", "submitted_compute_control": tokens[3],
                    "expected_stellar_control": None},
                   {"phase": "complete", "submitted_compute_control": tokens[3]}]
        readback = {"requests": [request(token, token) for token in tokens],
                    "stellar_requests": [request(tokens[2], None),
                        request(tokens[1], [0, 0, 0, 0]), request(tokens[0], tokens[0]),
                        request(tokens[3], tokens[3])]}
        result = optical_stellar_header_statistics(readback, {"frames": frames})
        self.assertEqual([r["state"] for r in result["frame_associations"]],
                         ["exact_match", "mismatch", "pending", "table_not_selected",
                          "legacy_table_selection_unavailable"])
        self.assertEqual(result["request_counts"],
                         {"exact_match": 2, "mismatch": 1, "pending": 1, "error": 0})
        self.assertEqual(result["unassociated_request_indices"], [3])
        self.assertEqual(result["frame_associations"][0]["request_indices"], [2])
        # Primary completion cannot stand in for a missing stellar readback.
        absent = optical_stellar_header_statistics({"requests": readback["requests"]},
                                                   {"frames": frames})
        self.assertEqual(absent["frame_association_counts"]["readback_not_enabled"], 3)
        bad = deepcopy(frames)
        bad[0]["expected_stellar_control"] = tokens[1]
        with self.assertRaises(ValueError):
            optical_stellar_header_statistics(readback, {"frames": bad})
        self.assertIsNone(optical_stellar_header_statistics(None, {"frames": [frames[-1]]}))

    def test_os_focus_association_preserves_boundary_uncertainty_and_app_conflict(self):
        report = capture([0.5, 0.5])
        report["metadata"] = {"recording_boundary_observed_unix_seconds": 0.1}
        stamps = ["1970-01-01T00:00:00.0999999Z", "1970-01-01T00:00:00.1Z",
                  "1970-01-01T01:00:00.1000001+01:00", "1970-01-01T00:00:00.6Z",
                  "1970-01-01T00:00:01.1Z", "1970-01-01T00:00:01.1000001Z"]
        launch = {"focus_observations": [{"utc": t, "own_window_is_foreground": True} for t in stamps]}
        result = summarize(report, launch)
        self.assertTrue(result["eligible_as_stationary_scene_baseline"])
        evidence = result["external_foreground_statistics"]
        self.assertEqual(evidence["nominal_recording_window_unix_seconds"], ["0.1", "1.1"])
        self.assertEqual([x["recording_association"] for x in evidence["observations"]],
                         ["before", "boundary", "inside", "inside", "boundary", "after"])
        self.assertEqual(evidence["maximum_unsampled_span_seconds"], 0.5)
        # A conflicting OS observation does not rewrite the app's own flags.
        launch["focus_observations"][3]["own_window_is_foreground"] = False
        result = summarize(report, launch)
        self.assertTrue(result["application_stationary_conditions_satisfied"])
        self.assertEqual(result["unfocused_recorded_frames"], 0)
        self.assertFalse(result["eligible_as_stationary_scene_baseline"])
        self.assertIn("os_not_foreground_during_recording", result["external_foreground_statistics"]["reasons"])
        launch["focus_observations"][3]["own_window_is_foreground"] = True
        launch["focus_observations"][4]["own_window_is_foreground"] = False
        self.assertIn("os_not_foreground_at_uncertain_recording_boundary",
                      external_foreground_statistics(report, launch)["reasons"])

    def test_missing_os_focus_is_not_silent_timing_eligibility(self):
        report = capture([1.0])
        self.assertFalse(summarize(report)["eligible_as_stationary_scene_baseline"])
        self.assertEqual(external_foreground_statistics(report, None)["reasons"], ["launch_sidecar_missing"])
        launch = {"focus_observations": []}
        self.assertEqual(external_foreground_statistics(report, launch)["reasons"], ["recording_boundary_unavailable"])
        report["metadata"] = {"recording_boundary_observed_unix_seconds": 10}
        launch["focus_observations"] = [
            {"utc": "1970-01-01T00:00:09Z", "own_window_is_foreground": True},
            {"utc": "1970-01-01T00:00:12Z", "own_window_is_foreground": True}]
        result = external_foreground_statistics(report, launch)
        self.assertFalse(result["eligible_for_focused_timing"])
        self.assertEqual(result["maximum_unsampled_span_seconds"], 1.0)
        self.assertEqual(result["reasons"], ["no_os_samples_strictly_inside_recording"])
        launch["focus_observations"] = [{"utc": "1970-01-01T00:00:10.5Z",
            "own_window_is_foreground": False, "foreground_window_is_null": True}]
        result = external_foreground_statistics(report, launch)
        self.assertEqual(result["foreground_unavailable_inside_count"], 1)
        self.assertFalse(result["eligible_for_focused_timing"])
        self.assertTrue(result["observations"][0]["foreground_window_is_null"])

    def test_explicit_unfocused_diagnostic_cannot_gain_focused_timing_eligibility(self):
        report = capture([0.5, 0.5])
        report["metadata"] = {"recording_boundary_observed_unix_seconds": 0}
        launch = {"focus_observations": [
            {"utc": "1970-01-01T00:00:00.25Z", "own_window_is_foreground": True},
            {"utc": "1970-01-01T00:00:00.75Z", "own_window_is_foreground": True}]}
        ordinary = external_foreground_statistics(report, launch)
        self.assertTrue(ordinary["eligible_for_focused_timing"])
        report["optical_motion_qa"] = {"unfocused_diagnostic_policy": True,
                                      "diagnostic_route_passed": True, "passed": False}
        diagnostic = external_foreground_statistics(report, launch)
        self.assertFalse(diagnostic["eligible_for_focused_timing"])
        self.assertEqual(diagnostic["reasons"], ["explicit_unfocused_optical_diagnostic"])
        self.assertEqual(diagnostic["association_counts"], ordinary["association_counts"])
        self.assertEqual(diagnostic["observations"], ordinary["observations"])
        self.assertEqual(diagnostic["raw_interval_duration_seconds"], ordinary["raw_interval_duration_seconds"])
        self.assertIn("explicit_unfocused_optical_diagnostic", external_foreground_statistics(report, None)["reasons"])
        launch["focus_observations"][1]["own_window_is_foreground"] = False
        report["unfocused_recorded_frames"] = 1
        conflicting = external_foreground_statistics(report, launch)
        self.assertIn("os_not_foreground_during_recording", conflicting["reasons"])
        self.assertIn("application_focus_not_verified", conflicting["reasons"])
        self.assertEqual(conflicting["association_counts"]["inside"]["not_foreground"], 1)
        report["optical_motion_qa"]["unfocused_diagnostic_policy"] = "false"
        with self.assertRaises(ValueError):
            external_foreground_statistics(report, launch)

    def test_os_focus_malformed_or_reordered_observations_rejected(self):
        report = capture([1.0])
        report["metadata"] = {"recording_boundary_observed_unix_seconds": 0}
        good = {"utc": "1970-01-01T00:00:00.5Z", "own_window_is_foreground": True}
        for bad in ({"utc": "1970-01-01T00:00:00.5", "own_window_is_foreground": True},
                    {"utc": "not a time", "own_window_is_foreground": True},
                    {"utc": good["utc"], "own_window_is_foreground": 1},
                    {**good, "foreground_window_is_null": True},
                    {**good, "foreground_window_is_null": 1}):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                external_foreground_statistics(report, {"focus_observations": [bad]})
        for rows in ([good, good], [good, {**good, "utc": "1970-01-01T00:00:00.4Z"}]):
            with self.assertRaises(ValueError):
                external_foreground_statistics(report, {"focus_observations": rows})
        report["metadata"]["recording_boundary_observed_unix_seconds"] = float("nan")
        with self.assertRaises(ValueError):
            external_foreground_statistics(report, {"focus_observations": [good]})

    def test_compute_headers_associate_exact_words_and_retain_warmup_failures(self):
        headers = [[1920, 1080, 0x1000001 + i, 7] for i in range(5)]
        def request(wanted, returned, error=None):
            return {"requested_extent_and_token": wanted,
                    "returned_extent_and_token": returned,
                    "exact_match": None if returned is None else wanted == returned,
                    "error": error}
        # Deliberately nonchronological requests: matching depends on exact
        # headers, including integer bits above f32 precision and the high half.
        requests = [request(headers[2], None), request(headers[0], headers[0]),
                    request(headers[1], [0, 0, 0, 0]),
                    request([1920, 1080, 1, 0], [0, 0, 0, 0]),
                    request(headers[3], None, "short readback"),
                    request(headers[4], headers[4]), request(headers[4], headers[4])]
        frames = [{"phase": "ascent", "physical_size": [1920, 1080],
                   "submitted_compute_control": h} for h in headers]
        frames += [{"phase": "recovery", "submitted_compute_control": [1920, 1080, 99, 8]},
                   {"phase": "recovery", "submitted_compute_control": None},
                   {"phase": "warmup_boundary"}]
        result = optical_header_statistics({"requests": requests}, {"frames": frames})
        self.assertEqual(result["request_counts"], {"exact_match": 3, "mismatch": 2,
                                                   "pending": 1, "error": 1})
        self.assertEqual([r["state"] for r in result["frame_associations"]],
                         ["exact_match", "mismatch", "pending", "error", "ambiguous_requests",
                          "not_requested", "no_submitted_packet", "legacy_token_unavailable"])
        self.assertEqual(result["unassociated_request_indices"], [3])
        self.assertEqual(result["frame_associations"][0]["request_indices"], [1])
        self.assertEqual(result["frame_associations"][4]["request_indices"], [5, 6])
        disabled = optical_header_statistics(None, {"frames": frames})
        self.assertEqual(disabled["frame_association_counts"]["readback_not_enabled"], 6)
        self.assertIsNone(optical_header_statistics(None, {"frames": [{"phase": "ascent"}]}))

    def test_compute_header_malformed_words_and_forged_matches_are_rejected(self):
        good = {"requested_extent_and_token": [1920, 1080, 1, 0],
                "returned_extent_and_token": [1920, 1080, 1, 0],
                "exact_match": True, "error": None}
        for bad in ([1920, 1080, -1, 0], [1920, 1080, 2**32, 0],
                    [1920, 1080, True, 0], [1920, 1080, 1.0, 0], [1920, 1080, 1]):
            for field in ("requested_extent_and_token", "returned_extent_and_token"):
                mutated = deepcopy(good)
                mutated[field] = bad
                with self.subTest(field=field, bad=bad), self.assertRaises(ValueError):
                    optical_header_statistics({"requests": [mutated]}, None)
        for field, bad in (("exact_match", False), ("exact_match", 1),
                           ("error", 12), ("requested_extent_and_token", [0, 1080, 1, 0])):
            mutated = deepcopy(good)
            mutated[field] = bad
            with self.subTest(field=field), self.assertRaises(ValueError):
                optical_header_statistics({"requests": [mutated]}, None)
        with self.assertRaises(ValueError):
            optical_header_statistics({"requests": [good]}, {"frames": [{
                "phase": "ascent", "physical_size": [640, 360],
                "submitted_compute_control": good["requested_extent_and_token"]}]})

    def test_optical_motion_keeps_slow_pending_unfocused_frames_and_validates_raw_association(self):
        values = [0.02, 0.03, 1.0, 0.04, 0.05, 0.06, 0.07, 0.08]
        report = capture(values)
        phases = ["warmup_boundary", "ascent", "forward_pan", "forward_pan",
                  "slide_zoom_in", "slide_zoom_out", "recovery", "complete"]
        flags = {"optical_active": "inactive_optical_intervals",
                 "optical_ready": "optical_pending_intervals",
                 "presentation_ready": "presentation_pending_intervals",
                 "render_matches_observer": "stale_render_intervals",
                 "focused": "unfocused_intervals"}
        qa = {"passed": False, "frames": [
            {"phase": phase, "interval_seconds": interval,
             **{flag: i != 2 for flag in flags}}
            for i, (phase, interval) in enumerate(zip(phases, values))
        ], **dict.fromkeys(flags.values(), 1)}
        report["optical_motion_qa"] = qa
        report["optical_motion_error"] = "retained failure diagnostic"
        result = summarize(report)
        self.assertFalse(result["eligible_as_stationary_scene_baseline"])
        self.assertEqual(result["optical_motion_interval_counts"], dict.fromkeys(flags.values(), 1))
        summaries = result["optical_motion_phase_statistics"]
        self.assertEqual(sum(p["sample_count"] for p in summaries), 8)
        self.assertAlmostEqual(sum(p["recorded_duration_seconds"] for p in summaries), 1.35)
        self.assertAlmostEqual(summaries[2]["throughput_frames_per_second"], 2 / 1.04)
        self.assertEqual(summaries[2]["frame_interval_ms"]["maximum"], 1000.0)
        self.assertEqual(summaries[2]["state_interval_counts"], dict.fromkeys(flags.values(), 1))
        for mutation in ("reorder", "drop", "interval", "phase", "flag", "counter"):
            bad = deepcopy(report)
            frames = bad["optical_motion_qa"]["frames"]
            if mutation == "reorder":
                frames[1], frames[2] = frames[2], frames[1]
            elif mutation == "drop":
                frames.pop()
            elif mutation == "interval":
                frames[2]["interval_seconds"] = 0.04
            elif mutation == "phase":
                frames[2]["phase"] = "unknown"
            elif mutation == "flag":
                frames[2]["focused"] = 0
            else:
                bad["optical_motion_qa"]["unfocused_intervals"] = 0
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                summarize(bad)
        report.pop("optical_motion_qa")
        report.pop("optical_motion_error")
        report["metadata"] = {"capture_mode": "refraction-flight-qa"}
        self.assertFalse(summarize(report)["eligible_as_stationary_scene_baseline"])

    def test_continuous_traversal_preserves_both_directions_and_settlement_intervals(self):
        report = capture([0.01, 0.03, 0.02, 0.2, 0.04])
        report["continuous_motion_qa"] = {"passed": True, "frames": [
            {"phase": phase, "interval_seconds": interval}
            for phase, interval in zip(["warmup_boundary", "ascent", "far_settle", "descent", "near_settle"], report["interval_seconds"])
        ]}
        result = summarize(report)
        self.assertFalse(result["eligible_as_stationary_scene_baseline"])
        phases = result["continuous_motion_phase_statistics"]
        self.assertEqual(sum(p["sample_count"] for p in phases), 5)
        self.assertAlmostEqual(sum(p["recorded_duration_seconds"] for p in phases), 0.3)
        self.assertEqual(phases[3]["frame_interval_ms"]["maximum"], 200.0)
        report["continuous_motion_qa"]["frames"][3]["interval_seconds"] = 0.01
        with self.assertRaises(ValueError):
            summarize(report)
        report["continuous_motion_qa"]["frames"].pop()
        with self.assertRaises(ValueError):
            summarize(report)
        report.pop("continuous_motion_qa")
        report["metadata"] = {"capture_mode": "continuous-flight-qa"}
        self.assertFalse(summarize(report)["eligible_as_stationary_scene_baseline"])

    def test_throughput_uses_total_time_not_mean_instantaneous_fps(self):
        result = summarize(capture([0.01, 0.03]))
        self.assertAlmostEqual(result["throughput_frames_per_second"], 50.0)
        self.assertAlmostEqual(result["frame_interval_ms"]["mean"], 20.0)
        self.assertAlmostEqual(result["frame_interval_ms"]["median"], 20.0)
        self.assertAlmostEqual(result["frame_interval_ms"]["p95_nearest_rank"], 30.0)
        self.assertTrue(result["application_stationary_conditions_satisfied"])
        self.assertFalse(result["eligible_as_stationary_scene_baseline"])
        incomplete = capture([0.01, 5.0])
        incomplete["incomplete_scene_frames"] = 1
        measured = summarize(incomplete)
        self.assertFalse(measured["eligible_as_stationary_scene_baseline"])
        self.assertEqual(measured["frame_interval_ms"]["maximum"], 5000.0)
        unfocused = capture([0.01, 0.03])
        unfocused["unfocused_recorded_frames"] = 1
        self.assertFalse(summarize(unfocused)["eligible_as_stationary_scene_baseline"])

    def test_rank_boundaries_are_exact_and_input_order_does_not_matter(self):
        result = summarize(capture([i / 1000 for i in range(100, 0, -1)]))
        timing = result["frame_interval_ms"]
        self.assertAlmostEqual(timing["median"], 50.5)
        self.assertAlmostEqual(timing["p95_nearest_rank"], 95.0)
        self.assertAlmostEqual(timing["p99_nearest_rank"], 99.0)
        single = summarize(capture([0.02]))["frame_interval_ms"]
        self.assertTrue(all(abs(value - 20.0) < 1e-12 for value in single.values()))

    def test_invalid_or_inconsistent_raw_samples_are_rejected(self):
        for malformed in [None, [], "capture", 0]:
            with self.subTest(malformed=malformed), self.assertRaises(ValueError):
                summarize(malformed)
        for values in [[], [0], [-0.1], [float("nan")], [float("inf")], [True]]:
            with self.subTest(values=values), self.assertRaises(ValueError):
                summarize(capture(values))
        wrong_count = capture([0.01])
        for invalid_count in [2, True, 1.0]:
            wrong_count["sample_count"] = invalid_count
            with self.assertRaises(ValueError):
                summarize(wrong_count)
        wrong_duration = capture([0.01])
        wrong_duration["recorded_duration_seconds"] = 1.0
        with self.assertRaises(ValueError):
            summarize(wrong_duration)
        with self.assertRaises(ValueError):
            summarize(capture([1e308]))

    def test_view_motion_is_nonstationary_and_phases_preserve_every_interval(self):
        report = capture([0.01, 0.03, 0.2, 0.02, 0.04])
        report["metadata"] = {"capture_mode": "nonstationary-view-qa"}
        report["view_motion_qa"] = {
            "passed": True,
            "phase_summaries": [{"incorrect_derived_data": "intentionally ignored"}],
            "frames": [
                {"phase": phase, "interval_seconds": interval}
                for phase, interval in zip(
                    ["zoom_in", "zoom_in", "ascent", "recovery", "recovery"],
                    report["interval_seconds"],
                )
            ],
        }
        result = summarize(report)
        self.assertFalse(result["eligible_as_stationary_scene_baseline"])
        self.assertEqual(result["view_motion_qa"], report["view_motion_qa"])
        phases = result["view_motion_phase_statistics"]
        self.assertEqual(sum(p["sample_count"] for p in phases), 5)
        self.assertAlmostEqual(sum(p["recorded_duration_seconds"] for p in phases), 0.3)
        self.assertAlmostEqual(phases[0]["throughput_frames_per_second"], 50.0)
        self.assertAlmostEqual(phases[0]["frame_interval_ms"]["median"], 20.0)
        self.assertAlmostEqual(phases[1]["frame_interval_ms"]["maximum"], 200.0)
        self.assertAlmostEqual(phases[2]["frame_interval_ms"]["p95_nearest_rank"], 40.0)
        report["view_motion_qa"]["frames"][2]["interval_seconds"] = 0.02
        with self.assertRaises(ValueError):
            summarize(report)
        report["view_motion_qa"]["frames"].pop()
        with self.assertRaises(ValueError):
            summarize(report)
        report.pop("view_motion_qa")
        self.assertFalse(summarize(report)["eligible_as_stationary_scene_baseline"])
        report["metadata"]["capture_mode"] = "stationary"
        self.assertTrue(summarize(report)["application_stationary_conditions_satisfied"])
        self.assertFalse(summarize(report)["eligible_as_stationary_scene_baseline"])


if __name__ == "__main__":
    unittest.main()
