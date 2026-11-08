"""Build the Dawnload brand kit: every SVG and PNG in docs/brand comes from here.

    docs/brand/source/fetch_fonts.sh
    python3 -m pip install -r docs/brand/source/requirements.txt
    python3 docs/brand/source/build_brand.py            # both colourways
    python3 docs/brand/source/build_brand.py green      # one

The green colourway (the app's) is written to docs/brand/, the violet alternate to docs/brand/violet/.
"""
import io
import json
import math
import sys
from pathlib import Path as FsPath

import resvg_py
from PIL import Image, ImageDraw

import geo
import text

KIT = FsPath(__file__).resolve().parents[1]
FONTS = [str(p) for p in (FsPath(__file__).resolve().parent / "fonts").glob("*.ttf")]

# ---------------------------------------------------------------------------
# Colourways. Every colourway fills the same roles; names are what the palette
# sheet and the README call them.
COLOURWAYS = {
    "violet": {
        "dir": "violet/",
        "names": {
            "brand": "Violet", "brand_text": "Lilac", "accent": "Volt", "accent_ink": "Moss",
            "night": "Night", "surface": "Dusk", "line": "Seam", "haze": "Haze",
            "paper": "Paper", "sand": "Mist", "smoke": "Smoke", "ink": "Ink",
            "failed": "Coral", "failed_ink": "Coral deep",
        },
        "brand": "#6622D1",  # the D, the one action, the app icon
        "brand_text": "#B795FF",  # violet for marks, links and focus on Night
        "brand_ink": "#6622D1",  # violet as text on Paper (6.9:1)
        "on_brand": "#F5F0FD",  # what sits on Violet: Paper (6.9:1)
        "accent": "#C8FF2E",  # Volt: progress and kept, the arrow's light
        "accent_ink": "#3E6B00",  # kept, as text on Paper
        "night": "#0D0720",
        "surface": "#1A0F38",
        "line": "#2C1D55",
        "haze": "#B9A6E0",
        "paper": "#F5F0FD",
        "sand": "#E3D9F5",
        "smoke": "#5E4E86",
        "ink": "#1E0F3D",
        "failed": "#FF6B6B",
        "failed_ink": "#C0262D",
    },
    "green": {
        "dir": "",
        "names": {
            "brand": "Signal", "brand_text": "Signal", "accent": "Signal", "accent_ink": "Forest",
            "night": "Pine", "surface": "Grove", "line": "Moss line", "haze": "Sage",
            "paper": "Paper", "sand": "Dew", "smoke": "Lichen", "ink": "Ink",
            "failed": "Coral", "failed_ink": "Coral deep",
        },
        "brand": "#17D36E",
        "brand_text": "#17D36E",  # 9.5:1 on Pine
        "brand_ink": "#0B7A3E",  # Signal is too light for text on Paper; Forest is 5.0:1
        "on_brand": "#07140E",  # Pine on Signal (9.5:1); never Paper on Signal (1.8:1)
        "accent": "#17D36E",  # one colour does both jobs in green
        "accent_ink": "#0B7A3E",
        "night": "#07140E",
        "surface": "#0F2219",
        "line": "#1B3428",
        "haze": "#9DC4AE",
        "paper": "#F2F8F3",
        "sand": "#DCE9DF",
        "smoke": "#4C6B5A",
        "ink": "#07140E",
        "failed": "#FF6B5C",
        "failed_ink": "#B8261A",
    },
}
ROLE_USE = {
    "brand": "The D, the one action per view, the app icon",
    "brand_text": "The D and links on Night",
    "accent": "Progress and Kept",
    "accent_ink": "Kept, as text on Paper",
    "night": "The ground. Dark canvas",
    "surface": "Raised surfaces on Night",
    "line": "Lines and empty tracks on Night",
    "haze": "Secondary text on Night",
    "paper": "Light canvas. Text on Night",
    "sand": "Lines and fills on Paper",
    "smoke": "Secondary text on Paper",
    "ink": "Text on Paper",
    "failed": "Failed, on Night",
    "failed_ink": "Failed, on Paper",
}

