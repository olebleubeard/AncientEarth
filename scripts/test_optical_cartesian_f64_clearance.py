"""Finite independent checks of the stored-f64 to f32 clearance bridge.

Exact Fractions check operand/curve containment; retained promoted trial bits
are fixtures only. No ODE integration, new GPU execution or global error claim.
"""
from fractions import Fraction as Q
import math
from pathlib import Path
import random
import struct
import unittest

from test_optical_normal_intervals import MIN_NORMAL, TINY_BOUND, widen
from test_optical_normal_map_interval import add, mul, div, f32, vadd, vsub, scale
from test_optical_cartesian_clearance import represented_point, transformed_box

ROOT = Path(__file__).resolve().parents[1]


def interval(value, forced_round=None):
    if not abs(value) < 3e37: return None
    if abs(value) < MIN_NORMAL: return (-TINY_BOUND, TINY_BOUND)
    rounded = f32(value) if forced_round is None else forced_round
    if not MIN_NORMAL <= abs(rounded) < f32(3e37): return None
    if rounded == value: return (rounded, rounded)
    result = widen(rounded, 1)
    if not (abs(result[0]) < f32(3e37) and abs(result[1]) < f32(3e37)
            and Q(result[0]) <= Q(value) <= Q(result[1])): return None
    return result


def vector(values):
    result = tuple(map(interval, values))
    assert all(x is not None for x in result)
    return result


def controls(position, coeff, distance, step):
    alpha = div(interval(distance), interval(step))
    observed = distance/step
    assert alpha[0] <= observed <= alpha[1] and alpha[1] <= f32(1.000002)
    a = (max(0., alpha[0]), alpha[1]); a2 = mul(a, a)
    a3, a4 = mul(a2, a), mul(a2, a2)
    p = vector(position); d, r3, r4, r5 = map(vector, coeff)
    c1 = scale(vadd(d, r3), a)
    c2 = scale(vadd(vsub(r4, r3), r5), a2)
    c3 = scale(vsub(scale(r4, (-1., -1.)), scale(r5, (2., 2.))), a3)
    c4 = scale(r5, a4)
    return [p, vadd(p, scale(c1, (.25, .25))),
            vadd(p, vadd(scale(c1, (.5, .5)), scale(c2, div((1., 1.), (6., 6.))))),
            vadd(p, vadd(vadd(scale(c1, (.75, .75)), scale(c2, (.5, .5))), scale(c3, (.25, .25)))),
            vadd(p, vadd(vadd(c1, c2), vadd(c3, c4)))], alpha


