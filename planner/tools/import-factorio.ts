// Builds public/data/factorio.json from Factorio's `--dump-data` output.
//
//   factorio --dump-data          # writes ~/.factorio/script-output/data-raw-dump.json
//   npm run import:factorio [-- <path to data-raw-dump.json>]
//
// Display names come from the game's English locale files and from enabled
// mods' zips.

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Building, GameData, Item, Recipe, Stack } from '../src/engine/types.ts';

const here = dirname(fileURLToPath(import.meta.url));
const dumpPath = process.argv[2] ?? join(homedir(), '.factorio/script-output/data-raw-dump.json');
const gameData = '/mnt/pomni/SteamLibrary/steamapps/common/Factorio/data';
const modsDir = join(homedir(), '.factorio/mods');
const outPath = join(here, '../public/data/factorio.json');

const BUILTIN_MODS = ['core', 'base', 'space-age', 'quality', 'elevated-rails'];
const SKIP_CATEGORIES = new Set(['recycling', 'parameters']);
const BUILDING_TYPES = ['assembling-machine', 'furnace', 'rocket-silo'];
const MAX_SKIPPED = 500;

type Proto = Record<string, any>;
const raw: Record<string, Record<string, Proto>> = JSON.parse(readFileSync(dumpPath, 'utf8'));

// ---------------------------------------------------------------------------
// Locale

type Locale = Record<string, Record<string, string>>;
const locale: Locale = {};

function parseCfg(text: string) {
  let section = '';
  for (const line of text.split(/\r?\n/)) {
    const t = line.trim();
    if (!t || t.startsWith(';') || t.startsWith('#')) continue;
    const sec = /^\[(.+)\]$/.exec(t);
    if (sec) {
      section = sec[1];
      continue;
    }
    const eq = t.indexOf('=');
    if (eq <= 0) continue;
    (locale[section] ??= {})[t.slice(0, eq)] = t.slice(eq + 1);
  }
}

function enabledMods(): string[] {
  const listPath = join(modsDir, 'mod-list.json');
  if (!existsSync(listPath)) return [];
  const list = JSON.parse(readFileSync(listPath, 'utf8')) as { mods: { name: string; enabled: boolean }[] };
  return list.mods.filter((m) => m.enabled && !BUILTIN_MODS.includes(m.name)).map((m) => m.name);
}

function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d;
  }
  return 0;
}

function loadLocale() {
  for (const mod of BUILTIN_MODS) {
    const dir = join(gameData, mod, 'locale/en');
    if (!existsSync(dir)) continue;
    for (const f of readdirSync(dir).filter((f) => f.endsWith('.cfg'))) parseCfg(readFileSync(join(dir, f), 'utf8'));
  }
  if (!existsSync(modsDir)) return;
  const zips = readdirSync(modsDir).filter((f) => f.endsWith('.zip'));
  for (const mod of enabledMods()) {
    const candidates = zips
      .map((f) => ({ f, m: new RegExp(`^${mod.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}_(\\d+(?:\\.\\d+)*)\\.zip$`).exec(f) }))
      .filter((c) => c.m)
      .sort((a, b) => compareVersions(b.m![1], a.m![1]));
    if (!candidates.length) continue;
    try {
      const text = execFileSync('unzip', ['-p', join(modsDir, candidates[0].f), '*/locale/en/*.cfg'], {
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
        stdio: ['ignore', 'pipe', 'ignore'],
      });
      parseCfg(text);
    } catch {
      // No English locale in this mod.
    }
  }
}

function humanize(id: string): string {
  const s = id.replace(/-/g, ' ');
  return s.charAt(0).toUpperCase() + s.slice(1);
}

const REF_SECTIONS: Record<string, string[]> = {
  ITEM: ['item-name', 'entity-name', 'equipment-name'],
  FLUID: ['fluid-name'],
  ENTITY: ['entity-name', 'item-name'],
  RECIPE: ['recipe-name'],
};

