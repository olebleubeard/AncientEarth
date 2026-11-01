"""Independent finite/source fixtures for the prospective Cartesian trial.

No ODE or GPU execution. Closed-form radial, zero-gradient motion checks the
tableau/dense layout; exact rational checks pin units and the fixed threshold.
"""
from fractions import Fraction as Q
from pathlib import Path
import re
import struct
import unittest

import mpmath as mp

import optical_cartesian_trial_reference as target


ROOT = Path(__file__).resolve().parents[1]
ASSEMBLED = ROOT/'artifacts/optical-cartesian-trial-validation-second.wgsl'
C = list(map(Q, ['0', '1/5', '3/10', '4/5', '8/9', '1', '1']))


def words(values):
    return list(struct.unpack('<36I', struct.pack('<36f', *values)))


def radial(speed=1.):
    values = [0.]*36
    values[0] = values[5] = values[10] = 2.**-22
    values[13] = 1.
    values[17] = 128.
    values[21] = speed
    values[23] = 256.
    values[25] = .0003
    return words(values)


def body(source, name):
    marker = f'fn {name}('
    assert source.count(marker) == 1
    start = source.index(marker)
    opening = source.index('{', start)
    depth = 0
    for i in range(opening, len(source)):
        if source[i] == '{': depth += 1
        if source[i] == '}': depth -= 1
        if depth == 0: return source[start:i+1]
    raise AssertionError('unclosed function')


def promoted(source):
    """Token-based expected transformation, independently of Rust replacements."""
    identifiers = {'f32': 'f64', 'State': 'ct64_State', 'DpStep': 'ct64_DpStep',
        'settings': 'ct64_settings', 'qmul': 'ct64_qmul', 'dopri_delta': 'ct64_dopri_delta',
        'dp_finite': 'ct64_dp_finite', 'dp_invalid': 'ct64_dp_invalid'}
    result = re.sub(r'\b[A-Za-z_]\w*\b', lambda m: identifiers.get(m[0], m[0]), source)
    result = re.sub(r'\bgeometry(?=\()', 'ct64_geometry', result)
    result = re.sub(r'\b(vec[234])(?=\()', r'\1<f64>', result)
    result = result.replace('bitcast<vec3<u32>>', 'bitcast<vec3<u64>>')
    result = result.replace('var h=0.0', 'var h:f64=0.0')
    return result


