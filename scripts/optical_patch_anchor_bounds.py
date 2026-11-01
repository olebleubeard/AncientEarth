"""Exact-rational host bounds for a chosen canonical source anchor.

This unused CPU helper assumes `columns` enclose the SAME ideal orthonormal
eye frame as the curve map. It neither computes nor certifies an inverse.
Chosen chart/height inputs are exact binary64 values; their entire discrepancy
from the supplied source vertex remains in the mandatory residual interval.
"""
from dataclasses import dataclass
from fractions import Fraction as Q
from functools import lru_cache
from math import isfinite,isqrt
import struct


def rational(value):
    if isinstance(value,Q):return value
    if isinstance(value,int):return Q(value)
    if not isinstance(value,float) or not isfinite(value):raise ValueError('Expected finite exact numeric input')
    return Q(value)


@dataclass(frozen=True)
class Interval:
    lo: Q
    hi: Q

    def __post_init__(self):
        object.__setattr__(self,'lo',rational(self.lo));object.__setattr__(self,'hi',rational(self.hi))
        if self.lo>self.hi:raise ValueError('Reversed interval')

    @staticmethod
    def point(x):return Interval(rational(x),rational(x))

    def __add__(self,other):
        other=interval(other);return Interval(self.lo+other.lo,self.hi+other.hi)
    __radd__=__add__
    def __neg__(self):return Interval(-self.hi,-self.lo)
    def __sub__(self,other):return self+-interval(other)
    def __rsub__(self,other):return interval(other)+-self
    def __mul__(self,other):
        other=interval(other);products=[a*b for a in (self.lo,self.hi) for b in (other.lo,other.hi)]
        return Interval(min(products),max(products))
    __rmul__=__mul__
    def __truediv__(self,other):
        other=interval(other)
        if other.lo<=0<=other.hi:raise ValueError('Division interval contains zero')
        return self*Interval(1/other.hi,1/other.lo)
    def square(self):
        return Interval(0 if self.lo<=0<=self.hi else min(self.lo**2,self.hi**2),max(self.lo**2,self.hi**2))
    def sqrt(self,bits=96):
        if self.lo<0:raise ValueError('Negative square-root interval')
        lower=sqrt_enclosure(self.lo,bits);upper=sqrt_enclosure(self.hi,bits)
        return Interval(lower.lo,upper.hi)
    def contains(self,value):return self.lo<=rational(value)<=self.hi
    def exact_strings(self):return [str(self.lo),str(self.hi)]


def interval(value):
    if isinstance(value,Interval):return value
    if isinstance(value,(tuple,list)):
        if len(value)!=2:raise ValueError('Expected two interval endpoints')
        return Interval(*value)
    return Interval.point(value)


