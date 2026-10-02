# -*- coding: utf-8 -*-
"""Turn the avenger_control project's preset database into the data module
modules/vps-avenger-presets.

The project's developer gave the database to this repository without restriction. It is three JSON
exports — a catalog of VPS Avenger's expansions, the oscillator tab names of every preset and
its macro names — in no format of Avenger's own: one flat object per file, every value a string,
lists separated by newlines, and the meaning in the key (`XP_<expansion>-<preset>OSCCount`). This
writes what VPS Avenger's overlay reads instead: a small index and one file per expansion, in the
format tools/avenger-presets/README.md and the data module's README describe.

    python tools/avenger-presets/convert.py <folder with the three exports>
    python tools/avenger-presets/convert.py <folder> --out <data module folder> --version 1.1.0

**What is kept is an allowlist.** The exports also carry lists of which expansions particular
people own, named after them, and expansions that come only through such a list. None of that is
anybody else's business: only the two rosters in ALLOW are read — the catalog, "Complete", and
"Factory Standard", what every owner of Avenger has — and only the expansions the catalog names.
Everything else is dropped without its key or its name ever being written: not to the output, not
to the screen, not to an error. Counts are said, never names.

**Checked as the project's tool checks it**, and more strictly, since this output ships: every
catalog expansion has its preset list, and its count agrees with it; its categories' blocks,
from each category's first preset and count, tile the list exactly; every preset has its
oscillators (1 to 8, each "OSC n: name") and its macro tabs (1 to 4, each five lines: three
knobs and two buttons). A problem that makes the data wrong is an error: nothing is written, and
the exit code is 1. One the data can stand — an oscillator name missing from its own key but in
the preset's oscillator list — is a warning, said with the preset's name (catalog data, never
personal).

Standard library only, so it runs wherever Python 3.9 or later does.
"""
import argparse
import json
import re
import sys
from pathlib import Path

# The format of the files written here; the data module's reader refuses any other
# (modules/vps-avenger-presets/src/main.luau, M.FORMAT). Raise both together.
FORMAT = 1

MODULE_ID = "com.platform.vps-avenger-presets"
# The data's licence and the reader's (src/main.luau, this repository's code): the developer gave
# the database to this repository without restriction, so both are the repository's (NOTICE).
LICENSE = "GPL-3.0-or-later"

# The only rosters read. Every other "Expansions …" key is somebody's own list and is dropped.
CATALOG = "Expansions Complete"
FACTORY = "Expansions Factory Standard"
ALLOW = (CATALOG, FACTORY)

SOURCES = {
    "xp": "AvengerXPDB_*.json",
    "osc": "AvengerOSCDB_*.json",
    "macro": "AvengerMacroDB_*.json",
}

MAX_OSC = 8
MAX_TABS = 4
KNOBS, BUTTONS = 3, 2

# Windows device names, which no file of a module may be named (docs/module-package-format.md,
# "Archives"), with or without an extension.
DEVICES = {"con", "prn", "aux", "nul"} | {f"com{i}" for i in range(10)} | {f"lpt{i}" for i in range(10)}


class Problems:
    """Errors and warnings, said at the end. Every text in here names catalog data only."""

    def __init__(self):
        self.errors = []
        self.warnings = []

    def error(self, text):
        self.errors.append(text)

    def warn(self, text):
        self.warnings.append(text)


def newest(folder, pattern):
    """The export matching `pattern` with the latest timestamp in its name. The project writes
    `AvengerXPDB_<YYYY-MM-DD_HH-MM-SS>.json`, so the name sorts as the time does."""
    found = sorted(p for p in folder.glob(pattern) if p.is_file())
    return found[-1] if found else None


def load(path):
    with path.open("r", encoding="utf-8-sig") as f:
        data = json.load(f)
    if not isinstance(data, dict):
        raise ValueError(f"{path.name} is not one JSON object")
    return data


