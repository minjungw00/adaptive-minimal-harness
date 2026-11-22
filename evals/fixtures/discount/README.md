# Checkout calculation
The checkout total is wrong for percentage discounts. `discounted_total(price, percent)` is used by existing callers. Inputs and ValueError behavior are public expectations. Run `python -m unittest -v` to inspect current behavior.
