# -*- coding: utf-8 -*-
"""Draw the fixed images `automation-platform ocr-bench` reads on a Mac.

    python tools/ocr-fixtures/make.py            write them into crates/host/bench-data/ocr/
    python tools/ocr-fixtures/make.py --check    say whether the committed ones are what this draws

The benchmark measures what Apple Vision costs on fixed pictures with known text, so that a CI
runner, the Intel MacBook Air and an Apple-silicon Mac all read the SAME pixels. Each layout is
drawn twice: at 1x, and at 2x the way a Retina display draws the same interface (twice the
pixels, the type drawn at twice the size, not an enlarged copy). The executable carries the
PNGs inside it (`include_bytes!` in crates/host/src/ocr/bench.rs), whose table also holds each
picture's size in points and the answers it accepts; the two have to agree, and a test there
checks the sizes against the PNG headers.

The fields imitate sforzando's read-outs, the ones a tester's Intel Air read slowly: a black
well set into grey chrome, light text in it, at the sizes the overlay reads. That shape is the
recogniser's content crop, then its panel-in-well step (backend/macos/ocr.rs, `Plan::content`).
The type is sized so that the digits come out about 11 pixels tall at 1x and 23 at 2x, as that
crop measures them, which is what ocr.rs measured on sforzando's own Polyphony field (about 11 px
at 1x): so the pipeline enlarges them as it enlarges the real ones, 5 times at 1x and twice at
2x. Drawn smaller, they were enlarged 8 and 4 times, and a lone digit sat further above Vision's
floor than a real one does.
`field-empty` is sforzando's Instrument field, which is the other way round: a light panel that
darkens toward its foot, dark text, and a dotted rule under it, as the Windows calibration shot
shows it (rows 20 to 43 of the field, the light grey 222 at the top). `val-db` and `val-ct` are
Melodyne's inspector: a flat light grey field (209) with black digits about 8 pixels tall at 1x.
The `none-` pictures hold ink that is not text — a level meter, a speaker symbol, an empty well
with its bevel, a text caret — and the only right answer on them is nothing: a recogniser that
reads something there invented it. `clipped-label` is a word whose lower half the region cuts
off: its upper half is there to be read, so the word is right and so is nothing.
`line` is wider than 400 points, so it takes the other path, the capture handed to Vision as it
is. They are imitations, drawn by FreeType rather than by macOS, and not a screenshot of any
plug-in. Real captures are cut by crop.py, beside this file.

The type is Aileron, the scalable font Pillow 10.1 and later carries inside itself
(`ImageFont.load_default(size)`), so no font file enters the repository. Its name table says
"No Rights Reserved." (dotcolon.net releases it under CC0).

Needs Pillow 10.1 or later with FreeType. Pictures drawn by another Pillow or FreeType may differ
in their antialiasing by a pixel value here and there; `--check` says so rather than failing
quietly, and the committed PNGs are what the benchmark measures either way.
"""
import argparse
import io
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont, features

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "crates" / "host" / "bench-data" / "ocr"

CHROME = (58, 58, 58)
WELL = (8, 8, 8)
VALUE = (220, 220, 220)
PAGE = (236, 236, 236)
INK = (32, 32, 32)
# sforzando's Instrument panel: 222 at the top, 196 at the foot, near-black text, a dotted rule.
PANEL_TOP = 222
PANEL_FOOT = 196
PANEL_INK = (16, 16, 16)
RULE = (150, 150, 150)
# Melodyne's inspector field: flat 209, two lighter-to-flat rows at its top edge, black digits.
INSPECTOR = (209, 209, 209)
INSPECTOR_EDGE = ((193, 193, 193), (202, 202, 202))
INSPECTOR_INK = (0, 0, 0)

# name, width and height in points, text, how it is drawn. Kept in step with FIXTURES in
# crates/host/src/ocr/bench.rs, which also holds the answers each one accepts: the text, or
# nothing for the `none-` pictures (their text here is only what is drawn, never an answer), or
# either for `clipped-label`.
LAYOUTS = [
    ("field-64", 40, 20, "64", "well-centre"),
    ("field-DEF", 38, 22, "DEF", "well-centre"),
    ("lone-1", 20, 20, "1", "well-centre"),
    ("field-empty", 123, 23, "empty", "panel-left"),
    ("line", 520, 24, "Instrument Polyphony Pitchbend Range Velocity Curve Release Time Volume", "page-left"),
    ("val-minus12", 40, 20, "-12", "well-centre"),
    ("val-db", 70, 14, "-0.5 dB", "inspector-centre"),
    ("val-ct", 50, 14, "+3 ct", "inspector-centre"),
    ("val-050", 44, 20, "0.50", "well-centre"),
    ("none-bars", 40, 20, "", "well-bars"),
    ("none-icon", 20, 20, "", "well-speaker"),
    ("none-well", 40, 20, "", "well-bevel"),
    ("clipped-label", 60, 8, "Velocity", "page-clipped"),
    ("none-caret", 40, 16, "", "inspector-caret"),
]
SCALES = (1, 2)
# 16 pt Aileron: digits about 11 px tall at 1x by the content crop's own measure (see above).
FIELD_PT = 16
LINE_PT = 12
# 13 pt: the Instrument panel's x-height about 7 px at 1x, as the shot measures "empty".
PANEL_PT = 13
# 12 pt: digits about 8 px tall at 1x, as the shot measures Melodyne's "0.00 dB".
INSPECTOR_PT = 12
INSET_PT = 2


