"""Whole-body reversal checks for the original-clearance diagnostic.

Run on retained offline WGSL before GPU execution. This verifies the declared
source/control transformation, not floating-point parity or geometric truth.
Five deliberate corruptions must be rejected by the same checks.
"""
import argparse
import json
import re
from pathlib import Path

from review_optical_cartesian_suffix_source import compact, digest, function, read, replace_once

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / 'artifacts/optical-normal-scene-reference-selected.wgsl'
FULL_REFERENCE = ROOT / 'artifacts/optical-primary-loop-control-checked-first.wgsl'
REJECTION = 'if !triangle_candidate(p,q,a,b,c,n,tube+band){return result;}'
CALL = 'let event=scene_event(state,event_end,event_delta,event_step,event_error);'
ABORT = 'if oc_pending!=0u{return trace_result(state.p,state.u,path,2u);}'
TRAVERSAL_GUARD = 'if oc_pending!=0u{return result;}'


def same(a, b):
    assert compact(a) == compact(b), 'Whole-body source mismatch'


def check(old, fast, fallback):
    original_names = set(re.findall(r'\bfn (\w+)\(', old)) - {'normal_scene'}
    changed = {'trace', 'face_event', 'terrain_events', 'marker_events'}
    retained = {}
    for label, source in [('fast', fast), ('fallback', fallback)]:
        names = set(re.findall(r'\bfn (\w+)\(', source))
        original = {n for n in names if not n.startswith('oc_')}
        assert original <= original_names
        for name in original - (changed if label == 'fast' else {'trace'}):
            same(function(source, name), function(old, name))
        trace = replace_once(function(source, 'trace'), 'oc_attempts+=1u;', '')
        if label == 'fast':
            trace = replace_once(trace, ABORT, '')
        same(trace, function(old, 'trace'))
        retained[label] = sorted(original)
    face = function(old, 'face_event')
    assert face.count(REJECTION) == 1
    prefix = face[:face.index(REJECTION)+len(REJECTION)]
    same(function(fast, 'face_event'), prefix+'oc_pending=1u;return result;}')
    assert 'advance_delta(' not in function(fast, 'face_event')
    assert fast.count('oc_pending=1u;') == 1
    for name in ('terrain_events', 'marker_events'):
        traversal = function(fast, name)
        assert traversal.count(TRAVERSAL_GUARD) == 2
        # The first guard precedes any traversal read; the other immediately
        # follows the unchanged face call, before another candidate/iteration.
        assert 'var result=empty_event();'+TRAVERSAL_GUARD in traversal
        begin = traversal.index('result=combine_event(result,face_event(')
        end = traversal.index(';', begin)+1
        assert traversal[end:].startswith(TRAVERSAL_GUARD)
        same(traversal.replace(TRAVERSAL_GUARD, ''), function(old, name))
    assert CALL+'\n        '+ABORT in function(fast, 'trace')
    same(function(fast, 'scene_event'), function(old, 'scene_event'))
    same(function(fast, 'terrain_coverage'), function(old, 'terrain_coverage'))

    entry = function(fast, 'oc_fast')
    cut = entry.index('oc_pending=0u;oc_attempts=0u;')
    head = entry[:cut]
    head = replace_once(head, 'fn oc_fast(', 'fn normal_scene(')
    head = replace_once(head,
        'terminals[0]=vec4(0u);for(var word=0u;word<4u;word+=1u){atomicStore(&oc_progress[word],0u);}',
        'terminals[0]=settings.control;')
    head = replace_once(head, 'oc_initialize(id.x);', '')
    head = replace_once(head, 'oc_masked(id.x);', '')
    original_entry = function(old, 'normal_scene')
    same(head, original_entry[:original_entry.index('let ray=trace(direction);')])
    same(entry[cut:], '''oc_pending=0u;oc_attempts=0u;
        let ray=trace(direction);oc_states[id.x].y=oc_attempts;
        if oc_pending!=0u{oc_states[id.x].x=1u;oc_states[id.x].w=1u;
            terminals[detail_index]=vec4(oc_attempts,0u,1u,1u);return;}
        oc_publish(id.x,ray,0u);}
    ''')
    assert fast.count('atomicStore(') == 1
    assert not any(x in fast for x in ('atomicAdd(', 'atomicMax('))
    assert 'if id.x==0u{terminals[0]=vec4(0u);for(' in entry
    initialize = function(fast, 'oc_initialize')
    for exact in ('oc_states[id]=vec4(2u,0u,0u,3u);',
                  'terminals[index]=vec4(0u);terminals[index+1u]=vec4(0u);terminals[index+2u]=vec4(0u);',
                  'terminals[detail_index]=vec4(0u);terminals[detail_index+1u]=vec4(0u);'):
        assert exact in initialize
    fallback_entry = function(fallback, 'oc_fallback')
    assert 'if oc_states[id].x!=1u{return;}' in fallback_entry
    assert 'let direction=bitcast<vec3<f32>>(terminals[detail_index+1u].xyz);' in fallback_entry
    assert 'oc_attempts=0u;let ray=trace(direction);oc_states[id].z=oc_attempts;' in fallback_entry
    assert 'normalize(' not in fallback_entry
    assert fallback_entry.index('oc_attempts=0u;') < fallback_entry.index('oc_publish(id,ray,1u);')
    publish = function(fallback, 'oc_publish')
    for exact in ('terminals[index]=bitcast<vec4<u32>>(vec4(ray.p,ray.path));',
                  'terminals[index+1u]=vec4(bitcast<vec3<u32>>(ray.u),ray.status);',
                  'terminals[index+2u]=vec4(bitcast<vec3<u32>>(ray.normal),(ray.material<<24u)|ray.marker);'):
        assert exact in publish
    assert 'oc_states[id].x=0u;oc_states[id].w=route;' in publish
    same(function(fast, 'oc_publish'), publish)
    counts = re.findall(r'if id\.x>=(\d+)u\{return;\}', original_entry)
    assert len(counts) == 1 and int(counts[0]) in (23, 2073600)
    same(function(fallback, 'oc_finalize'), 'fn oc_finalize(){if atomicLoad(&oc_progress[0])==0u && atomicLoad(&oc_progress[1])=='+counts[0]+'u && atomicLoad(&oc_progress[3])==0u{terminals[0]=settings.control;}}')
    assert fallback.count('atomicStore(') == 0
    return retained


