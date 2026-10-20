// Imports the build tabs of Laura's T9 Satisfactory workbook as manual-mode
// factories (PRD milestone 4).
//
//   npx tsx tools/import-t9.ts [workbook.ods|xlsx]
//
// Reads cached cell values only; the workbook is never written. Writes
// factories/satisfactory/t9-<tab>.json (re-imports overwrite) and
// tools/t9-recipe-map.json (generated mapping; its `manual` section is kept).
//
// Two tab templates exist in T9:
//   A. "Shard:" in E1/F1, "Scale:" in G1/H1, headers on row 3:
//      F base qty, G Recipe, H Qty (= F × scale), J Sharded (= H / shard),
//      K Size (= size × CEILING(J)). Machines = J, each at clock = shard.
//   B. "Scale:" in G1/H1, headers on row 2: G Recipe, H Qty, J "Size (2.5x)"
//      (= size × H, fractional). Some tabs add a "SHARDED" column E holding
//      the real machine count; then clock = H / E.
// H is always production-equivalent machines at 100% clock, so production
// matches the workbook whichever way the line is expressed. Neither template
// records somersloops per row (the sloops sheet tracks them separately), so
// none are imported.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import * as XLSX from 'xlsx';
import { normalizeFactory, nowIso } from '../src/engine/factory.ts';
import { POWER, type Factory, type GameData, type Line, type Recipe } from '../src/engine/types.ts';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '..');
export const DEFAULT_WORKBOOK = path.resolve(ROOT, '../20260425 T9 fixedrows.ods');
const DATA_PATH = path.join(ROOT, 'public/data/satisfactory.json');
const MAP_PATH = path.join(HERE, 't9-recipe-map.json');
const OUT_DIR = path.join(ROOT, 'factories/satisfactory');

/** Sheets that are not build tabs. */
export const NON_BUILD = new Set(['planner', 'sloops', 'floor calc', 'trucks', 'trains', 'drones', 'recipes', 'scratch', 'wtg']);

/** Workbook item names that differ from the game's display names. */
export const ITEM_ALIASES: Record<string, string> = {
  Screw: 'IronScrew',
  RIP: 'IronPlateReinforced',
  MF: 'ModularFrame',
  EIB: 'SteelPlateReinforced',
  HOR: 'HeavyOilResidue',
  HSC: 'HighSpeedConnector',
  Canister: 'FluidCanister',
  'Alclad Sheet': 'AluminumPlate',
  Nitrogen: 'NitrogenGas',
  'Packaged Nitrogen': 'PackagedNitrogenGas',
  'Rifle Cartridge': 'CartridgeStandard',
  'Automated Miner': 'BP_ItemDescriptorPortableMiner',
  'SAM Fluctator': 'SAMFluctuator',
  Diamond: 'Diamond',
  'Uranium Ore': 'OreUranium',
  'Packaged Turbo Fuel': 'TurboFuel',
  MW: POWER,
};

// --- workbook ---------------------------------------------------------------

export function loadWorkbook(file = DEFAULT_WORKBOOK): XLSX.WorkBook {
  // SheetJS logs a line per ODS conditional number format it can't model;
  // values are unaffected, so keep the console readable.
  const error = console.error;
  console.error = (...args: unknown[]) => {
    if (typeof args[0] === 'string' && args[0].startsWith('ODS number format')) return;
    error(...args);
  };
  try {
    return XLSX.read(fs.readFileSync(file), { cellFormula: false, cellHTML: false });
  } finally {
    console.error = error;
  }
}

type Cell = string | number | boolean | null;

function grid(ws: XLSX.WorkSheet): Cell[][] {
  return XLSX.utils.sheet_to_json<Cell[]>(ws, { header: 1, raw: true, defval: null, blankrows: true });
}

const col = (letter: string) => XLSX.utils.decode_col(letter);
const num = (v: Cell): number | undefined => (typeof v === 'number' && Number.isFinite(v) ? v : undefined);
const str = (v: Cell): string => (typeof v === 'string' ? v.trim() : '');

// --- item and recipe mapping -----------------------------------------------

export function itemResolver(data: GameData): (name: string) => string | undefined {
  const byName = new Map<string, string>();
  for (const i of Object.values(data.items)) byName.set(i.name.toLowerCase(), i.id);
  return (name) => ITEM_ALIASES[name] ?? byName.get(name.toLowerCase());
}

interface WorkbookRecipe {
  name: string;
  building: string;
  runtime: number;
  /** item name -> signed amount per cycle */
  amounts: Map<string, number>;
}