written = []
P = {}  # the active colourway
PREFIX = ""


def out(rel, content):
    p = KIT / (PREFIX + rel)
    p.parent.mkdir(parents=True, exist_ok=True)
    if isinstance(content, str):
        p.write_text(content)
    else:
        p.write_bytes(content)
    written.append(PREFIX + rel)
    return p


def fmt(v):
    return (f"{v:.2f}").rstrip("0").rstrip(".")


def svg(w, h, body, vb=None, title=None):
    vb = vb or f"0 0 {fmt(w)} {fmt(h)}"
    t = f"<title>{title}</title>" if title else ""
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{fmt(w)}" height="{fmt(h)}" viewBox="{vb}">{t}{body}</svg>\n'


def path(p, fill, extra=""):
    return f'<path d="{geo.svg_d(p)}" fill="{fill}"{extra}/>'


def raster(rel_svg, width):
    data = resvg_py.svg_to_bytes(svg_path=str(KIT / (PREFIX + rel_svg)), width=width, font_files=FONTS, skip_system_fonts=True)
    return Image.open(io.BytesIO(bytes(data))).convert("RGBA")


def png(rel_svg, width, rel_png=None):
    im = raster(rel_svg, width)
    buf = io.BytesIO()
    im.save(buf, "PNG", optimize=True)
    out(rel_png or rel_svg.replace(".svg", ".png"), buf.getvalue())


def label(t, key, size, x, y, fill, align="left", tracking=0.0, extra=""):
    p, _ = text.outline(t, key, size, x, y, tracking=tracking, align=align)
    return path(p, fill, extra)


# ---------------------------------------------------------------------------
# The marks


def mark(x, y, s, d_fill, arrow_fill=None):
    """The Cut D at (x, y), height 2s. With arrow_fill, the arrow is drawn in its cut."""
    body = path(geo.cut_d(x, y, s), d_fill)
    if arrow_fill:
        body += path(geo.arrow(x, y, s), arrow_fill)
    return body


def wordmark(size=200.0, x=0.0, y=0.0):
    p, w = text.outline("dawnload", "mark", size, x, y, tracking=-0.02)
    return p


def d_height(size):
    """The height of the wordmark's d, so the Cut D can stand beside it like a capital."""
    gl, _ = text.glyphs("d", "mark", size, 0, 0)
    b = gl[0][1].bounds
    return -b[1]


