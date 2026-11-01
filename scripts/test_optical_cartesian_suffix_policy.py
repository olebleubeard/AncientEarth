"""Focused policy regression: exact fallback versus same-ray error gates."""
import copy
from fractions import Fraction as Q
import math
import unittest

from review_optical_cartesian_suffix_policy import evaluate_selected_record


class SuffixPolicy(unittest.TestCase):
    def test_same_ray_and_exact_fallback_are_separate(self):
        old = [0]*12; old[3] = 123; old[7] = 0
        row = {'record_index': 0, 'original_status': 0, 'launch_words': [1, 2, 3],
               'original_terminal_words': old,
               'original_error': {'position_m': 14., 'path_m': .5, 'direction': 1e-6},
               'reference_convergence': {'position_m': .125, 'path_m': .0625, 'direction': 1e-12}}
        policy = {'schema': 'cartesian-suffix-prospective-scope-v1', 'records': [row]}
        for route in ('unhanded-original', 'original-fallback'):
            self.assertTrue(evaluate_selected_record(policy, 0, route, old, [1, 2, 3])['passed'])
            changed = list(old); changed[0] = 1
            self.assertFalse(evaluate_selected_record(policy, 0, route, changed, [1, 2, 3])['passed'])
        errors = {'position_m': 14.125, 'path_m': .5625, 'direction': 1e-6}
        self.assertTrue(evaluate_selected_record(policy, 0, 'accepted-suffix', old, [1, 2, 3], errors)['passed'])
        for metric in errors:
            bad = dict(errors)
            exact = Q(row['original_error'][metric])+Q(row['reference_convergence'][metric])
            bad[metric] = math.nextafter(float(exact), math.inf)
            self.assertFalse(evaluate_selected_record(policy, 0, 'accepted-suffix', old, [1, 2, 3], bad)['passed'])
        self.assertFalse(evaluate_selected_record(policy, 0, 'accepted-suffix', old, [4, 2, 3], errors)['passed'])
        for value in (math.nan, math.inf, -1.):
            bad = dict(errors, path_m=value)
            self.assertFalse(evaluate_selected_record(policy, 0, 'accepted-suffix', old, [1, 2, 3], bad)['passed'])
        self.assertFalse(evaluate_selected_record(policy, 0, 'pending', old, [1, 2, 3], errors)['passed'])
        surface = copy.deepcopy(policy); surface['records'][0]['original_status'] = 1
        self.assertFalse(evaluate_selected_record(surface, 0, 'accepted-suffix', old, [1, 2, 3], errors)['passed'])


if __name__ == '__main__':
    unittest.main()
