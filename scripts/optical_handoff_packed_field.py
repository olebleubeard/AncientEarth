"""CPU necessary screen: promote the exact uploaded field constants to f64.

This is a continuous-field DOP853 hypothesis, not WGSL arithmetic emulation.
The uploaded row matrix is deliberately not symmetrized. The five-update
normal is not asserted to be the exact gradient of a symmetric quadric.
Source slopes and one-sided endpoint discontinuities are retained as packed.
No scene, GPU controller, mixed-precision implementation or speed is tested.
"""
import hashlib
import inspect
import math
import struct
from fractions import Fraction as Q

import numpy as np


def dot(a, b):
    """Ordered scalar binary64 products/additions, without fused operations."""
    return (float(a[0])*float(b[0])+float(a[1])*float(b[1]))+float(a[2])*float(b[2])


def norm(a):
    return math.sqrt(dot(a, a))


def normalized(a):
    length = norm(a)
    assert math.isfinite(length) and length > 0, 'invalid normal denominator'
    return tuple(float(x)/length for x in a)


def bits64(a):
    return struct.pack('<3d', *a)


class PackedField:
    def __init__(self, data):
        assert len(data) == 140*16, 'unexpected primary packet layout'
        self.words = np.frombuffer(data, dtype='<u4').reshape(140, 4).copy()
        packet = self.words.view('<f4').astype(float)
        assert np.isfinite(packet[:11]).all()
        count = float(packet[9, 0])
        assert count.is_integer() and 2 <= count <= 128
        self.count = int(count)
        self.q = packet[:3, :3].tolist()
        self.origin = packet[3].tolist()
        self.layers = packet[12:12+self.count, :3].copy()
        assert np.isfinite(self.layers).all()
        assert np.all(np.diff(self.layers[:, 0]) > 0)
        assert self.words[10, 1] == self.words[12, 0], 'base bits differ'
        assert self.words[9, 1] == self.words[11+self.count, 0], 'top bits differ'
        self.lower, self.top = float(packet[10, 2]), float(packet[9, 1])
        assert self.lower < self.layers[0, 0] and packet[9, 3] >= .5
        # Positive branch indices over their full represented source interval.
        # Last-node slope is retained, but has no interval above the exit.
        for i in range(self.count-1):
            height, excess, slope = self.layers[i]
            end = excess+slope*(self.layers[i+1, 0]-height)
            assert math.isfinite(end) and min(1.+excess, 1.+end) > 0
        assert 1.+self.layers[0, 1] > 0

    def qmul(self, p):
        return tuple(dot(row, p) for row in self.q)

    def geometry(self, p):
        p = tuple(float(x) for x in p)
        assert len(p) == 3 and all(math.isfinite(x) for x in p)
        mp = self.qmul(p)
        qp = tuple(self.origin[i]+mp[i] for i in range(3))
        f = self.origin[3]+dot(p, tuple(2.*self.origin[i]+mp[i] for i in range(3)))
        assert all(math.isfinite(x) for x in (*mp, *qp, f))
        if norm(qp) < .75:
            return -1000000., np.array([0., 1., 0.])
        normal = normalized(qp)
        h = 0.
        for _ in range(5):
            previous = normal
            qn = self.qmul(normal)
            v = dot(normal, qp)
            radicand = v*v-f*dot(normal, qn)
            assert math.isfinite(radicand)
            denominator = v+math.sqrt(max(0., radicand))
            assert math.isfinite(denominator) and denominator > 0
            h = f/denominator
            normal = normalized(tuple(qp[i]-h*qn[i] for i in range(3)))
            assert math.isfinite(h) and all(math.isfinite(x) for x in normal)
            if bits64(normal) == bits64(previous):
                break
        return h, np.asarray(normal)

    def checked_geometry(self, p):
        h, normal = self.geometry(p)
        assert h != -1000000., 'original geometry sentinel reached'
        return h, normal

    def owner_bounds(self, owner):
        assert -1 <= owner < self.count-1
        return (self.lower if owner == -1 else float(self.layers[owner, 0]),
                float(self.layers[owner+1, 0]))

    def metadata(self):
        return {'primary_words': self.words.tolist(), 'q_rows_f64': self.q,
                'origin_f64': self.origin, 'layers_height_N_slope_f64': self.layers.tolist(),
                'lower_domain': self.lower, 'top': self.top,
                'q_is_bitwise_symmetric': bool(np.array_equal(
                    self.words[:3, :3], self.words[:3, :3].T)),
                'geometry_order': 'Scalar f64 (a0*b0+a1*b1)+a2*b2; sqrt; component division; max five updates; bitwise f64 fixed point. No fusion or WGSL builtin emulation.'}


