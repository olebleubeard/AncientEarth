"""Independent high-precision reference for a general encoded SPD quadric.

Input cases contain name, matrix_rows (3x3), q (3) and f0, describing
F(d)=f0+2*q.d+d.A.d at the query d=0. No rank-one/WGS84 inverse is used.
Optional dependencies are pinned in requirements-optics.txt.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

import mpmath as mp


def reference(matrix_rows, q_values, f0_value, digits=80):
    with mp.workdps(digits):
        # JSON floating values denote their actual decoded IEEE values. mpf
        # imports those binary values exactly at this precision; decimal
        # strings remain available for manufactured high-precision cases.
        number = lambda x: mp.mpf(x)
        a = mp.matrix([[number(x) for x in row] for row in matrix_rows])
        q = mp.matrix([number(x) for x in q_values])
        f0 = number(f0_value)
        if a.rows != 3 or a.cols != 3 or q.rows != 3:
            raise ValueError("reference requires a 3x3 matrix and three-vector")
        if not all(mp.isfinite(x) for x in [*a, *q, f0]):
            raise ValueError("nonfinite quadratic input")
        if a != a.T:
            raise ValueError("encoded matrix has skew; no silent symmetrization")
        eigenvalues = mp.eigsy(a, eigvals_only=True)
        if eigenvalues[0] <= 0:
            raise ValueError("quadratic matrix must be positive definite")
        level = (q.T * mp.lu_solve(a, q))[0] - f0
        if level <= 0:
            raise ValueError("quadratic does not define a nondegenerate ellipsoid")
        identity = mp.eye(3)

        def at(mu):
            # d=-mu*v, v=q+A*d is the actual normal covector at the foot.
            # Evaluate the GENERAL quadratic, not the implementation's
            # specialized rank-one scalar equation or iteration.
            v = mp.lu_solve(identity + mu * a, q)
            d = -mu * v
            value = f0 + 2 * (q.T * d)[0] + (d.T * a * d)[0]
            return value, v

        if f0 == 0:
            mu = mp.mpf(0)
        else:
            if f0 > 0:
                low, high = mp.mpf(0), mp.mpf(1)
                for _ in range(128):
                    if at(high)[0] < 0:
                        break
                    high *= 2
                else:
                    raise ValueError("outside root was not bracketed")
            else:
                high = mp.mpf(0)
                pole = -1 / eigenvalues[2]
                low = pole / 2
                for _ in range(128):
                    if at(low)[0] > 0:
                        break
                    low = (low + pole) / 2
                else:
                    raise ValueError("normal-tube inside root was not bracketed")
            # Monotonicity on I+mu*A positive definite gives a unique root.
            # Retain a bracket; stopping is independent of the candidate.
            for _ in range(4 * digits + 64):
                mid = (low + high) / 2
                if at(mid)[0] > 0:
                    low = mid
                else:
                    high = mid
                if high - low <= mp.power(10, -digits + 15):
                    break
            mu = (low + high) / 2
        residual, covector = at(mu)
        norm = mp.norm(covector)
        if norm == 0:
            raise ValueError("no unique surface normal")
        return {
            "height_m": mp.nstr(mu * norm, digits - 8),
            "normal": [mp.nstr(x / norm, digits - 8) for x in covector],
            "multiplier_m": mp.nstr(mu, digits - 8),
            "quadratic_residual_m": mp.nstr(residual, digits - 8),
        }


def self_check():
    # General triaxial ellipsoid with a rational orthogonal rotation. Prescribe
    # its surface normal and signed offset independently of the inverse solve.
    with mp.workdps(120):
        rotation = mp.matrix([[-mp.mpf(2)/3, mp.mpf(2)/15, mp.mpf(11)/15],
                              [mp.mpf(2)/3, -mp.mpf(1)/3, mp.mpf(2)/3],
                              [mp.mpf(1)/3, mp.mpf(14)/15, mp.mpf(2)/15]])
        assert mp.norm(rotation.T * rotation - mp.eye(3)) < mp.mpf('1e-115')
        a = rotation * mp.diag([mp.mpf(1)/6378137, mp.mpf(1)/6388137,
                                mp.mpf(6378137)/mp.mpf('6356752.314245179')**2]) * rotation.T
        n = mp.matrix([2, -3, 5]) / mp.sqrt(38)
        kappa = mp.sqrt(mp.mpf(6378137) / (n.T * mp.lu_solve(a, n))[0])
        for h in map(mp.mpf, ['-10000', '-0.000001', '0', '0.000001', '120000']):
            q = kappa * n + h * a * n
            f0 = 2 * h * kappa + h * h * (n.T * a * n)[0]
            result = reference([[str(a[i,j]) for j in range(3)] for i in range(3)],
                               [str(x) for x in q], str(f0), 90)
            assert abs(mp.mpf(result['height_m']) - h) < mp.mpf('1e-70')
            assert mp.norm(mp.matrix(result['normal']) - n) < mp.mpf('1e-70')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    self_check()
    if args.input is None:
        print('Five independent rotated triaxial reference self-checks passed.')
        return
    if args.output is None:
        parser.error('--output is required with --input')
    raw = args.input.read_bytes()
    data = json.loads(raw)
    rows = []
    for case in data['cases']:
        coarse = reference(case['matrix_rows'], case['q'], case['f0'], 80)
        fine = reference(case['matrix_rows'], case['q'], case['f0'], 110)
        with mp.workdps(115):
            error_h = abs(mp.mpf(coarse['height_m']) - mp.mpf(fine['height_m']))
            error_n = mp.norm(mp.matrix(coarse['normal']) - mp.matrix(fine['normal']))
            assert error_h < mp.mpf('1e-60') and error_n < mp.mpf('1e-60')
        rows.append({'name': case['name'], 'inputs': case, 'reference': fine,
                     'precision_recheck_height_m': str(error_h),
                     'precision_recheck_normal': str(error_n)})
    result = {'scope': 'Independent arbitrary symmetric-SPD encoded-quadric normal coordinates, using full matrix solves and monotone high-precision bracketing. Encoded input agreement is not physical Earth accuracy or a complete GPU arithmetic bound.',
              'input_sha256': hashlib.sha256(raw).hexdigest(), 'cases': rows}
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(f'{len(rows)} encoded-quadric reference cases written to {args.output}')


if __name__ == '__main__':
    main()