function resolveRefs(s: string, depth = 0): string {
  if (depth > 3) return s;
  return s.replace(/__(ITEM|FLUID|ENTITY|RECIPE)__([^_]+(?:_[^_]+)*?)__/g, (whole, kind: string, id: string) => {
    for (const sec of REF_SECTIONS[kind] ?? []) {
      const v = locale[sec]?.[id];
      if (v !== undefined) return resolveRefs(v, depth + 1);
    }
    return whole;
  });
}

function lookup(sections: string[], id: string): string | undefined {
  for (const sec of sections) {
    const v = locale[sec]?.[id];
    if (v === undefined) continue;
    const resolved = resolveRefs(v);
    // Parameterized strings ("__1__ dock") need runtime arguments we don't have.
    return /__\w+__/.test(resolved) ? undefined : resolved;
  }
  return undefined;
}

// ---------------------------------------------------------------------------
// Prototype helpers

function parseEnergyMW(s: string | undefined): number {
  if (!s) return 0;
  const m = /^([\d.]+)\s*([kMGT]?)W$/.exec(s.trim());
  if (!m) return 0;
  const scale: Record<string, number> = { '': 1e-6, k: 1e-3, M: 1, G: 1e3, T: 1e6 };
  return Number(m[1]) * scale[m[2]];
}

function parseEnergyMJ(s: string | undefined): number | undefined {
  if (!s) return undefined;
  const m = /^([\d.]+)\s*([kMGT]?)J$/.exec(s.trim());
  if (!m) return undefined;
  const scale: Record<string, number> = { '': 1e-6, k: 1e-3, M: 1, G: 1e3, T: 1e6 };
  return Number(m[1]) * scale[m[2]];
}

function round(x: number, digits = 6): number {
  const f = 10 ** digits;
  return Math.round(x * f) / f;
}

/** Every prototype that behaves like an item (has a stack size). */
function itemPrototypes(): Map<string, Proto> {
  const out = new Map<string, Proto>();
  for (const [, group] of Object.entries(raw)) {
    for (const [name, p] of Object.entries(group)) {
      if (p && typeof p === 'object' && typeof p.stack_size === 'number' && !out.has(name)) out.set(name, p);
    }
  }
  return out;
}

function stackList(entries: Proto[] | undefined, isResult: boolean): { stacks: Stack[]; ignored: Record<string, number>; bad?: string } {
  const amounts = new Map<string, number>();
  const ignored: Record<string, number> = {};
  // Lua empty tables are dumped as {} rather than [].
  const list: Proto[] = Array.isArray(entries) ? entries : Object.values(entries ?? {});
  for (const e of list) {
    if (e.type !== 'item' && e.type !== 'fluid') continue;
    let amount: number;
    if (typeof e.amount === 'number') amount = e.amount;
    else if (typeof e.amount_min === 'number' && typeof e.amount_max === 'number') amount = (e.amount_min + e.amount_max) / 2;
    else return { stacks: [], ignored, bad: `no amount for ${e.name}` };
    if (isResult) {
      amount = amount * (e.probability ?? 1) + (e.extra_count_fraction ?? 0);
      if (e.ignored_by_productivity) ignored[e.name] = (ignored[e.name] ?? 0) + e.ignored_by_productivity * (e.probability ?? 1);
    }
    amounts.set(e.name, (amounts.get(e.name) ?? 0) + amount);
  }
  const stacks = [...amounts].filter(([, a]) => a > 0).map(([item, amount]) => ({ item, amount: round(amount) }));
  return { stacks, ignored };
}

// ---------------------------------------------------------------------------
// Build

loadLocale();

const skipped: { id: string; reason: string }[] = [];
const skip = (id: string, reason: string) => {
  if (skipped.length < MAX_SKIPPED) skipped.push({ id, reason });
};