def sqrt_enclosure(value,bits=96):
    """floor(sqrt(value)*2**bits) by integer arithmetic, never libm."""
    value=rational(value)
    if value<0 or not isinstance(bits,int) or not 32<=bits<=512:raise ValueError('Invalid rational sqrt domain')
    scale=1<<bits
    lower_integer=isqrt((value.numerator<<(2*bits))//value.denominator)
    lower=Q(lower_integer,scale)
    return Interval(lower,lower if lower*lower==value else Q(lower_integer+1,scale))


@lru_cache(maxsize=8192)
def positive_float(bits):return struct.unpack('<f',struct.pack('<I',bits))[0]


@lru_cache(maxsize=8192)
def positive_rational(bits):return Q(positive_float(bits))


def floor_positive_bits(value):
    value=rational(value)
    if value<0 or value>positive_rational(0x7f7fffff):raise ValueError('Outside finite f32 domain')
    lo,hi=0,0x7f7fffff
    while lo<hi:
        middle=(lo+hi+1)//2
        if positive_rational(middle)<=value:lo=middle
        else:hi=middle-1
    return lo


def nearest_f32(value):
    """Exact ties-to-even selection, independent of double rounding/libm."""
    value=rational(value);sign=-1 if value<0 else 1;value=abs(value)
    lower=floor_positive_bits(value);a=positive_rational(lower)
    if a==value:return sign*positive_float(lower)
    upper=lower+1;b=positive_rational(upper)
    choice=lower if value-a<b-value or (value-a==b-value and lower%2==0) else upper
    return sign*positive_float(choice)


def pack(value):
    """Outward f32 endpoints, retaining every tiny signed value under FTZ."""
    value=interval(value)
    def endpoint(x,up):
        negative=x<0;absolute=abs(x);lower=floor_positive_bits(absolute)
        upper=lower+(positive_rational(lower)!=absolute)
        selected=(lower if up else upper) if negative else (upper if up else lower)
        result=positive_float(selected)*(-1 if negative else 1)
        if result and abs(result)<positive_float(0x00800000):
            result=(-positive_float(0x00800000) if negative else 0.) if not up else (0. if negative else positive_float(0x00800000))
        # A rational below the least subnormal can round toward zero.
        if result==0 and ((x<0 and not up) or (x>0 and up)):
            result=(-1 if x<0 else 1)*positive_float(0x00800000)
        return result
    result=(endpoint(value.lo,False),endpoint(value.hi,True))
    assert Q(result[0])<=value.lo<=value.hi<=Q(result[1])
    return result


def split(value):
    value=rational(value)
    def operand(x):
        rounded=nearest_f32(x)
        return 0. if 0<abs(rounded)<positive_float(0x00800000) else rounded
    hi=operand(value);lo=operand(value-Q(hi))
    residual=Interval.point(value-Q(hi)-Q(lo))
    return ((hi,lo),pack(residual))


def dot(a,b):
    if len(a)!=len(b):raise ValueError('Mismatched vector length')
    return sum((interval(x)*interval(y) for x,y in zip(a,b,strict=True)),Interval.point(0))


def prepare_columns(columns):
    if len(columns)!=3 or any(len(c)!=3 for c in columns):raise ValueError('Expected three frame columns')
    result=[[interval(v) for v in c] for c in columns]
    if any(max(abs(v.lo),abs(v.hi))>Q(1000001,1000000) for c in result for v in c):
        raise ValueError('Frame endpoint outside unit-frame admission')
    # Necessary consistency checks; these do not manufacture an orthonormal
    # frame or replace the caller's upstream ideal-frame enclosure theorem.
    for i in range(3):
        for j in range(3):
            if not dot(result[i],result[j]).contains(int(i==j)):
                raise ValueError('Frame intervals cannot enclose orthonormal columns')
    return result


def canonical_point(columns,e2,chart,height,a=6378137.0,sqrt_bits=96):
    """Return absolute F(q,H), expressed in the ideal eye basis, as intervals."""
    columns=prepare_columns(columns);e2=rational(e2);a=rational(a)
    if not 0<=e2<=Q(7,1000) or a!=6378137:raise ValueError('Unsupported canonical ellipsoid')
    if len(chart)!=2:raise ValueError('Expected two chart coordinates')
    q0,q1=map(rational,chart);height=rational(height)
    if q0*q0+q1*q1>1 or not -10000<=height<=120001:raise ValueError('Anchor outside admitted normal chart/tube')
    v=[Interval.point(q0),Interval.point(1),Interval.point(-q1)]
    length=(Interval.point(1)+q0*q0+q1*q1).sqrt(sqrt_bits)
    n=[x/length for x in v];axis=[c[2] for c in columns]
    z=dot(n,axis);denominator=Interval.point(1)-e2*z.square()
    if denominator.lo<=Q(99,100):raise ValueError('Ellipsoid denominator outside admitted domain')
    radius=Interval.point(a)/denominator.sqrt(sqrt_bits)
    return [radius*(x-e2*z*k)+height*x for x,k in zip(n,axis,strict=True)]


def make_anchor(columns,e2,chart,height,vertex,a=6378137.0,sqrt_bits=96):
    """Return the exact `anchor_enclosure` dictionary and rational provenance."""
    # The API's chosen seeds and source point are explicitly binary64 inputs.
    chart=tuple(chart);vertex=tuple(vertex)
    if len(chart)!=2 or len(vertex)!=3:raise ValueError('Invalid anchor dimensions')
    for x in (*chart,height,*vertex,e2,a):
        if not isinstance(x,(float,int)) or not isfinite(x):raise ValueError('Anchor seeds must be finite binary64 numbers')
    columns=prepare_columns(columns)
    mapped=canonical_point(columns,e2,chart,height,a,sqrt_bits)
    projected=[dot(c,vertex) for c in columns]
    residual=[x-y for x,y in zip(mapped,projected,strict=True)]
    return {'a':float(chart[0]),'b':float(chart[1]),'h':float(height),
            'a_split':split(chart[0]),'b_split':split(chart[1]),'h_split':split(height),
            'residual_interval':tuple(pack(x) for x in residual),
            'proof':{'scope':'Chosen exact binary64 anchor; full F(anchor)-ideal_frame_transpose*source_vertex residual. Caller supplies a certified enclosure of the same ideal orthonormal frame. No inverse, GPU arithmetic, ODE, or contact admission claim.',
                     'sqrt_fraction_bits':sqrt_bits,'chart_exact':list(map(lambda x:str(rational(x)),chart)),
                     'height_exact':str(rational(height)),'vertex_exact':list(map(lambda x:str(rational(x)),vertex)),
                     'e2_exact':str(rational(e2)),
                     'columns_exact':[[x.exact_strings() for x in c] for c in columns],
                     'mapped_exact_intervals':[x.exact_strings() for x in mapped],
                     'projected_source_exact_intervals':[x.exact_strings() for x in projected],
                     'residual_exact_intervals':[x.exact_strings() for x in residual]}}
