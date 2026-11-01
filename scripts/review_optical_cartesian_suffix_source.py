"""Independent retained-source gate for the selected Cartesian suffix.

Reverse the explicitly reviewed substitutions, then compare entire bodies.
This is source/provenance evidence, not GPU floating-point equivalence.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return Path(path).read_text(encoding='utf-8-sig')


def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for data in iter(lambda: stream.read(1024 * 1024), b''):
            h.update(data)
    return h.hexdigest()


def block_end(source, start):
    opening = source.index('{', start)
    depth, end = 1, opening + 1
    while depth:
        depth += (source[end] == '{') - (source[end] == '}')
        end += 1
    return end


def function(source, name):
    matches = list(re.finditer(r'\bfn ' + name + r'\(', source))
    assert len(matches) == 1, name
    start = matches[0].start()
    return source[start:block_end(source, start)]


def compact(source):
    return re.sub(r'\s+', '', source)


def same(left, right):
    assert compact(left) == compact(right)


def replace_once(source, old, new):
    assert source.count(old) == 1, old
    return source.replace(old, new)


def source_checks(prefix):
    old = read(ROOT / 'artifacts/optical-normal-scene-reference-selected.wgsl')
    modules = {name: read(str(prefix) + '-' + name + '.wgsl')
               for name in ('prefix', 'suffix', 'fallback')}
    fallback = replace_once(function(modules['fallback'], 'trace'),
                            'cs_fallback_attempts+=1u;', '')
    same(fallback, function(old, 'trace'))

    pre = function(modules['prefix'], 'cs_prefix_trace')
    pre = replace_once(pre,
        'fn cs_prefix_trace(direction:vec3<f32>,id:u32){cs_prefix_attempts=0u;var can_handoff=false;',
        'fn trace(direction:vec3<f32>)->Trace{')
    pre = replace_once(pre, 'cs_prefix_attempts=u32(i)+1u;', '')
    pre = replace_once(pre, ';can_handoff=true;', ';')
    begin = pre.index('        if can_handoff {')
    end = pre.index('        // Retain the h0 event', begin)
    handoff = pre[begin:end]
    assert 'cs_handoff(id,state,path,entry,g,owner,u32(i));return;' in handoff
    assert 'owner>=-1 && g.w!=0.0 && cs_point_scene_clear(' in handoff
    pre = pre[:begin] + pre[end:]
    pre, count = re.subn(r'cs_prefix_done\(id,(.*?);return;',
                         lambda m: 'return ' + m[1][:-1] + ';', pre)
    assert count == 15
    same(pre, function(old, 'trace'))

    controller = read(ROOT / 'src/desktop/shaders/refraction_dopri_hermite_trace.wesl')
    original = function(controller, 'dp_trace')
    suffix = function(modules['suffix'], 'cs_suffix_trace')
    original_loop = original.index('    for(var i=0;i<4096;i+=1){')
    suffix_loop = suffix.index('    for(var i=i32(prefix_attempts);i<4096;i+=1){')
    initial = suffix[:suffix_loop]
    for text in ['State(position.xyz,direction.xyz)', 'var path=position.w;let entry=direction.w;',
                 'cs_states[6u*id+4u]', 'dp_layer=bitcast<i32>(cs_states[6u*id+5u].x)',
                 'var departing=0;', 'let physical=true;']:
        assert text in initial
    suffix = original[:original_loop] + suffix[suffix_loop:]
    suffix = replace_once(suffix,
        'for(var i=i32(prefix_attempts);i<4096;i+=1){cs_suffix_attempts+=1u;',
        'for(var i=0;i<4096;i+=1){')
    start = original.index('        if physical && g.w==0.0 && vertical<0.0{')
    h0 = original[start:block_end(original, start)]
    suffix = replace_once(suffix,
        '        if g.w==0.0{cs_failure_reason=4u;return trace_result(state.p,state.u,path,2u);}', h0)
    suffix = replace_once(suffix,
        '        if !reference_safe{cs_failure_reason=4u;return trace_result(state.p,state.u,path,2u);}\n', '')
    a = original.index('        var reference_distance=-1.0;var reference=accepted;')
    b = original.index('        if contact>0 && contact_height==settings.budget.y', a)
    x = suffix.index('        if (g.w>0.0 && accepted.geometry.w<=0.0)')
    y = suffix.index('        if contact>0 && contact_height==settings.budget.y', x)
    replacement = suffix[x:y]
    assert '!(scene_error.y<=tolerance)' in replacement
    assert '!cs_curve_scene_clear(state,step,scene_error.y)' in replacement
    suffix = suffix[:x] + original[a:b] + suffix[y:]
    suffix = replace_once(suffix, 'cs_failure_reason=5u;return trace_result(state.p,state.u,path,2u);',
                          'return trace_result(state.p,state.u,path,2u);')
    same(suffix, original)

    helpers = []
    sources = [old, controller,
               read(ROOT / 'src/desktop/shaders/refraction_dopri_hermite.wesl'),
               read(ROOT / 'scripts/optical_normal_interval.wgsl'),
               read(ROOT / 'scripts/optical_cartesian_clearance.wgsl')]
    for source_index, source in enumerate(sources):
        for name in set(re.findall(r'\bfn (\w+)\(', source)) - {'trace', 'normal_scene'}:
            for module, assembled in modules.items():
                if re.search(r'\bfn ' + name + r'\(', assembled):
                    same(function(assembled, name), function(source, name))
                    helpers.append([source_index, module, name])

    entry = function(modules['prefix'], 'cs_prefix')
    launch = function(old, 'normal_scene').split('let ray=trace(direction);')[0]
    e = entry[:entry.index('cs_states[6u*id.x+2u]=')]
    e = replace_once(e, 'fn cs_prefix(', 'fn normal_scene(')
    e = replace_once(e, 'terminals[0]=vec4(0u);', 'terminals[0]=settings.control;')
    e = replace_once(e, 'cs_initialize_record(id.x);', '')
    e = replace_once(e, 'cs_masked(id.x);', '')
    same(e, launch)
    initialization = function(modules['prefix'], 'cs_initialize_record')
    assert 'word<6u' in initialization and 'cs_states[6u*id+word]=vec4(0u)' in initialization
    assert 'terminals[detail_index]=vec4(0u);terminals[detail_index+1u]=vec4(0u)' in initialization
    dispatch = read(ROOT / 'scripts/optical_cartesian_suffix_fixture.rs')
    sequence = re.findall(r'entry: "(cs_\w+)"', dispatch)
    assert sequence == ['cs_prefix', 'cs_suffix', 'cs_fallback', 'cs_validate', 'cs_finalize']
    final = function(modules['fallback'], 'cs_finalize')
    assert 'atomicLoad(&cs_progress[0])==0u' in final
    assert 'atomicLoad(&cs_progress[1])==23u' in final
    assert 'atomicLoad(&cs_progress[3])==0u' in final
    suffix_entry = function(modules['suffix'], 'cs_suffix')
    assert 'if ray.status==0u{' in suffix_entry
    assert 'cs_publish(id,ray,1u);return;' in suffix_entry
    fallback_entry = function(modules['fallback'], 'cs_fallback')
    assert 'bitcast<vec3<f32>>(cs_states[6u*id+2u].xyz)' in fallback_entry
    assert 'let ray=trace(direction)' in fallback_entry
    return {'prefix_return_wrappers_reversed': count,
            'whole_original_prefix_and_fallback_match': True,
            'whole_dp_controller_matches_after_reviewed_replacements': True,
            'original_launch_and_mask_expressions_match': True,
            'exact_helper_body_comparisons': sorted(helpers), 'dispatch_sequence': sequence,
            'initialization_routes_and_header_gate_reviewed': True}


def review(prefix):
    prefix = ROOT / prefix
    source = json.loads(read(str(prefix) + '-sources.json'))
    stability = json.loads(read(str(prefix) + '-input-stability.json'))
    lifecycle = json.loads(read(str(prefix) + '-lifecycle.json'))
    outputs = json.loads(read(str(prefix) + '-outputs.json'))
    validation = json.loads(read(str(prefix) + '-validation.json'))
    assert source['compiler_inputs_unchanged'] and stability['inputs_unchanged_through_run']
    compile_inputs = json.loads(read(str(prefix) + '-compile-inputs.json'))
    assert compile_inputs['inputs'] == stability['after_run']
    runtime_only = [item for item in source['files'] if item not in stability['after_run']]
    assert len(runtime_only) == 1 and runtime_only[0]['path'].endswith('.exe')
    assert source['files'][:-1] == stability['after_run']
    assert lifecycle['exit_code'] == 0 and not lifecycle['timed_out']
    assert validation['all_modules_valid'] and validation['module_count'] == 3
    for item in stability['after_run'] + outputs:
        assert digest(ROOT / item['path']) == item['sha256'], item['path']
    identity = []
    for name in ['primary', 'geometry', 'frames', 'field', 'certificates']:
        left = Path(str(prefix) + '-' + name + '.bin')
        right = ROOT / ('artifacts/optical-normal-scene-reference-selected-' + name + '.bin')
        assert left.read_bytes() == right.read_bytes(), name
        identity.append({'name': name, 'bytes': left.stat().st_size, 'sha256': digest(left)})
    return {'scope': 'Independent assembled-source and frozen-input gate only. No GPU numerical, physical accuracy, scene-completion, performance or production acceptance.',
            'checked_prefix': str(prefix.relative_to(ROOT)), 'input_files_verified': len(source['files']),
            'output_files_verified': len(outputs), 'runtime_executable_record': runtime_only[0],
            'runtime_executable_current_hash_matches': digest(ROOT / runtime_only[0]['path']) == runtime_only[0]['sha256'],
            'exact_baseline_uploads': identity,
            'assembled_source': source_checks(prefix), 'decision': 'approve-one-bounded-selected-GPU-diagnostic',
            'reviewer_source_sha256': digest(__file__)}


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('prefix')
    parser.add_argument('--output', required=True)
    args = parser.parse_args()
    output = ROOT / args.output
    assert not output.exists(), 'Preserve previous evidence'
    result = review(args.prefix)
    output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({k: result[k] for k in ['decision', 'input_files_verified', 'output_files_verified']}))