class F64ClearanceReview(unittest.TestCase):
    def test_f64_operands_are_enclosed_not_rounded_singletons(self):
        rng = random.Random(362040)
        cases = [0., -0., 2.**-1074, -2.**-1074, MIN_NORMAL,
                 math.nextafter(MIN_NORMAL, 0.), math.nextafter(MIN_NORMAL, math.inf),
                 math.nextafter(1., 2.), math.nextafter(1e9, math.inf), 1e-6, 100000.]
        cases += [rng.uniform(-1e9, 1e9) for _ in range(128)]
        for x in cases+[-x for x in cases]:
            result = interval(x); self.assertIsNotNone(result)
            self.assertLessEqual(Q(result[0]), Q(x)); self.assertLessEqual(Q(x), Q(result[1]))
        for x in (float('nan'), float('inf'), -float('inf'), 3e37, -3e37):
            self.assertIsNone(interval(x))
        self.assertEqual(interval(1.), (1., 1.))
        self.assertNotEqual(interval(math.nextafter(1., 2.)), (1., 1.))
        self.assertIsNone(interval(1.1, forced_round=2.))

    def test_exact_polynomial_containment_and_uploaded_affine_rows(self):
        rng = random.Random(370129)
        for _ in range(48):
            position = [rng.uniform(-2e6, 2e6) for _ in range(3)]
            coeff = [[rng.uniform(-s, s) for _ in range(3)] for s in (100000., 1000., 500., 100.)]
            step = rng.uniform(.001, 100000.)
            distance = step*rng.choice([0., .125, .5, .999999, 1.])
            ctrl, alpha = controls(position, coeff, distance, step)
            rows = [[f32(rng.uniform(-1.5, 1.5)) for _ in range(3)] for _ in range(3)]
            origin = [f32(rng.uniform(-3e5, 3e5)) for _ in range(3)]
            bounds = transformed_box(ctrl, origin, rows)
            for fraction in (Q(0), Q(1, 7), Q(1, 2), Q(1)):
                theta = Q(distance)/Q(step)*fraction
                point = represented_point(position, coeff, theta)
                for k in range(3):
                    value = Q(origin[k])+sum(Q(a)*p for a, p in zip(rows[k], point))
                    self.assertLessEqual(Q(bounds[k][0]), value)
                    self.assertLessEqual(value, Q(bounds[k][1]))
            # The wider f32 ratio also covers neighboring f64 division results.
            quotient = distance/step
            for q in (quotient, math.nextafter(quotient, 0.), math.nextafter(quotient, math.inf)):
                self.assertLessEqual(alpha[0], q); self.assertLessEqual(q, alpha[1])

    def test_retained_actual_f64_coefficients_and_rounding_majorant(self):
        raw_path = ROOT/'artifacts/optical-cartesian-direct-gpu-first-direct_promoted-iteration-3.bin'
        input_path = ROOT/'artifacts/optical-cartesian-direct-validation-second-inputs.bin'
        raw_inputs = struct.unpack('<396I', input_path.read_bytes())
        with raw_path.open('rb') as stream:
            for index in range(11):
                # Invocation=index, cycle=0 maps to this same input.
                stream.seek(index*8*1536)
                raw = stream.read(1536); header = struct.unpack('<8I', raw[:32])
                self.assertEqual((header[3], header[6], header[7]), (index, 64, 127))
                values = struct.unpack('<188d', raw[32:])
                position = struct.unpack('<3f', struct.pack('<3I', *raw_inputs[36*index+16:36*index+19]))
                coeff = [values[a:a+3] for a in (0, 11, 14, 17)]
                ctrl, _ = controls(position, coeff, values[77], values[77])
                for theta in (Q(0), Q(1, 4), Q(1, 2), Q(3, 4), Q(1)):
                    point = represented_point(position, coeff, theta)
                    for axis in range(3):
                        self.assertLessEqual(Q(min(c[axis][0] for c in ctrl)), point[axis])
                        self.assertLessEqual(point[axis], Q(max(c[axis][1] for c in ctrl)))
        # Same <=12-operation gamma bound, with the larger rounding unit for
        # either directed f64 rounding. The old f32 band is deliberately kept.
        u64, u32 = Q(1, 2**52), Q(1, 2**23)
        endpoint = Q(f32(1.000002))
        gamma12 = 12*u64/(1-12*u64)
        self.assertLess(gamma12*4*endpoint**4, 64*u32)
        self.assertLess(24*Q(1, 2**1022), u32)

    def test_source_has_required_outward_budget_and_rejection_guards(self):
        source = (ROOT/'scripts/optical_cartesian_f64_clearance.wgsl').read_text()
        self.assertIn('f64(result.x)>value||f64(result.y)<value', source)
        self.assertIn('observed_alpha<=f64(alpha.y)&&observed_alpha>=f64(alpha.x)', source)
        self.assertIn('nc_interval_add(cf64_interval(error),cf64_interval(allocation))', source)
        self.assertIn('f64(total_error.y)<=tolerance', source)
        self.assertIn('max(polynomial_sag.y,physical_sag.y)', source)
        self.assertIn('vec2(max(1.0,speed.y))', source)
        self.assertIn('cs_all_roots_clear(controls,tube)', source)
        self.assertNotIn('cs_exact3(', source)
        self.assertNotIn('return Trace(', source)


if __name__ == '__main__':
    unittest.main()
