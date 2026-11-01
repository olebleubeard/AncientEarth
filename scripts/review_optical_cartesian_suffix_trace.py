"""Parity-first accepted-step path diagnostic; never a corrected solver.

The rational sum retains the stored handoff path and stored accepted step
lengths. It is not exact physical arc length, an ODE error proof, or a replay
of compensated arithmetic inside the controller. It separates observed path
addition error from the remaining handoff/step/reference discrepancy.
"""
import argparse
from fractions import Fraction as Q
import hashlib
import json
import math
from pathlib import Path
import struct
import zipfile

import numpy as np

from review_optical_cartesian_suffix_source import compact, digest, function, read, replace_once

ROOT = Path(__file__).resolve().parents[1]
ORIGINAL = 'artifacts/optical-cartesian-suffix-selected-first'
BASELINE = 'artifacts/optical-cartesian-suffix-selected-baseline-first'


def load(path):
    return json.loads(read(ROOT / path))


def number(word):
    return struct.unpack('<f', struct.pack('<I', int(word)))[0]


def positive_addition_enclosure(a, b):
    """Both adjacent correct results; inputs are exactly represented f32."""
    exact = Q(a) + Q(b)
    rounded = struct.unpack('<f', struct.pack('<f', float(exact)))[0]
    word = struct.unpack('<I', struct.pack('<f', rounded))[0]
    if Q(rounded) == exact:
        return exact, exact
    if Q(rounded) < exact:
        return Q(rounded), Q(number(word+1))
    return Q(number(word-1)), Q(rounded)


def strip_trace(source, name):
    if name != 'fallback':
        source = replace_once(source,
            '@group(0) @binding(8) var<storage,read_write> cst_log:array<vec4<u32>>;', '')
    if name == 'prefix':
        source = replace_once(source, 'cst_log[id*4097u]=vec4(0u,0u,id,settings.control.z);', '')
    if name == 'suffix':
        source = replace_once(source, function(source, 'cst_store'), '')
        source = replace_once(source, 'cst_store(id,step,path,0u);', '')
        source = replace_once(source, 'cst_store(id,step,path,1u);', '')
    return source