def integrator(common, field, owner, normalize_initial):
    """Small, asserted edit of the frozen oracle; retain its integration policy."""
    assert common.digest('scripts/verify_optical_dop853.py') == common.ORACLE_SHA256
    source = inspect.getsource(common.oracle.integrate)
    replacements = [
        ('    origin = np.array(case["origin_ecef_m"], dtype=float)\n',
         '    origin = np.array(case["origin_local"], dtype=float)\n'),
        ('    h, up = geometry(origin)\n'
         '    layer = int(np.searchsorted(heights, h, side="right")) - 1\n'
         '    near = np.flatnonzero(abs(heights - h) < 2e-8)\n'
         '    if len(near):\n'
         '        layer = int(near[0]) if direction @ up >= 0 else int(near[0]) - 1\n',
         '    layer = _packed_owner\n'),
        ('            lower, upper = -10000.0, heights[0]\n',
         '            lower, upper = _packed_lower, heights[0]\n'),
        ('            slope = (refractivity[layer + 1] - n_minus_one) / (upper - lower)\n',
         '            slope = _packed_slopes[layer]\n'),
        ('            u = value[3:6]\n',
         '            assert np.isfinite(excess) and 1.0 + excess > 0\n'
         '            u = value[3:6]\n'),
        ('"endpoint_ecef_m": list(origin + state[:3]), "exit_direction": list(final_direction)',
         '"endpoint_local": list(origin + state[:3]), "u_local": list(final_direction)')]
    if not normalize_initial:
        replacements.append(('    direction /= np.linalg.norm(direction)\n', ''))
    for before, after in replacements:
        assert source.count(before) == 1, 'frozen oracle source drift'
        source = source.replace(before, after, 1)
    assert 'final_direction = state[3:6] / np.linalg.norm(state[3:6])' in source
    namespace = dict(common.oracle.__dict__, geometry=field.checked_geometry,
                     _packed_owner=owner, _packed_lower=field.lower,
                     _packed_slopes=field.layers[:, 2])
    exec(compile(source, '<frozen-oracle-promoted-uploaded-field>', 'exec'), namespace)
    return namespace['integrate'], hashlib.sha256(source.encode()).hexdigest()


def encode(result, saved_path, common):
    values = [common.round_f32(x) for x in result['endpoint_local']]
    values += [common.round_f32(Q(saved_path)+Q(result['geometric_path_m']))]
    values += [common.round_f32(x) for x in result['u_local']]
    return np.asarray(values, dtype='<f4').view('<u4').tolist()


def run_case(item, common):
    name, prefix, i, ref, words, terminal, limits, absolute = item
    assert words[22] == 1 and terminal[7] == 0 and words[14] == words[15] == 0
    assert words[8:11].tolist() == ref['launch_words']
    state = words.copy().view('<f4').astype(float)
    assert np.isfinite(state[:8]).all()
    field = PackedField((common.ROOT/(prefix+'-primary.bin')).read_bytes())
    owner = int(words[20].view('<i4'))
    lower, upper = field.owner_bounds(owner)
    h, _ = field.checked_geometry(state[:3])
    direction_norm = float(np.linalg.norm(state[4:7]))
    assert math.isfinite(direction_norm) and direction_norm > 0
    origin = np.asarray(ref['inputs']['origin_ecef_m'], dtype=float)
    basis = common.basis_at(origin)
    row = {'case': name, 'source_prefix': prefix, 'record_index': i,
           'saved_state_words': words.tolist(), 'saved_position_path': state[:4].tolist(),
           'saved_direction_entry': state[4:8].tolist(), 'basis': basis.tolist(),
           'original_eye_ecef': origin.tolist(), 'stored_owner': owner,
           'packed_height': float(h), 'packed_owner_bounds': [lower, upper],
           'packed_field': field.metadata(), 'frozen_limits_exact': limits,
           'raw_local_direction': state[4:7].tolist(),
           'initial_normalization_correction_norm': float(np.linalg.norm(state[4:7]/direction_norm-state[4:7])),
           'hypotheses': {}, 'passed': False}
    if not lower < h < upper:
        row['stop_reason'] = 'Promoted uploaded geometry is not strictly inside the saved owner; no owner reselection.'
        return row
    case = {'origin_local': state[:3].tolist(), 'initial_direction': state[4:7].tolist(),
            'profile_base_height_m': 0.,
            'layers': [{'height_m': float(k[0]), 'refractivity': float(k[1])} for k in field.layers]}
    for label, normalized_start in [('A_raw_initial_vector', False),
                                    ('B_normalized_initial_orientation', True)]:
        integrate, source_hash = integrator(common, field, owner, normalized_start)
        try:
            coarse = integrate(case, False, stop_at_lower_domain=True)
            tight = integrate(case, True, stop_at_lower_domain=True)
            assert coarse['terminal_kind'] == tight['terminal_kind'] == 'atmosphere_exit'
            encoded = [encode(x, float(state[3]), common) for x in (coarse, tight)]
        except (AssertionError, ValueError, FloatingPointError, OverflowError, ZeroDivisionError) as error:
            row['hypotheses'][label] = {'passed': False, 'unresolved': True,
                'oracle_adapter_source_sha256': source_hash,
                'error_type': type(error).__name__, 'error': str(error)}
            continue
        checks = [common.evaluate(x, ref, origin, basis, limits, absolute) for x in encoded]
        domain = all(state[3]-state[7]+x['geometric_path_m'] <= 2_000_000. for x in (coarse, tight))
        stable = encoded[0] == encoded[1] and checks[0]['passed'] == checks[1]['passed']
        difference = {'endpoint_m': float(np.linalg.norm(np.asarray(coarse['endpoint_local'])-tight['endpoint_local'])),
                      'direction': float(np.linalg.norm(np.asarray(coarse['u_local'])-tight['u_local'])),
                      'path_m': abs(coarse['geometric_path_m']-tight['geometric_path_m']),
                      'optical_path_m': abs(coarse['optical_path_m']-tight['optical_path_m'])}
        row['hypotheses'][label] = {'coarse': coarse, 'tight': tight,
            'oracle_adapter_source_sha256': source_hash, 'observed_convergence': difference,
            'encoded_words_coarse_tight': encoded, 'evaluations': checks,
            'encoding_and_gate_stable': stable, 'original_remaining_path_domain': bool(domain),
            'passed': stable and domain and checks[0]['passed'] and checks[1]['passed']}
    row['passed'] = len(row['hypotheses']) == 2 and all(x['passed'] for x in row['hypotheses'].values())
    if not row['passed']:
        row['stop_reason'] = 'Fixed promoted-field hypothesis failed or was inconclusive; stop before implementation.'
    return row
