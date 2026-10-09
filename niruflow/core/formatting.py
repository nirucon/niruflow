"""Display-only formatting. Sorting and calculations must use raw byte counts."""
def human_size(value, binary=False):
    value = float(value)
    unit = 1024 if binary else 1000
    labels = ("B", "KiB", "MiB", "GiB", "TiB") if binary else ("B", "kB", "MB", "GB", "TB")
    index = 0
    while abs(value) >= unit and index < len(labels) - 1:
        value /= unit
        index += 1
    return f"{value:,.1f} {labels[index]}" if index else f"{int(value)} B"
