# Factory Planner

A native desktop app (Rust + egui) for planning factories in Satisfactory,
Factorio and Dyson Sphere Program. It replaces the Streamlit prototype in the
parent folder and covers what the T9 workbook's per-build tabs do.

## Run it

```bash
cargo run --release -p planner-app                       # opens the window
cargo run --release -p planner-app -- t9-pasta           # straight to a factory
cargo run --release -p planner-app -- --game factorio --view balance <factory-id>
```

The binary is `target/release/planner`. Saves go to `factories/` next to this
README; set `PLANNER_ROOT` to use another folder (it must hold `data/` and
`factories/`).

## What it does

- **Target mode.** Ask for N/min of one or more items. A linear solver picks
  machine counts so every item balances across the whole factory: byproducts
  offset demand, loops solve exactly, and anything it can't balance is named
  (e.g. "Can't balance Crude Oil (126/min short)") with **Regenerate with
  standard recipes** and **Reset** next to it.
- **Manual mode.** Type recipe lines the way a workbook tab does (machines,
  clock, somersloops) and read the net balance, power and foundations. Lines
  left unlocked are sized to meet any targets.
- **Imports.** Click an input's `import` chip to import it for that line only;
  Shift-click to set every line using that item the same way. Raw resources
  and grid power are imported by default. Items nothing can make from scratch
  (crude oil, water) can't be un-imported, so a plan can't paint itself into
  that corner.
- **Satisfactory details.** Clock 1–250% (power × clock^1.32), somersloops
  (output × (1 + n/slots), power × that squared), generators (un-import power
  to have them built), floor area, and foundations = area × spacing ÷ 64 m²
  (spacing 4 by default).
- **Factorio / DSP.** Building choice per line (preferred building per
  category in Settings), building productivity, speed and productivity bonuses.
- **Worlds.** Add up factories with multipliers, see the net position, and save
  the result as one manual factory that keeps every line.
- **Objectives.** Fewest raw inputs (weighted by map scarcity), least power,
  or fewest machines. "Choose recipes for me" lets the solver pick among
  standard recipes and the alternates unlocked in Settings.
- Undo/redo (Ctrl+Z / Ctrl+Shift+Z), autosave, Export/Copy JSON.

## Data

Generated from the games' own files and committed to `data/`:

| Game | Source | Refresh |
| --- | --- | --- |
| Satisfactory | `<Steam>/Satisfactory/CommunityResources/Docs/en-US.json` | `cargo run -p planner-import -- satisfactory [path]` |
| Factorio | `factorio --dump-data` → `~/.factorio/script-output/data-raw-dump.json` (with your enabled mods), plus locale files | `cargo run -p planner-import -- factorio [path]` |
| Dyson Sphere Program | `../recipes_dsp.tsv` (the game exports nothing) | `cargo run -p planner-import -- dsp [path]` |

Satisfactory building footprints aren't in the docs export; they're a table in
`import/src/satisfactory.rs`.

## Your saves

`factories/<game>/<id>.json` — one factory per file, plain JSON, worth
committing. Worlds are in `factories/<game>/worlds/`, settings in
`factories/<game>/settings.json`. The format is the same one the TypeScript
version (branch `planner-typescript`) wrote.

- `cargo run -p planner-import -- t9 [workbook]` turns each build tab of the
  T9 workbook into a manual factory (`t9-<tab>`; re-running overwrites).
  Recipe-name mapping is in `data/t9-recipe-map.json`, with a `manual` section
  for overrides.
- `cargo run -p planner-import -- chains` converts the old Streamlit
  `../saved_chains/`.

## Code

| Path | What |
| --- | --- |
| `core/` | UI-free engine: model, per-machine math, LP solver (good_lp + microlp), edits, worlds, JSON storage |
| `import/` | `planner-import` CLI: game-data importers, T9 and saved-chain imports |
| `app/` | The egui app (`planner` binary) |

```bash
cargo test --workspace     # engine, golden builds on real data, importers, headless UI tests
cargo clippy --workspace --all-targets
```

The UI tests (`app/src/ui_tests.rs`) drive the real app with egui_kittest:
they click by label, type into pickers, and check the plan and the saved files.
`PLANNER_SCREENSHOT=out.ppm planner …` saves one rendered frame and quits.
