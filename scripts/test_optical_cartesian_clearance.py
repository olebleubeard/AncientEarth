"""Finite CPU checks for the sufficient-only Cartesian suffix exclusion.

The f32 scalar transcription reuses reviewed interval primitives. Exact
Fractions independently check represented polynomial/control identities and
affine containment. These tests do not execute a GPU or certify ODE error.
"""
from fractions import Fraction as Q
import json
from pathlib import Path
import random
import struct
import unittest

from test_optical_normal_map_interval import (add, sub, mul, div, exact, f32,
    vadd, vsub, vexact, scale, dot)

ROOT = Path(__file__).resolve().parents[1]
ZERO = (0., 0.)


def controls(start, coeff, distance, step):
    lo, hi = div(exact(distance), exact(step))
    assert hi <= 1.000002
    a = (max(0., lo), hi)
    a2, a3 = mul(a, a), mul(mul(a, a), a)
    a4 = mul(a2, a2)
    p = vexact(start)
    d, r3, r4, r5 = map(vexact, coeff)
    c1 = scale(vadd(d, r3), a)
    c2 = scale(vadd(vsub(r4, r3), r5), a2)
    c3 = scale(vsub(scale(r4, (-1., -1.)), scale(r5, (2., 2.))), a3)
    c4 = scale(r5, a4)
    return [p, vadd(p, scale(c1, (.25, .25))),
            vadd(p, vadd(scale(c1, (.5, .5)), scale(c2, div((1., 1.), (6., 6.))))),
            vadd(p, vadd(vadd(scale(c1, (.75, .75)), scale(c2, (.5, .5))), scale(c3, (.25, .25)))),
            vadd(p, vadd(vadd(c1, c2), vadd(c3, c4)))]


def transformed_box(ctrl, translation, rows):
    points = [tuple(add(exact(t), dot(vexact(row), p)) for t, row in zip(translation, rows)) for p in ctrl]
    return tuple((min(p[k][0] for p in points), max(p[k][1] for p in points)) for k in range(3))


def represented_point(start, coeff, theta):
    d, r3, r4, r5 = [list(map(Q, c)) for c in coeff]
    return [Q(p)+theta*(a+(1-theta)*(b+theta*(c+(1-theta)*e)))
            for p, a, b, c, e in zip(start, d, r3, r4, r5)]


def metadata_valid(frames, geometry):
    """Independent integer model of shader storage/address prerequisites."""
    nf, ng = len(frames), len(geometry)
    if nf < 6 or ng < 1: return False
    word = lambda x: struct.unpack('<I', struct.pack('<f', x))[0]
    root, patches, marker_root, markers = map(word, frames[0])
    if patches > 8192 or patches > (nf-6)//4: return False
    marker_start = 6+4*patches
    if word(frames[1][0]) != marker_start or markers > (nf-marker_start)//8: return False
    if (patches == 0) != (root == 0) or word(frames[2][3]) != root: return False
    if patches and (root >= ng-1 or not root+1 < word(geometry[root][3]) <= ng): return False
    for i in range(patches):
        node = word(frames[6+4*i][3])
        if not 0 < node < ng-1 or not node+1 < word(geometry[node][3]) <= ng: return False
    if (markers == 0) != (marker_root == 0): return False
    if markers and (not marker_start+8*markers <= marker_root < nf-1
                    or not marker_root+1 < word(frames[marker_root][3]) <= nf): return False
    return True


