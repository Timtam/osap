# avenger-presets: the preset database's converter

`convert.py` turns the avenger_control project's preset database into the data module [`modules/vps-avenger-presets`](../../modules/vps-avenger-presets), whose README describes the format it writes. Python 3.9 or later, standard library only.

```
python tools/avenger-presets/convert.py <folder>                    writes modules/vps-avenger-presets
python tools/avenger-presets/convert.py <folder> --version 1.1.0    and sets the module's version
python tools/avenger-presets/convert.py <folder> --out <folder>     writes somewhere else
python -m unittest discover -s tools/avenger-presets                 its tests, on a made-up export
```

`<folder>` holds the project's three exports: `AvengerXPDB_<date>_<time>.json` (the catalog), `AvengerOSCDB_…` (oscillator names) and `AvengerMacroDB_…` (macro names). Of each, the one with the latest date and time in its name is read. Nothing else in the folder is looked at, and the folder is never written to. The exports themselves do not belong in this repository: they hold more than the data module does (below).

## What it writes

Into the data module's folder: `module.toml`, `data/index.json` and one `data/expansions/<name>.json` per expansion. An expansion file an earlier run wrote and this one did not is removed. `src/main.luau`, `README.md` and `NOTICE` are written by hand and left alone. The version is `--version`, or the one `module.toml` already has, or 1.0.0; raise it whenever the data changes, so that the module manager's update check sees it. A folder whose `module.toml` belongs to another module is refused.

Every file is written in UTF-8 with LF line ends, one expansion or one preset per line, so that a new export's diff reads by expansion and by preset.

## What it keeps, and what it drops

The exports are more than a catalog. Beside the lists of what the catalog holds, the project's developer kept lists of which expansions particular people own, named after them. **None of that is anybody else's business, and none of it leaves the exports.**

- **Rosters kept — an allowlist.** Only two rosters are read: `Expansions Complete`, the catalog, and `Expansions Factory Standard`, what every owner of Avenger has.
- **Expansions kept — every one with a preset list** (the maintainer's decision of 2026-10-18): the catalog's, and those it does not name, with their categories, presets, oscillator names and macro names. The export dated 2026-09-26 has five the catalog does not name — Artist Series Nyhdroe, FairVenger CMII and its two add-on packs, Quantum State — whose names are product names, as the catalog's are. The index lists them in the catalog's order, each where that order puts it (`ordered()`: Factory and Factory 2 first, then alphabetical without regard to case, the catalog's own exceptions to that kept), and the summary names them:

  ```
  kept 5 expansion(s) the catalog does not name, each where its order puts it: Artist Series Nyhdroe, FairVenger CMII, …
  ```

- **Dropped — everything else.** Every other roster, the names in it that have no preset list, the tool's leftover "Init Preset" entry and the few FM routing keys. Their keys and names are never written: not to the data module, not to the screen, not into an error message. The converter says only how many there were:

  ```
  left out: 3 roster list(s) not on the allowlist; keys not read: 3 in the catalog export, 8 in the oscillator export, 7 in the macro export
  ```

  Those are the numbers of the export dated 2026-09-26: the three rosters' keys, and Init Preset's and the FM routing's.

Adding a roster to `ALLOW` in `convert.py` is the only way to keep more of the rosters, and a roster named after a person must never be added.

## What it checks

As the project's tool checks the database when it loads it, and more strictly, because this output ships:

- both allowlisted rosters are there, the catalog names no expansion twice, and Factory Standard is part of it (an expansion it names that the catalog does not is counted in the error, never named);
- every expansion of the catalog has a preset list, and every expansion with one — the catalog's or not, each checked as the catalog's are, by all of what follows — has a `PresetCount` that agrees with it;
- its categories' blocks — each from its first preset (`CategorieStart`, or `CategorieStartPosition` where a name repeats) for `CategorieCount` presets — cover the list exactly, with no gap and no overlap;
- every preset has 1 to 8 oscillators, each named `OSC <n>: <name>` for its own n, and 1 to 4 macro tabs of five names each;
- every value in the three files is a string.

**An error** — a count that disagrees, a category that does not start where the list says, a preset without its oscillators — means the data would be wrong: it is said on stderr, nothing is written, and the exit code is 1. **A warning** means the data can stand: an oscillator's name missing from its own key and taken from the preset's oscillator list (one preset in the 2026-09-26 export), or a preset name twice in one expansion, whose two entries then share one set of oscillator and macro names because their keys are the same (four in FairVenger CMII of that export, none in the catalog's). Errors and warnings name only expansion data: an expansion, a category, a preset — never a roster.

The run on the 2026-09-26 export: 129 expansions, 5 of them the catalog does not name, 18,446 presets, five warnings, 2.77 MB of data in 130 data files (the data module 1.1.0; 1.0.0 kept the catalog's 124 expansions, 17,571 presets).

## When a new export arrives

1. Put the three new files in a folder outside the repository.
2. `python tools/avenger-presets/convert.py <that folder> --version <the next version>`.
3. Read the summary: the warnings, the "kept" line and the "left out" counts. A new roster in "left out" is somebody's list and stays out. A new expansion in the "kept" line has a preset list but is not in the catalog: it is kept, its name written to the data module, so make sure it is a product's name, as the five so far are.
4. `git diff --stat modules/vps-avenger-presets` shows which expansions changed; a new expansion is a new file.
5. Run `cargo test -p host` (the overlay's scenarios use their own small catalog, so they do not change with the data) and the tests here.