class Review(unittest.TestCase):
    def test_exact_tableau_and_group_units(self):
        self.assertEqual([sum(map(Q, row)) for row in target.A], C)
        self.assertEqual(sum(map(Q, target.E)), 0)
        self.assertEqual(sum(map(Q, target.D)), 0)
        self.assertEqual(target.BOUND, Q(1, 128000))
        groups = target.groups(Q(256))
        covered = [j for _, first, width, _ in groups for j in range(first, first+width)]
        self.assertEqual(sorted(covered), list(range(77))+list(range(80, 188)))
        self.assertEqual(len(set(covered)), len(covered))
        for name, _, _, scale in groups:
            expected = (256**2 if name.endswith('_bend') else 256 if
                name in ('delta_u', 'end_normal', 'du', 'ur3', 'ur4', 'ur5', 'm') or
                name.endswith(('_u', '_normal', '_N', '_dPds')) else 1)
            self.assertEqual(scale, expected, name)

    def test_closed_form_radial_zero_gradient(self):
        with mp.workdps(100):
            for speed in (1., 1.25):
                data = radial(speed)
                result = target.reference(data)
                eps = mp.mpf('1e-85')
                n = mp.mpf(struct.unpack('<36f', struct.pack('<36I', *data))[25])
                self.assertLess(max(abs(x-y) for x, y in zip(result[:6], [0, 256*speed, 0, 0, 1, 0])), eps)
                for i, c in enumerate(C):
                    start = 80+14*i
                    h = 128+256*mp.mpf(c.numerator)/c.denominator*speed
                    expected = [0, h, 0, 0, speed, 0, 0, 1, 0, h, n, 0, 0, 0]
                    self.assertLess(max(abs(x-y) for x, y in zip(result[start:start+14], expected)), eps)
                for i in range(5):
                    expected = [0, 128+64*i*speed, 0, 0, 1, 0, 0, speed, 0]
                    self.assertLess(max(abs(x-y) for x, y in zip(result[32+9*i:41+9*i], expected)), eps)
                for i in list(range(11, 32))+list(range(178, 185))+[186, 187, 6]:
                    self.assertLess(abs(result[i]), eps)
                self.assertLess(abs(result[185]-speed), eps)

    def test_fixed_accuracy_and_validity_gates(self):
        data = radial()
        with mp.workdps(100):
            actual = list(map(float, target.reference(data)))
        self.assertTrue(target.compare(data, actual)['passed_arithmetic_gate'])
        bound = float(target.BOUND)
        for field, unit in [(178, 1.), (3, 256.), (91, 256.**2), (90, 256.)]:
            near = actual.copy(); near[field] += bound/(2*unit)
            far = actual.copy(); far[field] += 2*bound/unit
            self.assertTrue(target.compare(data, near)['passed_arithmetic_gate'])
            self.assertFalse(target.compare(data, far)['passed_arithmetic_gate'])
        for first, replacement in [(6, 1e30), (185, 0.)]:
            invalid = actual.copy(); invalid[first] = replacement
            self.assertFalse(target.compare(data, invalid)['complete_finite_valid_trial'])
        invalid = actual.copy(); invalid[86:89] = [0.]*3
        self.assertFalse(target.compare(data, invalid)['complete_finite_valid_trial'])

    def test_assembled_original_promotion_and_detail(self):
        shader = ASSEMBLED.read_text()
        geometry = (ROOT/'src/desktop/shaders/refraction.wesl').read_text()
        dp = (ROOT/'src/desktop/shaders/refraction_dopri_hermite.wesl').read_text()
        for name, original in [('qmul', geometry), ('geometry', geometry), ('dp_finite', dp),
                               ('dp_invalid', dp), ('dopri_delta', dp)]:
            self.assertEqual(body(shader, name), body(original, name))
            self.assertEqual(body(shader, 'ct64_'+name), promoted(body(original, name)))
        for bits, name in [('32', 'dopri_delta'), ('64', 'ct64_dopri_delta')]:
            detail = body(shader, name+'_detail')
            capture_pattern = rf'\n    ct_stage_{bits}\([^;]+\);'
            self.assertEqual(len(re.findall(capture_pattern, detail)), 7)
            detail = re.sub(capture_pattern, '', detail)
            error_pattern = rf'\n    ct_errors_{bits}\([^;]+\);'
            self.assertEqual(len(re.findall(error_pattern, detail)), 1)
            detail = re.sub(error_pattern, '', detail).replace(f'fn {name}_detail(', f'fn {name}(', 1)
            self.assertEqual(detail, body(shader, name))
            plain = body(shader, 'ct_core_'+bits)
            hashed = body(shader, 'ct_hash_core_'+bits)
            plain_fields = re.findall(r'result\[\d+u\]=(c\.[^;]+);', plain)
            hash_fields = re.findall(r'hash=ct_hash_value\(hash,f64\((c\.[^;]+)\)\);', hashed)
            self.assertEqual(len(plain_fields), 32)
            self.assertEqual(plain_fields, hash_fields)
            for variable in ('theta', 'v', 'p', 'u', 'derivative'):
                pattern = rf'let {variable}=([^;]+);'
                self.assertEqual(re.search(pattern, plain)[1], re.search(pattern, hashed)[1])
        self.assertIn('u32(bits>>32u)', body(shader, 'ct_hash_value'))
        self.assertNotIn('ct64_', body(shader, 'ct_load_32'))
        self.assertNotIn('ct_stage_', body(shader, 'dopri_delta'))


if __name__ == '__main__':
    unittest.main()
