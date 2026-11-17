# Vendored copy of "textable" 0.4.1 (MIT License).
# Plain-text tables for terminals and e-mails. Do not edit here: changes are
# lost on the next update (scripts/update_vendor.sh).
#
# Copyright (c) the textable authors.
#
# Permission is hereby granted, free of charge, to any person obtaining a copy
# of this software and associated documentation files (the "Software"), to deal
# in the Software without restriction, including without limitation the rights
# to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
# copies of the Software, and to permit persons to whom the Software is
# furnished to do so, subject to the following conditions:
#
# The above copyright notice and this permission notice shall be included in
# all copies or substantial portions of the Software.
#
# THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
# IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
# FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
"""Render rows of values as an aligned plain-text table.

>>> print(render([["a", 1], ["bb", 22]], headers=["name", "n"], align="lr"))
name   n
----  --
a      1
bb    22
"""

__version__ = "0.4.1"
__all__ = ["render", "Column", "TableError"]

import unicodedata


class TableError(ValueError):
    """Rows and headers that cannot form a table."""


def _cell_width(text):
    """Display width: wide East Asian characters take two cells, combining marks none."""
    width = 0
    for ch in text:
        if unicodedata.combining(ch):
            continue
        width += 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1
    return width


def _pad(text, width, align):
    gap = width - _cell_width(text)
    if gap <= 0:
        return text
    if align == "r":
        return " " * gap + text
    if align == "c":
        left = gap // 2
        return " " * left + text + " " * (gap - left)
    return text + " " * gap


def _to_text(value, missing):
    if value is None:
        return missing
    if isinstance(value, float):
        return f"{value:.2f}"
    return str(value)


class Column:
    """Header, alignment ("l", "r" or "c") and an optional maximum width."""

    __slots__ = ("header", "align", "max_width")

    def __init__(self, header, align="l", max_width=None):
        if align not in ("l", "r", "c"):
            raise TableError(f"bad alignment {align!r}")
        self.header = header
        self.align = align
        self.max_width = max_width

    def clip(self, text):
        if self.max_width is None or _cell_width(text) <= self.max_width:
            return text
        out = []
        used = 0
        for ch in text:
            w = _cell_width(ch)
            if used + w > self.max_width - 1:
                break
            out.append(ch)
            used += w
        return "".join(out) + "…"


def _columns(headers, align, ncols):
    if headers is None:
        headers = [""] * ncols
    if len(headers) != ncols:
        raise TableError(f"{len(headers)} headers for {ncols} columns")
    align = (align or "").ljust(ncols, "l")
    cols = []
    for header, a in zip(headers, align):
        cols.append(header if isinstance(header, Column) else Column(str(header), a))
    return cols


def render(rows, headers=None, align=None, sep="  ", rule="-", missing="", footer=None):
    """The table as one string, without a trailing newline.

    `rows` is a list of sequences of the same length. `align` is a string with
    one letter per column. `footer`, when given, is one more row printed after
    a second rule.
    """
    rows = [list(r) for r in rows]
    ncols = len(headers) if headers is not None else (len(rows[0]) if rows else 0)
    for r in rows + ([list(footer)] if footer else []):
        if len(r) != ncols:
            raise TableError(f"row with {len(r)} cells in a {ncols}-column table")
    cols = _columns(headers, align, ncols)

    body = [[c.clip(_to_text(v, missing)) for c, v in zip(cols, r)] for r in rows]
    foot = [c.clip(_to_text(v, missing)) for c, v in zip(cols, footer)] if footer else None

    widths = [_cell_width(c.header) for c in cols]
    for r in body + ([foot] if foot else []):
        for i, cell in enumerate(r):
            widths[i] = max(widths[i], _cell_width(cell))

    def line(cells):
        return sep.join(_pad(cell, w, c.align) for cell, w, c in zip(cells, widths, cols)).rstrip()

    out = []
    if any(c.header for c in cols):
        out.append(line([c.header for c in cols]))
        if rule:
            out.append(sep.join(rule * w for w in widths))
    out.extend(line(r) for r in body)
    if foot:
        if rule:
            out.append(sep.join(rule * w for w in widths))
        out.append(line(foot))
    return "\n".join(out)


def render_records(records, keys, headers=None, **kwargs):
    """Like render(), for dicts: one column per key in `keys`."""
    rows = [[rec.get(k) for k in keys] for rec in records]
    return render(rows, headers=headers or list(keys), **kwargs)
