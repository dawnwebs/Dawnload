"""Shape text with HarfBuzz and outline it into a skia-pathops Path, so no SVG
in the kit depends on installed fonts."""
import os

import uharfbuzz as hb
from fontTools.ttLib import TTFont
from pathops import Path, PathPen

FONT_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "fonts") + "/"
FONTS = {
    "mark": ("unbounded.ttf", {"wght": 800}),
    "display": ("unbounded.ttf", {"wght": 800}),
    "display-mid": ("unbounded.ttf", {"wght": 600}),
    "text": ("manrope.ttf", {"wght": 700}),
    "text-regular": ("manrope.ttf", {"wght": 500}),
    "mono": ("martianmono.ttf", {"wght": 500, "wdth": 100}),
    "mono-bold": ("martianmono.ttf", {"wght": 700, "wdth": 100}),
}
_cache = {}


def _load(key):
    if key not in _cache:
        fname, loc = FONTS[key]
        blob = hb.Blob.from_file_path(FONT_DIR + fname)
        font = hb.Font(hb.Face(blob))
        font.set_variations(loc)
        tt = TTFont(FONT_DIR + fname)
        gs = tt.getGlyphSet(location=loc) if "fvar" in tt else tt.getGlyphSet()
        _cache[key] = (font, tt, gs, tt["head"].unitsPerEm, tt.getGlyphOrder())
    return _cache[key]


def glyphs(txt, key="display", size=100.0, x=0.0, y=0.0, tracking=0.0):
    """Return [(char_index, Path)] per glyph, plus the advance width. y is the baseline."""
    font, tt, gs, upm, order = _load(key)
    buf = hb.Buffer()
    buf.add_str(txt)
    buf.guess_segment_properties()
    hb.shape(font, buf, {"kern": True, "liga": True})
    s = size / upm
    pen_x = 0
    out = []
    for info, pos in zip(buf.glyph_infos, buf.glyph_positions):
        g = Path()
        gs[order[info.codepoint]].draw(PathPen(g, glyphSet=gs))
        ox = x + (pen_x + pos.x_offset) * s
        oy = y - pos.y_offset * s
        out.append((info.cluster, g.transform(s, 0, 0, -s, ox, oy)))
        pen_x += pos.x_advance + tracking * upm
    width = (pen_x - tracking * upm) * s
    return out, width


def outline(txt, key="display", size=100.0, x=0.0, y=0.0, tracking=0.0, align="left"):
    """Return (Path, advance_width). y is the baseline. tracking in em units."""
    _, width = glyphs(txt, key, size, 0, y, tracking)
    if align == "center":
        x -= width / 2
    elif align == "right":
        x -= width
    parts, _ = glyphs(txt, key, size, x, y, tracking)
    acc = Path()
    for _, g in parts:
        acc.addPath(g)
    acc.simplify()
    return acc, width


def metrics(key):
    font, tt, gs, upm, order = _load(key)
    os2 = tt["OS/2"]
    return {"upm": upm, "cap": getattr(os2, "sCapHeight", 0), "xh": getattr(os2, "sxHeight", 0)}
