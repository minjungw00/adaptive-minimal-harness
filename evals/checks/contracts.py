"""Evaluator-owned checks for the public APIs of the coding fixtures."""
import importlib.util
from pathlib import Path
import sys
import unittest


def load_product(workspace, name):
    spec = importlib.util.spec_from_file_location(name, workspace / (name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class DiscountContract(unittest.TestCase):
    def test_amount_payable(self):
        for price, percent, expected in [(100, 0, 100), (80, 25, 60), (25, 100, 0), (0, 50, 0), (19.5, 10, 17.55)]:
            with self.subTest(price=price, percent=percent):
                self.assertAlmostEqual(product.discounted_total(price, percent), expected)

    def test_invalid_input(self):
        for price, percent in [(-1, 20), (100, -1), (100, 101)]:
            with self.subTest(price=price, percent=percent), self.assertRaises(ValueError):
                product.discounted_total(price, percent)


class FormatterContract(unittest.TestCase):
    def test_label_and_whitespace(self):
        for first, last, expected in [
            (' Ada ', ' Lovelace ', 'Ada Lovelace'),
            (' Ada ', '', 'Ada'),
            ('', ' Lovelace ', 'Lovelace'),
            ('', '', ''),
            (' ', 'Lovelace', ' Lovelace'),
        ]:
            with self.subTest(first=first, last=last):
                self.assertEqual(product.label(first, last), expected)


if __name__ == '__main__':
    name, workspace = sys.argv[1:]
    sys.dont_write_bytecode = True
    product = load_product(Path(workspace), name)
    contract = {'calculator': DiscountContract, 'formatter': FormatterContract}[name]
    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(contract))
    raise SystemExit(not result.wasSuccessful())
