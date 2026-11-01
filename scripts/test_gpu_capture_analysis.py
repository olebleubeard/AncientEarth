import unittest
from analyze_gpu_capture import analyze


class GpuCaptureAnalysis(unittest.TestCase):
    def test_complete_intervals_weighted_separately_and_invalid_gaps_remain_gaps(self):
        frame = {"metadata": {"recording_boundary_observed_unix_seconds": 0.0},
                 "completion_boundary_observed_unix_seconds": 10.0, "interval_seconds": [1.0] * 10}
        samples = []
        previous = None
        for second, value, status in [(0, 99, 0), (1, 10, 0), (3, 130, 0), (4, 80, 5), (5, 50, 0), (7, 70, 0), (10, 100, 0)]:
            stamp = f"1970-01-01T00:00:{second:02d}Z"
            samples.append(dict(timestampUtc=stamp, intervalStartUtc=previous, engines=[
                dict(InstanceName="pid_42_gpu_3d", CookedValue=value, Status=status),
                dict(InstanceName="pid_42_gpu_copy", CookedValue=0, Status=0),
                dict(InstanceName="pid_7_gpu_3d", CookedValue=100, Status=0)]))
            previous = stamp
        counters = dict(collection="persistent PDH query", processId=42, samples=samples)
        result = analyze(frame, counters, guard_seconds=1)
        # Complete intervals inside[1,9]: 3D uses[1,3] and[5,7], excludes the
        # invalid[3,4] and first recovered[4,5]; copies preserve every zero.
        self.assertEqual(result["engines"]["gpu_3d"]["mean_busy_percent"], 100)
        self.assertEqual(result["engines"]["gpu_3d"]["above_nominal_100_percent_intervals"], 1)
        self.assertEqual(result["engines"]["gpu_3d"]["covered_seconds"], 4)
        self.assertEqual(result["engines"]["gpu_copy"]["covered_seconds"], 6)
        self.assertEqual(result["engines"]["gpu_copy"]["zero_intervals"], 4)
        self.assertEqual(len(result["engines"]), 2)
        samples[2]["engines"].append(samples[2]["engines"][0])
        with self.assertRaises(ValueError):
            analyze(frame, counters, guard_seconds=1)

    def test_logs_define_old_capture_window_but_fresh_query_data_is_rejected(self):
        frame = {"metadata": {}, "interval_seconds": [1.0] * 20}
        log = ("1970-01-01T00:00:00.000000Z Frame capture warmup complete; recording application intervals\n"
               "1970-01-01T00:00:25Z Saved raw frame capture to file\n")
        counters = dict(collection="persistent PDH query", processId=1, samples=[])
        result = analyze(frame, counters, log)
        self.assertEqual(result["window_start_unix_seconds"], 2)
        self.assertEqual(result["window_end_unix_seconds"], 18)
        self.assertEqual(result["engines"], {})
        with self.assertRaises(ValueError):
            analyze(frame, {**counters, "collection": "fresh query"}, log)
        with self.assertRaises(ValueError):
            analyze(frame, counters)
        for intervals in [[1.0, -0.1], [True], [float("nan")], []]:
            with self.assertRaises(ValueError):
                analyze({**frame, "interval_seconds": intervals}, counters, log)


if __name__ == "__main__":
    unittest.main()
