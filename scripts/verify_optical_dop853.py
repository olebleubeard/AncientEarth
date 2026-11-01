"""Independent DOP853 convergence reference for the declared refractive field.

This is a diagnostic, not a replacement runtime solver. WGS84 height and its
normal are obtained from the ellipsoid-normal Lagrange multiplier equation,
independently of the application coordinate inverse. Each source layer is a
separate smooth initial-value problem; directed height events switch the
one-sided gradient without blending or stepping over a discontinuity.

The equatorial Bouguer quadratures independently calibrate this numerical
method. Agreement under tighter tolerances/shorter steps is convergence
evidence, not a rigorous global error bound or a real-atmosphere accuracy claim.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path

import numpy as np
import scipy
from scipy.integrate import solve_ivp

ROOT = Path(__file__).resolve().parents[1]
A = 6378137.0
FLATTENING = 1.0 / 298.257223563
BETA2 = (1.0 - FLATTENING) ** 2


def geometry(position):
    """Solve p_i=q_i(1+lambda/a_i^2), q on the ellipsoid.

    With mu=lambda/a^2 and p scaled by a, the constraint is
    (px^2+py^2)/(1+mu)^2 + beta^2*pz^2/(beta^2+mu)^2 = 1.
    Then p-q=mu*a*N, where N is the unnormalized ellipsoid normal.
    The atmospheric domain is far from the interior normal singularities.
    """
    p = position / A
    rho2 = p[0] * p[0] + p[1] * p[1]
    z2 = p[2] * p[2]
    mu = np.sqrt(rho2 + z2 / BETA2) - 1.0
    for _ in range(12):
        e, k = 1.0 + mu, BETA2 + mu
        value = rho2 / (e * e) + BETA2 * z2 / (k * k) - 1.0
        derivative = -2.0 * (rho2 / (e * e * e) + BETA2 * z2 / (k * k * k))
        correction = value / derivative
        mu -= correction
        if abs(correction) <= 2e-17:
            break
    normal = p / np.array([1.0 + mu, 1.0 + mu, BETA2 + mu])
    length = np.linalg.norm(normal)
    return mu * A * length, normal / length


def integrate(case, tighter, *, stop_at_lower_domain=False):
    origin = np.array(case["origin_ecef_m"], dtype=float)
    direction = np.array(case["initial_direction"], dtype=float)
    direction /= np.linalg.norm(direction)
    base = case.get("profile_base_height_m", 0.0)
    heights = np.array([base + k["height_m"] for k in case["layers"]])
    refractivity = np.array([k["refractivity"] for k in case["layers"]])
    assert len(heights) >= 2 and np.all(np.diff(heights) > 0)
    # Position is displacement from the exact input eye; the last component is
    # optical excess integral, avoiding an unnecessarily large n~1 path state.
    state = np.r_[np.zeros(3), direction, 0.0]
    h, up = geometry(origin)
    layer = int(np.searchsorted(heights, h, side="right")) - 1
    near = np.flatnonzero(abs(heights - h) < 2e-8)
    if len(near):
        layer = int(near[0]) if direction @ up >= 0 else int(near[0]) - 1
    target_path = case.get("target_path_m")
    path = 0.0
    function_calls, accepted_steps, contacts = 0, 0, 0
    lower_domain_contact = False
    maximum_step = 500.0 if tighter else 2000.0
    atol = ([2e-9] * 3 + [5e-16] * 3 + [1e-11]) if tighter else ([2e-7] * 3 + [2e-14] * 3 + [1e-9])
    for _ in range(1000):
        if layer >= len(heights) - 1 or (target_path is not None and path >= target_path):
            break
        if layer < 0:
            anchor, n_minus_one, slope = heights[0], refractivity[0], 0.0
            lower, upper = -10000.0, heights[0]
        else:
            anchor, n_minus_one = heights[layer], refractivity[layer]
            lower, upper = heights[layer], heights[layer + 1]
            slope = (refractivity[layer + 1] - n_minus_one) / (upper - lower)

        def derivative(_path, value):
            height, normal = geometry(origin + value[:3])
            excess = n_minus_one + slope * (height - anchor)
            u = value[3:6]
            return np.r_[u, (normal - u * (u @ normal)) * (slope / (1.0 + excess)), excess]

        def upper_event(_path, value):
            return geometry(origin + value[:3])[0] - upper

        def lower_event(_path, value):
            return geometry(origin + value[:3])[0] - lower

        upper_event.direction, lower_event.direction = 1, -1
        upper_event.terminal = lower_event.terminal = True
        stop = 2_000_000.0 if target_path is None else target_path
        solution = solve_ivp(derivative, (path, stop), state, method="DOP853",
                             rtol=5e-14 if tighter else 2e-12, atol=atol,
                             max_step=maximum_step, events=(upper_event, lower_event))
        assert solution.success, solution.message
        assert solution.t[-1] > path, "layer event made no forward progress"
        path, state = float(solution.t[-1]), solution.y[:, -1]
        function_calls += solution.nfev
        accepted_steps += len(solution.t) - 1
        if solution.status == 0:
            assert target_path is not None, "reference exhausted its finite path domain"
            break
        contacts += 1
        if len(solution.t_events[0]):
            layer += 1
        else:
            if layer < 0 and stop_at_lower_domain:
                lower_domain_contact = True
                break
            assert layer >= 0, "reference left the lower supported domain"
            layer -= 1
    else:
        raise AssertionError("too many layer contacts")
    final_direction = state[3:6] / np.linalg.norm(state[3:6])
    result = {"endpoint_ecef_m": list(origin + state[:3]), "exit_direction": list(final_direction),
            "geometric_path_m": path, "optical_path_m": path + float(state[6]),
            "direction_norm_drift": float(abs(np.linalg.norm(state[3:6]) - 1.0)),
            "function_calls": function_calls, "accepted_steps": accepted_steps, "layer_contacts": contacts,
            "relative_tolerance": 5e-14 if tighter else 2e-12, "absolute_tolerances": atol,
            "maximum_step_m": maximum_step}
    if stop_at_lower_domain:
        result['terminal_kind'] = ('lower_domain' if lower_domain_contact else
                                   'fixed_path' if target_path is not None and path >= target_path else
                                   'atmosphere_exit')
    return result


def differences(a, b):
    return {"endpoint_m": float(np.linalg.norm(np.array(a["endpoint_ecef_m"]) - b["endpoint_ecef_m"])),
            "direction": float(np.linalg.norm(np.array(a["exit_direction"]) - b["exit_direction"])),
            "path_m": abs(a["geometric_path_m"] - b["geometric_path_m"]),
            "optical_path_m": abs(a["optical_path_m"] - b["optical_path_m"])}


def coordinate_checks():
    """Independent direct geodetic parametrization checks the inverse normal."""
    worst_height, worst_normal, count = 0.0, 0.0, 0
    e2 = FLATTENING * (2.0 - FLATTENING)
    for latitude in [-90.0, -60.0, -29.305549, 0.0, 29.305549, 60.0, 90.0]:
        phi = math.radians(latitude)
        radius = A / math.sqrt(1.0 - e2 * math.sin(phi) ** 2)
        for longitude in [-180.0, -94.767015, 0.0, 90.0, 179.9]:
            lam = math.radians(longitude)
            normal = np.array([math.cos(phi) * math.cos(lam), math.cos(phi) * math.sin(lam), math.sin(phi)])
            for height in [-1000.0, 0.0, 1.72, 5000.0, 120000.0]:
                position = normal * np.array([radius + height, radius + height, radius * (1.0 - e2) + height])
                actual_height, actual_normal = geometry(position)
                worst_height = max(worst_height, abs(actual_height - height))
                worst_normal = max(worst_normal, float(np.linalg.norm(actual_normal - normal)))
                count += 1
    assert worst_height < 5e-9 and worst_normal < 4e-16
    return {"count": count, "maximum_height_error_m": worst_height, "maximum_normal_error": worst_normal}


def analyze(case, exact=None):
    low, high = integrate(case, False), integrate(case, True)
    result = {"name": case["name"], "inputs": case, "reference": high,
              "coarser_reference": low, "convergence_difference": differences(low, high)}
    if exact is not None:
        result["bouguer_difference"] = differences(high, exact)
        assert result["bouguer_difference"]["endpoint_m"] < 0.00005, result
        assert result["bouguer_difference"]["path_m"] < 0.00005, result
        assert result["bouguer_difference"]["optical_path_m"] < 0.00005, result
        assert result["bouguer_difference"]["direction"] < 1e-10, result
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bouguer", type=Path, default=ROOT / "artifacts/optical-bouguer-reference.json")
    parser.add_argument("--scene", type=Path)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/optical-dop853-reference.json")
    args = parser.parse_args()
    exact = json.loads(args.bouguer.read_text())
    coordinates = coordinate_checks()
    calibration = [analyze(case, case) for case in exact["cases"]]
    cases = []
    if args.scene:
        scene = json.loads(args.scene.read_text())["cpu_reference_rays"]
        for ray in scene["rays"]:
            hit = ray["outcome"]
            if not hit.get("resolved") or hit.get("marker_id") is not None or hit.get("surface_kind") is not None:
                continue
            case = {"name": f"coastal pixel {ray['pixel_xy']}", "layers": scene["profile_layers"],
                    "profile_base_height_m": scene["profile_base_height_m"],
                    "origin_ecef_m": ray["origin_ecef_m"], "initial_direction": ray["initial_direction"]}
            result = analyze(case)
            result["production_midpoint_difference"] = differences(hit, result["reference"])
            prototypes = []
            for family, records in hit.get("step_diagnostic", {}).items():
                if not isinstance(records, list):
                    continue
                for record in records:
                    if "endpoint_ecef_m" not in record:
                        continue
                    values = dict(record, geometric_path_m=record["path_m"])
                    prototypes.append({"family": family, "maximum_step_m": record["maximum_step_m"],
                                       "attempts": record["attempts"],
                                       "difference": differences(values, result["reference"])})
            result["prototype_comparisons"] = prototypes
            cases.append(result)
    report = {"method": "Independent ellipsoid-normal coordinates and knot-event DOP853 convergence",
              "scipy_version": scipy.__version__, "coordinate_checks": coordinates,
              "calibration": calibration, "cases": cases,
              "scope": "Smooth one-sided layer equations; no terrain/prism collision. Bouguer calibration is independent high-precision truth; non-equatorial results are tighter convergence evidence, not a proven global bound."}
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(f"Verified {len(calibration)} independent calibrations and {len(cases)} coastal clear rays: {args.output}")


if __name__ == "__main__":
    main()
