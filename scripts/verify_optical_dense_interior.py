"""Independent fixed-path interior references for diagnostic dense curves.

Uses the previously Bouguer-calibrated independent ellipsoid/DOP853 solver,
not the application's coordinate inverse, DP tableau or dense extension.
The target planes pass through reference interior points and are normal to
the reference ray tangent. A global curvature bound proves transversality
over each finite trial. Numerical convergence is not a rigorous ODE enclosure.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import math
from pathlib import Path

import numpy as np

import verify_optical_dop853 as oracle

ROOT = Path(__file__).resolve().parents[1]


def pose(latitude, longitude, height, bearing, elevation):
    phi, lam, azimuth, angle = map(math.radians, (latitude, longitude, bearing, elevation))
    e2 = oracle.FLATTENING * (2.0 - oracle.FLATTENING)
    normal = np.array([math.cos(phi) * math.cos(lam), math.cos(phi) * math.sin(lam), math.sin(phi)])
    east = np.array([-math.sin(lam), math.cos(lam), 0.0])
    north = np.cross(normal, east)
    n = oracle.A / math.sqrt(1.0 - e2 * math.sin(phi) ** 2)
    position = normal * np.array([n + height, n + height, n * (1.0 - e2) + height])
    direction = normal * math.sin(angle) + (east * math.sin(azimuth) + north * math.cos(azimuth)) * math.cos(angle)
    return position.tolist(), direction.tolist()


def generate():
    calibration = ROOT / "artifacts/optical-dopri-dense-dop853-reference.json"
    previous = json.loads(calibration.read_text())
    assert len(previous["calibration"]) == 8
    for check in previous["calibration"]:
        assert check["bouguer_difference"]["endpoint_m"] < 0.00005
        assert check["bouguer_difference"]["direction"] < 1e-10
    specifications = [
        ("vacuum tangent", [(0.0, 0.0), (120000.0, 0.0)], (0.0, 0.0, 1000.0, 90.0, 0.0)),
        ("weak gradient near tangent", [(0.0, 0.0004), (120000.0, 0.0)], (0.0, 0.0, 1000.0, 90.0, 0.01)),
        ("single-interval radial turn", [(0.0, 0.0005), (2000.0, 0.0001), (120000.0, 0.0)], (0.0, 0.0, 1000.0, 90.0, 0.1)),
        ("oblique non-equatorial", [(0.0, 0.0004), (120000.0, 0.0)], (-62.0, 70.0, 5000.0, 55.0, 0.05)),
    ]
    cases, trials = [], []
    for label, layers, parameters in specifications:
        origin, initial = pose(*parameters)
        base = {"layers": [{"height_m": h, "refractivity": n} for h, n in layers],
                "profile_base_height_m": 0.0, "origin_ecef_m": origin, "initial_direction": initial}
        minimum_index = 1.0 + min(n for _, n in layers)
        maximum_curvature = max(abs(n1 - n0) / (h1 - h0) for (h0, n0), (h1, n1) in zip(layers, layers[1:])) / minimum_index
        for length in (20000.0, 100000.0):
            full_case = dict(base, name=f"{label}, trial {length:g} m", target_path_m=length)
            full = oracle.integrate(full_case, True)
            assert full["layer_contacts"] == 0, full_case
            full_height, full_normal = oracle.geometry(np.array(full["endpoint_ecef_m"]))
            vertical = float(np.dot(full["exit_direction"], full_normal))
            if label == "single-interval radial turn" and length == 100000.0:
                assert vertical < 0.0, "The full trial must pass a genuine radial turn"
            trials.append({"inputs": full_case, "reference": full, "final_height_m": full_height,
                           "final_vertical_direction": vertical})
            # |u(s)-rotation from u(target)| is bounded by curvature times path.
            # The angle is below pi/2, so the tangent-normal plane is monotone.
            angle_bound = maximum_curvature * length
            assert angle_bound < math.pi / 2
            transverse_lower = math.cos(angle_bound)
            for fraction in (0.25, 0.5, 0.75):
                target = length * fraction
                inputs = dict(base, name=f"{label}, trial {length:g} m, fraction {fraction:g}", target_path_m=target)
                coarse, fine = oracle.integrate(inputs, False), oracle.integrate(inputs, True)
                difference = oracle.differences(coarse, fine)
                assert difference["endpoint_m"] < 1e-7, difference
                assert difference["path_m"] < 1e-9, difference
                assert difference["optical_path_m"] < 1e-7, difference
                assert difference["direction"] < 1e-12, difference
                assert fine["layer_contacts"] == 0
                if label == "vacuum tangent":
                    unit = np.array(initial) / np.linalg.norm(initial)
                    exact = np.array(origin) + target * unit
                    assert np.linalg.norm(np.array(fine["endpoint_ecef_m"]) - exact) < 1e-8
                    assert fine["optical_path_m"] == target
                height, normal = oracle.geometry(np.array(fine["endpoint_ecef_m"]))
                cases.append(dict(inputs, trial_path_m=length, target_fraction=fraction,
                                  endpoint_ecef_m=fine["endpoint_ecef_m"], exit_direction=fine["exit_direction"],
                                  geometric_path_m=fine["geometric_path_m"], optical_path_m=fine["optical_path_m"],
                                  plane_origin_ecef_m=fine["endpoint_ecef_m"], plane_normal_ecef=fine["exit_direction"],
                                  target_height_m=height, target_vertical_direction=float(np.dot(fine["exit_direction"], normal)),
                                  ray_curvature_bound_per_m=maximum_curvature, plane_transverse_lower_bound=transverse_lower,
                                  reference=fine, convergence_difference=difference))
    return {"method": "Independent ellipsoid-normal DOP853 fixed-path interior convergence references",
            "calibration_artifact": str(calibration.relative_to(ROOT)),
            "calibration_sha256": hashlib.sha256(calibration.read_bytes()).hexdigest(),
            "reference_source_sha256": hashlib.sha256((ROOT / "scripts/verify_optical_dop853.py").read_bytes()).hexdigest(),
            "scope": "24 finite points in eight smooth-source-interval trials. Tight/coarse convergence and prior eight Bouguer calibrations; not an exact-ODE enclosure, full scene collision test or GPU result. Plane transversality uses a global physical curvature bound, which does not automatically apply to a numerical polynomial.",
            "trials": trials, "cases": cases}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts/optical-dense-interior-reference.json")
    args = parser.parse_args()
    report = generate()
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    print(f"Verified {len(report['cases'])} interior points in {len(report['trials'])} trials: {args.output}")


if __name__ == "__main__":
    main()
