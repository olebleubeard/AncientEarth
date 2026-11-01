"""Finite scalar f32 producer checks with independent rational/Cartesian oracles.

This transcribes the proposed helper, not a GPU execution. Existing interval
primitive tests supply the allowed-rounding/FTZ lemmas; finite mixed arithmetic
here uses round-to-nearest followed by the same outward widening. Analytic
scalar-majorant and Hermite remainder proofs remain separate from sampling.
"""
from fractions import Fraction as Q
import json
import math
from pathlib import Path
import unittest

import mpmath as mp
import numpy as np

from test_optical_normal_intervals import MIN_NORMAL, TINY_BOUND, bits, value, widen
from test_optical_normal_map_interval import (ZERO, ONE, add, sub, mul, div, rsqrt,
    exact, f32, vadd, vsub, vexact, scale, polynomial, derivative, make_case,
    map_enclosure, canonical)


def positive(x):
    return x == 0 or MIN_NORMAL <= x < 3e37


def up_add(a, b):
    if not (positive(a) and positive(b)):
        raise ValueError('invalid upper bound')
    if a == 0: return b
    if b == 0: return a
    result = widen(f32(a+b), 1)[1]
    if not positive(result): raise ValueError('sum overflow')
    return result


def up_mul(a, b):
    if not (positive(a) and positive(b)):
        raise ValueError('invalid upper bound')
    if a == 0 or b == 0: return 0.
    if (bits(a)>>23)+(bits(b)>>23) > 378: raise ValueError('product domain')
    result = widen(f32(a*b), 1)[1]
    if not positive(result): raise ValueError('product overflow')
    return result


def up_div(a, b):
    if not (positive(a) and positive(b) and b > 0): raise ValueError('quotient domain')
    if not (0x800000 <= bits(b) <= 0x7e800000): raise ValueError('denominator domain')
    if a == 0: return 0.
    if (bits(a)>>23)-(bits(b)>>23) > 125: raise ValueError('quotient overflow')
    result = widen(f32(a/b), 8)[1]
    if not positive(result): raise ValueError('quotient overflow')
    return result


def upper_sum(values):
    result = 0.
    for x in values: result = up_add(result, x)
    return result


def absolute(interval):
    assert interval[0] <= interval[1] and all(abs(x) < 3e37 for x in interval)
    return max(map(abs, interval))


def coordinate_jets(case, t):
    c2,c3,c4,c5 = map(vexact, case['coeff'])
    a2 = vadd(vsub(c4,c3),c5)
    a3 = vsub(scale(c4,(-1.,-1.)),scale(c5,(2.,2.)))
    return [polynomial(case['start'],case['coeff'],t), derivative(case['coeff'],t),
            vadd(a2,scale(vadd(scale(a3,(3.,3.)),scale(c5,mul((6.,6.),t))),t)),
            vadd(a3,scale(c5,mul((4.,4.),t))),c5]


def fourth_upper(case, interval):
    if not 0 <= interval[0] <= interval[1] <= 1: raise ValueError('theta domain')
    if not 0 <= case['e_interval'][0] <= case['e_interval'][1] < f32(.007): raise ValueError('eccentricity')
    jets = coordinate_jets(case,interval)
    v = [up_add(absolute(j[1]),absolute(j[2])) for j in jets]
    height = [absolute(j[0]) for j in jets]
    if v[0] > 1: raise ValueError('chart domain')
    h = add(add(add(*map(exact,case['anchor_split'])),case['anchor_residual']),jets[0][0])
    if h[0] < -10000 or h[1] > 120001: raise ValueError('normal tube')
    height[0] = absolute(h);v[0] = 2.
    xx=[0.]*5;inv=[1.]+[0.]*4;normal=[1.]+[0.]*4
    for j in range(1,5):
        xx[j]=upper_sum(up_mul(v[i],v[j-i]) for i in range(j+1))
        inv[j]=up_div(upper_sum(up_mul(up_mul(float(2*j-i),xx[i]),inv[j-i]) for i in range(1,j+1)),float(2*j))
        normal[j]=upper_sum(up_mul(v[i],inv[j-i]) for i in range(j+1))
    e=case['e_interval'][1];d=sub(ONE,(e,e));radius=[up_mul(6378137.,rsqrt(d)[1])]+[0.]*4;dd=[0.]*5
    for j in range(1,5):
        dd[j]=up_mul(e,upper_sum(up_mul(normal[i],normal[j-i]) for i in range(j+1)))
        denominator=mul((float(2*j),)*2,d)[0]
        radius[j]=up_div(upper_sum(up_mul(up_mul(float(2*j-i),dd[i]),radius[j-i]) for i in range(1,j+1)),denominator)
    surface=upper_sum(up_mul(radius[i],normal[4-i]) for i in range(5))
    offset=upper_sum(up_mul(height[i],normal[4-i]) for i in range(5))
    return up_add(up_mul(up_add(1.,e),surface),offset),jets


