"""Independent heliocentric states and model-pole fixtures from CSPICE/ERFA.

Uses the existing unmodified DE440s asset. No downloads or production Rust
algorithms are invoked. Optional Python requirements are the same as the
existing astronomy reference generators.
"""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

import erfa
import numpy as np
import spiceypy as spice

ROOT = Path(__file__).resolve().parents[1]
KERNEL = ROOT / "assets/astronomy/de440s.bsp"
OUTPUT = ROOT / "crates/earth-math/tests/fixtures/orbital_reference.rs"
KERNEL_SHA256 = "c1c7feeab882263fc493a9d5a5b2ddd71b54826cdf65d8d17a76126b260a49f2"
TT_SECONDS = [-4_700_000_000.0, -3_155_760_000.125, 0.0,
              845_856_123.456, 3_155_760_000.75, 4_700_000_000.0]


def array(values) -> str:
    return "[" + ", ".join(repr(float(x)) for x in values) + "]"


def generate() -> str:
    if hashlib.sha256(KERNEL.read_bytes()).hexdigest() != KERNEL_SHA256:
        raise ValueError("Original DE440s hash differs")
    rows = [
        "// Original numerical results: scripts/verify_orbital_reference.py.",
        f"// CSPICE/spiceypy {spice.__version__}; ERFA/pyerfa {erfa.__version__}.",
        f"// Existing DE440s SHA256: {KERNEL_SHA256}",
        "// Sun-centered simultaneous J2000 states; IAU 2006/2000B model CIP.",
        "#[rustfmt::skip]",
        "pub type OrbitReference = (f64, [f64;3], [f64;3], [f64;3], [f64;3], [f64;3], [f64;3], f64);",
        "#[rustfmt::skip]",
        "pub const CASES: &[OrbitReference] = &[",
    ]
    spice.kclear()
    spice.furnsh(str(KERNEL.resolve()))
    try:
        for tt in TT_SECONDS:
            tdb = tt + erfa.dtdb(2451545.0, tt / 86400.0, 0.0, 0.0, 0.0, 0.0)
            earth, _ = spice.spkez(399, tdb, "J2000", "NONE", 10)
            moon, _ = spice.spkez(301, tdb, "J2000", "NONE", 10)
            earth *= 1000.0
            moon *= 1000.0
            # Independent original ERFA routines, including the published
            # P03 compatibility adjustment applied to this app's 2000B model.
            centuries = tt / (36525.0 * 86400.0)
            longitude, obliquity = erfa.nut00b(2451545.0, tt / 86400.0)
            factor = -2.7774e-6 * centuries
            longitude *= 1.0 + 0.4697e-6 + factor
            obliquity *= 1.0 + factor
            *_, rbpn = erfa.pn06(2451545.0, tt / 86400.0, longitude, obliquity)
            pole = rbpn[2]
            plane = np.cross(earth[:3], earth[3:])
            plane /= np.linalg.norm(plane)
            tilt = np.arccos(np.clip(pole @ plane, -1.0, 1.0))
            rows.append("    (" + ", ".join([
                repr(tt), array(earth[:3]), array(earth[3:]),
                array(moon[:3]), array(moon[3:]), array(pole), array(plane),
                repr(float(tilt)),
            ]) + "),")
        rows += ["];", "", "#[rustfmt::skip]", "pub const TRAIL: &[(f64, [f64;3])] = &["]
        center = TT_SECONDS[3]
        start = center - 365.25 * 86400.0 / 2.0
        for index in [0, 1, 2, 243, 487, 730, 1000, 1460, 1461]:
            tt = start + index * 21600.0
            tdb = tt + erfa.dtdb(2451545.0, tt / 86400.0, 0.0, 0.0, 0.0, 0.0)
            state, _ = spice.spkez(399, tdb, "J2000", "NONE", 10)
            rows.append(f"    ({tt!r}, {array(state[:3] * 1000.0)}),")
    finally:
        spice.kclear()
    return "\n".join(rows + ["];", ""])


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    result = generate()
    if args.check:
        if OUTPUT.read_text(encoding="utf-8") != result:
            raise SystemExit("Orbital references differ from original CSPICE/ERFA")
    else:
        OUTPUT.write_text(result, encoding="utf-8", newline="\n")
    print(f"Verified {len(TT_SECONDS)} independent heliocentric state/pole/tilt cases")


if __name__ == "__main__":
    main()