export function readWorkbookRecipes(wb: XLSX.WorkBook): Map<string, WorkbookRecipe> {
  const rows = grid(wb.Sheets.recipes);
  const out = new Map<string, WorkbookRecipe>();
  // A recipe's rows are contiguous. Some recipes appear twice (Silicon HSC);
  // a later block repeats the first, so only the first is read.
  let current = '';
  const done = new Set<string>();
  for (const r of rows.slice(1)) {
    const name = str(r[1]);
    const item = str(r[2]);
    const amount = num(r[3]);
    const runtime = num(r[4]);
    if (!name || !item || amount === undefined || !runtime) continue;
    if (name !== current) {
      if (current) done.add(current);
      current = name;
      if (done.has(name)) continue;
    } else if (done.has(name)) continue;
    const rec = out.get(name) ?? { name, building: str(r[5]), runtime, amounts: new Map() };
    rec.amounts.set(item, (rec.amounts.get(item) ?? 0) + amount);
    out.set(name, rec);
  }
  return out;
}

/** Per-minute signed flow of one machine at 100%. */
function gameVector(r: Recipe): Map<string, number> {
  const v = new Map<string, number>();
  const k = 60 / r.time;
  for (const s of r.inputs) v.set(s.item, (v.get(s.item) ?? 0) - s.amount * k);
  for (const s of r.outputs) v.set(s.item, (v.get(s.item) ?? 0) + s.amount * k);
  if (r.generatesMW) v.set(POWER, r.generatesMW);
  return v;
}

function workbookVector(w: WorkbookRecipe, resolve: (n: string) => string | undefined): Map<string, number> | undefined {
  const v = new Map<string, number>();
  for (const [name, amount] of w.amounts) {
    const id = resolve(name);
    if (!id) return undefined;
    v.set(id, (v.get(id) ?? 0) + (amount * 60) / w.runtime);
  }
  return v;
}

function sameVector(a: Map<string, number>, b: Map<string, number>, ignore?: string): boolean {
  const keys = new Set([...a.keys(), ...b.keys()]);
  for (const k of keys) {
    if (k === ignore) continue;
    const x = a.get(k) ?? 0;
    const y = b.get(k) ?? 0;
    if (Math.abs(x - y) > 1e-6 * Math.max(1, Math.abs(x), Math.abs(y))) return false;
  }
  return true;
}

export type MatchHow = 'name' | 'ingredients' | 'name-rates-differ' | 'manual';

export interface RecipeMapFile {
  /** Hand overrides: workbook recipe name -> game recipe id. Kept on regeneration. */
  manual: Record<string, string>;
  generated: Record<string, { id: string; how: MatchHow; note?: string }>;
  unmatched: { name: string; reason: string }[];
}

export function buildRecipeMap(
  data: GameData,
  wbRecipes: Map<string, WorkbookRecipe>,
  manual: Record<string, string> = {},
): RecipeMapFile {
  const resolve = itemResolver(data);
  const games = Object.values(data.recipes).map((r) => ({ r, v: gameVector(r) }));
  const strip = (n: string) => n.replace(/^Alternate:\s*/i, '').toLowerCase();
  const generated: RecipeMapFile['generated'] = {};
  const unmatched: RecipeMapFile['unmatched'] = [];

  for (const w of wbRecipes.values()) {
    if (manual[w.name]) {
      if (data.recipes[manual[w.name]]) generated[w.name] = { id: manual[w.name], how: 'manual' };
      else unmatched.push({ name: w.name, reason: `manual id ${manual[w.name]} not in game data` });
      continue;
    }
    const vec = workbookVector(w, resolve);
    const named = games.filter((g) => strip(g.r.name) === w.name.toLowerCase());
    if (!vec) {
      const missing = [...w.amounts.keys()].filter((n) => !resolve(n));
      unmatched.push({ name: w.name, reason: `unknown items: ${missing.join(', ')}` });
      continue;
    }
    const same = games.filter((g) => sameVector(g.v, vec));
    const byNameAndRates = same.find((g) => named.includes(g));
    // The workbook leaves cooling water out of its generator rows.
    const generator = vec.has(POWER) && !same.length ? games.filter((g) => g.r.generatesMW && sameVector(g.v, vec, 'Water')) : [];
    if (byNameAndRates) generated[w.name] = { id: byNameAndRates.r.id, how: 'name' };
    else if (generator.length === 1) {
      generated[w.name] = { id: generator[0].r.id, how: 'ingredients', note: 'generator; workbook omits cooling water' };
    }
    else if (same.length) {
      // Prefer a standard recipe when several share the same flows.
      const pick = same.find((g) => !g.r.alternate) ?? same[0];
      generated[w.name] = {
        id: pick.r.id,
        how: 'ingredients',
        note: same.length > 1 ? `also matches ${same.filter((g) => g !== pick).map((g) => g.r.id).join(', ')}` : undefined,
      };
    } else if (named.length === 1) {
      generated[w.name] = { id: named[0].r.id, how: 'name-rates-differ', note: diffNote(vec, named[0].v, data) };
    } else {
      unmatched.push({ name: w.name, reason: named.length ? 'several game recipes share the name; rates differ' : 'no game recipe with these flows or name' });
    }
  }
  return { manual, generated, unmatched };
}