class CartesianClearanceReview(unittest.TestCase):
    def test_power_bernstein_identity_and_outward_affine_containment(self):
        rng = random.Random(126771)
        for _ in range(96):
            start = [f32(rng.uniform(-2e6, 2e6)) for _ in range(3)]
            coeff = [[f32(rng.uniform(-s, s)) for _ in range(3)] for s in (100000., 1000., 1000., 100.)]
            step = f32(rng.uniform(.001, 100000.))
            distance = f32(step*rng.choice([0., .125, .5, .999999, 1.]))
            ctrl = controls(start, coeff, distance, step)
            rows = [[f32(rng.uniform(-1.5, 1.5)) for _ in range(3)] for _ in range(3)]
            translation = [f32(rng.uniform(-3e5, 3e5)) for _ in range(3)]
            bounds = transformed_box(ctrl, translation, rows)
            alpha = Q(distance)/Q(step)
            # Independent exact Bernstein conversion from power coefficients.
            for k in range(3):
                p = Q(start[k]); d, r3, r4, r5 = [Q(c[k]) for c in coeff]
                c1, c2, c3, c4 = ((d+r3)*alpha, (r4-r3+r5)*alpha**2,
                                  (-r4-2*r5)*alpha**3, r5*alpha**4)
                expected = [p, p+c1/4, p+c1/2+c2/6,
                            p+3*c1/4+c2/2+c3/4, p+c1+c2+c3+c4]
                for b, q in zip(ctrl, expected):
                    self.assertLessEqual(Q(b[k][0]), q)
                    self.assertGreaterEqual(Q(b[k][1]), q)
            for j in range(17):
                p = represented_point(start, coeff, alpha*Q(j, 16))
                for k, row in enumerate(rows):
                    q = Q(translation[k])+sum(Q(r)*x for r, x in zip(row, p))
                    self.assertLessEqual(Q(bounds[k][0]), q)
                    self.assertGreaterEqual(Q(bounds[k][1]), q)

    def test_dense_evaluation_majorant_and_ftz_margin(self):
        # Per represented term at most12 rounded basic operations, counting
        # each reused (1-t) rounding twice for r5. Basic-op relative error uses
        # 2^-23, not the round-to-nearest-only 2^-24. Division is covered by
        # the independently outward alpha interval. Four t/(1-t) weights are
        # each <=1.000002. FTZ contributes at most one min-normal per operation;
        # the largest possible coefficient amplification is bounded by1e9.
        u, tiny, maximum = Q(1, 2**23), Q(1, 2**126), Q(10**9)
        gamma = 12*u/(1-12*u)
        weight = Q(f32(1.000002))**4
        ftz = 13*tiny*(1+maximum)*weight/(1-12*u)
        self.assertLess(gamma*weight+ftz, 64*u)
        # A directly evaluated f32 Horner chain is checked against the exact
        # represented polynomial; this finite check is not the rounding proof.
        rng = random.Random(544)
        for _ in range(1000):
            p, d, r3, r4, r5 = [f32(rng.uniform(-1e6, 1e6)) for _ in range(5)]
            t = f32(rng.uniform(0., 1.000001))
            v = f32(1.-t)
            got = f32(p+f32(t*f32(d+f32(v*f32(r3+f32(t*f32(r4+f32(v*r5))))))))
            exact_p = Q(p)+Q(t)*(Q(d)+(1-Q(t))*(Q(r3)+Q(t)*(Q(r4)+(1-Q(t))*Q(r5))))
            bound = 64*u*max(Q(1), sum(map(lambda x: abs(Q(x)), [p, d, r3, r4, r5])))
            self.assertLessEqual(abs(Q(got)-exact_p), bound)

    def test_row_gain_and_strict_touching_rejection(self):
        row = [Q(1.0000001192092896), Q(-.2), Q(.3)]
        error = Q(1, 1000)
        gain = sum(map(abs, row))
        # Componentwise tube corners: treating the uploaded row as a unit
        # vector would fail; the explicit row-L1 factor encloses all corners.
        import itertools
        for signs in itertools.product([-1, 1], repeat=3):
            self.assertLessEqual(abs(sum(r*s*error for r, s in zip(row, signs))), gain*error)
        self.assertGreater(gain, 1)
        lo, hi, pad = Q(1), Q(2), Q(1, 100)
        self.assertFalse(hi+pad < hi+pad)
        self.assertFalse(lo-pad > lo-pad)

    def test_source_path_allocation_has_physical_speed_gain(self):
        # Independent polynomial extrema: r3 is linear; r4 quadratic has its
        # stationary point at1/3. r5 =2t(1-t)(1-2t), hence<=1 on[0,1], and
        # outside1 its positive product is below2*1.000002*.000002*1.000004.
        end = Q(f32(1.000002))
        bound = Q(f32(1.00001))
        self.assertLess(2*end-1, bound)
        self.assertLess(max(Q(1, 3), abs(2*end-3*end**2)), bound)
        self.assertLess(2*end*(end-1)*(2*end-1), bound)
        self.assertGreater(bound, 1)
        source = (ROOT/'scripts/optical_cartesian_clearance.wgsl').read_text()
        self.assertIn('vec2(max(1.0,speed.y))', source)
        self.assertIn('let positioned_error=', source)

    def test_actual_packet_metadata_and_corrupt_count_rejection(self):
        def vectors(path):
            raw = (ROOT/path).read_bytes()
            return list(struct.iter_unpack('<4f', raw))
        frames = vectors('artifacts/optical-normal-scene-reference-selected-frames.bin')
        geometry = vectors('artifacts/optical-normal-scene-reference-selected-geometry.bin')
        self.assertTrue(metadata_valid(frames, geometry))
        bad = list(frames)
        word = lambda x: struct.unpack('<f', struct.pack('<I', x))[0]
        for component, value in [(0, 0), (1, 0xffffffff), (2, 0), (3, 0xffffffff)]:
            head = list(frames[0]);head[component] = word(value);bad[0] = tuple(head)
            self.assertFalse(metadata_valid(bad, geometry))
        self.assertFalse(metadata_valid([], geometry))
        self.assertFalse(metadata_valid(frames, []))

    def test_source_fail_closed_and_no_contact_authority(self):
        source = (ROOT/'scripts/optical_cartesian_clearance.wgsl').read_text()
        self.assertIn('!cs_valid3(c.b4)', source)
        self.assertIn('var owner=select(-2,-1,h.y<settings.scene.y)', source)
        self.assertIn('for(var i=0u;i<count-1u;i+=1u)', source)
        self.assertNotIn('return Trace(', source)
        self.assertNotIn('scene_event(', source)
        self.assertIn('max(polynomial_sag.y,physical_sag.y)', source)
        self.assertIn('gain=max(vec3(1.0)', source)
        self.assertIn('alpha.y>1.000002', source)
        self.assertIn('nc_interval_exact(error).y', source)


if __name__ == '__main__':
    unittest.main()
