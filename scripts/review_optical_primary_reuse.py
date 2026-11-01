"""Independent native reuse log/header and unfiltered interval audit."""
import argparse
from collections import Counter
from datetime import datetime, timezone
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import re
from analyze_frame_capture import external_foreground_statistics


def utc(value):
    match = re.fullmatch(r'(\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d)(\.\d+)?Z', value)
    assert match
    stamp = datetime.fromisoformat(match[1]).replace(tzinfo=timezone.utc)
    delta = stamp - datetime(1970, 1, 1, tzinfo=timezone.utc)
    return Decimal(delta.days * 86400 + delta.seconds) + Decimal(match[2] or 0)


def motion_evidence(data, records, readback_data):
    qa = data.get('optical_motion_qa')
    if qa is None:
        return None
    frames = qa['frames']
    assert len(frames) == len(data['interval_seconds'])
    logs = {row['control']: (i,row) for i,row in enumerate(records)}
    header_states = {}
    for kind in ('requests', 'stellar_requests'):
        lookup = {}
        for row in readback_data[kind]:
            token = tuple(row['requested_extent_and_token'])
            returned = row['returned_extent_and_token']
            state = ('error' if row['error'] is not None else 'pending' if returned is None
                     else 'exact_match' if tuple(returned) == token else 'mismatch')
            lookup.setdefault(token, []).append(state)
        header_states[kind] = lookup
    phases = {}
    guard_keys = ('profile_generation','surface_generation','marker_generation',
                  'visible_marker_count','physical_size','scale_factor')
    pose_keys = ('eye_ecef_m','forward','up','vertical_fov_deg')
    changes = []
    changed_consecutive = Counter()
    suspect = []
    rows = []
    for index, (frame, interval) in enumerate(zip(frames, data['interval_seconds'])):
        assert frame['interval_seconds'] == interval
        phase = phases.setdefault(frame['phase'], {'frames':0,'duration':Decimal(0),
            'commands':Counter(),'primary':Counter(),'stellar':Counter(),'unfocused':0})
        phase['frames'] += 1; phase['duration'] += interval
        phase['unfocused'] += not frame['focused']
        differences = {key:[frames[0][key],frame[key]] for key in guard_keys if frame[key] != frames[0][key]}
        if differences:
            changes.append({'frame_index':index,'differences_from_initial_snapshot':differences})
        control = frame.get('submitted_compute_control')
        command = logs.get(tuple(control)) if control is not None else None
        kind = 'no_submission' if control is None else 'not_logged' if command is None else 'reused' if command[1]['reused'] else 'fresh'
        phase['commands'][kind] += 1
        association = {'frame_index':index,'phase':frame['phase'],'control':control,'command':kind}
        for producer, field, count_key in [('requests','submitted_compute_control','primary'),
                                           ('stellar_requests','expected_stellar_control','stellar')]:
            token = frame.get(field)
            states = header_states[producer].get(tuple(token), []) if token is not None else []
            state = 'not_selected' if token is None else 'not_requested' if not states else 'ambiguous_requests' if len(states)>1 else states[0]
            phase[count_key][state] += 1
            association[count_key+'_header'] = state
        if index:
            previous = frames[index-1]
            previous_control = previous.get('submitted_compute_control')
            prior = logs.get(tuple(previous_control)) if previous_control is not None else None
            changed = any(frame[key] != previous[key] for key in pose_keys)
            consecutive = command is not None and prior is not None and command[0] == prior[0]+1
            association['pose_or_lens_changed_from_previous_frame'] = changed
            association['consecutive_encoded_commands'] = consecutive
            if changed and consecutive:
                changed_consecutive[kind] += 1
                if kind == 'reused': suspect.append(index)
        rows.append(association)
    assert sum(phase['unfocused'] for phase in phases.values()) == qa['unfocused_intervals']
    return {'scope':'Exact token joins to command-encoding logs and producer headers. Changed-view check compares captured eye/forward/up/lens only when corresponding log commands are consecutive; it does not prove every serialized dependency or payload/fragment consumption.',
            'route_completed':qa['route_completed'],'passed':qa['passed'],
            'unfocused_diagnostic_policy':qa.get('unfocused_diagnostic_policy',False),
            'diagnostic_route_passed':qa.get('diagnostic_route_passed'), 'failure':qa['failure'],
            'recorded_guard_changes':changes,
            'changed_pose_or_lens_consecutive_command_counts':dict(changed_consecutive),
            'changed_pose_or_lens_reused_consecutive_frame_indices':suspect,
            'phases':[{**row,'phase':phase,'duration':str(row['duration']),
                       **{key:dict(row[key]) for key in ('commands','primary','stellar')}}
                      for phase,row in phases.items()], 'frame_associations':rows}


