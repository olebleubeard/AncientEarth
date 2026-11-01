"""Finite tests of the unused split-anchor map, with direct MP reference.

Host anchor residuals are manufactured exactly here. This neither certifies a
production inverse/georegistration nor changes any scene-contact allocation.
"""
import json
import unittest
import mpmath as mp
from test_optical_normal_map_interval import (
    A, ZERO, ONE, exact, add, sub, mul, div, square, rsqrt, vexact, vadd,
    vsub, scale, dot, polynomial, derivative, make_case, packed_interval,
    canonical, map_enclosure, f32,
)


def split(x):
    hi = f32(x)
    lo = f32(x-mp.mpf(hi))
    return (hi, lo), packed_interval(x-mp.mpf(hi)-mp.mpf(lo))


def anchor_enclosure(case, anchor, t):
    (a_hi, a_lo), a_error = anchor['a_split']
    (b_hi, b_lo), b_error = anchor['b_split']
    initial = (exact(case['start'][0]),
               sub(sub(sub(exact(case['start'][1]), exact(a_hi)), exact(a_lo)), a_error),
               sub(sub(sub(exact(case['start'][2]), exact(b_hi)), exact(b_lo)), b_error))
    delta = vadd(initial, polynomial((0., 0., 0.), case['coeff'], t))
    dq = derivative(case['coeff'], t)
    a0, b0 = add(add(exact(a_hi), exact(a_lo)), a_error), add(add(exact(b_hi), exact(b_lo)), b_error)
    a1, b1 = add(a0, delta[1]), add(b0, delta[2])
    r0 = rsqrt(add(ONE, add(square(a0), square(b0))))
    r1 = rsqrt(add(ONE, add(square(a1), square(b1))))
    v0 = (a0, ONE, sub(ZERO, b0))
    n0 = scale(v0, r0)
    delta_norm2 = add(mul(delta[1], add(mul(exact(2.), a0), delta[1])),
                      mul(delta[2], add(mul(exact(2.), b0), delta[2])))
    product = mul(r1, r0)
    inverse_s_sum = div(mul(product, product), add(r1, r0))
    dn = vsub(scale((delta[1], ZERO, sub(ZERO, delta[2])), r1),
              scale(v0, mul(delta_norm2, inverse_s_sum)))
    n = vadd(n0, dn)
    axis, e = case['axis_interval'], case['e_interval']
    z0, dz = dot(axis, n0), dot(axis, dn)
    z = add(z0, dz)
    d0, d1 = sub(ONE, mul(e, square(z0))), sub(ONE, mul(e, square(z)))
    ri0, ri1 = rsqrt(d0), rsqrt(d1)
    radius = mul(exact(float(A)), ri1)
    product = mul(ri0, ri1)
    dr = div(mul(mul(mul(mul(exact(float(A)), e), dz), add(z, z0)), mul(product, product)), add(ri1, ri0))
    ah, al = map(exact, case['anchor_split'])
    (vh, vl), ve = anchor['h_split']
    h = add(add(add(ah, al), delta[0]), case['anchor_residual'])
    dh = add(add(add(sub(ah, exact(vh)), sub(al, exact(vl))), delta[0]),
             sub(case['anchor_residual'], ve))
    tangent = vsub(dn, scale(axis, mul(e, dz)))
    initial_tangent = vsub(n0, scale(axis, mul(e, z0)))
    p = vadd(anchor['residual_interval'], vadd(vadd(scale(tangent, radius), scale(initial_tangent, dr)),
                                              vadd(scale(dn, h), scale(n0, dh))))
    w = (dq[1], ZERO, sub(ZERO, dq[2]))
    nprime = scale(vsub(w, scale(n, dot(n, w))), r1)
    meridian = vsub(axis, scale(n, z))
    coefficient = mul(div(mul(radius, e), d1), dot(nprime, axis))
    velocity = vsub(vadd(scale(n, dq[0]), scale(nprime, add(radius, h))), scale(meridian, coefficient))
    return p, velocity


def polynomial_mp(case, t):
    # Expanded power coefficients rather than nested interval transcription.
    result = []
    for j in range(3):
        a, b, c, d = [mp.mpf(x[j]) for x in case['coeff']]
        result.append(mp.mpf(case['start'][j])+(a+b)*t+(c-b+d)*t*t+(-c-2*d)*t**3+d*t**4)
    return result


