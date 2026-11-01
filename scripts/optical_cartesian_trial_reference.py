"""Independent high-precision arithmetic target for one fixed Cartesian trial.

The ordinary RK tableau constructs the stages independently of the shader's
common-derivative form. This is a finite arithmetic comparison, not an ODE,
source-ownership, contact or complete-controller acceptance test.
"""
import math
import struct
from fractions import Fraction

import mpmath as mp

DIGITS = 100
BOUND = Fraction(1, 128000)
A = [[], ['1/5'], ['3/40', '9/40'], ['44/45', '-56/15', '32/9'],
     ['19372/6561', '-25360/2187', '64448/6561', '-212/729'],
     ['9017/3168', '-355/33', '46732/5247', '49/176', '-5103/18656'],
     ['35/384', '0', '500/1113', '125/192', '-2187/6784', '11/84']]
E = ['71/57600', '0', '-71/16695', '71/1920', '-17253/339200', '22/525', '-1/40']
D = ['-12715105075/11282082432', '0', '87487479700/32700410799',
     '-10690763975/1880347072', '701980252875/199316789632',
     '-1453857185/822651844', '69997945/29380423']
assert sum(map(Fraction, E)) == sum(map(Fraction, D)) == 0
assert sum(map(Fraction, A[-1])) == 1


def rational(value):
    q = Fraction(value)
    return mp.mpf(q.numerator)/q.denominator


def dot(a, b):
    return sum(x*y for x, y in zip(a, b))


def norm(a):
    return mp.sqrt(dot(a, a))


def add(a, b):
    return [x+y for x, y in zip(a, b)]


def sub(a, b):
    return [x-y for x, y in zip(a, b)]


def scale(a, x):
    return [v*x for v in a]


def normalize(a):
    length = norm(a)
    assert length > 0
    return scale(a, 1/length)


def weighted(weights, vectors):
    return [sum(w*v[j] for w, v in zip(weights, vectors)) for j in range(3)]


def geometry(p, q, origin):
    """Five fixed inverse updates; no high-precision convergence substitution."""
    multiply = lambda v: [dot(row, v) for row in q]
    product = multiply(p)
    gradient = add(origin[:3], product)
    implicit = origin[3]+dot(p, add(scale(origin[:3], 2), product))
    assert norm(gradient) >= rational('3/4')
    normal = normalize(gradient)
    for _ in range(5):
        qn = multiply(normal)
        projection = dot(normal, gradient)
        denominator = projection+mp.sqrt(max(mp.mpf(0), projection**2-implicit*dot(normal, qn)))
        assert denominator > 0
        height = implicit/denominator
        normal = normalize(sub(gradient, scale(qn, height)))
    return normal+[height]


@mp.workdps(DIGITS)
def reference(input_words):
    assert len(input_words) == 36
    values = [mp.mpf(x) for x in struct.unpack('<36f', struct.pack('<36I', *input_words))]
    q = [values[4*j:4*j+3] for j in range(3)]
    origin, p0, u0, length, branch = values[12:16], values[16:19], values[20:23], values[23], values[24:27]
    assert length > 0 and norm(u0) > 0
    position, velocity, normals, indices, bends = [], [], [], [], []
    for row in A:
        weights = list(map(rational, row))
        p = add(p0, scale(weighted(weights, velocity), length))
        u = add(u0, scale(weighted(weights, bends), length))
        g = geometry(p, q, origin)
        excess = branch[1]+branch[2]*(g[3]-branch[0])
        assert rational('1/2') <= 1+excess <= mp.mpf('3e38')
        bend = scale(sub(g[:3], scale(u, dot(u, g[:3]))), branch[2]/(1+excess))
        position.append(p); velocity.append(u); normals.append(g); indices.append(excess); bends.append(bend)
    weights = list(map(rational, A[-1]))
    delta = scale(weighted(weights, velocity), length)
    du = scale(weighted(weights, bends), length)
    r3 = sub(scale(u0, length), delta)
    r4 = sub(sub(delta, scale(velocity[6], length)), r3)
    r5 = scale(weighted(list(map(rational, D)), velocity), length)
    ur3 = sub(scale(bends[0], length), du)
    ur4 = sub(sub(du, scale(bends[6], length)), ur3)
    ur5 = scale(weighted(list(map(rational, D)), bends), length)
    ep = scale(weighted(list(map(rational, E)), velocity), length)
    eu = scale(weighted(list(map(rational, E)), bends), length**2)
    en = length*sum(rational(w)*v for w, v in zip(E, indices))
    nr5 = length*sum(rational(w)*v for w, v in zip(D, indices))
    axis = normalize(u0)
    c0 = dot(u0, axis); c1 = dot(add(du, ur3), axis)
    c2 = dot(add(sub(ur4, ur3), ur5), axis)
    c3 = dot(sub(scale(ur4, -1), scale(ur5, 2)), axis); c4 = dot(ur5, axis)
    q2, q3 = c2-c4, c3+2*c4
    m = min(c0, c0+c1/4, c0+c1/2+c2/6, c0+3*c1/4+c2/2+c3/4,
            c0+c1+c2+c3+c4, c0+c1/2+q2/6, c0+3*c1/4+q2/2+q3/4)
    assert m > 0
    interior = max(norm(r5), 2*length*norm(ur5)/m, abs(nr5))/16
    error = max(norm(ep), norm(eu), abs(en))+interior
    core = delta+normalize(add(u0, du))+[error]+normals[6]+r3+r4+r5+du+ur3+ur4+ur5
    assert len(core) == 32
    for i in range(5):
        t = mp.mpf(i)/4; v = 1-t
        p = add(p0, scale(add(delta, scale(add(r3, scale(add(r4, scale(r5, v)), t)), v)), t))
        u = normalize(add(u0, scale(add(du, scale(add(ur3, scale(add(ur4, scale(ur5, v)), t)), v)), t)))
        derivative = scale(add(add(delta, scale(r3, 1-2*t)),
                               add(scale(r4, 2*t-3*t*t), scale(r5, 2*t-6*t*t+4*t*t*t))), 1/length)
        core += p+u+derivative
    core += [length, mp.mpf(0), mp.mpf(0)]
    for p, u, g, n, b in zip(position, velocity, normals, indices, bends):
        core += p+u+g+[n]+b
    core += ep+eu+[en, m, interior, nr5]
    assert len(core) == 188 and all(mp.isfinite(x) for x in core)
    return core


