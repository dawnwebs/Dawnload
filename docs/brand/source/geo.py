"""Geometry for the Dawnload marks, built as skia-pathops paths.

The Cut D is the Dawn D made solid and wide enough to carry a download arrow
punched through it. Units: the half-disc has radius R = 1; y points down.

    spine   the straight part left of the half-disc, SPINE wide
    corner  the two left corners are rounded by CORNER
    arrow   a round-capped stroke ARROW_W thick, centred on the D's width
"""
import math

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from pathops import Path, PathOp, op

K = 0.5522847498  # cubic handle length for a quarter circle

R = 1.0
SPINE = 0.53  # spine depth, in radii
CORNER = 0.18  # left corner rounding
WIDTH = SPINE + R
HEIGHT = 2 * R
ARROW_TOP = 0.48  # shaft starts this far below the top edge
ARROW_TIP = 1.40  # the arrow's point, from the top edge
ARROW_HEAD = 0.44  # half the head's span (each wing runs at 45°)
ARROW_W = 0.32  # stroke weight


def _arc(pen, cx, cy, r, a0, a1):
    """Append an arc from angle a0 to a1 (degrees, y down, clockwise positive) as cubics."""
    total = a1 - a0
    n = max(1, math.ceil(abs(total) / 90.0))
    step = math.radians(total / n)
    a = math.radians(a0)
    h = 4.0 / 3.0 * math.tan(step / 4.0)
    for _ in range(n):
        b = a + step
        x0, y0 = math.cos(a), math.sin(a)
        x1, y1 = math.cos(b), math.sin(b)
        pen.curveTo(
            (cx + r * (x0 - h * y0), cy + r * (y0 + h * x0)),
            (cx + r * (x1 + h * y1), cy + r * (y1 - h * x1)),
            (cx + r * x1, cy + r * y1),
        )
        a = b


def rect(x, y, w, h):
    p = Path()
    pen = p.getPen()
    pen.moveTo((x, y))
    pen.lineTo((x + w, y))
    pen.lineTo((x + w, y + h))
    pen.lineTo((x, y + h))
    pen.closePath()
    return p


def rrect(x, y, w, h, r):
    r = min(r, w / 2, h / 2)
    p = Path()
    pen = p.getPen()
    pen.moveTo((x + r, y))
    pen.lineTo((x + w - r, y))
    pen.curveTo((x + w - r + K * r, y), (x + w, y + r - K * r), (x + w, y + r))
    pen.lineTo((x + w, y + h - r))
    pen.curveTo((x + w, y + h - r + K * r), (x + w - r + K * r, y + h), (x + w - r, y + h))
    pen.lineTo((x + r, y + h))
    pen.curveTo((x + r - K * r, y + h), (x, y + h - r + K * r), (x, y + h - r))
    pen.lineTo((x, y + r))
    pen.curveTo((x, y + r - K * r), (x + r - K * r, y), (x + r, y))
    pen.closePath()
    return p


def circle(cx, cy, r):
    p = Path()
    pen = p.getPen()
    pen.moveTo((cx + r, cy))
    _arc(pen, cx, cy, r, 0, 360)
    pen.closePath()
    return p


def union(*paths):
    acc = Path()
    for p in paths:
        acc = op(acc, p, PathOp.UNION)
    return acc


def diff(a, b):
    return op(a, b, PathOp.DIFFERENCE)


def inter(a, b):
    return op(a, b, PathOp.INTERSECTION)


def stroke(x0, y0, x1, y1, w):
    """A round-capped straight stroke from (x0, y0) to (x1, y1)."""
    length = math.hypot(x1 - x0, y1 - y0)
    a = math.atan2(y1 - y0, x1 - x0)
    c, s = math.cos(a), math.sin(a)
    return rrect(-w / 2, -w / 2, length + w, w, w / 2).transform(c, s, -s, c, x0, y0)


def polyline(points, w):
    return union(*[stroke(*points[i], *points[i + 1], w) for i in range(len(points) - 1)])


def d_shape(x=0.0, y=0.0, s=1.0):
    """The solid D, its top-left corner at (x, y), height 2s."""
    p = Path()
    pen = p.getPen()
    sp, c = SPINE * s, CORNER * s
    pen.moveTo((x + sp, y))
    _arc(pen, x + sp, y + s, s, -90, 90)  # the half-disc
    pen.lineTo((x + c, y + 2 * s))
    pen.curveTo((x + c - K * c, y + 2 * s), (x, y + 2 * s - c + K * c), (x, y + 2 * s - c))
    pen.lineTo((x, y + c))
    pen.curveTo((x, y + c - K * c), (x + c - K * c, y), (x + c, y))
    pen.closePath()
    return p


def d_center(x=0.0, y=0.0, s=1.0):
    """Where cuts are centred: the middle of the D's width, half way down."""
    return x + WIDTH * s / 2, y + s


def arrow(x=0.0, y=0.0, s=1.0, w=None):
    """The download arrow, positioned for a D at (x, y) of size s."""
    w = (ARROW_W if w is None else w) * s
    cx = x + WIDTH * s / 2
    top, tip, head = y + ARROW_TOP * s, y + ARROW_TIP * s, ARROW_HEAD * s
    return union(stroke(cx, top, cx, tip, w), polyline([(cx - head, tip - head), (cx, tip), (cx + head, tip - head)], w))


def cut_d(x=0.0, y=0.0, s=1.0):
    """The mark: the D with the arrow punched through it."""
    return diff(d_shape(x, y, s), arrow(x, y, s))


# Cuts for the status glyphs: each is a shape punched out of the D.
def cut_shape(kind, x=0.0, y=0.0, s=1.0, w=None):
    w = (ARROW_W if w is None else w) * s
    cx, cy = d_center(x, y, s)
    if kind == "arrow":
        return arrow(x, y, s, w / s)
    if kind == "check":
        return polyline([(cx - 0.40 * s, cy + 0.02 * s), (cx - 0.10 * s, cy + 0.32 * s), (cx + 0.42 * s, cy - 0.26 * s)], w)
    if kind == "bang":
        return union(stroke(cx, cy - 0.48 * s, cx, cy + 0.08 * s, w), circle(cx, cy + 0.44 * s, w * 0.62))
    if kind == "pause":
        g = 0.20 * s
        return union(stroke(cx - g, cy - 0.36 * s, cx - g, cy + 0.36 * s, w * 0.9), stroke(cx + g, cy - 0.36 * s, cx + g, cy + 0.36 * s, w * 0.9))
    if kind == "merge":  # two streams joining into one: ffmpeg merging audio and video
        return union(
            polyline([(cx - 0.36 * s, cy - 0.46 * s), (cx, cy - 0.06 * s), (cx + 0.36 * s, cy - 0.46 * s)], w * 0.9),
            stroke(cx, cy - 0.06 * s, cx, cy + 0.48 * s, w * 0.9),
        )
    if kind == "dots":
        r = w * 0.55
        return union(circle(cx - 0.34 * s, cy, r), circle(cx, cy, r), circle(cx + 0.34 * s, cy, r))
    raise ValueError(kind)


def svg_d(path, a=1, b=0, c=0, d=1, e=0, f=0, digits=2):
    pen = SVGPathPen(None, ntos=lambda v: (f"{v:.{digits}f}").rstrip("0").rstrip("."))
    path.draw(TransformPen(pen, (a, b, c, d, e, f)))
    return pen.getCommands()
