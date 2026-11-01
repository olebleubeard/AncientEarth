"""Independent assembled-source gate for the two isolated f64 suffix cases."""
import hashlib
import io
import json
from pathlib import Path
import re
import unittest

from test_optical_cartesian_trial_reference import body
import test_optical_cartesian_f64_clearance as bridge_tests

ROOT = Path(__file__).resolve().parents[1]
OLD = 'artifacts/optical-cartesian-suffix-selected-first'
CASES = {
    'selected': 'artifacts/optical-cartesian-f64-selected-validation-first',
    'atmosphere-100-gradient': 'artifacts/optical-cartesian-f64-atmosphere-validation-first',
}


def text(name):
    return (ROOT/name).read_text(encoding='utf-8').replace('\r\n', '\n')


def sha(name):
    with (ROOT/name).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def names(source):
    return set(re.findall(r'\bfn (\w+)\(', source))


def compact(source):
    return re.sub(r'\s+', '', source)


def promote(source):
    """Independent token transform; uploaded field access is widened on use."""
    source = re.sub(r'\bsettings\.layers\[([^\]]+)\]', r'cf64_layer(\1)', source)
    source = re.sub(r'\bsettings\.(q0|q1|q2|origin|budget|scene)\b', r'cf64_\1()', source)
    assert 'settings.' not in source
    rename = {'State', 'Trace', 'DpStep', 'DpContact', 'qmul', 'knot_bound',
              'trace_result', 'dopri_delta', 'cs_suffix_trace'}
    source = re.sub(r'\bgeometry(?=\()', 'cf64_geometry', source)
    source = re.sub(r'\b[A-Za-z_]\w*\b', lambda m: 'f64' if m[0] == 'f32' else
                    'cf64_'+m[0] if m[0] in rename or m[0].startswith('dp_') else m[0], source)
    source = re.sub(r'\b(vec[234])(?=\()', r'\1<f64>', source)
    source = source.replace('bitcast<vec3<u32>>', 'bitcast<vec3<u64>>')
    source = re.sub(r'\bvar (h|low|allocation|contact_height)=0\.0', r'var \1:f64=0.0', source)
    return source