def lines(value):
    """A newline-separated list, as the exports write one: empty lines dropped."""
    return [x for x in str(value).split("\n") if x != ""]


def number(value):
    text = str(value).strip()
    return int(text) if re.fullmatch(r"\d+", text) else None


def slug(name, taken):
    """A file name for expansion `name`: ASCII letters and digits, the rest one hyphen, unique
    among `taken` (compared without case, as Windows and a Mac's default volume compare)."""
    s = re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-") or "expansion"
    if s in DEVICES:
        s = "x-" + s
    base, n = s, 2
    while s in taken:
        s = f"{base}-{n}"
        n += 1
    taken.add(s)
    return s


def oscillators(osc, prefix, where, problems):
    """The tab names of one preset's oscillators, "OSC n: " dropped; "" for a tab that carries
    Avenger's default caption for its own place ("OSC 3" on the third), which says the sound
    designer did not name it. A name missing from its own key is taken from the preset's list
    (`OSCList`), with a warning; one missing from both is an error."""
    count = number(osc.get(prefix + "OSCCount", ""))
    if count is None or not 1 <= count <= MAX_OSC:
        problems.error(f"{where}: OSCCount is {osc.get(prefix + 'OSCCount')!r}, not 1 to {MAX_OSC}")
        return None
    listed = lines(osc.get(prefix + "OSCList", ""))
    if len(listed) != count:
        problems.warn(f"{where}: {len(listed)} lines in OSCList for {count} oscillators")
    out = []
    for n in range(1, count + 1):
        raw = osc.get(f"{prefix}OSC{n}Name")
        if raw is None:
            raw = next((x for x in listed if x.startswith(f"OSC {n}:")), None)
            if raw is None:
                problems.error(f"{where}: no name for oscillator {n}, in its key or in OSCList")
                return None
            problems.warn(f"{where}: oscillator {n}'s name taken from OSCList, its own key is missing")
        m = re.fullmatch(r"OSC (\d+):(.*)", raw, re.S)
        if not m or int(m.group(1)) != n:
            problems.error(f"{where}: oscillator {n} is {raw!r}, not 'OSC {n}: <name>'")
            return None
        name = m.group(2).strip()
        out.append("" if name == f"OSC {n}" else name)
    return out


def macros(macro, prefix, where, problems):
    """One preset's macro tabs, each five names — three knobs, then two buttons — with "" for a
    name that is Avenger's default for its place: knob k of tab t is "Macro <3(t-1)+k>", button b
    "MacroBtn <2(t-1)+b>", counted across the tabs as Avenger counts them. A default-looking name
    somewhere else ("Macro 6" on the first tab) is a name somebody gave it, and is kept."""
    count = number(macro.get(prefix + "-Macro TAB Count", ""))
    if count is None or not 1 <= count <= MAX_TABS:
        problems.error(f"{where}: Macro TAB Count is {macro.get(prefix + '-Macro TAB Count')!r}, not 1 to {MAX_TABS}")
        return None
    tabs = []
    for t in range(1, count + 1):
        raw = macro.get(f"{prefix}-Macro TAB {t} List")
        if raw is None:
            problems.error(f"{where}: no list for macro tab {t}")
            return None
        names = [x.strip() for x in str(raw).split("\n")]
        if len(names) != KNOBS + BUTTONS:
            problems.error(f"{where}: macro tab {t} has {len(names)} lines, not {KNOBS + BUTTONS}")
            return None
        tab = []
        for slot, name in enumerate(names, start=1):
            if slot <= KNOBS:
                default = f"Macro {(t - 1) * KNOBS + slot}"
            else:
                default = f"MacroBtn {(t - 1) * BUTTONS + slot - KNOBS}"
            tab.append("" if name == default else name)
        tabs.append(tab)
    return tabs