def build_logos():
    s = 100
    pad = 0.22 * 2 * s
    w, h = geo.WIDTH * s + 2 * pad, 2 * s + 2 * pad
    for name, d, a in [
        ("", P["brand"], None),
        ("-night", P["brand_text"], P["accent"] if P["accent"] != P["brand"] else None),
        ("-paper", P["paper"], None),
        ("-ink", P["ink"], None),
        ("-mono", "currentColor", None),
    ]:
        out(f"logo/dawnload-mark{name}.svg", svg(w, h, mark(pad, pad, s, d, a), title="Dawnload"))
    out("logo/dawnload-mark-solid.svg", svg(w, h, path(geo.d_shape(pad, pad, s), "currentColor"), title="Dawnload"))

    # Wordmark
    wm = wordmark(200)
    x0, y0, x1, y1 = wm.bounds
    pad = 0.12 * (y1 - y0)
    vb = f"{fmt(x0 - pad)} {fmt(y0 - pad)} {fmt(x1 - x0 + 2 * pad)} {fmt(y1 - y0 + 2 * pad)}"
    W, H = x1 - x0 + 2 * pad, y1 - y0 + 2 * pad
    for name, ink in [("ink", P["ink"]), ("paper", P["paper"]), ("brand", P["brand"]), ("mono", "currentColor")]:
        out(f"logo/dawnload-wordmark-{name}.svg", svg(W, H, path(wm, ink), vb=vb, title="Dawnload"))

    # Lockup: the Cut D stands beside the wordmark at the height of its d.
    dh = d_height(200)
    s = dh / 2
    gap = 0.30 * dh
    dx = x0 - gap - geo.WIDTH * s
    pad = 0.3 * dh
    vb = f"{fmt(dx - pad)} {fmt(-dh - pad)} {fmt(x1 - dx + 2 * pad)} {fmt(max(y1, 0) + dh + 2 * pad)}"
    W, H = x1 - dx + 2 * pad, max(y1, 0) + dh + 2 * pad
    lock = {
        "colour": (P["brand"], P["ink"], None),  # on Paper or white
        "night": (P["brand_text"], P["paper"], P["accent"] if P["accent"] != P["brand"] else None),
        "brand": (P["on_brand"], P["on_brand"], None),  # on the brand colour
        "mono": ("currentColor", "currentColor", None),
    }
    for name, (dfill, ink, arrow_fill) in lock.items():
        body = mark(dx, -dh, s, dfill, arrow_fill) + path(wm, ink)
        out(f"logo/dawnload-lockup-{name}.svg", svg(W, H, body, vb=vb, title="Dawnload"))

    # Stacked: the Cut D above the wordmark.
    wmc, tw = text.outline("dawnload", "mark", 200, 0, 0, tracking=-0.02, align="center")
    b = wmc.bounds
    s = (b[2] - b[0]) * 0.20
    gap = 0.34 * s
    top = b[1] - gap - 2 * s
    pad = 0.5 * s
    vb = f"{fmt(b[0] - pad)} {fmt(top - pad)} {fmt(b[2] - b[0] + 2 * pad)} {fmt(b[3] - top + 2 * pad)}"
    W, H = b[2] - b[0] + 2 * pad, b[3] - top + 2 * pad
    for name, (dfill, ink, arrow_fill) in lock.items():
        body = mark(-geo.WIDTH * s / 2, top, s, dfill, arrow_fill) + path(wmc, ink)
        out(f"logo/dawnload-stacked-{name}.svg", svg(W, H, body, vb=vb, title="Dawnload"))

    for rel, wpx in [
        ("logo/dawnload-mark.svg", 512),
        ("logo/dawnload-lockup-colour.svg", 1600),
        ("logo/dawnload-lockup-night.svg", 1600),
        ("logo/dawnload-stacked-colour.svg", 1000),
        ("logo/dawnload-stacked-night.svg", 1000),
    ]:
        png(rel, wpx)


# ---------------------------------------------------------------------------
# App icon (macOS): an 824 tile on a 1024 canvas, the grid Apple's template uses.

TILE = (100, 100, 824, 824, 185.4)


def icon_svg(kind):
    x, y, w, h, rad = TILE
    tile = geo.rrect(x, y, w, h, rad)
    s = 232  # the D is 464 tall: 56% of the tile
    dw = geo.WIDTH * s
    dx = 512 - dw / 2
    dy = 512 - s
    defs = (
        '<defs><filter id="s" x="-10%" y="-10%" width="120%" height="125%">'
        '<feDropShadow dx="0" dy="12" stdDeviation="14" flood-color="#000" flood-opacity="0.3"/></filter></defs>'
    )
    lift = P["accent"] if P["accent"] != P["brand"] else None
    if kind == "default":
        ground, d, a = P["brand"], P["on_brand"], None
    elif kind == "dark":
        ground, d, a = P["night"], P["brand_text"] if P["brand"] != P["brand_text"] else P["brand"], lift
    elif kind == "light":
        ground, d, a = P["paper"], P["brand"], None
    else:  # mono: tinted and clear appearances recolour it
        ground, d, a = "#1C1C1E", "#FFFFFF", None
    body = defs + f'<g filter="url(#s)">{path(tile, ground)}</g>' + mark(dx, dy, s, d, a)
    return svg(1024, 1024, body, title="Dawnload")


