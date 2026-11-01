"""Prospective scoped suffix policy; preserves the initial baseline freeze.

This file neither runs a candidate nor changes the original fixture limits.
Its per-record check is usable only after exact packet and launch association.
The caller must obtain the route from authored diagnostics, never infer it
from a numerical difference or from whether an accuracy check would pass.
"""
from datetime import datetime, timezone
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BASE = 'artifacts/optical-cartesian-suffix-selected-baseline-first'
METRICS = ('position_m', 'path_m', 'direction')


def digest(path):
    return hashlib.sha256((ROOT/path).read_bytes()).hexdigest()


def read(path):
    return json.loads((ROOT/path).read_text(encoding='utf-8-sig'))


def evaluate_selected_record(policy, index, route, terminal_words, launch_words, errors=None):
    """Exact route/parity gates and per-ray finite measured-error comparison.

    `errors` must come from the independent oracle at the SAME frozen physical
    inputs/reference, not a recomputed candidate-specific reference model.
    """
    assert policy['schema'] == 'cartesian-suffix-prospective-scope-v1'
    row = policy['records'][index]
    assert row['record_index'] == index
    reasons = []
    if list(launch_words) != row['launch_words']:
        reasons.append('launch-words-differ')
    if len(terminal_words) != 12:
        reasons.append('terminal-layout')
    elif route in ('unhanded-original', 'original-fallback'):
        if list(terminal_words) != row['original_terminal_words']:
            reasons.append('original-terminal-words-differ')
    elif route == 'accepted-suffix':
        if row['original_status'] != 0 or terminal_words[7] != 0:
            reasons.append('suffix-must-preserve-original-sky-class')
        if errors is None or set(errors) != set(METRICS):
            reasons.append('missing-independent-error-metrics')
        elif 'original_error' not in row:
            reasons.append('no-frozen-original-sky-reference')
        else:
            for metric in METRICS:
                value = errors[metric]
                if not isinstance(value, (int, float)) or not math.isfinite(value) or value < 0:
                    reasons.append(metric+'-invalid')
                elif Q(value) > Q(row['original_error'][metric])+Q(row['reference_convergence'][metric]):
                    reasons.append(metric+'-worse-than-same-original-ray')
    else:
        reasons.append('unknown-or-incomplete-route')
    return {'passed': not reasons, 'reasons': reasons}


def make_amendment():
    baseline, contract = read(BASE+'.json'), read(BASE+'-contract.json')
    assert baseline['contract_sha256'] == digest(BASE+'-contract.json')
    assert baseline['candidate_readback_used'] is False and baseline['reference_count'] == 10
    # The baseline source and original artifacts remain untouched. Retain the
    # frozen failing absolute-limit result; the amendment changes only scope.
    for path, expected in contract['input_sha256'].items():
        assert digest(path) == expected, path
    references = {r['record_index']: r for r in baseline['cases']}
    rows = []
    for old in contract['records']:
        row = dict(old)
        if old['record_index'] in references:
            reference = references[old['record_index']]
            assert reference['launch_words'] == old['launch_words']
            assert reference['original_terminal_words'] == old['original_terminal_words']
            row['original_error'] = reference['original_error']
            row['reference_convergence'] = reference['reference_convergence']
            row['per_metric_error_ceiling_exact'] = {
                key: str(Q(row['original_error'][key])+Q(row['reference_convergence'][key])) for key in METRICS}
            row['original_exceeds_old_fixture_limits'] = {
                key: row['original_error'][key] > baseline['unchanged_absolute_limits'][key] for key in METRICS}
        rows.append(row)
    return {
        'schema': 'cartesian-suffix-prospective-scope-v1', 'created_utc': datetime.now(timezone.utc).isoformat(),
        'baseline_report': BASE+'.json', 'baseline_sha256': digest(BASE+'.json'),
        'initial_contract': BASE+'-contract.json', 'initial_contract_sha256': digest(BASE+'-contract.json'),
        'policy_source': 'scripts/review_optical_cartesian_suffix_policy.py',
        'policy_source_sha256': digest('scripts/review_optical_cartesian_suffix_policy.py'),
        'candidate_readback_used': False, 'initial_contract_and_report_preserved': True,
        'reason': 'The initial protocol incorrectly applied the old atmosphere fixture absolute limits to all selected-camera rays while also requiring exact original fallback. The frozen original selected matrix itself exceeds those limits. This amendment is approved before any candidate errors and strengthens selected-matrix nonregression from aggregate maxima to each same ray.',
        'original_atmosphere_fixture': {
            'retained_review': 'artifacts/optical-atmosphere-gpu-review-summary.json',
            'review_sha256': digest('artifacts/optical-atmosphere-gpu-review-summary.json'),
            'unchanged_limits': baseline['unchanged_absolute_limits'],
            'required': 'Retain the original fixture cases and absolute thresholds. This selected-camera amendment does not waive that separate acceptance stage.'},
        'selected23': {
            'required_input_identity': 'All uploaded field/primary/geometry/frame/selection inputs and each actual launch must match the frozen original packet; route tags must be authored runtime diagnostics.',
            'unhanded_or_fallback': 'All twelve original terminal words must match exactly for every original status, including preexisting sky errors above the separate fixture limits.',
            'accepted_suffix': 'Original and candidate must both be sky. Position, path and direction errors must each be no greater than the SAME original ray error plus that ray measured coarse/tight reference difference. Compare exact rational values of the retained binary64 errors; no aggregate-envelope substitution.',
            'usefulness': 'At least one actual suffix completion and the required original near-scene outcomes are necessary before further work. Zero handoff/completion does not pass by exact fallback alone.'},
        'original_selected_maxima': baseline['original_maximum_error'],
        'original_selected_meets_old_fixture_absolute_limits': baseline['original_meets_absolute_limits'],
        'synthetic_matrix_policy': 'Inversion, turning, vacuum-entry/exit, aperture and readiness cases need their independently reviewed original baselines/limits before seeing their candidate errors. This amendment does not establish those gates.',
        'limitations': 'Finite selected-ray and fixture acceptance only. Canonical-vs-packed field/coordinate effects remain included in the measured errors; reference convergence is not a rigorous exact-ODE error tube, image-wide accuracy proof or native-performance result.',
        'records': rows}


if __name__ == '__main__':
    output = ROOT/'artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json'
    assert not output.exists(), 'Preserve the prospective amendment.'
    result = make_amendment()
    output.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n', encoding='utf-8')
    print(json.dumps({'output': str(output), 'records': len(result['records']),
                      'candidate_readback_used': result['candidate_readback_used']}))