function diffNote(wb: Map<string, number>, game: Map<string, number>, data: GameData): string {
  const keys = new Set([...wb.keys(), ...game.keys()]);
  const parts: string[] = [];
  for (const k of keys) {
    const a = wb.get(k) ?? 0;
    const b = game.get(k) ?? 0;
    if (Math.abs(a - b) > 1e-6) parts.push(`${data.items[k]?.name ?? k}: workbook ${round(a)}/min, game ${round(b)}/min`);
  }
  return parts.join('; ');
}

const round = (n: number) => Number(n.toFixed(4));

// --- build tabs ---------------------------------------------------------------

export interface TabRow {
  row: number;
  recipe: string;
  /** Production-equivalent machines at 100% (workbook H, already scaled). */
  qty: number;
  /** Real machine count when the tab records one (template A Sharded, B SHARDED). */
  sharded?: number;
}

export interface ParsedTab {
  name: string;
  template: 'A' | 'B';
  title?: string;
  shard: number;
  scale: number;
  headerRow: number;
  rows: TabRow[];
  /** Left-hand block: miners, imports, exports, scratch notes. */
  side: { section: string; label: string; item: string; rate?: number }[];
  /** Workbook totals as computed by its own formulas. */
  totalDraw?: number;
  totalSize?: number;
  foundationsText?: string;
  /** Workbook item balance: item name -> {minus, plus}. */
  balance: Map<string, { minus: number; plus: number }>;
}

export function parseTab(ws: XLSX.WorkSheet, name: string): ParsedTab | { name: string; error: string } {
  const g = grid(ws);
  const at = (r: number, c: number): Cell => g[r]?.[c] ?? null;

  let headerRow = -1;
  for (let r = 0; r < 6; r++) if (str(at(r, col('G'))) === 'Recipe') headerRow = r;
  if (headerRow < 0) return { name, error: 'no "Recipe" header in column G' };

  const template: 'A' | 'B' = str(at(0, col('E'))) === 'Shard:' ? 'A' : 'B';
  const shard = template === 'A' ? (num(at(0, col('F'))) ?? 1) : 1;
  const scale = str(at(0, col('G'))) === 'Scale:' ? (num(at(0, col('H'))) ?? 1) : 1;
  const shardedColB = str(at(headerRow, col('E'))).toUpperCase() === 'SHARDED';

  // Header cells by text, to find the balance columns.
  const header = g[headerRow] ?? [];
  const minusCol = header.findIndex((v) => str(v) === 'gross-');

  const rows: TabRow[] = [];
  const side: ParsedTab['side'] = [];
  const balance = new Map<string, { minus: number; plus: number }>();
  let section = str(at(headerRow, col('A'))) || 'Notes';

  for (let r = headerRow + 1; r < g.length; r++) {
    const recipe = str(at(r, col('G')));
    const qty = num(at(r, col('H')));
    if (recipe && qty !== undefined && qty > 1e-9) {
      let sharded: number | undefined;
      if (template === 'A') sharded = num(at(r, col('J')));
      else if (shardedColB) {
        const e = num(at(r, col('E')));
        if (e !== undefined && e > 0) sharded = e;
      }
      rows.push({ row: r + 1, recipe, qty, sharded });
    }

    const a = str(at(r, col('A')));
    const b = str(at(r, col('B')));
    const c = num(at(r, col('C')));
    if (a && !b && c === undefined) section = a;
    else if (b && (c !== undefined || a)) side.push({ section, label: a, item: b, rate: c });

    if (minusCol > 0) {
      const item = str(at(r, minusCol - 1));
      const minus = num(at(r, minusCol));
      const plus = num(at(r, minusCol + 1));
      if (item && minus !== undefined && plus !== undefined) balance.set(item, { minus, plus });
    }
  }

  const title = template === 'A' ? str(at(0, col('A'))) || undefined : undefined;
  const sizeCell = template === 'A' ? at(0, col('K')) : at(0, col('J'));
  const textCell = template === 'A' ? at(0, col('L')) : at(0, col('K'));
  return {
    name,
    template,
    title,
    shard,
    scale,
    headerRow: headerRow + 1,
    rows,
    side,
    totalDraw: num(at(0, col('I'))),
    totalSize: num(sizeCell),
    foundationsText: str(textCell) || undefined,
    balance,
  };
}

export function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
}