def categories(xp, expansion, presets, problems):
    """The expansion's categories in the order the export lists them (alphabetical), each with
    the 1-based place of its first preset in `presets` and how many it has. Their blocks must
    tile the preset list: every preset in exactly one category."""
    listed = lines(xp.get(f"XP {expansion}", ""))
    if not listed:
        problems.error(f"{expansion}: no categories")
        return None
    out, spans = [], []
    for name in listed:
        base = f"XP_{expansion}-{name}Categorie"
        count = number(xp.get(base + "Count", ""))
        if count is None:
            problems.error(f"{expansion}/{name}: CategorieCount is {xp.get(base + 'Count')!r}")
            return None
        start = None
        if count > 0:
            position = xp.get(base + "StartPosition")
            first = xp.get(base + "Start")
            if position not in (None, ""):
                p = number(position)
                if p is None or not 1 <= p <= len(presets):
                    problems.error(f"{expansion}/{name}: CategorieStartPosition {position!r} is outside the list")
                    return None
                start = p
                if first is not None and presets[p - 1] != first:
                    problems.error(f"{expansion}/{name}: position {p} holds {presets[p - 1]!r}, not {first!r}")
                    return None
            elif first is not None and first in presets:
                start = presets.index(first) + 1
            else:
                problems.error(f"{expansion}/{name}: its first preset {first!r} is not in the list")
                return None
            spans.append((start, count, name))
        out.append({"name": name, "start": start or 0, "count": count})
    at = 1
    for start, count, name in sorted(spans):
        if start != at:
            problems.error(f"{expansion}: category {name} starts at {start}, where {at} was expected")
            return None
        at += count
    if at != len(presets) + 1:
        problems.error(f"{expansion}: the categories hold {at - 1} presets, the list {len(presets)}")
        return None
    return out


def convert(xp, osc, macro, problems):
    """The index and the expansions' files, as Python values, or None after an error."""
    for key in ALLOW:
        if not isinstance(xp.get(key), str) or not lines(xp[key]):
            problems.error(f"the roster {key!r} is missing or empty")
            return None
    catalog = lines(xp[CATALOG])
    factory = lines(xp[FACTORY])
    if len(set(catalog)) != len(catalog):
        problems.error("the catalog names an expansion twice")
        return None
    # Counted, not named: an expansion the catalog does not name is never written anywhere.
    missing = sum(1 for e in factory if e not in catalog)
    if missing:
        problems.error(f"Factory Standard names {missing} expansion(s) the catalog does not")
        return None
    taken, expansions, files = set(), [], {}
    for expansion in catalog:
        presets = lines(xp.get(f"XP-{expansion}PresetList", ""))
        if not presets:
            problems.error(f"{expansion}: no preset list")
            continue
        count = number(xp.get(f"XP-{expansion}PresetCount", ""))
        if count != len(presets):
            problems.error(f"{expansion}: PresetCount is {xp.get(f'XP-{expansion}PresetCount')!r}, "
                           f"the list has {len(presets)}")
            continue
        seen = {}
        for name in presets:
            seen[name] = seen.get(name, 0) + 1
        for name, n in seen.items():
            if n > 1:
                problems.warn(f"{expansion}: {name!r} is in its list {n} times; they share one "
                              "oscillator and macro entry, as their keys are the same")
        cats = categories(xp, expansion, presets, problems)
        entries = []
        for i, name in enumerate(presets, start=1):
            where = f"{expansion}/{name} (preset {i})"
            prefix = f"XP_{expansion}-{name}"
            o = oscillators(osc, prefix, where, problems)
            m = macros(macro, prefix, where, problems)
            if o is None or m is None:
                continue
            entries.append({"name": name, "osc": o, "macros": m})
        if cats is None or len(entries) != len(presets):
            continue
        rel = f"data/expansions/{slug(expansion, taken)}.json"
        expansions.append({"name": expansion, "file": rel, "presets": presets})
        files[rel] = {"format": FORMAT, "name": expansion, "categories": cats, "presets": entries}
    if problems.errors:
        return None
    return {"format": FORMAT, "factoryStandard": factory, "expansions": expansions}, files


