# -*- coding: utf-8 -*-
"""Cut the real pictures `automation-platform ocr-bench --pictures` reads, out of calibration shots.

    python tools/ocr-fixtures/crop.py                      cut both sets from the shots
    python tools/ocr-fixtures/crop.py --check              say whether the written files are what
                                                           the shots give, write nothing
    python tools/ocr-fixtures/crop.py --calibration DIR    take the shots from DIR

real.toml, beside this file, lists every picture: the calibration shot, where the module's frame
starts in it, the region as the module has it, the answers that are right, whether it must be read,
and how the answers were checked. The shots are the overlay runtime's calibration output, which is
git-ignored (modules/*/calibration/), so only a machine that has them can cut these again; by
default they are looked for in modules/overlay-runtime/calibration under the repository.

Two sets, each a folder with a manifest.toml that `--pictures` reads:
  ci     crates/host/bench-data/ocr/real/ — value fields and plain control words only, committed,
         with a NOTICE naming the plug-ins they come from, which CI reads with --pictures;
  local  crates/host/tests-data-local/ocr-real/ — wordmarks, logos, preset and instrument names,
         git-ignored, for this machine and for a tester's Mac.
Each picture is written as <name>@1x.png: the shots are Windows captures at 1x, so a pixel is a
point. A file in a set's folder that real.toml does not list is removed, so the folder and its
manifest agree (ocr-bench's tests check that they do).

Needs Python 3.11 or later (tomllib) and Pillow.
"""
import argparse
import io
import sys
import tomllib
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
LIST = HERE / "real.toml"
SETS = {
    "ci": ROOT / "crates" / "host" / "bench-data" / "ocr" / "real",
    "local": ROOT / "crates" / "host" / "tests-data-local" / "ocr-real",
}
DEFAULT_SHOTS = ROOT / "modules" / "overlay-runtime" / "calibration"


def toml_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def manifest(entries, plugins):
    out = [
        "# What each picture in this folder is, for `automation-platform ocr-bench --pictures`.",
        "# Written by tools/ocr-fixtures/crop.py from tools/ocr-fixtures/real.toml; change that and",
        "# run it again rather than editing this. Each is <name>@<scale>x.png beside this file.",
        "",
    ]
    for e in entries:
        x1, y1, x2, y2 = e["region"]
        ox, oy = e["origin"]
        source = (
            f"{plugins[e['plugin']]}: {e['what']}. Cut from the Windows calibration shot {e['shot']}, "
            f"region {x1},{y1},{x2},{y2} from ({ox},{oy})"
        )
        out += [
            "[[picture]]",
            f"name = {toml_str(e['name'])}",
            "scale = 1",
            f"w_pt = {x2 - x1}",
            f"h_pt = {y2 - y1}",
            "accept = [" + ", ".join(toml_str(a) for a in e["accept"]) + "]",
            f"must_read = {'true' if e['must_read'] else 'false'}",
            'platform = "windows"',
            f"source = {toml_str(source)}",
            f"checked = {toml_str(e['checked'])}",
            "",
        ]
    return "\n".join(out)


def notice(entries, plugins):
    names = []
    for e in entries:
        if plugins[e["plugin"]] not in names:
            names.append(plugins[e["plugin"]])
    lines = [
        "The pictures in this folder are small crops of screenshots of commercial audio plug-ins,",
        "captured on Windows:",
        "",
        *[f"  - {n}" for n in names],
        "",
        "Each shows one value field or one plain control word, as a plug-in draws it, and nothing else:",
        "no wordmark, logo, preset or instrument name. They are here only as test input for the",
        "text-recognition benchmark (`automation-platform ocr-bench --pictures`), which measures how",
        "well the recognisers read what an accessibility overlay reads from these plug-ins.",
        "",
        "This project is not affiliated with, endorsed by or sponsored by the makers of these",
        "plug-ins. The plug-ins, their names and their interfaces belong to their makers. The pictures",
        "are not covered by this repository's licence. tools/ocr-fixtures/real.toml in the source",
        "repository, https://github.com/Timtam/osap, says where each one was cut from.",
        "",
    ]
    return "\n".join(lines)


