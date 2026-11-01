"""Independent canonical F(anchor) minus rotated source-vertex residual audit."""
import argparse
import json
import struct
from pathlib import Path
import mpmath as mp
from review_optical_normal_ideal_frame import number


def single(bits):
    return mp.mpf(struct.unpack('<f',struct.pack('<I',bits))[0])


def enclosed(lo,hi,value):
    assert single(lo)<=value<=single(hi), (lo,hi,str(value))


def review(report):
    maximum_width=mp.mpf(0)
    maximum_residual=mp.mpf(0)
    with mp.workdps(90):
        a,e=number(report['a_bits']),number(report['e2_bits'])
        assert len(report['cases'])==80
        for case in report['cases']:
            east,up,north=[list(map(number,case[key])) for key in ('raw_east_bits','raw_up_bits','raw_north_bits')]
            lon=mp.atan2(-east[0],east[1]);lat=mp.atan2(up[2],north[2])
            sl,cl,s,c=mp.sin(lon),mp.cos(lon),mp.sin(lat),mp.cos(lat)
            columns=[[-sl,cl,0],[c*cl,c*sl,s],[s*cl,s*sl,-c]]
            axis=[mp.mpf(0),s,-c]
            chart=list(map(number,case['canonical_chart_bits']))
            height=number(case['canonical_height_bits'])
            normal=[chart[0],mp.mpf(1),-chart[1]]
            norm=mp.sqrt(sum(x*x for x in normal));normal=[x/norm for x in normal]
            z=sum(x*y for x,y in zip(normal,axis));radius=a/mp.sqrt(1-e*z*z)
            # Direct full geocentric map, followed by subtraction of the
            # independently rotated source; no approximate inverse is truth.
            mapped=[radius*(n-e*z*k)+height*n for n,k in zip(normal,axis)]
            vertex=list(map(number,case['vertex_bits']))
            source=[sum(x*y for x,y in zip(column,vertex)) for column in columns]
            for j,value in enumerate(x-y for x,y in zip(mapped,source)):
                lo,hi=case['mapped_minus_vertex_lower_bits'][j],case['mapped_minus_vertex_upper_bits'][j]
                enclosed(lo,hi,value)
                maximum_width=max(maximum_width,single(hi)-single(lo));maximum_residual=max(maximum_residual,abs(value))
            for j,value in enumerate(chart):
                residual=value-single(case['chart_hi_bits'][j])-single(case['chart_lo_bits'][j])
                enclosed(*case['chart_residual_bits'][j],residual)
            residual=height-sum(map(single,case['height_bits']))
            enclosed(*case['height_residual_bits'],residual)
    return dict(cases=80,passed=True,maximum_component_width_m=float(maximum_width),maximum_actual_component_residual_m=float(maximum_residual),
                scope='Actual Rust-exported f32/f64 bits, direct 90-digit canonical map and orthonormal source rotation. Finite source-anchor host evidence; no GPU/contact or global inverse-accuracy claim.')


if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('input',type=Path);parser.add_argument('--output',type=Path);args=parser.parse_args()
    result=review(json.loads(args.input.read_text(encoding='utf-8-sig')));output=json.dumps(result,indent=2)
    if args.output:
        assert not args.output.exists(),'retain earlier evidence';args.output.write_text(output+'\n',encoding='utf-8')
    print(output)
