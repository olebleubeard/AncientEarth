"""Independent high-precision RHS differences; never subtract rounded RHS values."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import mpmath as mp

mp.mp.dps = 90


def exact(bits):
    value = struct.unpack("<d", struct.pack("<Q", bits))[0]
    p, q = value.as_integer_ratio()
    return mp.mpf(p) / q


def vector(bits):
    return [exact(x) for x in bits]


def dot(a, b):
    return sum(x*y for x, y in zip(a, b))


def norm(a):
    return mp.sqrt(dot(a, a))


def original(axis, u, h, refractivity, gradient, q, a, eccentricity):
    # Independent principal-curvature / shape-operator construction.
    n = [q[0], mp.mpf(1), -q[1]]
    length = norm(n)
    n = [x/length for x in n]
    z = dot(axis, n)
    vertical = dot(u, n)
    d = 1-eccentricity*z*z
    prime_radius = a/mp.sqrt(d)
    meridian_radius = a*(1-eccentricity)/d**mp.mpf("1.5")
    polar_tangent = [k-z*ni for k, ni in zip(axis, n)]
    transverse = [ui-vertical*ni for ui, ni in zip(u, n)]
    correction = a*eccentricity/(d**mp.mpf("1.5")*(prime_radius+h)*(meridian_radius+h))
    normal_s = [t/(prime_radius+h)+correction*dot(polar_tangent,u)*p
                for t, p in zip(transverse, polar_tangent)]
    chart_h = [(normal_s[0]-q[0]*normal_s[1])/(n[1]*vertical),
               (-normal_s[2]-q[1]*normal_s[1])/(n[1]*vertical)]
    direction_h = [gradient/(1+refractivity)*(ni/vertical-ui) for ni,ui in zip(n,u)]
    return chart_h, direction_h, 1/vertical, refractivity/vertical


def review(path):
    raw = path.read_bytes()
    data = json.loads(raw)
    a, e = exact(data["a_bits"]), exact(data["eccentricity_squared_bits"])
    maxima = {name: mp.mpf(0) for name in ["geometric_chart_relative", "chart_delta_relative",
              "direction_delta_relative", "path_delta_relative", "optical_excess_relative"]}
    maximum_absolute = dict(maxima)
    tiny_count = zero_count = 0
    launch_defect = mp.mpf(0)
    for case in data["cases"]:
        axis, launch = vector(case["axis_bits"]), vector(case["launch_bits"])
        q0, dq, du = vector(case["q0_bits"]), vector(case["dq_bits"]), vector(case["du_bits"])
        assert norm(axis) == 1  # This fixture uses the three exact coordinate axes.
        launch_defect = max(launch_defect, abs(norm(launch)-1))
        h, refractivity, gradient = [exact(case[k+"_bits"]) for k in ["height","refractivity","gradient"]]
        q1, u1 = [x+y for x,y in zip(q0,dq)], [x+y for x,y in zip(launch,du)]
        baseline = original(axis,launch,h,0,0,q0,a,e)
        full = original(axis,u1,h,refractivity,gradient,q1,a,e)
        expected = {"geometric_chart":baseline[0], "chart_delta":[x-y for x,y in zip(full[0],baseline[0])],
                    "direction_delta":full[1], "path_delta":[full[2]-baseline[2]], "optical_excess":[full[3]]}
        tiny_count += 0 < norm(dq) < mp.mpf(2)**-45
        zero_count += norm(dq) == 0
        for key, reference in expected.items():
            encoded = case["result_bits"][key]
            got = vector(encoded if isinstance(encoded,list) else [encoded])
            error = norm([x-y for x,y in zip(got,reference)])
            magnitude = norm(reference)
            if magnitude == 0:
                assert error == 0, (key, case, error)
                relative = mp.mpf(0)
            else:
                relative = error/magnitude
                # Relative to the small difference itself, not to the baseline.
                assert relative < mp.mpf("5e-12"), (key, relative, case)
            metric=key+"_relative"
            maxima[metric]=max(maxima[metric],relative)
            maximum_absolute[metric]=max(maximum_absolute[metric],error)
    return {"input":str(path),"input_sha256":hashlib.sha256(raw).hexdigest(),
            "cases":len(data["cases"]),"zero_deviation_cases":zero_count,
            "tiny_chart_deviation_cases_below_2_power_minus_45":tiny_count,
            "maximum_relative_errors":{k:float(v) for k,v in maxima.items()},
            "maximum_absolute_errors":{k.removesuffix("_relative"):float(v) for k,v in maximum_absolute.items()},
            "maximum_encoded_launch_norm_defect":float(launch_defect),
            "reference":"90-digit independent principal-curvature shape operator, evaluated at exact encoded operands and their real split sums; subtraction occurs before output rounding",
            "scope":"Finite RHS algebra evidence only, for upward well-conditioned states and exact unit coordinate polar axes. Encoded launch normalization error is reported. No trajectory, GPU, whole-cell, or global accuracy claim. Replacing the numerical geometric baseline with an exact line requires a separate bounded consistency term; f64 rounded q0+dq reconstruction is not ignored by the oracle."}


if __name__ == "__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("input",type=Path)
    parser.add_argument("--output",type=Path,required=True)
    args=parser.parse_args()
    result=review(args.input)
    args.output.write_text(json.dumps(result,indent=2)+"\n")
    print(json.dumps(result,indent=2))
