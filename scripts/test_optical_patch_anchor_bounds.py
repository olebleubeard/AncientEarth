"""Finite host-anchor checks with independent direct geocentric references."""
from fractions import Fraction as Q
import json
import math
from pathlib import Path
import struct
import unittest

import mpmath as mp
import numpy as np

from optical_patch_anchor_bounds import Interval,canonical_point,make_anchor,pack,split,sqrt_enclosure,nearest_f32


def scalar(q):
    q=Q(q);return mp.mpf(q.numerator)/q.denominator


def f64(bits):return struct.unpack('<d',struct.pack('<Q',bits))[0]


def actual_frame():
    packet=json.loads(Path('artifacts/optical-immutable-patch-second.json').read_text())
    seeds=json.loads(Path('artifacts/optical-immutable-patch-second-eye-inputs.json').read_text())
    ex,ey,uz,nz=map(f64,seeds['eyes'][0]['raw_east_xy_upz_northz_bits'])
    longitude=mp.atan2(-mp.mpf(ex),mp.mpf(ey));latitude=mp.atan2(mp.mpf(uz),mp.mpf(nz))
    columns=[[-mp.sin(longitude),mp.cos(longitude),mp.mpf(0)],
             [mp.cos(latitude)*mp.cos(longitude),mp.cos(latitude)*mp.sin(longitude),mp.sin(latitude)],
             [mp.sin(latitude)*mp.cos(longitude),mp.sin(latitude)*mp.sin(longitude),-mp.cos(latitude)]]
    return packet['eyes'][0]['ideal_columns_interval'],columns,f64(seeds['eccentricity_bits'])


def direct(columns,e2,chart,height):
    # Rotate the normalized chart direction into ECEF, apply geocentric
    # WGS84 Cartesian equations there, then project back into the frame.
    e2=mp.mpf(e2)
    v=[mp.mpf(chart[0]),mp.mpf(1),-mp.mpf(chart[1])];length=mp.sqrt(sum(x*x for x in v))
    n=[sum(columns[j][i]*v[j]/length for j in range(3)) for i in range(3)]
    r=mp.mpf(6378137)/mp.sqrt(1-mp.mpf(e2)*n[2]**2)
    p=[(r+height)*n[0],(r+height)*n[1],(r*(1-e2)+height)*n[2]]
    return [sum(c[i]*p[i] for i in range(3)) for c in columns]


