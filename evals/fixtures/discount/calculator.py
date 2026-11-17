def discounted_total(price, percent):
    """Return the amount payable after a percentage discount."""
    if price < 0 or not 0 <= percent <= 100:
        raise ValueError("invalid input")
    return price * percent / 100
