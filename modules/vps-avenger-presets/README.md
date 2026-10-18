# VPS Avenger preset database

`com.platform.vps-avenger-presets`: the catalog of VPS Avenger's expansions as data — the 124 the project's catalog lists, and since 1.1.0 the five more its export has preset lists for: Artist Series Nyhdroe, FairVenger CMII and its two add-on packs, and Quantum State. For every one of the 129 expansions, its categories and presets; for every one of its 18,446 presets, the names of its oscillator tabs and of its macros. The overlay module [`vps-avenger`](../vps-avenger) reads it when it is installed, and works without it when it is not.

It comes from the preset database of the avenger_control project, whose developer gave it to this repository without restriction. It is licensed as the rest of the repository, GPL-3.0-or-later: see [NOTICE](NOTICE).

The export it was made from is dated 2026-09-26. An expansion released after that is not in it.

## What is in it

| Path | What |
|---|---|
| `module.toml` | Written by [`tools/avenger-presets/convert.py`](../../tools/avenger-presets), with the data. |
| `src/main.luau` | The reader. Hand-written; the only code that knows the format. |
| `data/index.json` | The expansions in the catalog's order, each with its file and its preset names. 0.35 MB. |
| `data/expansions/<name>.json` | One file per expansion: its categories, and every preset's oscillators and macros. 2 to 110 KB each. |
| `NOTICE` | Where the data comes from, and its licence. |

134 files; the 130 of data are 2.77 MB. Nothing here says who owns which expansion: of the exports' rosters the converter keeps the catalog's and nothing else, and of their expansions every one with a preset list (`tools/avenger-presets/README.md`).

## The format (format 1)

Every file is one JSON object in UTF-8, written by the converter with one expansion or one preset per line, so that a new export's diff reads by expansion and by preset. Expansion, category and preset names are exactly as the export has them: a trailing space, two spaces in a row and the acute accent `´` used as an apostrophe are kept, because they are the names Avenger shows. Oscillator and macro names are trimmed at both ends, as the project's tool trims them. A reader that compares them with text read off the screen evens those out itself (`modules/vps-avenger/src/presets.luau`).

**`data/index.json`**

```json
{"format":1,
"exported":"2026-09-26 18:57",
"factoryStandard":["Factory","Factory 2","Synthstruments"],
"expansions":[
{"name":"Factory","file":"data/expansions/factory.json","presets":["AR A State of Trance", …]},
…
]}
```

- `format` — 1. The reader refuses any other number.
- `exported` — when the export was made, from its file name.
- `factoryStandard` — the expansions every owner of Avenger has.
- `expansions` — every expansion, in the catalog's order: Factory and Factory 2 first, the rest alphabetical without regard to case. The five the project's catalog does not name are where that rule puts them (Quantum State between Psy 2 and Retrowave 80s), and so is the list "Complete" the overlay shows; where the catalog's own order is not alphabetical — 8 Bit Era before 16 Bit Era, Trance One to Trance Four by their numbers — it is kept. `file` is the expansion's file, relative to this folder. `presets` are its preset names in the export's order — **category block after category block, and alphabetical within a block; not alphabetical overall**. That is the order the project's tool steps through with ◀ and ▶; that Avenger does the same is what the overlay's log checks (TODO.md, "VPS Avenger").

**`data/expansions/<name>.json`**

```json
{"format":1,
"name":"Factory",
"categories":[{"name":"Arp","start":1,"count":96},{"name":"Bass","start":97,"count":136}, …],
"presets":[
{"name":"AR A State of Trance","osc":["Arp","Bassline"],"macros":[["Fatter","Shape","Kick & SC","Variation","Pitch Slide"]]},
…
]}
```