def dyadic(interval):
    if not (0 <= interval[0] < interval[1] <= 1): raise ValueError('range')
    for depth in range(25):
        inverse=float(2**depth)
        left=f32(interval[0]*inverse);right=f32(interval[1]*inverse)
        if left==math.floor(left) and right==math.floor(right) and f32(right-left)==1:
            return value((254-(bits(inverse)>>23))<<23),inverse
    raise ValueError('unaligned dyadic')


def midpoint(interval):
    return f32(f32(interval[0]*.5)+f32(interval[1]*.5))


def control_radius(row):
    delta=vsub(row,vexact(tuple(map(midpoint,row))))
    return upper_sum(map(absolute,delta))


def certify(case, interval, limit=1e20):
    width,inverse=dyadic(interval)
    fourth,_=fourth_upper(case,interval)
    square=up_mul(width,width)
    fit=up_mul(up_mul(fourth,up_mul(square,square)),.0625)
    dfit=up_div(up_mul(up_mul(up_mul(fourth,square),width),10.),3.)
    p0,d0=map_enclosure(case,(interval[0],)*2);p3,d2=map_enclosure(case,(interval[1],)*2)
    origin=tuple(map(midpoint,p0));p0=vsub(p0,vexact(origin));p3=vsub(p3,vexact(origin))
    third=div((width,)*2,(3.,3.))
    controls=[p0,vadd(p0,scale(d0,third)),vsub(p3,scale(d2,third)),p3]
    derivatives=[d0,vsub(vsub(scale(vsub(p3,p0),mul((3.,3.),(inverse,)*2)),d0),d2),d2]
    radius=up_add(max(map(control_radius,controls)),up_mul(3.,fit))
    if radius>limit: raise ValueError('fit budget')
    return dict(origin=origin,p=controls,d=derivatives,fit=fit,dfit=dfit,radius=radius,fourth=fourth)


def manufactured():
    for axis in [(0.,1.,0.),(.6,.8,0.),(0.,.31,-.95)]:
        for offset in [0.,.015,.3]:
            yield make_case(axis,1200.,1190.,[.125,offset,-offset/3],
                            [[60.,.0001,.00003],[-.25,-.00001,.000002],
                             [.06,.000002,-.0000003],[-.005,-.0000001,.00000002]])


def captured_cases():
    """Actual encoded curve/parameter intervals, without reserializing f32s."""
    capture=np.fromfile('artifacts/optical-height-trial-capture-readback.bin',dtype='<u4').reshape(7,13864)
    uniform=np.fromfile('artifacts/optical-height-trial-capture-uniform.bin',dtype='<u4')
    uf=uniform.view('<f4')
    certificate=np.fromfile('artifacts/optical-normal-scene-packet-certificates.bin',dtype='<u4')
    cf=certificate.view('<f4')
    for ray,record in enumerate(capture):
        for trial in range(int(record[36])):
            w=record[40+108*trial:40+108*(trial+1)];f=w.view('<f4')
            if w[96]!=2:continue
            if np.array_equal(w[8:10],uniform[2:4]):residual=cf[16:18]
            else:
                matches=[i for i in range(int(certificate[2])) if np.array_equal(w[8:10],uniform[20+4*i:22+4*i])]
                assert len(matches)==1
                offset=4*(int(certificate[5])+matches[0]);residual=cf[offset:offset+2]
            coeff=[list(map(float,f[12+4*i:15+4*i])) for i in range(4)]
            coeff[0][0]=float(f[3])
            for c in coeff[1:]:c[0]=0.
            case={'start':tuple(map(float,f[:3])),'coeff':coeff,
                  'anchor_split':tuple(map(float,f[8:10])), 'eye_split':tuple(map(float,uf[2:4])),
                  'anchor_residual':tuple(map(float,residual)), 'eye_residual':tuple(map(float,cf[16:18])),
                  'axis_interval':tuple((float(cf[8+i]),float(cf[12+i])) for i in range(3)),
                  'e_interval':(float(cf[11]),float(cf[15]))}
            exact_remaining=Q(float(f[98]))-Q(float(f[48]))-Q(float(f[97]))
            assert exact_remaining>0
            limit=f32(float(exact_remaining/4))
            if Q(limit)>exact_remaining/4:limit=value(bits(limit)-1)
            yield ray,trial,case,limit


