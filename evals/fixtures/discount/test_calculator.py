import unittest
from calculator import discounted_total

class CalculatorTests(unittest.TestCase):
    def test_no_discount(self): self.assertEqual(discounted_total(100, 0), 100)
    def test_partial_discount(self): self.assertEqual(discounted_total(100, 20), 80)
    def test_full_discount(self): self.assertEqual(discounted_total(100, 100), 0)
    def test_invalid(self):
        with self.assertRaises(ValueError): discounted_total(-1, 20)
if __name__ == "__main__": unittest.main()
