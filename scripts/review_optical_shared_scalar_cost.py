"""Independent raw-batch arithmetic and scope audit of CPU operation costs."""
import argparse
from decimal import Decimal, localcontext
import hashlib
import json
import math
from pathlib import Path


def review(path):
    path = Path(path)
    text = path.read_text()
    data = json.loads(text)
    exact = json.loads(text, parse_float=Decimal)
    control = data['scalar_cost_control']
    cases = data['cases']
    accepted = [case for case in cases if case['transport'] == 'enclosed common-schedule exit']
    assert len(cases) == data['cells_tested'] == 12
    assert len(accepted) == data['transport_enclosed_cells'] == control['shared_repeated_exit_count'] == 9
    assert data['tile_size'] == 4 and data['scene_admitted_cells'] == 0
    assert not data['complete_frame'] and not data['curves_retained']
    reference_path = Path('artifacts/optical-shared-transport-cell4-exact-input-v2.json')
    reference = json.loads(reference_path.read_text())
    assert data['arithmetic_input_bits'] == reference['arithmetic_input_bits']
    for old, new in zip(reference['cases'], cases, strict=True):
        for key, value in old.items():
            if key not in ('curves', 'partial_curves'):
                assert new[key] == value, (old['cell'], key)
    records = []
    with localcontext() as ctx:
        ctx.prec = 60
        for index, (record, raw) in enumerate(zip(control['scalar_records'], exact['scalar_cost_control']['scalar_records'], strict=True)):
            selection = cases if index == 0 else accepted
            assert record['cells'] == len(selection)
            pixels = sum(case['cell'][2] * case['cell'][3] for case in selection)
            assert record['native_pixels'] == pixels
            assert record['scalar_exits_per_workload'] + record['scalar_deferred_per_workload'] == pixels
            assert 0 <= record['scalar_accepted_steps_per_workload'] <= record['scalar_attempts_per_workload']
            assert math.isfinite(record['retained_checksum'])
            batches = raw['raw_batch_seconds']
            assert len(batches) == 7 and all(value.is_finite() and value > 0 for value in batches)
            repeats = record['iterations_per_batch']
            assert repeats == 25
            seconds = sorted(batches)[3] / repeats
            assert abs(Decimal.from_float(record['median_seconds_per_workload']) - seconds) <= seconds * Decimal('1e-15')
            shared_seconds = exact['elapsed_seconds'] if index == 0 else exact['scalar_cost_control']['shared_accepted_only_seconds']
            assert shared_seconds.is_finite() and shared_seconds > 0
            records.append({'selection': record['selection'], 'cells': len(selection), 'native_pixels': pixels,
                            'raw_scalar_batch_seconds': list(map(str, batches)), 'repeats_per_batch': repeats,
                            'scalar_median_seconds_per_workload': str(seconds),
                            'scalar_median_ms_per_workload': str(seconds * 1000),
                            'shared_single_workload_seconds': str(shared_seconds),
                            'scalar_exits_per_workload': record['scalar_exits_per_workload'],
                            'scalar_deferred_per_workload': record['scalar_deferred_per_workload'],
                            'scalar_attempts_per_workload': record['scalar_attempts_per_workload'],
                            'scalar_accepted_steps_per_workload': record['scalar_accepted_steps_per_workload']})
    return {'scope': 'CPU operation-cost control only, with raw scalar batch medians recomputed independently. Shared durations are single runs, including diagnostic result construction; the first all-cell duration also includes per-cell logging. Scalar checks and common-cell interval/Taylor certification have different acceptance work. These measurements establish neither numerical equivalence nor a GPU/native performance ratio.',
            'input': str(path), 'input_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'reference_input': str(reference_path), 'reference_sha256': hashlib.sha256(reference_path.read_bytes()).hexdigest(),
            'shared_inputs_and_existing_case_outcomes_unchanged': True,
            'historical_scope_correction': {'field': 'scalar_cost_control.scope',
                'retained_original_wording': control['scope'],
                'reviewed_wording': 'The scalar control uses floating-point acceptance checks with a 256-epsilon heuristic pad. No rigorous roundoff certificate for that pad is claimed. The original artifact is preserved unchanged.'},
            'literal_taylor_ad_gpu_port_accepted': False,
            'interpretation': 'The current shared CPU implementation costs substantially more than independently solving every tested pixel. This does not support porting that implementation as an acceleration; a changed representation would require fresh cost and correctness evidence.',
            'records': records}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('input'); parser.add_argument('--output', required=True)
    args = parser.parse_args(); output = Path(args.output)
    assert not output.exists(), 'retain earlier evidence'
    result = review(args.input); output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: v for k, v in result.items() if k != 'historical_scope_correction'}, indent=2))