def review(prefix):
    path = Path(prefix + '.json')
    data = json.loads(path.read_text(), parse_float=Decimal)
    launch_path = Path(prefix + '.launch.json')
    launch = json.loads(launch_path.read_text(encoding='utf-8-sig'))
    intervals = data['interval_seconds']; ordered = sorted(intervals); n = len(intervals)
    assert n == data['sample_count'] and all(v.is_finite() and v > 0 for v in intervals)
    duration = sum(intervals, Decimal(0))
    assert abs(duration - data['recorded_duration_seconds']) < Decimal('1e-10')
    begin = data['metadata']['recording_boundary_observed_unix_seconds']; end = begin + duration
    association = lambda t: 'before' if t < begin else 'after' if t > end else 'boundary' if t in (begin, end) else 'inside'
    log_path = Path(prefix + '.stderr.log')
    log = log_path.read_text(encoding='utf-8-sig')
    pattern = r'(\S+) DEBUG .*Optical primary payload encoded control=\[([^\]]+)\] reused=(true|false)'
    records = []
    for stamp, control, reused in re.findall(pattern, log):
        token = tuple(map(int, control.split(',')))
        records.append({'utc': stamp, 'control': token, 'reused': reused == 'true', 'association': association(utc(stamp))})
    assert records
    assert len({r['control'] for r in records}) == len(records)
    logged = {r['control']: r for r in records}
    readbacks = {}
    for key in ('requests', 'stellar_requests'):
        rows = data['optical_compute_readback'][key]; counts = Counter(); associated = Counter()
        mismatches = []
        for index, row in enumerate(rows):
            requested = tuple(row['requested_extent_and_token']); returned = row['returned_extent_and_token']
            result = ('error' if row['error'] is not None else 'pending' if returned is None else
                      'exact_match' if requested == tuple(returned) else 'zero_header' if returned == [0, 0, 0, 0] else 'nonzero_mismatch')
            assert row['exact_match'] is (None if returned is None else result == 'exact_match')
            counts[result] += 1
            if requested in logged:
                command = logged[requested]
                associated[(command['association'], command['reused'], result)] += 1
            if result not in ('exact_match', 'pending'):
                mismatches.append({'index': index, 'requested': requested, 'returned': returned, 'kind': result})
        readbacks[key] = {'count': len(rows), 'outcomes': dict(counts),
                          'matching_request_token_to_command_log': [
                              {'command_window': part, 'reused_command': reused, 'readback_result': result, 'count': count}
                              for (part, reused, result), count in sorted(associated.items())],
                          'mismatches': mismatches}
    first, last = data['metadata']['view'], data['final_view']
    differences = {key: [first.get(key), last.get(key)] for key in first.keys() | last.keys() if first.get(key) != last.get(key)}
    os_counts = Counter((association(utc(row['utc'])), row['own_window_is_foreground'], row.get('foreground_window_is_null'))
                        for row in launch['focus_observations'])
    foreground = external_foreground_statistics(json.loads(path.read_text()), launch)
    def nearest_rank(percent):
        index = (n * percent + 99) // 100 - 1
        return ordered[index] * 1000
    return {'scope': 'Native engineering diagnostic. Logs observe command encoding; exact GPU readback headers observe producer tokens. Token-to-log association does not time readback completion or prove fragment consumption, presentation or payload bit identity. Every positive application interval is retained; this is not GPU duration, focused timing or smooth-flight acceptance. No changed-view invalidation test is inferred from stationary commands.',
            'capture': prefix, 'capture_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
            'launch_sha256': hashlib.sha256(launch_path.read_bytes()).hexdigest(),
            'stderr_sha256': hashlib.sha256(log_path.read_bytes()).hexdigest(),
            'executable_sha256': launch['executable_sha256'],
            'logged_commands': len(records), 'logged_reused': sum(r['reused'] for r in records),
            'logged_fresh': sum(not r['reused'] for r in records),
            'command_window_counts': [dict(window=part, reused=reused, count=count)
                for (part, reused), count in sorted(Counter((r['association'], r['reused']) for r in records).items())],
            'readbacks': readbacks, 'recording_window_unix_seconds': [str(begin), str(end)],
            'raw_view_changed_flag': data['view_changed_during_capture'], 'start_final_view_differences': differences,
            'physical_start_final_view_equal_after_excluding_stellar_control': not {k: v for k, v in differences.items() if k != 'optical_stellar_control'},
            'stellar_token_in_view_configuration': 'optical_stellar_control' in first or 'optical_stellar_control' in last,
            'stationary_flag_caveat': 'Historical captures which include optical_stellar_control in view configuration can report change from that token alone. The raw flag remains preserved. Endpoint equality alone cannot exclude an unrecorded intermediate view change.',
            'application_unfocused_frames': data['unfocused_recorded_frames'], 'incomplete_scene_frames': data['incomplete_scene_frames'],
            'os_observation_counts': [dict(window=part, foreground=flag, null_foreground=null, count=count)
                                      for (part, flag, null), count in sorted(os_counts.items())],
            'eligible_for_focused_timing': foreground['eligible_for_focused_timing'],
            'focused_timing_rejection_reasons':foreground['reasons'],
            'motion_evidence':motion_evidence(data,records,data['optical_compute_readback']),
            'interval_statistics': {'count': n, 'duration_seconds': str(duration), 'raw_throughput_fps': str(Decimal(n) / duration),
                'median_ms': str((ordered[(n-1)//2]+ordered[n//2])/2*1000), 'p95_ms': str(nearest_rank(95)),
                'p99_ms': str(nearest_rank(99)), 'maximum_ms': str(ordered[-1]*1000)}}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(); parser.add_argument('prefix'); parser.add_argument('--output',type=Path); args = parser.parse_args()
    result = review(args.prefix); output = args.output or Path(args.prefix + '.independent-review.json')
    assert not output.exists(), 'retain previous evidence'
    output.write_text(json.dumps(result, indent=2, default=str) + '\n')
    display={k:v for k,v in result.items() if k not in ('readbacks','motion_evidence')}
    if result['motion_evidence'] is not None:
        display['motion_evidence']={k:v for k,v in result['motion_evidence'].items() if k!='frame_associations'}
    print(json.dumps(display, indent=2, default=str))
