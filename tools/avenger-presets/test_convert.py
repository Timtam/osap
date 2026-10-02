# -*- coding: utf-8 -*-
"""The converter, run on a made-up export of three expansions — never the real one.

    python -m unittest discover -s tools/avenger-presets

The made-up export has what the real one has: the two rosters that are kept, and one that must
never leave it — a list of what one owner has, with an expansion that comes only through it. What
these check first is that nothing of that list, not its key, not its name, not its expansion,
reaches the data module or anything the converter prints.
"""
import contextlib
import io
import json
import re
import tempfile
import unittest
from pathlib import Path

import convert

# Stand-ins for the kind of thing the converter must drop. No real person is named anywhere here.
OWNER_ROSTER = "Expansions Owner Seven"
PRIVATE = "Private Pack"


def export(**changes):
    """The three made-up exports as dicts: Alpha (four presets in two category blocks, the second
    before the first alphabetically), Beta (two), and PRIVATE only in OWNER_ROSTER."""
    xp = {
        "Expansions Complete": "Alpha\nBeta",
        "Expansions Factory Standard": "Alpha",
        OWNER_ROSTER: f"Alpha\n{PRIVATE}",
        "XP Alpha": "Arp\nBass",
        "XP-AlphaPresetList": "BA One\nBA Two\nAR One\nAR Two",
        "XP-AlphaPresetCount": "4",
        "XP-AlphaPresetIndex": "0",
        "XP_Alpha-ArpCategorieStart": "AR One",
        "XP_Alpha-ArpCategorieCount": "2",
        "XP_Alpha-BassCategorieStart": "BA One",
        "XP_Alpha-BassCategorieCount": "2",
        "XP Beta": "Pads",
        "XP-BetaPresetList": "PD Palm´s Pad\nPD Plain ",
        "XP-BetaPresetCount": "2",
        "XP_Beta-PadsCategorieStart": "PD Palm´s Pad",
        "XP_Beta-PadsCategorieCount": "2",
        f"XP {PRIVATE}": "Leads",
        f"XP-{PRIVATE}PresetList": "LD Secret",
        f"XP-{PRIVATE}PresetCount": "1",
        f"XP_{PRIVATE}-LeadsCategorieStart": "LD Secret",
        f"XP_{PRIVATE}-LeadsCategorieCount": "1",
    }
    osc, macro = {}, {}
    presets = {"Alpha": ["BA One", "BA Two", "AR One", "AR Two"], "Beta": ["PD Palm´s Pad", "PD Plain "],
               PRIVATE: ["LD Secret"]}
    for e, names in presets.items():
        for i, n in enumerate(names):
            p = f"XP_{e}-{n}"
            osc[p + "OSCCount"] = "2"
            osc[p + "OSCList"] = f"OSC 1: Sub {i}\nOSC 2: OSC 2"
            osc[p + "OSC1Name"] = f"OSC 1: Sub {i}"
            osc[p + "OSC2Name"] = "OSC 2: OSC 2"
            macro[p + "-Macro TAB Count"] = "1"
            macro[p + "-Macro TAB 1 List"] = "Tone\nMacro 2\nMacro 6\nMacroBtn 1\nWide"
    for name, value in changes.items():
        target, key = name.split(":", 1)
        {"xp": xp, "osc": osc, "macro": macro}[target][key] = value
    return xp, osc, macro


def run(xp, osc, macro, *extra):
    """Writes the exports to a folder, converts them into another, and gives back the exit code,
    what was printed, and the output folder's files as {relative path: text}."""
    src = tempfile.TemporaryDirectory()
    out = tempfile.TemporaryDirectory()
    folder, dest = Path(src.name), Path(out.name)
    for prefix, data in (("AvengerXPDB", xp), ("AvengerOSCDB", osc), ("AvengerMacroDB", macro)):
        (folder / f"{prefix}_2026-09-26_18-57-03.json").write_text(json.dumps(data), encoding="utf-8")
    printed = io.StringIO()
    with contextlib.redirect_stdout(printed), contextlib.redirect_stderr(printed):
        code = convert.main([str(folder), "--out", str(dest), *extra])
    files = {p.relative_to(dest).as_posix(): p.read_text(encoding="utf-8") for p in dest.rglob("*") if p.is_file()}
    src.cleanup()
    out.cleanup()
    return code, printed.getvalue(), files


