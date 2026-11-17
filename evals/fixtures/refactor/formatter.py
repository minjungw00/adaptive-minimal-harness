def label(first, last):
    if first and last:
        return first.strip() + " " + last.strip()
    if first:
        return first.strip()
    if last:
        return last.strip()
    return ""