def build_icons():
    for kind in ["default", "dark", "light", "mono"]:
        name = "dawnload-app-icon" + ("" if kind == "default" else f"-{kind}")
        out(f"app-icon/{name}.svg", icon_svg(kind))
        png(f"app-icon/{name}.svg", 1024, f"app-icon/png/{name}-1024.png")
    for s in [512, 256, 128, 64, 32, 16]:
        png("app-icon/dawnload-app-icon.svg", s, f"app-icon/png/dawnload-app-icon-{s}.png")


# ---------------------------------------------------------------------------
# Graphics


def field_tile(faint, accent, size=360):
    """A seamless tile of small download arrows on a staggered grid; one is lit."""
    step = size / 6
    w = step * 0.13
    body = ""
    for row in range(6):
        for col in range(6):
            cx = step * (col + (0.75 if row % 2 else 0.25))
            cy = step * (row + 0.5)
            hl = step * 0.17
            # parts are drawn separately: unioning these small strokes trips pathops on some positions
            parts = [
                geo.stroke(cx, cy - hl * 1.3, cx, cy + hl, w),
                geo.stroke(cx - hl * 0.9, cy + hl * 0.1, cx, cy + hl, w),
                geo.stroke(cx, cy + hl, cx + hl * 0.9, cy + hl * 0.1, w),
            ]
            lit = (row, col) == (3, 2)
            for part in parts:
                body += path(part, accent if lit else faint)
    return body


def build_graphics():
    # The Field: rows of small arrows, one lit.
    for name, g, f, a in [("night", P["night"], P["line"], P["accent"]), ("paper", P["paper"], P["sand"], P["brand"])]:
        tile = field_tile(f, a)
        body = f'<defs><pattern id="f" width="360" height="360" patternUnits="userSpaceOnUse">{tile}</pattern></defs>'
        body += f'<rect width="1080" height="540" fill="{g}"/><rect width="1080" height="540" fill="url(#f)"/>'
        out(f"graphics/dawnload-field-{name}.svg", svg(1080, 540, body))
        out(f"graphics/dawnload-field-tile-{name}.svg", svg(360, 360, f'<rect width="360" height="360" fill="{g}"/>' + tile))

    # Progress: the D fills from the top, the way the arrow points; full, it is Kept.
    steps = [0, 0.25, 0.5, 0.75, 1.0]
    s = 54
    cell = 190
    for ground, track, lab_col, name in [(P["night"], P["line"], P["haze"], "night"), (P["paper"], P["sand"], P["smoke"], "paper")]:
        w, h = cell * len(steps) + 40, 300
        body = f'<rect width="{w}" height="{h}" fill="{ground}"/>'
        for i, f in enumerate(steps):
            x = 20 + cell * i + (cell - geo.WIDTH * s) / 2
            y = 60
            dshape = geo.d_shape(x, y, s)
            if f < 1:
                cut = geo.cut_shape("arrow", x, y, s)
                body += path(geo.diff(dshape, cut), track)
                if f > 0:
                    filled = geo.inter(geo.diff(dshape, cut), geo.rect(x - 5, y - 5, 4 * s, 2 * s * f + 5))
                    body += path(filled, P["brand_text"] if name == "night" else P["brand"])
                t = f"{int(f * 100)}%"
            else:
                kept = P["accent"] if name == "night" else P["accent_ink"]
                body += path(geo.diff(dshape, geo.cut_shape("check", x, y, s)), kept)
                t = "Kept"
            body += label(t, "text", 22, x + geo.WIDTH * s / 2, y + 2 * s + 56, lab_col, align="center")
        out(f"graphics/dawnload-fill-{name}.svg", svg(w, h, body))

    build_states()
    build_loader()
    build_construction()
    build_palette()


STATES = [("queued", "Queued", "dots"), ("pulling", "Pulling", "arrow"), ("polishing", "Polishing", "merge"),
          ("kept", "Kept", "check"), ("failed", "Failed", "bang"), ("paused", "Paused", "pause")]