export function tabToFactory(tab: ParsedTab, map: RecipeMapFile, data: GameData, now = nowIso()): { factory: Factory; unmapped: string[] } {
  const id = `t9-${slug(tab.name)}`;
  const lines: Line[] = [];
  const unmapped: string[] = [];
  for (const row of tab.rows) {
    const hit = map.generated[row.recipe];
    if (!hit || !data.recipes[hit.id]) {
      unmapped.push(`${row.recipe} ×${round(row.qty)} (row ${row.row})`);
      continue;
    }
    // Express the line as real machines × clock; production stays qty × 100%.
    let machines = row.qty;
    let clock: number | undefined;
    if (row.sharded !== undefined && row.sharded > 0 && Math.abs(row.sharded - row.qty) > 1e-9) {
      machines = row.sharded;
      clock = row.qty / row.sharded;
    }
    const line: Line = { id: `${id}-r${row.row}`, recipe: hit.id, machines };
    if (clock !== undefined) line.clock = clock;
    if (hit.how === 'name-rates-differ') line.note = `Workbook rates differ from the game: ${hit.note}`;
    lines.push(line);
  }

  const notes: string[] = [];
  if (tab.title) notes.push(tab.title);
  notes.push(
    `Imported from T9 workbook tab "${tab.name}" (template ${tab.template}, scale ${round(tab.scale)}${tab.template === 'A' ? `, shard ${round(tab.shard)}` : ''}).`,
  );
  if (tab.totalSize !== undefined) notes.push(`Workbook size total: ${round(tab.totalSize)} m²${tab.foundationsText ? ` ${tab.foundationsText}` : ''}.`);
  const sideBySection = new Map<string, string[]>();
  for (const s of tab.side) {
    const list = sideBySection.get(s.section) ?? [];
    list.push(`${s.label ? s.label + ' ' : ''}${s.item}${s.rate !== undefined ? ` ${round(s.rate)}/min` : ''}`);
    sideBySection.set(s.section, list);
  }
  for (const [section, list] of sideBySection) notes.push(`${section}: ${list.join('; ')}`);
  if (unmapped.length) notes.push(`Unmapped workbook recipes (not imported): ${unmapped.join('; ')}`);

  const existingCreated = readExisting(id)?.createdAt;
  const factory = normalizeFactory({
    id,
    game: 'satisfactory',
    name: tab.name,
    notes: notes.join('\n'),
    mode: 'manual',
    objective: 'min-raw',
    lines,
    spacing: 4,
    createdAt: existingCreated ?? now,
    updatedAt: now,
  });
  return { factory, unmapped };
}

function readExisting(id: string): Factory | undefined {
  try {
    return JSON.parse(fs.readFileSync(path.join(OUT_DIR, `${id}.json`), 'utf8'));
  } catch {
    return undefined;
  }
}

export function loadData(): GameData {
  return JSON.parse(fs.readFileSync(DATA_PATH, 'utf8'));
}

export function readManualMap(): Record<string, string> {
  try {
    return (JSON.parse(fs.readFileSync(MAP_PATH, 'utf8')) as RecipeMapFile).manual ?? {};
  } catch {
    return {};
  }
}

// --- CLI ----------------------------------------------------------------------

function main() {
  const file = process.argv[2] ? path.resolve(process.argv[2]) : DEFAULT_WORKBOOK;
  const data = loadData();
  const wb = loadWorkbook(file);
  const map = buildRecipeMap(data, readWorkbookRecipes(wb), readManualMap());
  fs.writeFileSync(MAP_PATH, JSON.stringify(map, null, 2) + '\n');

  fs.mkdirSync(OUT_DIR, { recursive: true });
  const imported: string[] = [];
  const skipped: string[] = [];
  for (const name of wb.SheetNames) {
    if (NON_BUILD.has(name)) continue;
    const tab = parseTab(wb.Sheets[name], name);
    if ('error' in tab) {
      skipped.push(`${name}: ${tab.error}`);
      continue;
    }
    if (!tab.rows.length) {
      skipped.push(`${name}: no recipe rows`);
      continue;
    }
    const { factory, unmapped } = tabToFactory(tab, map, data);
    fs.writeFileSync(path.join(OUT_DIR, `${factory.id}.json`), JSON.stringify(factory, null, 2) + '\n');
    imported.push(`${name} → ${factory.id} (${factory.lines.length} lines${unmapped.length ? `, ${unmapped.length} unmapped` : ''})`);
  }

  const counts = Object.values(map.generated).reduce<Record<string, number>>((a, g) => ((a[g.how] = (a[g.how] ?? 0) + 1), a), {});
  console.log(`Workbook: ${file}`);
  console.log(`Recipe map: ${JSON.stringify(counts)}, unmatched ${map.unmatched.length}`);
  for (const u of map.unmatched) console.log(`  unmatched: ${u.name} — ${u.reason}`);
  console.log(`Imported ${imported.length} tabs:`);
  for (const s of imported) console.log(`  ${s}`);
  if (skipped.length) console.log(`Skipped ${skipped.length}:`);
  for (const s of skipped) console.log(`  ${s}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
