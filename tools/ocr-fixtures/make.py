# -*- coding: utf-8 -*-
"""Draw the fixed images `automation-platform ocr-bench` reads on a Mac.

    python tools/ocr-fixtures/make.py            write them into crates/host/bench-data/ocr/
    python tools/ocr-fixtures/make.py --check    say whether the committed ones are what this draws

The benchmark measures what Apple Vision costs on fixed pictures with known text, so that a CI
runner, the Intel MacBook Air and an Apple-silicon Mac all read the SAME pixels. Each layout is
drawn twice: at 1x, and at 2x the way a Retina display draws the same interface (twice the
pixels, the type drawn at twice the size, not an enlarged copy). The executable carries the
PNGs inside it (`include_bytes!` in crates/host/src/ocr/bench.rs), whose table also holds each
picture's size in points and the text it must read; the two have to agree, and a test there
checks the sizes against the PNG headers.

The fields imitate sforzando's read-outs, the ones a tester's Intel Air read slowly: a black
well set into grey chrome, light text in it, at the sizes the overlay reads. That shape is the
recogniser's content crop, then its panel-in-well step (backend/macos/ocr.rs, `Plan::content`).
The type is sized so that the digits come out about 11 pixels tall at 1x and 23 at 2x, as that
crop measures them, which is what ocr.rs measured on sforzando's own Polyphony field (about 11 px
at 1x): so the pipeline enlarges them as it enlarges the real ones, 5 times at 1x and twice at
2x. Drawn smaller, they were enlarged 8 and 4 times, and a lone digit sat further above Vision's
floor than a real one does.
`line` is wider than 400 points, so it takes the other path, the capture handed to Vision as it
is. They are imitations, drawn by FreeType rather than by macOS, and not a screenshot of any
plug-in.

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

# name, width and height in points, text, how it is drawn. Kept in step with FIXTURES in
# crates/host/src/ocr/bench.rs.
LAYOUTS = [
    ("field-64", 40, 20, "64", "well-centre"),
    ("field-DEF", 38, 22, "DEF", "well-centre"),
    ("lone-1", 20, 20, "1", "well-centre"),
    ("field-empty", 123, 23, "empty", "well-left"),
    ("line", 520, 24, "Instrument Polyphony Pitchbend Range Velocity Curve Release Time Volume", "page-left"),
]
SCALES = (1, 2)
# 16 pt Aileron: digits about 11 px tall at 1x by the content crop's own measure (see above).
FIELD_PT = 16
LINE_PT = 12
INSET_PT = 2


def draw(w_pt, h_pt, text, style, s):
    w, h = w_pt * s, h_pt * s
    if style.startswith("well"):
        img = Image.new("RGB", (w, h), CHROME)
        d = ImageDraw.Draw(img)
        inset = INSET_PT * s
        d.rectangle([inset, inset, w - inset - 1, h - inset - 1], fill=WELL)
        font = ImageFont.load_default(FIELD_PT * s)
        if style == "well-centre":
            d.text((w / 2, h / 2), text, font=font, fill=VALUE, anchor="mm")
        else:
            d.text((inset + 4 * s, h / 2), text, font=font, fill=VALUE, anchor="lm")
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