// Buildings
const buildings: Record<string, Building> = {};
for (const type of BUILDING_TYPES) {
  for (const [name, p] of Object.entries(raw[type] ?? {})) {
    const flags: string[] = p.flags ?? [];
    if (!flags.includes('player-creation') || p.hidden) {
      skip(name, `${type} not player-creatable or hidden`);
      continue;
    }
    const box = p.collision_box as [[number, number], [number, number]] | undefined;
    const footprint = box ? Math.ceil(box[1][0] - box[0][0]) * Math.ceil(box[1][1] - box[0][1]) : undefined;
    const b: Building = {
      id: name,
      name: lookup(['entity-name'], name) ?? humanize(name),
      powerMW: round(parseEnergyMW(p.energy_usage)),
      speed: p.crafting_speed ?? 1,
      categories: p.crafting_categories ?? [],
    };
    const prod = p.effect_receiver?.base_effect?.productivity;
    if (prod) b.baseProductivity = prod;
    if (typeof p.module_slots === 'number') b.moduleSlots = p.module_slots;
    if (footprint) b.footprint = footprint;
    buildings[name] = b;
  }
}

const buildingsByCategory = new Map<string, string[]>();
for (const b of Object.values(buildings)) {
  for (const c of b.categories ?? []) {
    const list = buildingsByCategory.get(c) ?? [];
    list.push(b.id);
    buildingsByCategory.set(c, list);
  }
}
for (const list of buildingsByCategory.values()) list.sort((a, b) => (buildings[b].speed ?? 1) - (buildings[a].speed ?? 1) || a.localeCompare(b));

// Recipes
const itemProtos = itemPrototypes();
const fluidProtos = raw.fluid ?? {};
const recipes: Record<string, Recipe> = {};
const mainProduct = new Map<string, string | undefined>();

function itemName(id: string): string {
  if (fluidProtos[id]) return lookup(['fluid-name'], id) ?? humanize(id);
  const p = itemProtos.get(id);
  return (
    lookup(['item-name'], id) ??
    (p?.place_result ? lookup(['entity-name'], p.place_result) : undefined) ??
    (p?.place_as_equipment_result ? lookup(['equipment-name'], p.place_as_equipment_result) : undefined) ??
    lookup(['entity-name', 'equipment-name'], id) ??
    humanize(id)
  );
}

for (const [name, r] of Object.entries(raw.recipe ?? {})) {
  const category: string = r.category ?? 'crafting';
  if (SKIP_CATEGORIES.has(category)) continue; // recycling alone is ~thousands; not recorded
  if (r.hidden) {
    skip(name, 'hidden');
    continue;
  }
  if (r.parameter) {
    skip(name, 'parameter recipe');
    continue;
  }
  const res = stackList(r.results, true);
  if (res.bad) {
    skip(name, res.bad);
    continue;
  }
  if (!res.stacks.length) {
    skip(name, 'no results');
    continue;
  }
  const ing = stackList(r.ingredients, false);
  if (ing.bad) {
    skip(name, ing.bad);
    continue;
  }
  const categories = [category, ...((r.additional_categories as string[] | undefined) ?? [])];
  const able = new Set<string>();
  for (const c of categories) for (const b of buildingsByCategory.get(c) ?? []) able.add(b);
  if (!able.size) {
    skip(name, `no machine for category ${categories.join(', ')}`);
    continue;
  }
  const machineList = [...able].sort((a, b) => (buildings[b].speed ?? 1) - (buildings[a].speed ?? 1) || a.localeCompare(b));

  const main: string | undefined = r.main_product || (res.stacks.length === 1 ? res.stacks[0].item : undefined);
  mainProduct.set(name, main);

  const recipe: Recipe = {
    id: name,
    name: lookup(['recipe-name'], name) ?? (main ? itemName(main) : humanize(name)),
    time: r.energy_required ?? 0.5,
    inputs: ing.stacks,
    outputs: res.stacks,
    buildings: machineList,
    category,
  };
  if (r.allow_productivity) recipe.allowProductivity = true;
  if (Object.keys(res.ignored).length) recipe.ignoredByProductivity = res.ignored;
  recipes[name] = recipe;
}