def dropped(xp, catalog):
    """How much was left out, in counts: the rosters not on the allowlist, and the expansions with
    data that the catalog does not name. No key and no name leaves this function."""
    rosters = sum(1 for k in xp if k.startswith("Expansions ") and k not in ALLOW)
    lists = set()
    for k in xp:
        m = re.fullmatch(r"XP-(.*)PresetList", k, re.S)
        if m:
            lists.add(m.group(1))
    outside = len(lists - set(catalog))
    return rosters, outside


def used_keys(xp, osc, macro, index):
    """How many keys of each export the conversion did not read, counted, never listed. A macro's
    own name key (`-Macro <n> Name`, `-Macro Button <n> Name`) repeats a line of its tab's list,
    which is what is read, and counts as read."""
    used = {"xp": set(ALLOW), "osc": set(), "macro": set()}
    for e in index["expansions"]:
        name = e["name"]
        used["xp"].update({f"XP-{name}PresetList", f"XP-{name}PresetCount", f"XP-{name}PresetIndex",
                           f"XP {name}"})
        for c in lines(xp.get(f"XP {name}", "")):
            for suffix in ("Count", "Start", "StartPosition"):
                used["xp"].add(f"XP_{name}-{c}Categorie{suffix}")
        for p in e["presets"]:
            prefix = f"XP_{name}-{p}"
            used["osc"].update({prefix + "OSCCount", prefix + "OSCList"})
            used["osc"].update(f"{prefix}OSC{n}Name" for n in range(1, MAX_OSC + 1))
            used["macro"].add(prefix + "-Macro TAB Count")
            for t in range(1, MAX_TABS + 1):
                used["macro"].add(f"{prefix}-Macro TAB {t} List")
            used["macro"].update(f"{prefix}-Macro {n} Name" for n in range(1, MAX_TABS * KNOBS + 1))
            used["macro"].update(f"{prefix}-Macro Button {n} Name" for n in range(1, MAX_TABS * BUTTONS + 1))
    return {
        "xp": sum(1 for k in xp if k not in used["xp"]),
        "osc": sum(1 for k in osc if k not in used["osc"]),
        "macro": sum(1 for k in macro if k not in used["macro"]),
    }


