"""Prospective original-reference envelope for the pinned selected23 packet.

Only the ten original completed sky rays have exit-error envelopes. The other
thirteen retain exact terminal gates. No candidate result is read by this tool.
Canonical field/coordinate packing and normalized-reference-launch differences
remain in measured errors; convergence is not a rigorous global error bound.
"""
from datetime import datetime, timezone
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import scipy

from verify_optical_dop853 import geometry, integrate, differences

ROOT = Path(__file__).resolve().parents[1]
BASE = 'artifacts/optical-normal-scene-reference-selected'


def read(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def digest(path):
    with (ROOT/path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def main(prefix):
    target = ROOT/(prefix+'.json')
    contract_file = ROOT/(prefix+'-contract.json')
    assert not target.exists() and not contract_file.exists()
    report = read(BASE+'.json')
    assert report['reference'] and report['selected'] and report['record_count'] == 23
    assert report['diagnostic_bytes_per_record'] == 32
    stability = read(BASE+'-input-stability.json')
    assert stability['inputs_unchanged_through_run']
    for item in read(BASE+'-outputs.json'):
        assert digest(item['path']) == item['sha256']
    raw = np.fromfile(ROOT/(BASE+'-readback.bin'), dtype='<u4')
    assert raw.nbytes == report['output_bytes'] == 1856
    assert raw[:4].tolist() == report['header'] == report['expected_header']
    terminal = raw[4:4+12*23].reshape(23, 12)
    details = raw[4+12*23:].reshape(23, 8)
    canonical = report['fixture']['clear_fixture']['cases'][0]['canonical_inputs']
    origin = np.asarray(canonical['origin_ecef_m'], dtype=float)
    _, up = geometry(origin)
    east = np.array([-origin[1], origin[0], 0.]); east /= np.linalg.norm(east)
    basis = np.column_stack((east, up, -np.cross(up, east)))
    samples = report['samples']
    assert [s['record_index'] for s in samples] == list(range(23))
    for i, sample in enumerate(samples):
        assert sample['terminal_words'] == terminal[i].tolist()
        assert sample['diagnostic_words'] == details[i].tolist()
    sky = np.flatnonzero(terminal[:, 7] == 0).tolist()
    assert len(sky) == 10 and sorted(set(terminal[:, 7].tolist())) == [0, 1, 2]
    inputs = [BASE+suffix for suffix in ('.json', '.wgsl', '-readback.bin', '-primary.bin', '-geometry.bin',
              '-frames.bin', '-field.bin', '-certificates.bin', '-sources.json', '-outputs.json', '-input-stability.json')]
    inputs += ['scripts/verify_optical_dop853.py', 'scripts/review_optical_cartesian_suffix_baseline.py']
    fingerprints = {path: digest(path) for path in inputs}
    contract = {
        'created_utc': datetime.now(timezone.utc).isoformat(),
        'scope': __doc__.strip(), 'baseline': BASE, 'input_sha256': fingerprints,
        'python_version': sys.version, 'numpy_version': np.__version__, 'scipy_version': scipy.__version__,
        'source_aware_reference': {'coarse': {'rtol': 2e-12, 'max_step_m': 2000.},
                                  'tight': {'rtol': 5e-14, 'max_step_m': 500.}},
        'unchanged_absolute_limits': {'position_m': 5., 'path_m': 5., 'direction': 3e-6},
        'prospective_rule': 'For this exact selected matrix, candidate maximum error in each component must not exceed the original maximum plus the maximum measured coarse/tight convergence difference in that component, and must satisfy the unchanged absolute limits. This aggregate matrix rule is not a per-ray/global proof. Candidate readback is never an input to this freeze.',
        'exact_terminal_record_indices': [i for i in range(23) if i not in sky],
        'sky_record_indices': sky,
        'records': [{'record_index': i, 'pixel': s['pixel'], 'original_status': int(terminal[i, 7]),
                     'launch_words': details[i, 4:7].tolist(), 'original_terminal_words': terminal[i].tolist()}
                    for i, s in enumerate(samples)],
        'scope_limit': 'Synthetic inversion/turning/vacuum/readiness/aperture matrices require separate original baselines before seeing their candidate errors. This freeze alone does not authorize full-frame timing or adoption.'}
    contract_file.write_text(json.dumps(contract, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    rows = []
    for i in sky:
        launch = details[i, 4:7].copy().view('<f4').astype(float)
        f = terminal[i].copy().view('<f4').astype(float)
        assert np.isfinite(launch).all() and np.isfinite(f[:7]).all()
        case = dict(canonical, initial_direction=(basis@launch).tolist())
        case.pop('target_path_m', None)
        coarse, tight = integrate(case, False), integrate(case, True)
        convergence = differences(coarse, tight)
        error = {'position_m': float(np.linalg.norm(origin+basis@f[:3]-tight['endpoint_ecef_m'])),
                 'path_m': abs(float(f[3])-tight['geometric_path_m']),
                 'direction': float(np.linalg.norm(basis@f[4:7]-tight['exit_direction']))}
        row = {'record_index': i, 'pixel': samples[i]['pixel'], 'launch_words': details[i, 4:7].tolist(),
               'original_terminal_words': terminal[i].tolist(), 'inputs': case,
               'tight_reference': tight, 'coarse_reference': coarse,
               'reference_convergence': {'position_m': convergence['endpoint_m'], 'path_m': convergence['path_m'],
                                         'direction': convergence['direction']},
               'original_error': error}
        rows.append(row)
        print(json.dumps({k: row[k] for k in ('record_index', 'pixel', 'reference_convergence', 'original_error')}), flush=True)
    maxima = {key: max(r['original_error'][key] for r in rows) for key in ('position_m', 'path_m', 'direction')}
    uncertainty = {key: max(r['reference_convergence'][key] for r in rows) for key in maxima}
    ceilings = {key: maxima[key]+uncertainty[key] for key in maxima}
    assert all(digest(path) == value for path, value in fingerprints.items())
    result = {'schema': 'cartesian-suffix-selected-baseline-v1', 'created_utc': datetime.now(timezone.utc).isoformat(),
              'contract': str(contract_file.relative_to(ROOT)), 'contract_sha256': digest(contract_file),
              'scope': __doc__.strip(), 'reference_count': len(rows), 'unchanged_inputs_through_reference': True,
              'original_maximum_error': maxima, 'maximum_reference_convergence_difference': uncertainty,
              'prospective_candidate_maximum_error_ceiling': ceilings,
              'unchanged_absolute_limits': contract['unchanged_absolute_limits'],
              'original_meets_absolute_limits': all(maxima[k] <= contract['unchanged_absolute_limits'][k] for k in maxima),
              'exact_terminal_record_indices': contract['exact_terminal_record_indices'],
              'candidate_readback_used': False, 'cases': rows}
    target.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k != 'cases'}, indent=2), flush=True)


if __name__ == '__main__':
    main(sys.argv[1] if len(sys.argv) > 1 else 'artifacts/optical-cartesian-suffix-selected-baseline-first')