def audit(prefix, full=False):
    reference = FULL_REFERENCE if full else REFERENCE
    old = read(reference)
    paths = {n: Path(str(prefix)+'-'+n+'.wgsl') for n in ('fast', 'fallback')}
    fast, fallback = read(paths['fast']), read(paths['fallback'])
    retained = check(old, fast, fallback)
    corruptions = [
        ('missing trace abort', replace_once(fast, ABORT, ''), fallback),
        ('missing face flag', replace_once(fast, 'oc_pending=1u;', ''), fallback),
        ('missing progress reset', replace_once(fast, 'atomicStore(&oc_progress[word],0u);', ''), fallback),
        ('fallback launch renormalization', fast, replace_once(fallback, 'let ray=trace(direction);', 'let ray=trace(normalize(direction));')),
        ('changed field expression', fast, replace_once(fallback, 'let mp=qmul(p);', 'let mp=qmul(p)+vec3(1.0);')),
    ]
    rejected = []
    for name, a, b in corruptions:
        try:
            check(old, a, b)
        except (AssertionError, ValueError):
            rejected.append(name)
        else:
            raise AssertionError('Comparator admitted deliberate corruption: '+name)
    return {'schema': 'original-clearance-assembled-source-review-v1',
            'full': full, 'reference': str(reference), 'reference_sha256': digest(reference),
            'modules': {n: {'path': str(p), 'sha256': digest(p)} for n, p in paths.items()},
            'retained_original_functions': retained, 'source_reversal_passed': True,
            'deliberate_corruptions_rejected': rejected,
            'scope': 'Exact declared source/control transformation and reset/launch checks. No GPU output, physical accuracy or timing acceptance follows.'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    parser.add_argument('--full', action='store_true', help='Use the fixed retained original 1920x1080 entry and record count.')
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    result = audit(args.prefix, args.full)
    with args.output.open('x', encoding='utf-8') as stream:
        stream.write(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))