def png_bytes(img):
    buf = io.BytesIO()
    img.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def check_entry(e, plugins):
    for key in ("name", "set", "plugin", "shot", "origin", "region", "accept", "must_read", "what", "checked"):
        if key not in e:
            raise SystemExit(f"real.toml: {e.get('name', '?')}: '{key}' is missing")
    if e["set"] not in SETS:
        raise SystemExit(f"real.toml: {e['name']}: set '{e['set']}' is neither ci nor local")
    if e["plugin"] not in plugins:
        raise SystemExit(f"real.toml: {e['name']}: plugin '{e['plugin']}' is not in [plugins]")
    x1, y1, x2, y2 = e["region"]
    if x2 <= x1 or y2 <= y1:
        raise SystemExit(f"real.toml: {e['name']}: the region {e['region']} is empty")
    if e["must_read"] and any(a.strip() == "" for a in e["accept"]):
        raise SystemExit(f"real.toml: {e['name']}: a picture that must be read cannot accept nothing")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--check", action="store_true", help="compare with the written files, write nothing")
    ap.add_argument("--calibration", type=Path, default=DEFAULT_SHOTS, help="the folder of calibration shots")
    args = ap.parse_args()
    data = tomllib.loads(LIST.read_text(encoding="utf-8"))
    plugins = data["plugins"]
    entries = data["picture"]
    names = [e["name"] for e in entries]
    if len(set(names)) != len(names):
        raise SystemExit("real.toml: a name is listed twice")
    for e in entries:
        check_entry(e, plugins)
    missing = sorted({e["shot"] for e in entries if not (args.calibration / e["shot"]).is_file()})
    if missing:
        print(f"no calibration shot {', '.join(missing)} in {args.calibration}; nothing cut")
        return 2
    differ = 0
    for set_name, folder in SETS.items():
        these = [e for e in entries if e["set"] == set_name]
        files = {}
        for e in these:
            x1, y1, x2, y2 = e["region"]
            ox, oy = e["origin"]
            with Image.open(args.calibration / e["shot"]) as shot:
                if ox + x2 > shot.width or oy + y2 > shot.height:
                    raise SystemExit(f"{e['name']}: the region leaves {e['shot']} ({shot.width}x{shot.height})")
                img = shot.convert("RGB").crop((ox + x1, oy + y1, ox + x2, oy + y2))
            files[f"{e['name']}@1x.png"] = img
        texts = {"manifest.toml": manifest(these, plugins)}
        if set_name == "ci":
            texts["NOTICE"] = notice(these, plugins)
        if args.check:
            for name, img in files.items():
                path = folder / name
                if not path.exists():
                    print(f"missing: {set_name}/{name}")
                    differ += 1
                    continue
                with Image.open(path) as old:
                    if old.size != img.size or old.convert("RGB").tobytes() != img.tobytes():
                        print(f"differs: {set_name}/{name}")
                        differ += 1
            for name, text in texts.items():
                path = folder / name
                if not path.exists() or path.read_text(encoding="utf-8") != text:
                    print(f"differs: {set_name}/{name}")
                    differ += 1
            if folder.exists():
                for path in folder.iterdir():
                    if path.suffix == ".png" and path.name not in files:
                        print(f"not in real.toml: {set_name}/{path.name}")
                        differ += 1
            continue
        folder.mkdir(parents=True, exist_ok=True)
        for path in folder.iterdir():
            if path.suffix == ".png" and path.name not in files:
                path.unlink()
                print(f"removed {path.relative_to(ROOT).as_posix()}, which real.toml does not list")
        for name, img in files.items():
            (folder / name).write_bytes(png_bytes(img))
            print(f"wrote {(folder / name).relative_to(ROOT).as_posix()} ({img.width}x{img.height} px)")
        for name, text in texts.items():
            (folder / name).write_text(text, encoding="utf-8", newline="\n")
            print(f"wrote {(folder / name).relative_to(ROOT).as_posix()}")
    if args.check:
        print("every picture is what the shots give" if differ == 0 else f"{differ} file(s) differ")
        return 1 if differ else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