def state_glyph(kind, cut, x, y, s, fill):
    d = geo.d_shape(x, y, s)
    if kind == "queued":  # an outline: nothing has started
        ring = geo.diff(d, geo.d_shape(x + 0.24 * s, y + 0.24 * s, s * 0.76))
        return path(geo.union(ring, geo.cut_shape("dots", x, y, s, 0.30)), fill)
    w = 0.34 if kind != "pulling" else geo.ARROW_W
    return path(geo.diff(d, geo.cut_shape(cut, x, y, s, w)), fill)


def build_states():
    # Single glyphs on a 24 grid, in currentColor, for the app and the web.
    S = 24
    s = 9.0
    x = (S - geo.WIDTH * s) / 2
    y = (S - 2 * s) / 2
    for kind, _, cut in STATES:
        out(f"graphics/states/dawnload-{kind}.svg", svg(S, S, state_glyph(kind, cut, x, y, s, "currentColor")))
    roles = {"queued": P["haze"], "pulling": P["brand_text"], "polishing": P["brand_text"], "kept": P["accent"], "failed": P["failed"], "paused": P["haze"]}
    cell = 160
    w, h = cell * len(STATES), 190
    body = f'<rect width="{w}" height="{h}" fill="{P["night"]}"/>'
    s = 32
    for i, (kind, lab, cut) in enumerate(STATES):
        gx = i * cell + (cell - geo.WIDTH * s) / 2
        body += state_glyph(kind, cut, gx, 34, s, roles[kind])
        body += label(lab, "text", 20, i * cell + cell / 2, 156, P["paper"], align="center")
    out("graphics/dawnload-states-night.svg", svg(w, h, body))


def build_loader():
    """The arrow travels down through the D and comes round again. Loops."""
    s = 60
    w, h = geo.WIDTH * s + 20, 2 * s + 20
    x, y = 10, 10
    d = geo.d_shape(x, y, s)
    a = geo.arrow(x, y, s)
    dur = 1.4
    mask = (
        f'<mask id="m"><path d="{geo.svg_d(d)}" fill="#fff"/>'
        f'<g><animateTransform attributeName="transform" type="translate" dur="{dur}s" repeatCount="indefinite" '
        f'calcMode="spline" keyTimes="0;1" keySplines="0.55 0 0.45 1" values="0 {fmt(-1.6 * s)};0 {fmt(1.6 * s)}"/>'
        f'<path d="{geo.svg_d(a)}" fill="#000"/>'
        f'<path d="{geo.svg_d(a)}" fill="#000" transform="translate(0 {fmt(-1.6 * s)})"/></g></mask>'
    )
    body = f"<defs>{mask}</defs>" + f'<rect width="{fmt(w)}" height="{fmt(h)}" fill="{P["brand"]}" mask="url(#m)"/>'
    out("graphics/dawnload-loader.svg", svg(w, h, body, title="Loading"))


def build_construction():
    """How the Cut D is drawn."""
    s = 150
    w, h = 1180, 520
    x, y = 160, 110
    line = P["haze"]
    body = f'<rect width="{w}" height="{h}" fill="{P["night"]}"/>'
    body += mark(x, y, s, P["brand_text"], None)
    cx = x + geo.SPINE * s
    body += f'<circle cx="{fmt(cx)}" cy="{fmt(y + s)}" r="{s}" fill="none" stroke="{line}" stroke-width="1.5" stroke-dasharray="4 6"/>'
    body += f'<line x1="{fmt(cx)}" y1="{y - 30}" x2="{fmt(cx)}" y2="{fmt(y + 2 * s + 30)}" stroke="{line}" stroke-width="1" stroke-dasharray="2 5"/>'
    mid = x + geo.WIDTH * s / 2
    body += f'<line x1="{fmt(mid)}" y1="{y - 30}" x2="{fmt(mid)}" y2="{fmt(y + 2 * s + 30)}" stroke="{P["paper"]}" stroke-width="1" stroke-dasharray="6 4"/>'
    body += f'<circle cx="{fmt(cx)}" cy="{fmt(y + s)}" r="4" fill="{P["paper"]}"/>'
    notes = [
        "Half-disc of radius r on a spine 0.53r deep",
        "Left corners rounded 0.18r",
        "Arrow on the centre of the width, 0.32r stroke,",
        "round caps, wings at 45°",
        "Shaft from 0.48r to the point at 1.40r",
        "Height 2r, width 1.53r",
    ]
    for i, t in enumerate(notes):
        body += label(t, "mono", 17, 560, 150 + i * 44, line)
    body += f'<line x1="{fmt(cx)}" y1="{fmt(y + s)}" x2="{fmt(cx - s)}" y2="{fmt(y + s)}" stroke="{P["paper"]}" stroke-width="1.5"/>'
    body += label("r", "mono", 18, cx - s / 2 - 5, y + s - 10, P["paper"])
    out("graphics/dawnload-construction.svg", svg(w, h, body))