class Converter(unittest.TestCase):
    def test_the_owner_list_and_what_only_it_has_never_leave_the_export(self):
        code, printed, files = run(*export())
        self.assertEqual(code, 0, printed)
        everything = printed + "".join(files.values()) + "".join(files)
        for secret in (OWNER_ROSTER, "Owner Seven", PRIVATE, "LD Secret", "private-pack"):
            self.assertNotIn(secret, everything)
        self.assertIn("left out: 1 roster list(s) not on the allowlist, 1 expansion(s) the catalog "
                      "does not name", printed)

    def test_the_format(self):
        code, printed, files = run(*export())
        self.assertEqual(code, 0, printed)
        self.assertEqual(sorted(files), ["data/expansions/alpha.json", "data/expansions/beta.json",
                                         "data/index.json", "module.toml"])
        index = json.loads(files["data/index.json"])
        self.assertEqual(index["format"], convert.FORMAT)
        self.assertEqual(index["exported"], "2026-09-26 18:57")
        self.assertEqual(index["factoryStandard"], ["Alpha"])
        self.assertEqual([e["name"] for e in index["expansions"]], ["Alpha", "Beta"])
        self.assertEqual(index["expansions"][1]["presets"], ["PD Palm´s Pad", "PD Plain "])
        alpha = json.loads(files["data/expansions/alpha.json"])
        self.assertEqual(alpha["categories"], [{"name": "Arp", "start": 3, "count": 2},
                                               {"name": "Bass", "start": 1, "count": 2}])
        first = alpha["presets"][0]
        self.assertEqual(first["name"], "BA One")
        # "OSC 2" on the second tab is Avenger's default, so empty; "Macro 6" on the first tab's
        # third knob is not that knob's default ("Macro 3"), so kept.
        self.assertEqual(first["osc"], ["Sub 0", ""])
        self.assertEqual(first["macros"], [["Tone", "", "Macro 6", "", "Wide"]])
        # One preset per line.
        lines = files["data/expansions/alpha.json"].split("\n")
        self.assertEqual(sum(1 for line in lines if line.startswith('{"name":')), 4)
        self.assertIn('id = "com.platform.vps-avenger-presets"', files["module.toml"])
        self.assertIn('version = "1.0.0"', files["module.toml"])
        self.assertIn('license = "GPL-3.0-or-later"', files["module.toml"])

    def test_the_readers_format_is_the_converters(self):
        reader = Path(__file__).resolve().parents[2] / "modules" / "vps-avenger-presets" / "src" / "main.luau"
        m = re.search(r"^M\.FORMAT = (\d+)$", reader.read_text(encoding="utf-8"), re.M)
        self.assertIsNotNone(m)
        self.assertEqual(int(m.group(1)), convert.FORMAT)

    def test_a_missing_name_is_taken_from_the_list_with_a_warning(self):
        xp, osc, macro = export()
        del osc["XP_Alpha-BA TwoOSC1Name"]
        code, printed, files = run(xp, osc, macro)
        self.assertEqual(code, 0, printed)
        self.assertIn("warning: Alpha/BA Two (preset 2): oscillator 1's name taken from OSCList", printed)
        self.assertEqual(json.loads(files["data/expansions/alpha.json"])["presets"][1]["osc"], ["Sub 1", ""])

    def test_an_error_writes_nothing(self):
        for change, message in (
            ({"xp:XP-AlphaPresetCount": "5"}, "Alpha: PresetCount is '5', the list has 4"),
            ({"xp:XP_Alpha-ArpCategorieCount": "1"}, "Alpha: the categories hold 3 presets, the list 4"),
            ({"xp:XP_Alpha-BassCategorieStart": "BA Two"}, "Alpha: category Bass starts at 2, where 1 was expected"),
            ({"osc:XP_Beta-PD Plain OSCCount": "9"}, "Beta/PD Plain  (preset 2): OSCCount is '9', not 1 to 8"),
            ({"macro:XP_Alpha-AR One-Macro TAB 1 List": "Tone\nDrive"}, "macro tab 1 has 2 lines, not 5"),
            ({"xp:Expansions Factory Standard": "Alpha\nGamma"}, "Factory Standard names 1 expansion(s) the catalog does not"),
        ):
            with self.subTest(message=message):
                code, printed, files = run(*export(**change))
                self.assertEqual(code, 1, printed)
                self.assertIn(message, printed)
                self.assertIn("nothing written", printed)
                self.assertEqual(files, {})
                # Not even an expansion of an allowed roster, when the catalog does not name it.
                for secret in (PRIVATE, "Gamma"):
                    self.assertNotIn(secret, printed)

    def test_a_name_twice_in_one_expansion_and_a_category_placed_by_position(self):
        # Alpha's list with "BA One" again at its end, as the first preset of a category of its
        # own: the export then says by place where that category starts (CategorieStartPosition),
        # since the name alone would point at the first "BA One". None in the real catalog.
        changes = {
            "xp:XP Alpha": "Arp\nBass\nExtra",
            "xp:XP-AlphaPresetList": "BA One\nBA Two\nAR One\nAR Two\nBA One",
            "xp:XP-AlphaPresetCount": "5",
            "xp:XP_Alpha-ExtraCategorieStart": "BA One",
            "xp:XP_Alpha-ExtraCategorieStartPosition": "5",
            "xp:XP_Alpha-ExtraCategorieCount": "1",
        }
        code, printed, files = run(*export(**changes))
        self.assertEqual(code, 0, printed)
        self.assertIn("warning: Alpha: 'BA One' is in its list 2 times; they share one oscillator and macro "
                      "entry, as their keys are the same", printed)
        alpha = json.loads(files["data/expansions/alpha.json"])
        self.assertEqual(alpha["categories"], [{"name": "Arp", "start": 3, "count": 2},
                                               {"name": "Bass", "start": 1, "count": 2},
                                               {"name": "Extra", "start": 5, "count": 1}])
        self.assertEqual([p["name"] for p in alpha["presets"]], ["BA One", "BA Two", "AR One", "AR Two", "BA One"])
        self.assertEqual(alpha["presets"][4]["osc"], alpha["presets"][0]["osc"])
        # A place that does not hold the category's first preset is an error, and writes nothing.
        changes["xp:XP_Alpha-ExtraCategorieStartPosition"] = "4"
        code, printed, files = run(*export(**changes))
        self.assertEqual(code, 1, printed)
        self.assertIn("Alpha/Extra: position 4 holds 'AR Two', not 'BA One'", printed)
        self.assertEqual(files, {})

    def test_the_version_is_kept_or_given(self):
        xp, osc, macro = export()
        with tempfile.TemporaryDirectory() as d:
            src = Path(d) / "src"
            out = Path(d) / "out"
            src.mkdir()
            for prefix, data in (("AvengerXPDB", xp), ("AvengerOSCDB", osc), ("AvengerMacroDB", macro)):
                (src / f"{prefix}_2026-09-26_18-57-03.json").write_text(json.dumps(data), encoding="utf-8")
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(convert.main([str(src), "--out", str(out), "--version", "1.2.0"]), 0)
                self.assertEqual(convert.main([str(src), "--out", str(out)]), 0)
            self.assertIn('version = "1.2.0"', (out / "module.toml").read_text(encoding="utf-8"))
            (out / "module.toml").write_text('id = "com.example.other"\n', encoding="utf-8")
            with self.assertRaises(SystemExit):
                with contextlib.redirect_stdout(io.StringIO()):
                    convert.main([str(src), "--out", str(out)])


if __name__ == "__main__":
    unittest.main()
