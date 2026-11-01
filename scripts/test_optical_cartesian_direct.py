"""Bounded independent source/ABI and malformed-payload checks; no GPU run."""
import json
from pathlib import Path
import re
import struct
import unittest

import optical_cartesian_trial_reference as reference
import review_optical_cartesian_direct as review
from test_optical_cartesian_trial_reference import body


ROOT = Path(__file__).resolve().parents[1]
OLD = ROOT/'artifacts/optical-cartesian-trial-validation-second.wgsl'
NEW = ROOT/'artifacts/optical-cartesian-direct-validation-second.wgsl'


class DirectReview(unittest.TestCase):
    def test_all_retained_functions_and_masks(self):
        old, new = OLD.read_text(), NEW.read_text()
        names = lambda s: set(re.findall(r'\bfn (\w+)\(', s))
        removed = {'ct_hash_core_32', 'ct_hash_core_64', 'ct_hash_value', 'ct_hash',
                   'trial_detail', 'cost_reference', 'cost_promoted'}
        self.assertEqual(names(new), names(old)-removed |
                         {'cd_header', 'direct_reference', 'direct_promoted'})
        for name in names(old)-removed:
            actual = body(new, name)
            if name in ('ct_stage_32', 'ct_stage_64'):
                bits = name[-2:]
                insertion = f'    cd_stage_mask_{bits}=cd_stage_mask_{bits}|(1u<<(base/14u));\n'
                self.assertEqual(actual.count(insertion), 1)
                actual = actual.replace(insertion, '', 1)
            self.assertEqual(actual, body(old, name), name)
        for bits, name, call in [('32', 'direct_reference', 'dopri_delta_detail'),
                                 ('64', 'direct_promoted', 'ct64_dopri_delta_detail')]:
            entry = body(new, name)
            self.assertLess(entry.index(f'cd_stage_mask_{bits}=0u'), entry.index('let c='+call+'('))
            self.assertIn(f'let values=ct_core_{bits}(s,h,c);', entry)
            self.assertIn('let output_base=384u*(ct_policy.y*id.x+k);', entry)
            self.assertIn('j<80u', entry); self.assertIn('j<108u', entry)
            self.assertIn('output_base+8u+2u*j', entry)
            self.assertIn('output_base+168u+2u*j', entry)
            self.assertGreater(entry.index('cd_header('), entry.index('ct_capture_'+bits+'[j]'))
        header = body(new, 'cd_header')
        self.assertTrue(header.rstrip().endswith('ct_output[base]=ct_policy.w;\n}'))
        self.assertEqual(NEW.read_text().count('@compute'), 2)

    def test_exact_all_record_and_word_coverage(self):
        records = []
        counts = [0]*11
        offsets = list(range(8))
        offsets += [8+2*j+k for j in range(80) for k in (0, 1)]
        offsets += [168+2*j+k for j in range(108) for k in (0, 1)]
        self.assertEqual(sorted(offsets), list(range(384)))
        self.assertEqual(len(set(offsets)), 384)
        for invocation in range(2816):
            within = []
            for cycle in range(8):
                record = 8*invocation+cycle
                self.assertEqual(divmod(record, 8), (invocation, cycle))
                identity = (invocation+17*cycle) % 11
                counts[identity] += 1; within.append(identity); records.append(record)
            self.assertEqual(len(set(within)), 8)
        self.assertEqual(records, list(range(22528)))
        self.assertEqual(counts, [2048]*11)
        self.assertEqual(22528*384*4*16, 553648128)
        self.assertEqual(sum(1 << (base//14) for base in range(0, 98, 14)), 127)

    def test_actual_payload_precision_and_failure_modes(self):
        report = json.loads((ROOT/'artifacts/optical-cartesian-direct-validation-second-validation.json').read_text())
        packed = report['fixture']['mapping'][3]['input_words']
        values = list(map(float, reference.reference(packed)))
        payload = struct.pack('<188d', *values)
        self.assertTrue(review.comparison(packed, payload, 64)['passed_arithmetic_gate'])
        original = [struct.unpack('<f', struct.pack('<f', x))[0] for x in values]
        original_result = review.comparison(packed, struct.pack('<188d', *original), 32)
        self.assertTrue(original_result['complete_finite_valid_trial'])
        self.assertFalse(original_result['passed_arithmetic_gate'])
        changed = values.copy(); changed[178] += 2*float(reference.BOUND)
        self.assertFalse(review.comparison(packed, struct.pack('<188d', *changed), 64)['passed_arithmetic_gate'])
        changed = values.copy(); changed[90] = float('nan')
        refusal = review.comparison(packed, struct.pack('<188d', *changed), 64)
        self.assertFalse(refusal['complete_finite_valid_trial'])
        self.assertEqual(refusal['nonfinite_scalar_ids'], [90])
        changed = values.copy(); changed[185] = 0.
        self.assertFalse(review.comparison(packed, struct.pack('<188d', *changed), 64)['complete_finite_valid_trial'])


if __name__ == '__main__':
    unittest.main()