- `name` — the expansion, as in the index. The file's name is made from it: ASCII letters and digits, the rest a hyphen (`Effects: EDM` is `effects-edm.json`).
- `categories` — in the export's order, which is alphabetical. `start` is the place of the category's first preset in `presets`, counted from 1, and `count` how many follow it; the blocks cover `presets` exactly, so every preset is in one category. An empty category has `start` 0.
- `presets` — the same presets in the same order as the index lists them, so the i-th here is the i-th there. A name can be in one list twice (four are, in FairVenger CMII's): the export keeps one set of oscillator and macro names per name, so both have the same. Each has:
  - `osc` — the oscillator tabs' names, 1 to 8, in tab order, the export's `OSC n:` taken off.
  - `macros` — the macro tabs, 1 to 4, each five names: three knobs, then two buttons.

**An empty name is a default one**: the sound designer did not name that tab or macro, and Avenger shows its own caption there. That caption is `OSC <n>` for the n-th oscillator; for macros it counts across the tabs — knob k of tab t is `Macro <3(t-1)+k>`, button b is `MacroBtn <2(t-1)+b>`. A name that looks like a default but is not the default of its own place — `Macro 6` on the first tab — is a name somebody gave it, and is kept as written.

## Reading it

The module is a `code_module`, so that its reader runs inside the VM of the module that uses it and still reads **this** folder: `host.resource` resolves under the module whose code makes the call ([the path rule](../../docs/api/require.md)). A dependent lists it under `optional_dependencies` and asks `host.tryRequire("com.platform.vps-avenger-presets")`, which is `nil` when it is not installed:

```luau
local P = host.tryRequire("com.platform.vps-avenger-presets")
if P then
  local index, err = P.index()            -- nil and an error value, when it cannot be read
  local x = index and P.expansion("Factory")
  if x then
    local category, k, n = P.category(x, 1)   -- "Arp", 1, 96
    for _, o in ipairs(P.oscillators(x, 1)) do host.log.info(o.name) end   -- Arp, Bassline
  end
end
```

| Call | Returns |
|---|---|
| `P.FORMAT` | 1, the format this reader reads. |
| `P.index()` | The index as above, or `nil` and an [error value](../../docs/api/errors.md#error-values), code `invalid` (the index cannot be read, is not JSON, is of another format, lists no expansions, or has one without a name, a file or presets). Read at the first call and kept: the same table every time. |
| `P.expansion(name)` | The expansion's file as above, or `nil` and an error value: `not_found` for a name the index does not have; `invalid` for a file that cannot be read, is not JSON, is of another format, or does not hold the presets the index lists, and for an index that cannot be read. Read at the first call for that name and kept. |
| `P.category(x, i)` | The category of preset `i` of expansion file `x`: its name, the preset's place in it (from 1) and how many it holds; `nil` when no block holds `i`. |
| `P.oscillators(x, i)` | `{ { name, default }, … }`, one per tab; a default name comes back as Avenger's caption with `default = true`. |
| `P.macros(x, i)` | `{ { knobs = { three }, buttons = { two } }, … }`, one per tab, each name `{ name, default }`. |

**Cost.** Nothing is read when the module loads. `P.index()` reads and decodes 0.35 MB once, an expansion's file 2 to 110 KB once: each one `host.resource.read` and one `host.json.decode`, on the calling thread, which is the main one. Measured in a debug test build, on 1.0.0's 0.3 MB index: 30 to 50 ms for the index, 25 ms for the largest file. A file that cannot be read is not read again until the module using it is reloaded, and every call after answers the same error value. Since 1.2.0 the second value is an error value — its message the sentence 1.1.0 answered, word for word — so compare `err.code`, and hand the message on with `tostring` or `..`.

**Turning it off.** Unticking this module in the module manager does not stop what `vps-avenger` does with it: a `code_module`'s code runs in its dependents' VMs, and disabling it stops only its own VM ([what a code module must not do](../../docs/api/require.md#code-module-rules)). To run the overlay without the database, remove this folder and reload VPS Avenger.

## Updating it

When the avenger_control project exports a new database — new expansions, corrected names — run the converter on it and raise the version, so that the module manager's update check sees a new one:

```
python tools/avenger-presets/convert.py <folder with the three exports> --version 1.3.0
```

It rewrites `module.toml` and `data/`, and leaves `src/`, this README and `NOTICE` alone. [`tools/avenger-presets/README.md`](../../tools/avenger-presets/README.md) says what it checks and what it drops.
