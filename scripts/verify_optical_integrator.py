"""Independent high-precision equatorial ray fixtures, not a runtime solver.

For a radial index n(r), d(n r x u)/ds = 0. Its magnitude B gives
dtheta/dr = B/(r sqrt((nr)^2-B^2)), ds/dr = nr/sqrt((nr)^2-B^2).
The optical integral has one additional factor n. At the WGS84 equator,
geodetic height is exactly r-a and the equatorial normal is radial, so these
quadratures independently test the three-dimensional implemented equations.

Each source knot is a separate integration interval. Square-root substitutions
regularize a tangent launch or a simple radial turning point. No production
integrator, coordinate inverse, or downloaded data is used. Stored input
numbers are promoted from their actual f64 values; launch direction is
normalized in high precision. The Rust ray constructor can add its own finite
f64 normalization rounding, which is not covered by the quadrature check.
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

import mpmath as mp

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "artifacts/optical-bouguer-reference.json"
A = 6378137
VACUUM = [(0.0, 0.0), (120000.0, 0.0)]
LAYERED = [(0.0, .0003), (2.0, .0002999),
           (80.0, .00029), (1000.0, .00026), (8000.0, .00011),
           (25000.0, .000018), (60000.0, .000001), (120000.0, 0.0)]
THIN = [(0.0, .0003), (30.0, .000299), (30.02, .0002989995),
        (80.0, .000293), (2000.0, .00023), (120000.0, 0.0)]
TURNING = [(0.0, .0005), (1000.0, .0003), (2000.0, .0001),
           (3000.0, 0.0), (120000.0, 0.0)]
TURNING_KNOT = [(0.0, .0004), (1000.0, .0003), (2000.0, .0001),
                (3000.0, 0.0), (120000.0, 0.0)]
CASES = [
    ("vacuum vertical", VACUUM, 100.0, 90.0, False),
    ("vacuum tangent", VACUUM, 100.0, 0.0, False),
    ("vacuum below reference", VACUUM, -20.0, .75, False),
    ("layered near horizon", LAYERED, 1.72, .2, False),
    ("layered steep below reference", LAYERED, -20.0, 60.0, False),
    ("thin layer near horizon", THIN, 1.72, .15, False),
    ("outward turn and return to initial height", TURNING, 1000.0, .2, True),
    ("outward turn then cross departure gradient knot", TURNING_KNOT,
     1000.0, .2, True, 950.0),
]


def calculate(spec, digits):
    name, knots, height, angle, turning, *return_heights = spec
    with mp.workdps(digits):
        # Use the same rounded input direction at every reference precision.
        with mp.workdps(100):
            direction = ([1.0, 0.0, 0.0] if angle == 90 else
                         [float(mp.sin(mp.radians(angle))),
                          float(mp.cos(mp.radians(angle))), 0.0])
        u = [mp.mpf(x) for x in direction]
        norm = mp.sqrt(sum(x*x for x in u))
        u = [x/norm for x in u]
        r0 = mp.mpf(float(A + height))
        radii = [mp.mpf(A) + mp.mpf(h) for h, _ in knots]
        indices = [1 + mp.mpf(n) for _, n in knots]

        def layer(r):
            if r < radii[0]:
                return indices[0], mp.mpf(0)
            i = max(0, min(len(radii)-2,
                          sum(x <= r for x in radii)-1))
            slope = (indices[i+1]-indices[i])/(radii[i+1]-radii[i])
            return indices[i] + slope*(r-radii[i]), slope

        n0, _ = layer(r0)
        invariant = n0*r0*u[1]
        radius_end = radii[-1]
        turning_height = None
        if turning:
            lo, hi = r0, mp.mpf(A + 2000)
            assert layer(lo)[0]*lo > invariant > layer(hi)[0]*hi
            for _ in range(digits*4):
                middle = (lo+hi)/2
                if layer(middle)[0]*middle > invariant:
                    lo = middle
                else:
                    hi = middle
            radius_end = (lo+hi)/2
            turning_height = radius_end-A

        def integrate(start, end, turn_at_end=False, tangent_at_start=False):
            # Integrate every smooth source interval separately, including
            # the distinct lower branch after returning through the launch knot.
            boundaries = [start] + [r for r in radii if start < r < end] + [end]
            totals = [mp.mpf(0)]*3
            for left, right in zip(boundaries, boundaries[1:]):
                middle = (left+right)/2
                nm, slope = layer(middle)
                n = lambda r: nm+slope*(r-middle)
                tangent = tangent_at_start and left == start
                turn_end = turn_at_end and right == end

                def components(r):
                    nr = n(r)*r
                    radicand = (nr-invariant)*(nr+invariant)
                    assert radicand > 0
                    root = mp.sqrt(radicand)
                    return [invariant/(r*root), nr/root, n(r)*nr/root]

                def transformed(t, component):
                    endpoint = right if turn_end else left
                    if not t:
                        ne = n(endpoint)
                        derivative = ne+endpoint*slope
                        denominator = mp.sqrt(2*invariant*abs(derivative))
                        return [2*invariant/(endpoint*denominator),
                                2*invariant/denominator,
                                2*ne*invariant/denominator][component]
                    r = right-t*t if turn_end else left+t*t
                    return 2*t*components(r)[component]

                for k in range(3):
                    if tangent or turn_end:
                        value = mp.quadgl(lambda t: transformed(t, k),
                                          [0, mp.sqrt(right-left)])
                    else:
                        value = mp.quad(lambda r: components(r)[k], [left, right])
                    totals[k] += value
            return totals

        theta, path, optical = integrate(r0, radius_end, turning,
                                         not turning and angle == 0)
        if turning:
            theta, path, optical = 2*theta, 2*path, 2*optical
            radius_end = r0
            if return_heights:
                radius_end = mp.mpf(A) + mp.mpf(return_heights[0])
                assert radius_end < r0
                extra = integrate(radius_end, r0)
                theta, path, optical = [value + addition for value, addition
                                        in zip((theta, path, optical), extra)]
        end_n, _ = layer(radius_end)
        tangent_component = invariant/(end_n*radius_end)
        radial_component = mp.sqrt(1-tangent_component*tangent_component)
        if turning:
            radial_component = -radial_component
        c, s = mp.cos(theta), mp.sin(theta)
        position = [radius_end*c, radius_end*s, mp.mpf(0)]
        final_direction = [radial_component*c-tangent_component*s,
                           radial_component*s+tangent_component*c, mp.mpf(0)]
        if knots == VACUUM:
            analytic = -r0*u[0] + mp.sqrt(radius_end**2-(r0*u[1])**2)
            assert abs(path-analytic) < mp.mpf("1e-45")
            assert max(abs(position[i] - ([r0, 0, 0][i]+u[i]*path))
                       for i in range(3)) < mp.mpf("1e-40")
        return {
            "name": name, "layers": [{"height_m": h, "refractivity": n} for h, n in knots],
            "profile_base_height_m": 0.0,
            "origin_ecef_m": [float(r0), 0.0, 0.0], "initial_direction": direction,
            "target": ("descend_through_departure_knot" if return_heights else
                       "return_to_initial_height" if turning else "atmosphere_top"),
            "top_height_m": knots[-1][0], "geometric_path_m": float(path),
            **({"target_path_m": float(path)} if turning else {}),
            "optical_path_m": float(optical), "endpoint_ecef_m": list(map(float, position)),
            "exit_direction": list(map(float, final_direction)),
            "turning_height_m": None if turning_height is None else float(turning_height),
            "_high_precision": [mp.nstr(v, 70) for v in [path, optical, *position, *final_direction]],
        }


def generate():
    cases = []
    for spec in CASES:
        low, high = calculate(spec, 60), calculate(spec, 85)
        with mp.workdps(90):
            discrepancy = max(abs(mp.mpf(x)-mp.mpf(y))
                              for x, y in zip(low['_high_precision'], high['_high_precision']))
            assert discrepancy < mp.mpf('1e-35'), (spec[0], discrepancy)
        high['precision_recheck_max_absolute'] = float(discrepancy)
        del high['_high_precision']
        cases.append(high)
    return {"method": "Equatorial Bouguer invariant; separate high-precision quadrature per source interval",
            "precision_decimal_digits": [60, 85], "mpmath_version": mp.__version__,
            "scope": "Numerical references for a radial equatorial refractive field. No terrain/marker occlusion, native GPU or real-weather accuracy claim. Turning cases stop either at first return to the initial radius or after crossing its gradient discontinuity into a distinct lower branch. Input direction has ordinary f64 normalization rounding in a runtime solver.",
            "cases": cases}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--output', type=Path, default=OUTPUT)
    args = parser.parse_args()
    text = json.dumps(generate(), indent=2) + '\n'
    if args.check:
        assert args.output.read_text() == text, 'Bouguer reference differs'
    else:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(text)
    print(f'Verified {len(CASES)} independent Bouguer cases: {args.output}')