def anchor_reference(case, anchor):
    n = [anchor['a'], mp.mpf(1), -anchor['b']]
    magnitude = mp.sqrt(sum(x*x for x in n))
    n = [x/magnitude for x in n]
    z = sum(x*y for x, y in zip(n, case['axis']))
    radius = A/mp.sqrt(1-case['e']*z*z)
    p = [radius*(x-case['e']*z*k)+anchor['h']*x for x, k in zip(n, case['axis'])]
    z_eye = case['axis'][1]
    radius_eye = A/mp.sqrt(1-case['e']*z_eye*z_eye)
    eye = [radius_eye*(x-case['e']*z_eye*k)+case['eye']*x
           for x, k in zip((mp.mpf(0), mp.mpf(1), mp.mpf(0)), case['axis'])]
    return [x-y for x, y in zip(p, eye)]


class AnchoredMap(unittest.TestCase):
    def test_near_and_distant_anchor_enclosures(self):
        maxima = {'anchored_width_m': 0., 'unanchored_width_m': 0.}
        count = 0
        with mp.workdps(90):
            for axis in [(0., 0., -1.), (0., 1., 0.), (.3, -.4, .5)]:
                for chart in [(0., 0.), (.01, -.02), (.35, -.4)]:
                    for height in [-9999.875, 0., 100000.123456789]:
                        for length in [.000001, .0001, .003]:
                            coeff = [[15., length, -length*.4], [-2., length*.03, length*.02],
                                     [1., -length*.01, length*.01], [.125, length*.002, -length*.003]]
                            case = make_case(axis, height, height-37., (.03125, *chart), coeff)
                            middle = polynomial_mp(case, mp.mpf('.5'))
                            residual = tuple(map(mp.mpf, ['.00000001', '-.00000002', '.00000003']))
                            anchor = dict(a=middle[1]+mp.mpf('1e-7'), b=middle[2]-mp.mpf('2e-7'),
                                          h=case['anchor']+middle[0]+mp.mpf('.25'), residual=residual,
                                          residual_interval=tuple(map(packed_interval, residual)))
                            anchor.update(a_split=split(anchor['a']), b_split=split(anchor['b']), h_split=split(anchor['h']))
                            origin = anchor_reference(case, anchor)
                            for t in [0., .5, 1.]:
                                p, dp = anchor_enclosure(case, anchor, exact(t))
                                expected = [x-y+r for x, y, r in zip(canonical(case, mp.mpf(t)), origin, residual)]
                                derivative_truth = [mp.diff(lambda q: canonical(case, q)[j], mp.mpf(t)) for j in range(3)]
                                for interval, value in zip(p+dp, expected+derivative_truth):
                                    self.assertLessEqual(mp.mpf(interval[0]), value)
                                    self.assertGreaterEqual(mp.mpf(interval[1]), value)
                                maxima['anchored_width_m'] = max(maxima['anchored_width_m'], *(hi-lo for lo, hi in p))
                                old, _ = map_enclosure(case, exact(t))
                                maxima['unanchored_width_m'] = max(maxima['unanchored_width_m'], *(hi-lo for lo, hi in old))
                                count += 1
        self.assertEqual(count, 243)
        print(json.dumps({'scope': 'manufactured certified anchors; scalar finite enclosure only',
                          'count': count, **maxima}, indent=2))

    def test_far_chart_close_anchor_precision(self):
        with mp.workdps(90):
            case = make_case((0., .6, -.8), 100., -26., (0., .35, -.4),
                             [[10., 1e-6, -1e-6], [0., 0., 0.], [0., 0., 0.], [0., 0., 0.]])
            q = polynomial_mp(case, mp.mpf('.5'))
            anchor = dict(a=q[1], b=q[2], h=case['anchor']+q[0],
                          residual_interval=(ZERO, ZERO, ZERO))
            anchor.update(a_split=split(anchor['a']), b_split=split(anchor['b']), h_split=split(anchor['h']))
            result, _ = anchor_enclosure(case, anchor, exact(.5))
            for lo, hi in result:
                self.assertLessEqual(lo, 0.)
                self.assertGreaterEqual(hi, 0.)
            width = max(hi-lo for lo, hi in result)
            self.assertLess(width, .0001)
            print(json.dumps({'far_chart_exact_anchor_width_m': width}))


if __name__ == '__main__':
    unittest.main()
