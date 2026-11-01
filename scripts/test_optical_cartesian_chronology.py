"""Exact-rational sufficient chronology fixtures; no GPU rounding simulation."""
from fractions import Fraction as F
from pathlib import Path
import unittest


def quotient(a,b):
    if b[0]<=0<=b[1]:return None
    choices=[x/y for x in a for y in b]
    return min(choices),max(choices)


def exclusion(start,end,before,derivative,best):
    if not (0<=start<end<=1 and start<=best[0]<=best[1]<=end):return False,None
    correction=quotient(before,derivative)
    if correction is None:return False,None
    root=(max(start,start-correction[1]),min(end,start-correction[0]))
    if root[0]>root[1]:return False,root
    return root[0]>best[1],root


class ChronologyTests(unittest.TestCase):
    def test_linear_exact_tube_earliest_root_both_signs_and_overlap(self):
        count=0
        for start,end in [(F(0),F(1)),(F(1,4),F(3,4))]:
            length=end-start
            for speed in [F(-7),F(-1,8),F(1,8),F(7)]:
                for fraction in [F(1,4),F(1,2),F(3,4)]:
                    root=start+fraction*length
                    for relative_tube in [F(0),F(1,64),F(1,8)]:
                        tube=abs(speed)*length*relative_tube
                        before=speed*(start-root)
                        exact=(max(start,root-tube/abs(speed)),min(end,root+tube/abs(speed)))
                        for upper in [start,start+length/4,start+length/2,end]:
                            skip,bound=exclusion(start,end,(before-tube,before+tube),(speed,speed),(start,upper))
                            self.assertEqual(bound,exact)
                            self.assertEqual(skip,exact[0]>upper)
                            count+=1
        self.assertEqual(count,288)

    def test_quadratic_mean_value_encloses_every_possible_sampled_tube_root(self):
        checked=0;skipped=0
        for start,end in [(F(0),F(1)),(F(1,4),F(3,4))]:
            length=end-start
            for slope in [F(-4),F(-2),F(2),F(4)]:
                for bend in [F(-1,4),F(0),F(1,4)]:
                    for fraction in [F(1,4),F(3,4)]:
                        root=start+fraction*length
                        f=lambda t:slope*(t-root)+bend*(t-root)**2
                        first=slope+2*bend*(start-root)
                        acceleration=2*abs(bend)
                        derivative=(first-acceleration*length,first+acceleration*length)
                        tube=F(1,32)
                        before=(f(start)-tube,f(start)+tube)
                        best=(start,start+length/3)
                        skip,bound=exclusion(start,end,before,derivative,best)
                        self.assertIsNotNone(bound)
                        skipped+=skip
                        for index in range(257):
                            theta=start+length*F(index,256)
                            # At every such point, e(theta)=-f(theta) is an
                            # admissible uniform-tube perturbation yielding
                            # a root. No smoothness or derivative of e used.
                            if abs(f(theta))<=tube:
                                self.assertLessEqual(bound[0],theta)
                                self.assertGreaterEqual(bound[1],theta)
                                if skip:self.assertGreater(theta,best[1])
                                checked+=1
        self.assertGreater(checked,100)
        self.assertGreater(skipped,0)

    def test_ambiguous_derivative_empty_range_and_exact_touch_never_skip(self):
        for derivative in [(F(-1),F(1)),(F(0),F(1)),(F(-1),F(0))]:
            self.assertFalse(exclusion(F(0),F(1),(F(-1),F(0)),derivative,(F(0),F(1,4)))[0])
        # Exact contact at the current upper bound must retain original work.
        self.assertFalse(exclusion(F(0),F(1),(F(-1,4),F(-1,4)),(F(1),F(1)),(F(0),F(1,4)))[0])
        # Even an empty crossing enclosure conservatively defers in this
        # particular helper; it is not a general plane-miss certificate.
        self.assertFalse(exclusion(F(0),F(1),(F(1),F(2)),(F(1),F(1)),(F(0),F(1,4)))[0])
        # Missing monotonicity of path is forbidden by the separate host
        # admission, rather than inferred from the plane derivative sign.
        source=Path('scripts/optical_cartesian_chronology.wgsl').read_text()
        self.assertIn('nc_face_point(frame,context.start,curve.numerical_error)',source)
        self.assertIn('nc_interval_sign(derivative)==0',source)
        self.assertIn('return clipped.x>best.upper;',source)
        adapter=Path('scripts/optical_positive_chronology_fixture.rs').read_text()
        self.assertIn('path_scale.is_finite() && path_scale > 0.0',adapter)


if __name__=='__main__':unittest.main()