def well(w, h, s):
    img = Image.new("RGB", (w, h), CHROME)
    d = ImageDraw.Draw(img)
    inset = INSET_PT * s
    d.rectangle([inset, inset, w - inset - 1, h - inset - 1], fill=WELL)
    return img, d, inset


def draw(w_pt, h_pt, text, style, s):
    w, h = w_pt * s, h_pt * s
    if style in ("well-centre", "well-left"):
        img, d, inset = well(w, h, s)
        font = ImageFont.load_default(FIELD_PT * s)
        if style == "well-centre":
            d.text((w / 2, h / 2), text, font=font, fill=VALUE, anchor="mm")
        else:
            d.text((inset + 4 * s, h / 2), text, font=font, fill=VALUE, anchor="lm")
    elif style == "well-bars":
        # A level meter: six bars of different heights standing on the well's floor.
        img, d, inset = well(w, h, s)
        floor = h - inset - 2 * s
        for i, tall in enumerate((5, 9, 12, 7, 10, 4)):
            x = inset + (4 + 5 * i) * s
            d.rectangle([x, floor - tall * s, x + 3 * s - 1, floor - 1], fill=VALUE)
    elif style == "well-speaker":
        # A speaker: the magnet, the cone, and two sound waves.
        img, d, inset = well(w, h, s)
        cy = h / 2
        d.rectangle([4 * s, cy - 2 * s, 6 * s - 1, cy + 2 * s - 1], fill=VALUE)
        d.polygon([(6 * s, cy - 2 * s), (10 * s, cy - 5 * s), (10 * s, cy + 5 * s), (6 * s, cy + 2 * s)], fill=VALUE)
        for r in (3, 6):
            box = [10 * s - r * s, cy - r * s, 10 * s + r * s, cy + r * s]
            d.arc(box, start=-50, end=50, fill=VALUE, width=s)
    elif style == "well-bevel":
        # Empty, with a lighter bevel along its lower and right edges: ink, and nothing to read.
        img, d, inset = well(w, h, s)
        edge = (96, 96, 96)
        d.line([(inset, h - inset - 1), (w - inset - 1, h - inset - 1)], fill=edge, width=1)
        d.line([(w - inset - 1, inset), (w - inset - 1, h - inset - 1)], fill=edge, width=1)
    elif style == "panel-left":
        img = Image.new("RGB", (w, h), PAGE)
        d = ImageDraw.Draw(img)
        for y in range(h):
            v = round(PANEL_TOP + (PANEL_FOOT - PANEL_TOP) * y / max(h - 1, 1))
            d.line([(0, y), (w - 1, y)], fill=(v, v, v))
        font = ImageFont.load_default(PANEL_PT * s)
        # The text in the panel's upper part and the rule under it, where the shot has them:
        # text centred 9.5 points down, the rule 18 points down, dotted every 5 points.
        d.text((10 * s, round(9.5 * s)), text, font=font, fill=PANEL_INK, anchor="lm")
        for x in range(0, w, 5 * s):
            d.rectangle([x + s, 18 * s, x + 2 * s - 1, 19 * s - 1], fill=RULE)
    elif style.startswith("inspector"):
        img = Image.new("RGB", (w, h), INSPECTOR)
        d = ImageDraw.Draw(img)
        for i, c in enumerate(INSPECTOR_EDGE):
            d.rectangle([0, i * s, w - 1, (i + 1) * s - 1], fill=c)
        if style == "inspector-centre":
            font = ImageFont.load_default(INSPECTOR_PT * s)
            d.text((w / 2, h / 2 + s), text, font=font, fill=INSPECTOR_INK, anchor="mm")
        else:
            # A text caret: one point wide, 10 points tall, near the left edge.
            d.rectangle([6 * s, 3 * s, 7 * s - 1, 13 * s - 1], fill=INSPECTOR_INK)
    elif style == "page-clipped":
        # A label whose lower half lies below the region: the text's middle on the bottom edge.
        img = Image.new("RGB", (w, h), PAGE)
        d = ImageDraw.Draw(img)
        font = ImageFont.load_default(LINE_PT * s)
        d.text((4 * s, h), text, font=font, fill=INK, anchor="lm")
    else:
        img = Image.new("RGB", (w, h), PAGE)
        d = ImageDraw.Draw(img)
        font = ImageFont.load_default(LINE_PT * s)
        if d.textlength(text, font=font) > w - 16 * s:
            raise SystemExit(f"'{text}' does not fit {w_pt} points at {LINE_PT} pt")
        d.text((8 * s, h / 2), text, font=font, fill=INK, anchor="lm")
    return img


def png_bytes(img):
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true", help="compare with the committed files, write nothing")
    args = ap.parse_args()
    if not features.check("freetype2"):
        raise SystemExit("this Pillow has no FreeType, so it cannot draw Aileron")
    OUT.mkdir(parents=True, exist_ok=True)
    differ = 0
    for name, w_pt, h_pt, text, style in LAYOUTS:
        for s in SCALES:
            path = OUT / f"{name}@{s}x.png"
            img = draw(w_pt, h_pt, text, style, s)
            if args.check:
                if not path.exists():
                    print(f"missing: {path.name}")
                    differ += 1
                    continue
                with Image.open(path) as old:
                    if old.convert("RGB").tobytes() != img.tobytes() or old.size != img.size:
                        print(f"differs: {path.name}")
                        differ += 1
            else:
                path.write_bytes(png_bytes(img))
                print(f"wrote {path.relative_to(ROOT).as_posix()} ({img.size[0]}x{img.size[1]} px, '{text}')")
    if args.check:
        print("all pictures are what this draws" if differ == 0 else f"{differ} picture(s) differ")
        return 1 if differ else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
