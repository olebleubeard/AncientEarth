"""Original 80-digit WGS84 coordinate fixtures for continuous distant flight.

No production implementation or downloaded data is used.  The exact decimal
WGS84 defining constants are evaluated with mpmath, then rounded once to f64.
Heights in these fixtures are numerical samples, not an application-domain
policy or a promise about physical geodetic accuracy in space.
"""
from __future__ import annotations

import argparse
from pathlib import Path

import mpmath as mp

ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "crates/earth-math/tests/fixtures/space_coordinates.rs"
LOCATIONS = [
    (-90, -73), (-89.999999, 179.999999), (-67.4, -179.999999),
    (-29.305549, 94.767015), (0, 0), (0, 180),
    (29.305549, -94.767015), (78.9, 73.4), (89.999999, -179.999999),
    (90, 137),
]
HEIGHTS = [0, 120_000, 5_000_000, 1_000_000_000, 149_597_870_700, 598_391_482_800]


def generate() -> str:
    mp.mp.dps = 80
    a = mp.mpf("6378137")
    f = 1 / mp.mpf("298.257223563")
    e2 = f * (2 - f)
    rows = [
        "// Original 80-digit results: scripts/verify_space_coordinates.py.",
        f"// mpmath {mp.__version__}; exact decimal WGS84 a and inverse flattening.",
        "// latitude degrees, longitude degrees, height metres, ECEF metres.",
        "#[rustfmt::skip]",
        "pub const CASES: &[(f64, f64, f64, [f64; 3])] = &[",
    ]
    for height in HEIGHTS:
        for latitude, longitude in LOCATIONS:
            phi = mp.mpf(str(latitude)) * mp.pi / 180
            lam = mp.mpf(str(longitude)) * mp.pi / 180
            n = a / mp.sqrt(1 - e2 * mp.sin(phi) ** 2)
            h = mp.mpf(height)
            xyz = [
                (n + h) * mp.cos(phi) * mp.cos(lam),
                (n + h) * mp.cos(phi) * mp.sin(lam),
                (n * (1 - e2) + h) * mp.sin(phi),
            ]
            # At a pole, exact x=y=0 and longitude is not observable from ECEF.
            if abs(latitude) == 90:
                xyz[0] = xyz[1] = mp.mpf(0)
            coordinate = "[" + ", ".join(repr(float(x)) for x in xyz) + "]"
            rows.append(
                f"    ({float(latitude)!r}, {float(longitude)!r}, "
                f"{float(height)!r}, {coordinate}),"
            )
    return "\n".join(rows + ["];", ""])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    expected = generate()
    if args.check:
        if OUTPUT.read_text(encoding="utf-8") != expected:
            raise SystemExit("Space coordinate fixtures differ")
        print(f"Space coordinate fixtures match ({len(LOCATIONS) * len(HEIGHTS)} cases)")
    else:
        OUTPUT.write_text(expected, encoding="utf-8", newline="\n")
        print(f"Wrote {OUTPUT}")


if __name__ == "__main__":
    main()