LIGHT_INK = {"accent", "brand_text", "haze", "paper", "sand", "failed"}


def build_palette():
    keys = ["brand", "brand_text", "accent", "night", "surface", "line", "haze",
            "paper", "sand", "smoke", "ink", "accent_ink", "failed", "failed_ink"]
    seen, swatches = set(), []
    for k in keys:
        if P[k].upper() in seen:
            continue
        seen.add(P[k].upper())
        swatches.append(k)
    cols = 7
    cw, ch, g = 168, 200, 16
    rows = math.ceil(len(swatches) / cols)
    w = cols * cw + (cols + 1) * g
    h = rows * ch + (rows + 1) * g
    body = f'<rect width="{w}" height="{h}" fill="{P["night"]}"/>'
    for i, k in enumerate(swatches):
        x = g + (i % cols) * (cw + g)
        y = g + (i // cols) * (ch + g)
        col = P[k]
        light = k in LIGHT_INK or (k == "brand" and P["on_brand"] == P["night"])
        ink = P["night"] if light else P["paper"]
        body += path(geo.rrect(x, y, cw, ch, 14), col)
        if k in ("night",):
            body += path(geo.diff(geo.rrect(x, y, cw, ch, 14), geo.rrect(x + 1.5, y + 1.5, cw - 3, ch - 3, 12.5)), P["line"])
        body += label(P["names"][k], "display-mid", 19, x + 14, y + 34, ink)
        body += label(col.upper(), "mono", 13, x + 14, y + 58, ink)
        use = ROLE_USE[k].replace("Night", P["names"]["night"])
        words, line1 = use.split(), ""
        lines = []
        for wd in words:
            if len(line1 + " " + wd) > 24 and line1:
                lines.append(line1)
                line1 = wd
            else:
                line1 = (line1 + " " + wd).strip()
        lines.append(line1)
        for j, ln in enumerate(lines[-2:]):
            body += label(ln, "text-regular", 12, x + 14, y + ch - 18 - (len(lines[-2:]) - 1 - j) * 15, ink, extra=' opacity="0.85"')
    out("graphics/dawnload-palette.svg", svg(w, h, body))


# ---------------------------------------------------------------------------
# Web


def build_web():
    # Favicon: the Cut D in the brand colour on its own.
    s = 14
    out("web/favicon.svg", svg(32, 32, mark((32 - geo.WIDTH * s) / 2, 2, s, P["brand"])))
    for sz in [16, 32, 48]:
        png("web/favicon.svg", sz, f"web/favicon-{sz}.png")
    for sz in [180, 192, 512]:
        png("app-icon/dawnload-app-icon.svg", sz, f"web/icon-{sz}.png")

    # Avatar, 400: the app icon's face without the tile.
    s = 116
    body = f'<rect width="400" height="400" fill="{P["brand"]}"/>' + mark(200 - geo.WIDTH * s / 2, 200 - s, s, P["on_brand"])
    out("web/dawnload-avatar.svg", svg(400, 400, body))
    png("web/dawnload-avatar.svg", 400)

    # Open Graph card, 1200 × 630: the brand ground, a giant Cut D running off the right edge.
    W, H = 1200, 630
    ink = P["on_brand"]
    sub = P["on_brand"]
    body = f'<rect width="{W}" height="{H}" fill="{P["brand"]}"/>'
    s = 330
    body += mark(760, H / 2 - s, s, ink)
    body += path(wordmark(104, 80, 250), ink)
    body += label("Keep what you find.", "display-mid", 42, 84, 360, ink, tracking=-0.01)
    body += label("Video, audio and galleries from the web,", "text-regular", 25, 86, 424, sub, extra=' opacity="0.85"')
    body += label("downloaded to your Mac.", "text-regular", 25, 86, 458, sub, extra=' opacity="0.85"')
    body += label("A DAWN LABS APP", "mono", 14, 86, 560, sub, tracking=0.08, extra=' opacity="0.7"')
    out("web/dawnload-og.svg", svg(W, H, body))
    png("web/dawnload-og.svg", 1200)

    # Banner, 1500 × 500: Night with the Field, and the lockup.
    W, H = 1500, 500
    body = f'<defs><pattern id="f" width="360" height="360" patternUnits="userSpaceOnUse">{field_tile(P["line"], P["line"])}</pattern></defs>'
    body += f'<rect width="{W}" height="{H}" fill="{P["night"]}"/><rect width="{W}" height="{H}" fill="url(#f)" opacity="0.7"/>'
    size = 132
    wm = wordmark(size)
    b = wm.bounds
    dh = d_height(size)
    s = dh / 2
    gap = 0.30 * dh
    total = (b[2] - b[0]) + gap + geo.WIDTH * s
    ox = (W - total) / 2 + geo.WIDTH * s + gap - b[0]
    oy = 250
    body += f'<rect x="{fmt(ox - gap - geo.WIDTH * s - 40)}" y="{fmt(oy - dh - 40)}" width="{fmt(total + 80)}" height="{fmt(dh + 80)}" fill="{P["night"]}"/>'
    lift = P["accent"] if P["accent"] != P["brand"] else None
    body += mark(ox - gap - geo.WIDTH * s, oy - dh, s, P["brand_text"], lift) + path(wordmark(size, ox, oy), P["paper"])
    body += label("Keep what you find.", "display-mid", 34, W / 2, oy + 110, P["haze"], align="center", tracking=-0.01)
    out("web/dawnload-banner.svg", svg(W, H, body))
    png("web/dawnload-banner.svg", 1500)


# ---------------------------------------------------------------------------
# Tokens


def build_tokens():
    css = [f"/* Dawnload brand tokens ({NAME} colourway). Generated by docs/brand/source/build_brand.py. */", ":root {"]
    for k in ["brand", "brand_text", "brand_ink", "on_brand", "accent", "accent_ink", "night", "surface", "line", "haze",
              "paper", "sand", "smoke", "ink", "failed", "failed_ink"]:
        css.append(f"  --dl-{k.replace('_', '-')}: {P[k]};")
    css += [
        "",
        "  /* Roles. Dark is the default: Dawnload lives on Night. */",
        "  --dl-canvas: var(--dl-night);",
        "  --dl-raised: var(--dl-surface);",
        "  --dl-rule: var(--dl-line);",
        "  --dl-text: var(--dl-paper);",
        "  --dl-text-2: var(--dl-haze);",
        "  --dl-action: var(--dl-brand);",
        "  --dl-on-action: var(--dl-on-brand);",
        "  --dl-link: var(--dl-brand-text);",
        "  --dl-progress: var(--dl-accent);",
        "  --dl-kept: var(--dl-accent);",
        "  --dl-failed: var(--dl-failed);",
        "",
        "  --dl-font-display: 'Unbounded', 'Manrope', system-ui, sans-serif;",
        "  --dl-font-text: 'Manrope', system-ui, -apple-system, sans-serif;",
        "  --dl-font-mono: 'Martian Mono', ui-monospace, 'SF Mono', monospace;",
        "",
        "  --dl-radius-spine: 6px;  /* the D's small corners: fields, rows */",
        "  --dl-radius-card: 20px;",
        "  --dl-radius-pill: 999px; /* the D's round side: buttons, chips */",
        "",
        "  --dl-pull: 260ms cubic-bezier(0.3, 0, 0, 1);      /* the arrow presses down */",
        "  --dl-settle: 520ms cubic-bezier(0.16, 1, 0.3, 1); /* things arriving */",
        "}",
        "",
        "[data-theme='light'] {",
        "  --dl-canvas: var(--dl-paper);",
        "  --dl-raised: #FFFFFF;",
        "  --dl-rule: var(--dl-sand);",
        "  --dl-text: var(--dl-ink);",
        "  --dl-text-2: var(--dl-smoke);",
        "  --dl-link: var(--dl-brand-ink);",
        "  --dl-progress: var(--dl-brand);",
        "  --dl-kept: var(--dl-accent-ink);",
        "  --dl-failed: var(--dl-failed-ink);",
        "}",
        "",
    ]
    out("web/dawnload-tokens.css", "\n".join(css))
    data = {k: v for k, v in P.items() if k not in ("dir", "names")}
    out("web/dawnload-tokens.json", json.dumps({"colourway": NAME, "names": P["names"], "color": data}, indent=2) + "\n")


# ---------------------------------------------------------------------------
# preview.png


def rounded(im, r):
    m = Image.new("L", im.size, 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, im.width - 1, im.height - 1], r, fill=255)
    o = im.copy()
    o.putalpha(Image.composite(im.getchannel("A"), m, m))
    return o