class PatchAnchorBounds(unittest.TestCase):
    def test_rational_sqrt_and_f32_signed_ftz(self):
        values=[Q(0),Q(1),Q(2),Q(9,16),Q(1,10),Q(1,2**250),Q(2**100+1,3)]
        for q in values:
            result=sqrt_enclosure(q)
            self.assertLessEqual(result.lo**2,q);self.assertGreaterEqual(result.hi**2,q)
            self.assertLessEqual(result.hi-result.lo,Q(1,2**96))
        for exponent in [-200,-149,-126,-30,0,30]:
            for mantissa in [Q(1),Q(3,2),Q(2)-Q(1,2**24)]:
                for sign in [-1,1]:
                    q=sign*mantissa*Q(2)**exponent
                    lo,hi=pack(Interval.point(q));self.assertLessEqual(Q(lo),q);self.assertGreaterEqual(Q(hi),q)
                    self.assertTrue(all(x==0 or abs(x)>=2**-126 for x in [lo,hi]))
                    (a,b),(c,d)=split(q)
                    self.assertLessEqual(Q(a)+Q(b)+Q(c),q);self.assertGreaterEqual(Q(a)+Q(b)+Q(d),q)
                    self.assertTrue(all(x==0 or abs(x)>=2**-126 for x in [a,b]))
        for bits in [0x3f800000,0x3f800001,0x40000000,0x00800000]:
            a=struct.unpack('<f',struct.pack('<I',bits))[0];b=struct.unpack('<f',struct.pack('<I',bits+1))[0]
            middle=(Q(a)+Q(b))/2
            self.assertEqual(nearest_f32(middle),a if bits%2==0 else b)
            self.assertEqual(nearest_f32(-middle),-(a if bits%2==0 else b))
        for operation in [lambda:sqrt_enclosure(-1),lambda:Interval(2,1),lambda:Interval.point(1)/Interval(-1,1),lambda:pack(Interval.point(Q(2)**200))]:
            with self.assertRaises(ValueError):operation()

    def test_actual_frame_direct_ecef_residuals(self):
        with mp.workdps(100):
            bounds,columns,e2=actual_frame()
            faces=np.memmap('artifacts/optical-positive-contact-packet-first-faces.bin',dtype='<f8',mode='r').reshape(-1,3,3)
            checks=0
            for chart in [(0.,0.),(1e-6,-2e-6),(.01,-.02),(.35,-.4)]:
                for height in [-9999.875,-26.00552287325263,0.,100000.123456789]:
                    for face in [225388,618567,618572]:
                        vertex=tuple(map(float,faces[face,0]));result=make_anchor(bounds,e2,chart,height,vertex)
                        expected=direct(columns,e2,chart,mp.mpf(height))
                        residual=[p-sum(c[i]*mp.mpf(vertex[i]) for i in range(3)) for p,c in zip(expected,columns,strict=True)]
                        for actual,target in zip(canonical_point(bounds,e2,chart,height),expected,strict=True):
                            self.assertLessEqual(scalar(actual.lo),target);self.assertGreaterEqual(scalar(actual.hi),target)
                        for actual,target in zip(result['residual_interval'],residual,strict=True):
                            self.assertLessEqual(mp.mpf(actual[0]),target);self.assertGreaterEqual(mp.mpf(actual[1]),target)
                        for key,q in [('a_split',chart[0]),('b_split',chart[1]),('h_split',height)]:
                            (hi,lo),(a,b)=result[key]
                            self.assertLessEqual(Q(hi)+Q(lo)+Q(a),Q(q));self.assertGreaterEqual(Q(hi)+Q(lo)+Q(b),Q(q))
                        checks+=1
            self.assertEqual(checks,48)

    def test_interface_and_domain_failures(self):
        identity=[[(1.,1.),(0.,0.),(0.,0.)],[(0.,0.),(1.,1.),(0.,0.)],[(0.,0.),(0.,0.),(1.,1.)]]
        exact=make_anchor(identity,0.,(0.,0.),16.,(0.,6378153.,0.))
        self.assertEqual(exact['residual_interval'],((0.,0.),(0.,0.),(0.,0.)))
        for kwargs in [dict(chart=(1.,1.)),dict(height=-10001.),dict(e2=.1),dict(vertex=(math.nan,0.,0.)),dict(columns=[[(0.,0.)]*3]*3)]:
            args=dict(columns=identity,e2=0.,chart=(0.,0.),height=0.,vertex=(0.,6378137.,0.));args.update(kwargs)
            with self.assertRaises(ValueError):make_anchor(**args)
        # Use the existing shader-transcription consumer on a simple affine
        # curve. Its result must include the independently known sphere line.
        from test_optical_normal_map_anchored import anchor_enclosure
        from test_optical_normal_map_interval import make_case,exact as fexact
        case=make_case((0.,0.,1.),10.,0.,(0.,0.,0.),[[2.,0.,0.],[0.,0.,0.],[0.,0.,0.],[0.,0.,0.]])
        # make_case uses WGS84; the equator is identical to the sphere here.
        anchor=make_anchor(identity,float(case['e']),(0.,0.),11.,(0.,6378148.,0.))
        for theta,target in [(0.,-1.),(.5,0.),(1.,1.)]:
            p,v=anchor_enclosure(case,anchor,fexact(theta))
            for bounds,value in zip(p,[(0.),target,0.],strict=True):
                self.assertLessEqual(bounds[0],value);self.assertGreaterEqual(bounds[1],value)
            self.assertLessEqual(v[1][0],2.);self.assertGreaterEqual(v[1][1],2.)


if __name__=='__main__':unittest.main()