// Raw resources
const rawIds = new Set<string>();
const addMinable = (m: Proto | undefined) => {
  if (!m) return;
  if (m.result) rawIds.add(m.result);
  for (const r of Array.isArray(m.results) ? m.results : Object.values(m.results ?? {})) if ((r as Proto)?.name) rawIds.add((r as Proto).name);
  if (m.fluid) rawIds.add(m.fluid);
};
for (const p of Object.values(raw.resource ?? {})) addMinable(p.minable);
for (const p of Object.values(raw.plant ?? {})) addMinable(p.minable);
for (const p of Object.values(raw.fish ?? {})) addMinable(p.minable);
for (const p of Object.values(raw.tile ?? {})) if (p.fluid) rawIds.add(p.fluid);
for (const name of Object.keys(raw['asteroid-chunk'] ?? {})) {
  if (!name.startsWith('parameter-') && name !== 'asteroid-chunk-unknown') rawIds.add(name);
}

const produced = new Set<string>();
const used = new Set<string>();
for (const r of Object.values(recipes)) {
  for (const s of r.outputs) produced.add(s.item);
  for (const s of r.inputs) used.add(s.item);
}
for (const id of used) if (!produced.has(id)) rawIds.add(id);

// Items
const items: Record<string, Item> = {};
const keep = new Set<string>([...produced, ...used, ...[...rawIds].filter((id) => itemProtos.has(id) || fluidProtos[id])]);
for (const id of [...keep].sort()) {
  const fluid = fluidProtos[id];
  const proto = fluid ?? itemProtos.get(id);
  if (!proto) {
    skip(id, 'referenced but no item or fluid prototype');
    continue;
  }
  const item: Item = { id, name: itemName(id) };
  if (fluid) item.form = 'liquid';
  if (rawIds.has(id)) item.raw = true;
  const mj = parseEnergyMJ(proto.fuel_value);
  if (mj) item.energyMJ = round(mj);
  items[id] = item;
}

// Default recipe per item
const isBarrel = (id: string) => /barrel/.test(id) || id.startsWith('empty-');
const producers = new Map<string, string[]>();
for (const r of Object.values(recipes)) {
  for (const s of r.outputs) {
    const list = producers.get(s.item) ?? [];
    list.push(r.id);
    producers.set(s.item, list);
  }
}
for (const [id, list] of producers) {
  if (!items[id]) continue;
  let pick = list.find((r) => r === id);
  pick ??= list.find((r) => !isBarrel(r) && mainProduct.get(r) === id);
  pick ??= list.find((r) => !isBarrel(r));
  pick ??= list[0];
  items[id].recipe = pick;
}

const defaultImports = Object.values(items)
  .filter((i) => i.raw)
  .map((i) => i.id)
  .sort();

const data: GameData = {
  game: 'factorio',
  name: 'Factorio',
  source: `${dumpPath} (${(raw['utility-constants'] ? 'Factorio 2.0 ' : '') + enabledMods().join(', ')})`,
  generatedAt: new Date().toISOString(),
  items,
  buildings,
  recipes,
  features: {
    clock: false,
    sloops: false,
    power: true,
    productivity: true,
    speedBonus: true,
    footprint: true,
    areaUnit: 'tiles',
  },
  defaultImports,
  skipped,
};

function sortKeys(v: unknown): unknown {
  if (Array.isArray(v)) return v.map(sortKeys);
  if (v && typeof v === 'object') {
    return Object.fromEntries(
      Object.keys(v as object)
        .sort()
        .map((k) => [k, sortKeys((v as Record<string, unknown>)[k])]),
    );
  }
  return v;
}

writeFileSync(outPath, JSON.stringify(sortKeys(data), null, 1) + '\n');
console.log(
  `factorio: ${Object.keys(recipes).length} recipes, ${Object.keys(items).length} items, ` +
    `${Object.keys(buildings).length} buildings, ${defaultImports.length} raw, ${skipped.length} skipped -> ${outPath}`,
);