def run():
    old = text(OLD+'-suffix.wgsl')
    original_controller = body(old, 'cs_suffix_trace')
    owner = '        if i==i32(prefix_attempts) && !cf64_initial_owner(state,g,cf64_dp_layer){cs_failure_reason=2u;return cf64_trace_result(state.p,state.u,path,2u);}\n'
    old_scene = '''        let scene_error=nc_interval_add(nc_interval_exact(accepted.error),nc_interval_exact(allocation));
        if !nc_interval_valid(scene_error) || !(scene_error.y<=tolerance) || !cs_curve_scene_clear(state,step,scene_error.y){
            cs_failure_reason=3u;return trace_result(state.p,state.u,path,2u);
        }'''
    new_scene = '''        if !cf64_curve_scene_clear(state.p,dp_curve.delta.p,dp_curve.r3,dp_curve.r4,dp_curve.r5,step,dp_curve_step,accepted.error,allocation,tolerance){
            cs_failure_reason=3u;return trace_result(state.p,state.u,path,2u);
        }'''
    assert original_controller.count(old_scene) == 1
    start = original_controller.index('    for(var i=i32(prefix_attempts);')
    tail = original_controller[start:].replace(old_scene, new_scene)
    tail = tail.replace('let tolerance=max(0.002,4e-7*length(state.p));',
                        'let tolerance=0.25*max(0.002,4e-7*length(state.p));')
    expected_tail = promote(tail)
    summaries = []
    reviewed_paths = [__file__, 'scripts/optical_cartesian_suffix_f64_fixture.rs',
        'scripts/optical_cartesian_f64_clearance.wgsl',
        'scripts/test_optical_cartesian_f64_clearance.py',
        'scripts/optical_cartesian_clearance.wgsl', 'scripts/optical_normal_interval.wgsl',
        'docs/science/cartesian-f64-suffix-design.md',
        'scripts/optical_cartesian_suffix_atmosphere_fixture.rs',
        'scripts/optical_cartesian_suffix_fixture.rs', OLD+'-suffix.wgsl']
    numeric_names = {n for n in names(old) if n.startswith('dp_') or n in
                     {'qmul', 'geometry', 'knot_bound', 'trace_result', 'dopri_delta'}}
    promoted_source = None
    for case, prefix in CASES.items():
        source = text(prefix+'-suffix.wgsl')
        if promoted_source is None: promoted_source = source
        else: assert source == promoted_source, 'Atmosphere must change uploaded data, not suffix equations'
        actual_names = names(source)
        checked = []
        for name in numeric_names:
            new = 'cf64_'+name
            if new in actual_names:
                assert body(source, new) == promote(body(old, name)), name
                assert 'f32' not in body(source, new)
                checked.append(name)
        assert len(checked) >= 18
        bridge_source = text('scripts/optical_cartesian_f64_clearance.wgsl')
        for name in names(bridge_source):
            assert body(source, name) == body(bridge_source, name)
        unchanged = []
        for name in actual_names:
            if not name.startswith('cf64_') and name != 'cs_suffix':
                assert body(source, name) == body(old, name), name
                unchanged.append(name)
        controller = body(source, 'cf64_cs_suffix_trace')
        assert controller.count(owner) == 1
        actual_tail = controller[controller.index('    for(var i=i32(prefix_attempts);'):].replace(owner, '')
        assert actual_tail == expected_tail
        initial = controller[:controller.index('    for(var i=i32(prefix_attempts);')]
        assert 'var state=cf64_State(position.xyz,direction.xyz);var path=position.w;let entry=direction.w;' in initial
        assert 'var proposed:f64=100000.0;var g=cf64_geometry(state.p);' in initial
        assert 'normalize' not in initial and 'f32' not in controller
        assert 'cf64_dp_layer=bitcast<i32>(cs_states[6u*id+5u].x)' in initial
        assert 'let prefix_attempts=cs_states[6u*id+3u].x;' in initial
        for field in ('q0', 'q1', 'q2', 'origin', 'budget', 'scene'):
            assert body(source, 'cf64_'+field) == f'fn cf64_{field}()->vec4<f64>{{return vec4<f64>(settings.{field});}}'
        assert body(source, 'cf64_layer') == 'fn cf64_layer(index:u32)->vec4<f64>{return vec4<f64>(settings.layers[index]);}'
        for name, slot in [('position', '6u*id'), ('direction', '6u*id+1u')]:
            assert body(source, 'cf64_saved_'+name) == f'fn cf64_saved_{name}(id:u32)->vec4<f64>{{return vec4<f64>(bitcast<vec4<f32>>(cs_states[{slot}]));}}'
        admission = body(source, 'cf64_initial_owner')
        for predicate in ('owner< -1||owner>=i32(count)-1', 'g.w>scene.z&&g.w<budget.y',
                          'g.w==0.0', 'dot(state.u,state.u)>0.0',
                          'if owner<0{return g.w<scene.y;}',
                          'g.w>cf64_layer(u32(owner)).x&&g.w<cf64_layer(u32(owner)+1u).x'):
            assert predicate in admission
        entry = body(source, 'cs_suffix')
        assert 'if cs_states[6u*id+2u].w!=1u{return;}' in entry
        assert 'ray.status==0u&&cf64_dp_finite' in entry
        assert entry.count('vec3<f32>') == 2 and entry.count('f32(ray.path)') == 1
        assert 'cs_publish(id,trace_result(vec3<f32>(ray.p),vec3<f32>(ray.u),f32(ray.path),0u),1u);return;' in entry
        assert 'cs_states[6u*id+3u].w=cs_failure_reason;cs_states[6u*id+2u].w=2u;' in entry
        for module in ('prefix', 'fallback'):
            original = text(OLD+'-'+module+'.wgsl')
            current = text(prefix+'-'+module+'.wgsl')
            if case == 'selected':
                assert current == original
            else:
                baseline = text('artifacts/optical-cartesian-matrix-atmosphere-100-gradient-reference-first-'+module+'.wgsl')
                assert names(current) == names(original)
                for name in names(current):
                    expected = body(original, name)
                    if name in ('scene_event', 'terrain_coverage'):
                        stub = ('return SceneEvent(-1.0,vec3(0.0),0u,0u,0u,0.0,0.0);'
                                if name == 'scene_event' else 'return 1u;')
                        expected = expected[:expected.index('{')+1]+stub+'}'
                    if name == 'cs_prefix':
                        launch = re.search(r'let atmosphere_rays=.*?;let direction=normalize\(atmosphere_rays\[id.x%3u\]\);', baseline)[0]
                        old_launch = 'let direction=normalize(settings.right.xyz*optical.x+settings.up.xyz*optical.y+settings.forward.xyz*optical.z);'
                        assert expected.count(old_launch) == 1
                        expected = expected.replace(old_launch, launch)
                    assert compact(body(current, name)) == compact(expected), (case, module, name)
            reviewed_paths += [prefix+'-'+module+'.wgsl', OLD+'-'+module+'.wgsl']
        reference_prefix = OLD if case == 'selected' else 'artifacts/optical-cartesian-matrix-atmosphere-100-gradient-reference-first'
        for suffix in ('-primary.bin', '-field.bin', '-geometry.bin', '-frames.bin', '-certificates.bin'):
            assert (ROOT/(prefix+suffix)).read_bytes() == (ROOT/(reference_prefix+suffix)).read_bytes()
            reviewed_paths += [prefix+suffix, reference_prefix+suffix]
        reviewed_paths += [prefix+'-suffix.wgsl', prefix+'-validation.json']
        validation = json.loads(text(prefix+'-validation.json'))
        assert validation['all_modules_valid'] and validation['module_count'] == 3
        assert validation['selected'] and validation['state_words_per_record'] == 24
        assert validation['cartesian_suffix_f64'] == case
        summaries.append({'case': case, 'promoted_functions_exact': sorted(checked),
            'unchanged_interval_publication_functions_exact': sorted(unchanged),
            'controller_tail_reversal_exact': True, 'raw_handoff_no_initial_normalization': True,
            'counted_strict_initial_owner_guard': True, 'final_success_only_f32_conversion': True,
            'five_frozen_uploads_exact': True, 'prefix_fallback_expected_transforms_exact': True})
    suite = unittest.defaultTestLoader.loadTestsFromModule(bridge_tests)
    output = io.StringIO(); outcome = unittest.TextTestRunner(stream=output, verbosity=2).run(suite)
    assert outcome.wasSuccessful() and outcome.testsRun == 4
    hashes = {str(Path(p).relative_to(ROOT) if Path(p).is_absolute() else p): sha(p)
              for p in reviewed_paths}
    return {'schema': 'cartesian-f64-suffix-independent-source-review-v1', 'passed': True,
        'cases': summaries, 'sha256': hashes, 'bridge_groups': outcome.testsRun,
        'bridge_test_output': output.getvalue(), 'gpu_executions': 0, 'ode_executions': 0,
        'proof_review': [
            'Conversion equality uses exact normal f32 widening; non-singletons explicitly compare both widened endpoints against the actual stored f64. Tiny values get signed normal bounds; invalid and overflow decline.',
            'The interval quotient contains exact accepted_distance/full_step and observes the local native quotient. Its f32 eight-neighbor margin also dominates independently rounded f64 quotients over this admitted denominator domain; zero/tiny numerator gets the wider normal FTZ bound.',
            'Quartic power-to-Bernstein controls enclose the entire scaled prefix, not only the endpoint; affine uploaded rows and existing source/frame/tube gains are unchanged.',
            'The retained 64*2^-23 L1 band dominates the <=12-operation f64 dense-position error and subnormal losses. Coefficient and parameter conversion error are already enclosed independently.',
            'Physical/polynomial sag and derivative speed gain remain; outward error plus allocation must be no larger than the actual f64 tolerance before exclusion.',
            'Strict recorded initial owner is checked inside the first counted attempt; subsequent source roots/ownership, h0 guards, normalization, caps and fallback follow exact promoted original equations.',
            'The old source-height guards remain numerical controls for the uploaded field; this review does not upgrade them to a rigorous global ODE or canonical-height proof.'],
        'scope': 'Approves only the two selected untimed diagnostics under frozen required-route and numerical gates. Earlier rejected workloads remain rejected. No runtime, complete-ray accuracy or performance result.'}


if __name__ == '__main__':
    result = run()
    destination = ROOT/'artifacts/optical-cartesian-f64-source-review-first.json'
    with destination.open('x', encoding='utf-8') as stream:
        json.dump(result, stream, indent=2, allow_nan=False); stream.write('\n')
    print(json.dumps({'passed': result['passed'], 'cases': len(result['cases']),
                      'source_hashes': len(result['sha256']), 'bridge_groups': result['bridge_groups']}))
