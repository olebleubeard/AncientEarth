"""Independent lunar emission-event pole/orbit-plane fixtures from CSPICE.

Uses existing source kernels and Python reference dependencies; no download,
production coefficient table, or Rust implementation is invoked.
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
PCK = ROOT / ".cache/astronomy/pck00011.tpc"
OUTPUT = ROOT / "crates/earth-math/tests/fixtures/lunar_reference.rs"
INPUTS = [(KERNEL, "c1c7feeab882263fc493a9d5a5b2ddd71b54826cdf65d8d17a76126b260a49f2"),
          (PCK, "3dff7b1dbeceaa01f25467767d3fa25816051c85d162d1edf04acb310ee28bb1")]
TT_SECONDS = [-4_700_000_000.0, -2_000_000_000.125, 0.0,
              845_856_123.456, 3_155_760_000.75, 4_700_000_000.0]


def array(values) -> str:
    return "[" + ", ".join(repr(float(x)) for x in values) + "]"


def generate() -> str:
    for source, digest in INPUTS:
        if hashlib.sha256(source.read_bytes()).hexdigest() != digest:
            raise ValueError(f"Original source hash differs: {source}")
    rows = [
        "// Independent numerical results: scripts/verify_lunar_reference.py.",
        f"// CSPICE/spiceypy {spice.__version__}; ERFA/pyerfa {erfa.__version__}.",
        "// Geocentric reception; converged light time; all vector axes J2000.",
        "// Pole and simultaneous Moon-minus-Earth orbit state share emission TDB.",
        "#[rustfmt::skip]",
        "pub type LunarReference = (f64, f64, f64, [f64;3], [f64;3], [f64;3], [f64;3], f64);",
        "#[rustfmt::skip]",
        "pub const CASES: &[LunarReference] = &[",
    ]
    spice.kclear()
    try:
        for source, _ in INPUTS:
            spice.furnsh(str(source.resolve()))
        for tt in TT_SECONDS:
            received = tt + erfa.dtdb(2451545.0, tt / 86400.0, 0.0, 0.0, 0.0, 0.0)
            _, light_time = spice.spkez(301, received, "J2000", "CN", 399)
            emission = received - light_time
            state, _ = spice.spkez(301, emission, "J2000", "NONE", 399)
            radial = state[:3] / np.linalg.norm(state[:3])
            plane = np.cross(state[:3], state[3:])
            plane /= np.linalg.norm(plane)
            tangent = np.cross(plane, radial)
            pole = spice.pxform("IAU_MOON", "J2000", emission)[:, 2]
            tilt = np.degrees(np.arccos(np.clip(pole @ plane, -1.0, 1.0)))
            rows.append("    (" + ", ".join([
                repr(tt), repr(float(emission)), repr(float(light_time)),
                array(pole), array(plane), array(radial), array(tangent),
                repr(float(tilt)),
            ]) + "),")
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
            raise SystemExit("Lunar references differ from original CSPICE/ERFA")
    else:
        OUTPUT.write_text(result, encoding="utf-8", newline="\n")
    print(f"Verified {len(TT_SECONDS)} independent lunar emission pole/orbit-plane cases")


if __name__ == "__main__":
    main()