def review(prefix):
    report, old = load(prefix+'.json'), load(ORIGINAL+'.json')
    assert report['cartesian_suffix_trace'] and report['cartesian_suffix'] and report['selected']
    assert not report['timed'] and report['iterations'] == []
    assert report['record_count'] == 23 and report['state_words_per_record'] == 24
    assert report['steps'] == old['steps']
    assert report['auxiliary_layout'] == {'bytes': 1507696, 'log_words_per_record': 16388, 'max_steps': 4096}
    sources, compiled, stability = [load(prefix+'-'+suffix+'.json')
                                    for suffix in ('sources', 'compile-inputs', 'input-stability')]
    assert sources['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    assert compiled['inputs'] == stability['after_run'] == sources['files'][:-1]
    assert sources['files'][-1]['path'].endswith('.exe')
    outputs = load(prefix+'-outputs.json')
    for item in sources['files'] + outputs + load(ORIGINAL+'-outputs.json'):
        assert digest(ROOT / item['path']) == item['sha256'], item['path']
    lifecycle = load(prefix+'-lifecycle.json')
    assert lifecycle['exit_code'] == 0 and not lifecycle['timed_out']
    assert not lifecycle['terminated_by_wrapper'] and lifecycle['failure'] is None
    # Old host/design source legitimately changed for the trace wrapper. Verify
    # its preserved archive against the OLD manifest, not current mutable files.
    archived = load(ORIGINAL+'-source-archive.json')
    assert digest(ROOT / archived['archive']) == archived['archive_sha256']
    old_manifest = {x['path']: x['sha256'] for x in load(archived['source_manifest'])['files']}
    with zipfile.ZipFile(ROOT / archived['archive']) as archive:
        for item in archived['entries']:
            assert hashlib.sha256(archive.read(item['archive_entry'])).hexdigest() == item['sha256']
            assert old_manifest[item['path']] == item['sha256']
    validation = load(prefix+'-validation.json')
    assert validation['all_modules_valid'] and validation['modules'] == ['prefix', 'suffix', 'fallback']
    module_checks = []
    for name in ('prefix', 'suffix', 'fallback'):
        source = read(ROOT / (prefix+'-'+name+'.wgsl'))
        original = read(ROOT / (ORIGINAL+'-'+name+'.wgsl'))
        assert compact(strip_trace(source, name)) == compact(original)
        module_checks.append({'name': name, 'source_sha256': digest(ROOT / (prefix+'-'+name+'.wgsl')),
                              'old_source_sha256': digest(ROOT / (ORIGINAL+'-'+name+'.wgsl')),
                              'entire_module_equal_after_exact_telemetry_removal': True})
    input_checks, parity = [], []
    for name in ('primary', 'field', 'frames', 'geometry', 'certificates', 'readback', 'states', 'progress'):
        path, old_path = ROOT / (prefix+'-'+name+'.bin'), ROOT / (ORIGINAL+'-'+name+'.bin')
        a, b = path.read_bytes(), old_path.read_bytes()
        item = {'name': name, 'bytes': len(a), 'equal': a == b,
                'sha256': digest(path), 'old_sha256': digest(old_path)}
        (input_checks if name in ('primary', 'field', 'frames', 'geometry', 'certificates') else parity).append(item)
    result = {'scope': __doc__.strip(), 'prefix': prefix,
              'input_and_output_hashes_verified': True, 'current_source_input_count': len(sources['files']),
              'output_hash_count': len(outputs), 'old_source_archive_entries_verified': len(archived['entries']),
              'old_source_archive_sha256': archived['archive_sha256'], 'module_checks': module_checks,
              'input_identity': input_checks, 'old_buffer_parity': parity,
              'telemetry_interpretation_allowed': all(x['equal'] for x in input_checks+parity),
              'policy_unchanged': True, 'numerical_correction_approved': False,
              'reviewer_source_sha256': digest(__file__)}
    if not result['telemetry_interpretation_allowed']:
        result['decision'] = 'reject-telemetry-interpretation-buffer-parity-failed'
        return result
    words = np.fromfile(ROOT / (prefix+'-readback.bin'), dtype='<u4')
    states = np.fromfile(ROOT / (prefix+'-states.bin'), dtype='<u4').reshape(23, 24)
    terminal = words[4:280].reshape(23, 12)
    assert words.nbytes == 1856 and states.nbytes == 2208
    assert words[:4].tolist() == report['expected_header'] == report['header']
    logs = np.fromfile(ROOT / (prefix+'-auxiliary.bin'), dtype='<u4')
    assert logs.nbytes == 1507696
    logs = logs.reshape(23, 4097, 4)
    baseline = load(BASELINE+'.json')
    contract = load(BASELINE+'-contract.json')
    policy = load('artifacts/optical-cartesian-suffix-selected-scope-amendment-first.json')
    assert digest(ROOT / (BASELINE+'.json')) == policy['baseline_sha256']
    assert digest(ROOT / (BASELINE+'-contract.json')) == policy['initial_contract_sha256']
    references = {r['record_index']: r for r in baseline['cases']}
    rows, transition_count, zero_steps = [], 0, 0
    for i in range(23):
        count, overflow, record, token = map(int, logs[i, 0])
        assert count <= 4096 and overflow == 0 and record == i and token == report['header'][2]
        route = int(states[i, 22])
        assert route in (0, 1), 'This retained scope does not interpret unpublished fallback suffix ends'
        assert report['samples'][i]['state_words'] == states[i].tolist()
        assert report['samples'][i]['terminal_words'] == terminal[i].tolist()
        row = {'record_index': i, 'pixel': report['samples'][i]['pixel'], 'route': route,
               'log_header_words': logs[i, 0].tolist(), 'accepted_step_count': count,
               'suffix_outer_attempt_count': int(states[i, 13])}
        if route == 0:
            assert count == 0
            rows.append(row)
            continue
        assert terminal[i, 7] == 0 and states[i, 11] == 0
        handoff = number(states[i, 3])
        final = number(terminal[i, 3])
        assert math.isfinite(handoff) and math.isfinite(final) and 0 <= handoff <= final
        active = logs[i, 1:1+count]
        cumulative = Q(handoff)
        rounding_sum = Q(0)
        steps, previous_attempt = [], 0
        for j, item in enumerate(active):
            step_word, before_word, attempt, kind = map(int, item)
            step, before = number(step_word), number(before_word)
            assert math.isfinite(step) and math.isfinite(before) and step >= 0 and before >= 0
            assert previous_attempt < attempt <= int(states[i, 13])
            previous_attempt = attempt
            assert kind == 0 or (kind == 1 and j == count-1)
            if j == 0:
                assert before_word == int(states[i, 3])
            after_word = int(active[j+1, 1]) if j+1 < count else int(terminal[i, 3])
            after = number(after_word)
            lower, upper = positive_addition_enclosure(before, step)
            assert lower <= Q(after) <= upper, (i, j, before, step, after)
            exact_addition = Q(before) + Q(step)
            addition_error = Q(after) - exact_addition
            before_error = Q(before) - cumulative
            cumulative += Q(step)
            rounding_sum += addition_error
            assert Q(after)-cumulative == rounding_sum
            steps.append({'words': item.tolist(), 'step_m': step, 'path_before_m': before,
                          'observed_path_after_word': after_word,
                          'exact_cumulative_path': str(cumulative),
                          'before_minus_exact_cumulative_m': float(before_error),
                          'this_observed_addition_error_exact': str(addition_error),
                          'this_observed_addition_error_m': float(addition_error)})
            transition_count += 1
            zero_steps += step == 0
        if count == 0:
            assert int(terminal[i, 3]) == int(states[i, 3])
        assert Q(final)-cumulative == rounding_sum
        ref = references[i]
        assert contract['records'][i]['launch_words'] == states[i, 8:11].tolist()
        reference = Q(ref['tight_reference']['geometric_path_m'])
        original_error = Q(ref['original_error']['path_m'])
        convergence = Q(ref['reference_convergence']['path_m'])
        exact_error, stored_error = abs(cumulative-reference), abs(Q(final)-reference)
        row.update(handoff_path_m=handoff, stored_terminal_path_m=final,
                   exact_handoff_plus_steps=str(cumulative), exact_handoff_plus_steps_m=float(cumulative),
                   stored_minus_exact_accumulation=str(rounding_sum),
                   stored_minus_exact_accumulation_m=float(rounding_sum),
                   frozen_reference_path_m=float(reference),
                   exact_sum_reference_error_m=float(exact_error),
                   stored_terminal_reference_error_m=float(stored_error),
                   original_reference_error_m=float(original_error),
                   frozen_reference_convergence_m=float(convergence),
                   exact_sum_reference_error_exact=str(exact_error),
                   stored_terminal_reference_error_exact=str(stored_error),
                   exact_sum_meets_frozen_path_error_rule=exact_error <= original_error+convergence,
                   actual_terminal_meets_frozen_path_error_rule=stored_error <= original_error+convergence,
                   final_log_kind=int(active[-1, 3]) if count else None,
                   logs=steps)
        rows.append(row)
    result.update(accepted_step_records=transition_count, zero_steps_retained=zero_steps,
                  all_observed_additions_within_correct_rounding_enclosure=True,
                  exact_sum_failing_path_records=[r['record_index'] for r in rows if r['route'] == 1 and not r['exact_sum_meets_frozen_path_error_rule']],
                  actual_failing_path_records=[r['record_index'] for r in rows if r['route'] == 1 and not r['actual_terminal_meets_frozen_path_error_rule']],
                  auxiliary_sha256=digest(ROOT / (prefix+'-auxiliary.bin')),
                  reference_report_sha256=digest(ROOT / (BASELINE+'.json')),
                  decision='diagnosis-only-original-rejection-unchanged', records=rows,
                  limitations=[
                      'The exact sum retains all preexisting handoff and accepted-step errors; it does not establish their individual causes.',
                      'A hypothetical exact sum is not an implemented compensated solver and cannot establish its branch, state, or final-f32 behavior.',
                      'Trace parity is finite execution evidence, not a universal compiler-context equivalence proof.',
                      'No new tolerance, correction, full-frame timing or production adoption is authorized.'])
    return result


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    output = ROOT / args.output
    assert not output.exists(), 'Preserve previous evidence'
    result = review(args.prefix)
    output.write_text(json.dumps(result, indent=2, allow_nan=False)+'\n')
    print(json.dumps({k: result.get(k) for k in ['decision', 'telemetry_interpretation_allowed', 'accepted_step_records', 'actual_failing_path_records', 'exact_sum_failing_path_records']}))
