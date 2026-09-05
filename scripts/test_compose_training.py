import json
from pathlib import Path
import re
import unittest

from audit_human_catalog import shape
from compose_training import source


class CompositionTests(unittest.TestCase):
    def test_separation_determinism_and_clock_domains(self):
        catalog = Path(__file__).resolve().parents[1] / 'evals/human-v2.json'
        held = json.loads(catalog.read_text())
        excluded = {shape(row['policy']) for row in held}
        cases, counts = source(23, 40, 20, excluded)
        self.assertEqual((cases, counts), source(23, 40, 20, excluded))
        self.assertEqual(len(cases), 60)
        shapes = {shape(c['policy']) for c in cases}
        self.assertEqual(len(shapes), len(cases))
        self.assertFalse(shapes & excluded)
        for case in cases:
            self.assertTrue(case['group'].startswith('composed-train-'))
            self.assertLessEqual(len(case['keys']), 8)
            relative = [int(n) for n in re.findall(r'older\((\d+)\)', case['policy'])]
            absolute = [int(n) for n in re.findall(r'after\((\d+)\)', case['policy'])]
            self.assertLessEqual(len({bool(n & (1 << 22)) for n in relative}), 1)
            self.assertLessEqual(len({n >= 500_000_000 for n in absolute}), 1)
            for digest in ('$sha256', '$hash160'):
                if digest in case['policy']:
                    self.assertIn(digest, case['prompt'])

    def test_shape_retains_thresholds_and_clock_units(self):
        self.assertEqual(shape('and(pk(A),and(pk(B),pk(C)))'), shape('thresh(3,pk(C),pk(B),pk(A))'))
        self.assertNotEqual(shape('thresh(2,pk(A),pk(B),pk(C))'), shape('thresh(3,pk(A),pk(B),pk(C))'))
        self.assertNotEqual(shape('older(4194306)'), shape('older(1024)'))
        self.assertNotEqual(shape('after(1800000000)'), shape('after(499999999)'))
        self.assertNotEqual(shape('pk(A)'), shape('and(pk(A),pk(A))'))


if __name__ == '__main__':
    unittest.main()
