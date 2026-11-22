import unittest
from formatter import label
class LabelTests(unittest.TestCase):
    def test_both(self): self.assertEqual(label(" Ada ", " Lovelace "), "Ada Lovelace")
    def test_first(self): self.assertEqual(label(" Ada ", ""), "Ada")
    def test_last(self): self.assertEqual(label("", " Lovelace "), "Lovelace")
    def test_empty(self): self.assertEqual(label("", ""), "")
if __name__ == "__main__": unittest.main()