def groups(length):
    """(name,start,width,metre multiplier) for all non-padding outputs."""
    result = [('delta_p', 0, 3, 1), ('delta_u', 3, 3, length), ('error', 6, 1, 1),
              ('end_normal', 7, 3, length), ('end_height', 10, 1, 1)]
    for i, name in enumerate(('r3', 'r4', 'r5', 'du', 'ur3', 'ur4', 'ur5')):
        result.append((name, 11+3*i, 3, 1 if i < 3 else length))
    for i in range(5):
        for j, (name, multiplier) in enumerate((('p', 1), ('u', length), ('dPds', length))):
            result.append((f'dense_{i}_{name}', 32+9*i+3*j, 3, multiplier))
    for i in range(7):
        for name, offset, width, multiplier in [('p', 0, 3, 1), ('u', 3, 3, length),
                ('normal', 6, 3, length), ('height', 9, 1, 1), ('N', 10, 1, length), ('bend', 11, 3, length**2)]:
            result.append((f'stage_{i}_{name}', 80+14*i+offset, width, multiplier))
    result += [('ep', 178, 3, 1), ('eu', 181, 3, 1), ('en', 184, 1, 1),
               ('m', 185, 1, length), ('interior', 186, 1, 1), ('nr5', 187, 1, 1)]
    return result


def compare(input_words, encoded_values):
    """Caller freezes this algorithm before GPU output; no fitted threshold."""
    assert len(encoded_values) == 188 and all(math.isfinite(v) for v in encoded_values)
    with mp.workdps(DIGITS):
        target = reference(input_words)
        length = target[77]
        assert encoded_values[77] == float(length) and encoded_values[78:80] == [0., 0.]
        complete = encoded_values[6] < 1e29 and encoded_values[185] > 0
        for i in range(7):
            normal = list(map(mp.mpf, encoded_values[86+14*i:89+14*i]))
            complete = complete and rational('1/2') < norm(normal) < 2
            complete = complete and rational('1/2') <= 1+mp.mpf(encoded_values[90+14*i]) <= mp.mpf('3e38')
        bound = rational(BOUND)
        comparisons = []
        for name, first, width, multiplier in groups(length):
            difference = norm([mp.mpf(encoded_values[j])-target[j] for j in range(first, first+width)])*multiplier
            comparisons.append({'field': name, 'difference_metres': mp.nstr(difference, 40),
                                'within_bound': bool(difference <= bound)})
        return {'complete_finite_valid_trial': bool(complete), 'bound_metres_exact': str(BOUND),
                'target': [mp.nstr(x, 105) for x in target], 'comparisons': comparisons,
                'passed_arithmetic_gate': bool(complete and all(x['within_bound'] for x in comparisons))}