def build_preview():
    W, G = 1600, 24
    inner = W - 2 * G
    tiles = []
    y = G
    og_w = round(inner * 0.6)
    og = rounded(raster("web/dawnload-og.svg", og_w), 22)
    side = inner - og_w - G
    iw = (side - 2 * G) // 3
    icons = [raster(f"app-icon/dawnload-app-icon{k}.svg", iw + 40) for k in ["", "-dark", "-light"]]
    tiles.append((og, G, y))
    for i, im in enumerate(icons):
        tiles.append((im, G + og_w + G + i * (iw + G) - 20, y - 6))
    st = rounded(raster("graphics/dawnload-states-night.svg", side), 18)
    sy = y + iw + 20
    tiles.append((st, G + og_w + G, sy))
    lk_y = sy + st.height + G
    room = y + og.height - lk_y
    lk = raster("logo/dawnload-lockup-colour.svg", side)
    if lk.height > room:
        lk = raster("logo/dawnload-lockup-colour.svg", round(side * room / lk.height))
    plate = Image.new("RGBA", (side, max(room, 1)), Image.new("RGBA", (1, 1), P["paper"]).getpixel((0, 0)))
    plate.alpha_composite(lk, ((side - lk.width) // 2, (room - lk.height) // 2))
    tiles.append((rounded(plate, 18), G + og_w + G, lk_y))
    y += og.height + G
    pal = rounded(raster("graphics/dawnload-palette.svg", inner), 18)
    tiles.append((pal, G, y))
    y += pal.height + G
    half = (inner - G) // 2
    fill = rounded(raster("graphics/dawnload-fill-night.svg", half), 18)
    field = rounded(raster("graphics/dawnload-field-night.svg", half).crop((0, 0, half, fill.height)), 18)
    tiles += [(fill, G, y), (field, G + half + G, y)]
    y += fill.height + G
    bg = Image.new("RGBA", (1, 1), P["night"]).getpixel((0, 0))
    canvas = Image.new("RGBA", (W, y), bg)
    for im, x, yy in tiles:
        canvas.alpha_composite(im, (x, yy))
    buf = io.BytesIO()
    canvas.convert("RGB").save(buf, "PNG", optimize=True)
    out("preview.png", buf.getvalue())


def build(name):
    global P, PREFIX, NAME
    NAME = name
    P = COLOURWAYS[name]
    PREFIX = P["dir"]
    build_logos()
    build_icons()
    build_graphics()
    build_web()
    build_tokens()
    build_preview()


if __name__ == "__main__":
    names = sys.argv[1:] or list(COLOURWAYS)
    for n in names:
        build(n)
    print(f"{len(written)} files written")