def captured_summary():
    rows=[]
    for ray,trial,case,limit in captured_cases():
        initial_position,_=map_enclosure(case,(0.,0.))
        endpoint_floor=sum((Q(hi)-Q(lo))/2 for lo,hi in initial_position)
        nominal_near=sum(case['anchor_split'])+case['start'][0]+min(0,case['coeff'][0][0])<=167
        try:
            piece=certify(case,(0.,1.))
            radius=piece['radius'];fourth=piece['fourth'];failure=None
            # Exact retained interval endpoints give a separate rational
            # check of the sufficient scalar admission's final comparison.
            radius_lower=max(sum((Q(hi)-Q(lo))/2 for lo,hi in row) for row in piece['p'])+3*Q(piece['fit'])
            assert Q(radius)>=radius_lower
        except ValueError as exc:
            radius=None;fourth=None;failure=str(exc)
        rows.append({'capture_ray':ray,'capture_trial':trial,'quarter_remaining_limit_m':limit,
                     'full_piece_radius_m':radius,'fourth_factorial_bound_m':fourth,
                     'single_piece_sufficient':radius is not None and radius<=limit,'decline':failure,
                     'initial_map_l1_halfwidth_exact_m':str(endpoint_floor),
                     'initial_map_interval_floor_exceeds_limit':endpoint_floor>Q(limit),
                     'nominal_height_minimum_at_most_167m':nominal_near})
    assert len(rows)==495
    return {'scope':'CPU f32 transcription only; every retained accepted H curve, one full dyadic piece each. '
                    'No GPU execution, subdivision search, actual output ABI, complete contact or performance acceptance.',
            'curve_count':len(rows),'single_piece_sufficient':sum(r['single_piece_sufficient'] for r in rows),
            'initial_map_interval_floor_exceeds_limit':sum(r['initial_map_interval_floor_exceeds_limit'] for r in rows),
            'nominal_near_scene_curves':sum(r['nominal_height_minimum_at_most_167m'] for r in rows),
            'near_scene_initial_map_interval_floor_exceeds_limit':sum(r['nominal_height_minimum_at_most_167m'] and r['initial_map_interval_floor_exceeds_limit'] for r in rows),
            'cases':rows}


