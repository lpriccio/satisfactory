# Factory Planner

A local web app for planning factories in Satisfactory, Factorio and Dyson
Sphere Program. It replaces the Streamlit prototype in the parent folder and
covers what the T9 workbook's per-build tabs do.

## Run it

```bash
npm install
npm run dev          # http://127.0.0.1:5173, saves to factories/
```

Or build once and serve the built app with the same file API:

```bash
npm run build && npm run serve      # http://127.0.0.1:5174
```

Over SSH: `ssh -L 5173:127.0.0.1:5173 <host>`, then open http://127.0.0.1:5173 locally.

The `dist/` folder also works as a static site (e.g. GitHub Pages). Without the
file server it saves to browser storage, and share links carry the whole factory.

## What it does

- **Target mode.** Ask for N/min of one or more items; a linear solver (HiGHS,
  in the browser) picks machine counts so every item balances across the whole
  factory. Byproducts offset demand, loops solve exactly, and an item it can't
  balance is named instead of silently imported.
- **Manual mode.** Type recipe lines the way a workbook tab does (machines,
  clock, somersloops) and read the net balance, power and foundations. Lines
  left unlocked are sized to meet any targets.
- **Imports.** Click an input's `import` chip to import it for that line only;
  Shift-click to set every line using that item the same way. Raw resources and
  grid power are imported by default; un-import power to have generators built.
- **Satisfactory details.** Clock 1–250% (power × clock^1.32), somersloops
  (output × (1 + n/slots), power × that squared), generators, floor area, and
  foundations = area × spacing ÷ 64 m² (spacing 4 by default).
- **Factorio / DSP.** Building choice per line, building productivity,
  speed and productivity bonuses.
- **Worlds.** Add up factories with multipliers, see the net position, and save
  the result as one manual factory that keeps every line.
- **Objectives.** Fewest raw inputs (weighted by map scarcity), least power, or
  fewest machines. "Choose recipes for me" lets the solver pick among standard
  recipes and the alternates you've unlocked in Settings.
- Undo/redo (Ctrl+Z / Ctrl+Shift+Z), autosave, and a URL for every view.

## Data

Generated from the games' own files and committed to `public/data/`:

| Game | Source | Refresh |
| --- | --- | --- |
| Satisfactory | `<Steam>/Satisfactory/CommunityResources/Docs/en-US.json` | `npm run import:satisfactory [path]` |
| Factorio | `factorio --dump-data` → `~/.factorio/script-output/data-raw-dump.json` (with your enabled mods), plus locale files | `npm run import:factorio [path]` |
| Dyson Sphere Program | `../recipes_dsp.tsv` (the game exports nothing) | `npm run import:dsp` |

Satisfactory building footprints aren't in the docs export; they live in
`tools/satisfactory-footprints.ts`.

## Your saves

`factories/<game>/<id>.json` — one factory per file, plain JSON, worth committing.
Worlds are in `factories/<game>/worlds/`, settings in `factories/<game>/settings.json`.

- `npm run import:t9 [workbook]` turns each build tab of the T9 workbook into a
  manual factory (`t9-<tab>`; re-running overwrites). Recipe name mapping is in
  `tools/t9-recipe-map.json`, with a `manual` section for overrides.
- `npm run migrate:chains` converts the old Streamlit `../saved_chains/`.

## Code

| Path | What |
| --- | --- |
| `src/engine/` | UI-free model: types, per-machine math, LP solver, edits, worlds |
| `src/lib/` | Browser runtime: storage, routing, share links, editor state |
| `src/ui/` | Svelte components |
| `server/` | The JSON file API (dev middleware and `npm run serve`) |
| `tools/` | Importers |
| `tests/` | Vitest: engine, golden builds on real data, importers |
| `e2e/smoke.ts` | Headless browser run-through against a running server (`npm run e2e`) |

```bash
npm test             # unit + golden tests
npm run check        # svelte-check / TypeScript
npm run e2e          # needs `npm run dev` running and Chromium at /usr/bin/chromium-browser
```