def dump(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def index_text(index, exported):
    """index.json: one expansion per line, so a new export's diff reads by expansion."""
    head = [f'"format":{FORMAT}', f'"exported":{dump(exported)}',
            f'"factoryStandard":{dump(index["factoryStandard"])}']
    rows = ",\n".join(dump(e) for e in index["expansions"])
    return "{" + ",\n".join(head) + ',\n"expansions":[\n' + rows + "\n]}\n"


def expansion_text(x):
    """An expansion's file: one preset per line."""
    head = [f'"format":{FORMAT}', f'"name":{dump(x["name"])}', f'"categories":{dump(x["categories"])}']
    rows = ",\n".join(dump(p) for p in x["presets"])
    return "{" + ",\n".join(head) + ',\n"presets":[\n' + rows + "\n]}\n"


def manifest(version, exported):
    return (
        "# Written by tools/avenger-presets/convert.py: change that, not this file.\n"
        f'id = "{MODULE_ID}"\n'
        'name = "VPS Avenger preset database"\n'
        f'version = "{version}"\n'
        f'license = "{LICENSE}"\n'
        "# A code module, so that its reader (src/main.luau) runs in the VM of the module that uses\n"
        "# it and reads this module's own files: com.platform.vps-avenger, as an optional dependency.\n"
        "code_module = true\n"
        f"# The catalog of VPS Avenger's expansions, exported {exported} from the avenger_control\n"
        "# project's preset database, given by its developer to this repository without\n"
        "# restriction: see NOTICE and README.md. No supported_os: it is data, the same on both\n"
        "# systems.\n"
        "\n"
        "[capabilities]\n"
        "# src/main.luau reads this module's own data files, and nothing else.\n"
        'require = ["resource"]\n'
    )


def exported_from(path):
    m = re.search(r"_(\d{4}-\d{2}-\d{2})_(\d{2})-(\d{2})-\d{2}\.json$", path.name)
    return f"{m.group(1)} {m.group(2)}:{m.group(3)}" if m else "unknown"


def existing_version(out):
    toml = out / "module.toml"
    if not toml.is_file():
        return None
    text = toml.read_text(encoding="utf-8")
    ident = re.search(r'^id\s*=\s*"([^"]*)"', text, re.M)
    if ident and ident.group(1) != MODULE_ID:
        raise SystemExit(f"{toml} belongs to {ident.group(1)!r}, not {MODULE_ID!r}: not writing there")
    version = re.search(r'^version\s*=\s*"([^"]*)"', text, re.M)
    return version.group(1) if version else None


def write(out, index, files, version, exported):
    """Writes module.toml and data/; the hand-written files (src/, README.md, NOTICE) are left as
    they are. An expansion file of an earlier run that this one did not write is removed."""
    folder = out / "data" / "expansions"
    folder.mkdir(parents=True, exist_ok=True)
    for old in folder.glob("*.json"):
        if f"data/expansions/{old.name}" not in files:
            old.unlink()
    for rel, x in files.items():
        (out / rel).write_text(expansion_text(x), encoding="utf-8", newline="\n")
    (out / "data" / "index.json").write_text(index_text(index, exported), encoding="utf-8", newline="\n")
    (out / "module.toml").write_text(manifest(version, exported), encoding="utf-8", newline="\n")


def main(argv=None):
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("source", type=Path, help="the folder holding the three exports")
    parser.add_argument("--out", type=Path, default=here.parent.parent / "modules" / "vps-avenger-presets",
                        help="the data module's folder (default: modules/vps-avenger-presets)")
    parser.add_argument("--version", help="the data module's version (default: the one it has, or 1.0.0)")
    args = parser.parse_args(argv)

    paths = {k: newest(args.source, pat) for k, pat in SOURCES.items()}
    for k, p in paths.items():
        if p is None:
            print(f"no {SOURCES[k]} in {args.source}", file=sys.stderr)
            return 1
    xp, osc, macro = (load(paths[k]) for k in ("xp", "osc", "macro"))
    for name, data in (("xp", xp), ("osc", osc), ("macro", macro)):
        if not all(isinstance(v, str) for v in data.values()):
            print(f"{paths[name].name}: a value that is not a string", file=sys.stderr)
            return 1

    problems = Problems()
    result = convert(xp, osc, macro, problems)
    for w in problems.warnings:
        print("warning: " + w)
    if result is None:
        for e in problems.errors:
            print("error: " + e, file=sys.stderr)
        print(f"{len(problems.errors)} error(s): nothing written", file=sys.stderr)
        return 1
    index, files = result
    exported = exported_from(paths["xp"])
    version = args.version or existing_version(args.out) or "1.0.0"
    write(args.out, index, files, version, exported)

    rosters, outside = dropped(xp, lines(xp[CATALOG]))
    unused = used_keys(xp, osc, macro, index)
    presets = sum(len(e["presets"]) for e in index["expansions"])
    size = sum(p.stat().st_size for p in (args.out / "data").rglob("*") if p.is_file())
    print(f"read {', '.join(p.name for p in paths.values())}")
    print(f"wrote {len(index['expansions'])} expansions, {presets} presets: data/index.json and "
          f"{len(files)} expansion files, {size / 1024:.0f} KB, and module.toml (version {version}) "
          f"in {args.out}")
    print(f"left out: {rosters} roster list(s) not on the allowlist, {outside} expansion(s) the "
          f"catalog does not name; keys not read: {unused['xp']} in the catalog export, "
          f"{unused['osc']} in the oscillator export, {unused['macro']} in the macro export")
    print(f"{len(problems.warnings)} warning(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