class CartesianProducerTests(unittest.TestCase):
    def test_positive_arithmetic_and_exact_dyadic_domain(self):
        inputs=[0.,MIN_NORMAL,TINY_BOUND,2.**-60,2.**-1,1.,value(0x3f800001),2.,2.**60,2.**100]
        for a in inputs:
            for b in inputs:
                for operation,truth in [(up_add,Q(a)+Q(b)),(up_mul,Q(a)*Q(b))]:
                    try: got=operation(a,b)
                    except ValueError: continue
                    self.assertGreaterEqual(Q(got),truth)
                if b:
                    try: got=up_div(a,b)
                    except ValueError: continue
                    self.assertGreaterEqual(Q(got),Q(a)/Q(b))
        for bad in [-1.,float('nan'),float('inf'),value(1)]:
            for op in [up_add,up_mul,up_div]:
                with self.assertRaises(ValueError): op(bad,0.)
        for depth in [0,1,3,12,24]:
            n=2**depth
            for k in sorted({0,n//2,n-1}):
                width,inverse=dyadic((k/n,(k+1)/n))
                self.assertEqual(Q(width)*Q(inverse),1)
                self.assertEqual(Q(width),Q(1,n))
        for interval in [(0.,.3),(.25,.75),(0.,2.**-25),(.5,.5),(-.5,0.)]:
            with self.assertRaises(ValueError):dyadic(interval)

    def test_jets_and_majorant_against_independent_derivatives(self):
        mp.mp.dps=75;checked=0
        for case in manufactured():
            bound,jets=fourth_upper(case,(.25,.5))
            for theta in [mp.mpf('.25'),mp.mpf('.375'),mp.mpf('.5')]:
                for component in range(3):
                    c2,c3,c4,c5=map(lambda c:mp.mpf(c[component]),case['coeff'])
                    coefficients=[mp.mpf(case['start'][component]),c2+c3,c4-c3+c5,-c4-2*c5,c5]
                    for order in range(5):
                        exact_jet=sum(coefficients[k]*math.comb(k,order)*theta**(k-order) for k in range(order,5))
                        self.assertLessEqual(mp.mpf(jets[order][component][0]),exact_jet)
                        self.assertGreaterEqual(mp.mpf(jets[order][component][1]),exact_jet)
                fourth=[mp.diff(lambda t:canonical(case,t)[i],theta,4)/24 for i in range(3)]
                self.assertGreaterEqual(mp.mpf(bound),mp.sqrt(sum(v*v for v in fourth)))
                checked+=1
        self.assertEqual(checked,27)
        case=next(manufactured());case['start']=(0.,1.,1.)
        with self.assertRaises(ValueError):fourth_upper(case,(0.,1.))
        case=next(manufactured());case['anchor_split']=(200000.,0.)
        with self.assertRaises(ValueError):fourth_upper(case,(0.,1.))

    def test_endpoint_fit_removal_and_original_theta_derivatives(self):
        mp.mp.dps=75;checked=0
        for case in manufactured():
            for interval in [(0.,1.),(.25,.5)]:
                piece=certify(case,interval)
                for x in map(mp.mpf,['0','.25','.5','.75','1']):
                    theta=mp.mpf(interval[0])+x*(interval[1]-interval[0])
                    wp=[(1-x)**3,3*x*(1-x)**2,3*x*x*(1-x),x**3]
                    wd=[(1-x)**2,2*x*(1-x),x*x]
                    for i in range(3):
                        truth=canonical(case,theta)[i];speed=mp.diff(lambda t:canonical(case,t)[i],theta)
                        rp=mp.mpf(piece['fit']) if 0<x<1 else 0
                        rd=mp.mpf(piece['dfit']) if 0<x<1 else 0
                        p=[mp.mpf(piece['origin'][i])+sum(w*mp.mpf(piece['p'][k][i][side]) for k,w in enumerate(wp)) for side in [0,1]]
                        d=[sum(w*mp.mpf(piece['d'][k][i][side]) for k,w in enumerate(wd)) for side in [0,1]]
                        self.assertLessEqual(p[0]-rp,truth);self.assertGreaterEqual(p[1]+rp,truth)
                        self.assertLessEqual(d[0]-rd,speed);self.assertGreaterEqual(d[1]+rd,speed)
                    checked+=1
                # Convex weights bound the local encoded-control uncertainty;
                # even an exact endpoint must retain its interval radius.
                exact_radius=max(sum((Q(hi)-Q(lo))/2 for lo,hi in row) for row in piece['p'])+3*Q(piece['fit'])
                self.assertGreaterEqual(Q(piece['radius']),exact_radius)
                with self.assertRaises(ValueError):certify(case,interval,0.)
        self.assertEqual(checked,90)

    def test_source_contract(self):
        source=Path('scripts/optical_cartesian_producer.wgsl').read_text()
        for text in ['full_curve.theta_end=1.0','piece.origin_lo=vec3(0.0)',
                     'piece.d0=left.velocity;piece.d2=right.velocity;',
                     'total<=fit_limit','depth<=24u','nc_interval_product_safe(a,b)',
                     'nc_interval_quotient_safe(a,b)','nc_interval_up_steps(a/b,8u)']:
            self.assertIn(text,source)

    def test_all_retained_curves_single_piece_finite_budget_screen(self):
        report=captured_summary()
        self.assertEqual(report['curve_count'],495)
        self.assertTrue(all(r['decline'] is None for r in report['cases']))


if __name__=='__main__':unittest.main()
