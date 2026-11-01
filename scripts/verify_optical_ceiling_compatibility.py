"""Exact-input high-precision check of the current scene ceiling headroom.

This checks the retained positive packet only. It is not a generic admission
rule for zero ceilings or a substitute for source-geometry identity checks.
"""
import hashlib
import json
from pathlib import Path
import struct
import mpmath as mp


def f64(bits):
    return mp.mpf(struct.unpack('<d', struct.pack('<Q', bits))[0])


def main():
    paths = [Path('artifacts/optical-scene-ceiling-constant-bits.json'),
             Path('artifacts/optical-scene-height-ceiling-reviewed.json'),
             Path('artifacts/optical-normal-scene-packet.json')]
    constants, source, packet = [json.loads(p.read_text(encoding='utf-8-sig')) for p in paths]
    with mp.workdps(90):
        a, b, e = (f64(constants[k]) for k in ['a_bits', 'b_bits', 'e2_bits'])
        difference = abs(b-a*mp.sqrt(1-e))
        bound = mp.mpf(constants['polar_axis_gap_upper_m'])
        source_ceiling = mp.mpf(source['canonical_height_ceiling_m'])
        packed_ceiling = mp.mpf(packet['canonical_height_ceiling_m'])
        assert difference <= bound
        assert source_ceiling+bound < packed_ceiling
        report = dict(scope='Current retained positive scene ceiling only: exact binary64 semiaxis/eccentricity inputs, high-precision polar-axis gap and packed headroom. Source scene identity is separately verified by the scene preparer; no generic zero-ceiling admission.',
                      passed=True, exact_polar_axis_difference_m=mp.nstr(difference, 35),
                      outward_polar_axis_gap_upper_m=float(bound),
                      source_ceiling_m=float(source_ceiling), packed_ceiling_m=float(packed_ceiling),
                      remaining_headroom_m=mp.nstr(packed_ceiling-source_ceiling-bound, 35),
                      inputs=[{'path':str(p), 'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in paths])
    output=Path('artifacts/optical-scene-ceiling-compatibility.json')
    assert not output.exists(), 'Preserve earlier evidence'
    output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
