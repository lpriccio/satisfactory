// Converts the old Streamlit saved chains (../saved_chains/<game>/*.json) into
// target-mode factories under factories/<game>/<id>.json, keeping each old
// chain id. Usage: npx tsx tools/migrate-chains.ts
//
// Satisfactory names in the old app came from a hand-made TSV with its own
// abbreviations (MF, RIP, HSC) and shortened alternate names, so recipes are
// matched by name first and otherwise by their ingredient/product signature
// against ../recipes.tsv; each signature match also teaches the item aliases.

import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defaultRecipe } from '../src/engine/solve.ts';
import type { Factory, GameData, GameId, LineSettings, Recipe } from '../src/engine/types.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '../..');
const PLANNER = resolve(HERE, '..');

export interface OldChain {
  id: string;
  name: string;
  description?: string;
  target_item: string;
  target_rate: number;
  recipe_selections?: Record<string, string>;
  speed_multipliers?: Record<string, number>;
  productivity_multipliers?: Record<string, number>;
  imported_items?: string[];
  created_at?: string;
  updated_at?: string;
}

interface LegacyRecipe {
  name: string;
  time: number;
  inputs: [string, number][];
  outputs: [string, number][];
}

// --- legacy TSV ---------------------------------------------------------------

/** Parse the old Satisfactory recipes.tsv the way the old loader did. */
export function parseLegacyTsv(tsv: string): Map<string, LegacyRecipe> {
  const rows = tsv.split(/\r?\n/).map((l) => l.split('\t').map((c) => c.trim()));
  const header = rows[0];
  const at = (row: string[], name: string) => row[header.indexOf(name)] ?? '';
  const out = new Map<string, LegacyRecipe>();
  for (const row of rows.slice(1)) {
    const name = at(row, 'Recipe');
    const building = at(row, 'Building');
    if (!name || name === 'xxx' || !building || building.startsWith('#')) continue;
    const time = Number(at(row, 'Runtime'));
    const amount = Number(at(row, 'Amount'));
    const item = at(row, 'Item');
    if (!(time > 0) || !item || !Number.isFinite(amount) || amount === 0 || item === 'MW') continue;
    const r = out.get(name) ?? { name, time, inputs: [], outputs: [] };
    (amount < 0 ? r.inputs : r.outputs).push([item, Math.abs(amount)]);
    out.set(name, r);
  }
  for (const [k, r] of out) if (!r.outputs.length) out.delete(k);
  return out;
}

// --- name and signature matching ----------------------------------------------

const norm = (s: string) => s.trim().toLowerCase();
const stripAlt = (s: string) => s.replace(/^alternate:\s*/i, '');
const close = (a: number, b: number) => Math.abs(a - b) <= 0.01 * Math.max(Math.abs(a), Math.abs(b));

/** All ways to pair legacy stacks with game stacks (same amounts, consistent with known aliases). */
function pairings(
  legacy: [string, number][],
  game: Recipe['inputs'],
  itemMap: Map<string, string>,
): Map<string, string>[] {
  if (legacy.length !== game.length) return [];
  const results: Map<string, string>[] = [];
  const used = new Set<number>();
  const pick = (i: number, acc: Map<string, string>) => {
    if (i === legacy.length) {
      results.push(new Map(acc));
      return;
    }
    const [item, amount] = legacy[i];
    const known = itemMap.get(item);
    for (let j = 0; j < game.length; j++) {
      if (used.has(j) || !close(amount, game[j].amount)) continue;
      if (known && known !== game[j].item) continue;
      used.add(j);
      acc.set(item, game[j].item);
      pick(i + 1, acc);
      acc.delete(item);
      used.delete(j);
    }
  };
  pick(0, new Map());
  return results;
}

function signatureMatches(l: LegacyRecipe, g: Recipe, itemMap: Map<string, string>): Map<string, string>[] {
  if (g.generatesMW || !close(l.time, g.time)) return [];
  const ins = pairings(l.inputs, g.inputs, itemMap);
  if (!ins.length) return [];
  const outs = pairings(l.outputs, g.outputs, itemMap);
  const merged: Map<string, string>[] = [];
  for (const a of ins) for (const b of outs) merged.push(new Map([...a, ...b]));
  return merged;
}

export interface NameMaps {
  items: Map<string, string>;
  recipes: Map<string, string>;
}

/** Build legacy-name -> game-id maps for items and recipes. */
export function buildNameMaps(data: GameData, legacy: Map<string, LegacyRecipe>): NameMaps {
  const itemByName = new Map(Object.values(data.items).map((i) => [norm(i.name), i.id]));
  const recipeByName = new Map<string, string>();
  for (const r of Object.values(data.recipes)) {
    recipeByName.set(norm(r.name), r.id);
    if (!recipeByName.has(norm(stripAlt(r.name)))) recipeByName.set(norm(stripAlt(r.name)), r.id);
  }
  for (const r of Object.values(data.recipes)) recipeByName.set(norm(r.id), recipeByName.get(norm(r.id)) ?? r.id);

  const items = new Map<string, string>();
  const allLegacyItems = new Set<string>();
  for (const l of legacy.values()) for (const [i] of [...l.inputs, ...l.outputs]) allLegacyItems.add(i);
  for (const name of allLegacyItems) {
    const id = itemByName.get(norm(name)) ?? (data.items[name] ? name : undefined);
    if (id) items.set(name, id);
  }

  const recipes = new Map<string, string>();
  const games = Object.values(data.recipes);
  const pending = new Set(legacy.keys());
  const accept = (l: LegacyRecipe, gid: string, pairs: Map<string, string>) => {
    recipes.set(l.name, gid);
    for (const [a, b] of pairs) if (!items.has(a)) items.set(a, b);
    pending.delete(l.name);
  };

  // Pass 1: names, confirmed by signature when one is possible.
  for (const name of [...pending]) {
    const l = legacy.get(name)!;
    const gid = recipeByName.get(norm(name));
    if (!gid) continue;
    const pairs = signatureMatches(l, data.recipes[gid], items);
    if (pairs.length) accept(l, gid, pairs[0]);
  }

  // Pass 2: signatures, repeated while new aliases unlock more matches.
  let progress = true;
  while (progress) {
    progress = false;
    for (const name of [...pending]) {
      const l = legacy.get(name)!;
      const found: [Recipe, Map<string, string>[]][] = [];
      for (const g of games) {
        const p = signatureMatches(l, g, items);
        if (p.length) found.push([g, p]);
      }
      if (!found.length) continue;
      // A candidate whose name matches wins; otherwise accept only an unambiguous match.
      const named = found.find(([g]) => norm(stripAlt(g.name)) === norm(name) || norm(g.name) === norm(name));
      const choice = named ?? (found.length === 1 && found[0][1].length === 1 ? found[0] : undefined);
      if (!choice) continue;
      accept(l, choice[0].id, choice[1][0]);
      progress = true;
    }
  }

  // Unconfirmed name matches still beat nothing (e.g. a recipe the game rebalanced).
  for (const name of [...pending]) {
    const gid = recipeByName.get(norm(name));
    if (gid) {
      recipes.set(name, gid);
      pending.delete(name);
    }
  }
  return { items, recipes };
}

// --- chain -> factory ----------------------------------------------------------

function iso(s: string | undefined, fallback: string): string {
  if (!s) return fallback;
  const d = new Date(s);
  return Number.isNaN(d.getTime()) ? fallback : d.toISOString();
}

export function migrateChain(chain: OldChain, game: GameId, data: GameData, maps: NameMaps): { factory: Factory; problems: string[] } {
  const problems: string[] = [];
  const itemByName = new Map(Object.values(data.items).map((i) => [norm(i.name), i.id]));
  const mapItem = (name: string): string | undefined =>
    maps.items.get(name) ?? itemByName.get(norm(name)) ?? (data.items[name] ? name : undefined);
  const mapRecipe = (name: string): string | undefined => {
    if (maps.recipes.has(name)) return maps.recipes.get(name);
    if (data.recipes[name]) return name;
    return Object.values(data.recipes).find((r) => norm(r.name) === norm(name) || norm(stripAlt(r.name)) === norm(name))?.id;
  };

  const target = mapItem(chain.target_item);
  if (!target) problems.push(`target item "${chain.target_item}"`);

  const recipeChoice: Record<string, string> = {};
  for (const [itemName, recipeName] of Object.entries(chain.recipe_selections ?? {})) {
    const item = mapItem(itemName);
    const recipe = mapRecipe(recipeName);
    if (!item || !recipe) {
      problems.push(`recipe choice "${itemName}" → "${recipeName}"`);
      continue;
    }
    // The old app let you pick a recipe that only makes the item as a byproduct; keep it
    // only when the recipe really outputs the item.
    if (!data.recipes[recipe].outputs.some((s) => s.item === item)) {
      problems.push(`recipe choice "${itemName}" → "${recipeName}" (that recipe doesn't make it)`);
      continue;
    }
    if (recipe !== defaultRecipe(data, item)) recipeChoice[item] = recipe;
  }

  const lineSettings: Record<string, LineSettings> = {};
  const setting = (recipeName: string, patch: LineSettings) => {
    const recipe = mapRecipe(recipeName);
    if (!recipe) {
      problems.push(`multiplier for recipe "${recipeName}"`);
      return;
    }
    lineSettings[recipe] = { ...lineSettings[recipe], ...patch };
  };
  for (const [name, m] of Object.entries(chain.speed_multipliers ?? {})) {
    if (m === 1) continue;
    setting(name, data.features.clock ? { clock: m } : { speedBonus: m - 1 });
  }
  for (const [name, m] of Object.entries(chain.productivity_multipliers ?? {})) {
    if (m === 1) continue;
    if (!data.features.productivity) {
      problems.push(`productivity for "${name}" (not a feature of this game)`);
      continue;
    }
    setting(name, { prodBonus: Number((m - 1).toFixed(6)) });
  }

  const imports: string[] = [];
  for (const name of chain.imported_items ?? []) {
    const item = mapItem(name);
    if (!item) problems.push(`imported item "${name}"`);
    else if (!data.defaultImports.includes(item) && !imports.includes(item)) imports.push(item);
  }

  const now = new Date().toISOString();
  const notes = [chain.description?.trim(), problems.length ? `Migration: couldn't map ${problems.join('; ')}.` : '']
    .filter(Boolean)
    .join('\n\n');
  const createdAt = iso(chain.created_at, now);
  const factory: Factory = {
    schema: 1,
    id: chain.id,
    game,
    name: chain.name,
    ...(notes ? { notes } : {}),
    mode: 'target',
    objective: 'min-raw',
    targets: target ? [{ item: target, rate: chain.target_rate }] : [],
    recipeChoice,
    lineSettings,
    lines: [],
    imports: { items: imports.sort(), edges: [], exclude: [] },
    spacing: 4,
    createdAt,
    updatedAt: iso(chain.updated_at, createdAt),
  };
  return { factory, problems };
}

// --- main ------------------------------------------------------------------------

function loadData(game: GameId): GameData {
  return JSON.parse(readFileSync(join(PLANNER, 'public/data', `${game}.json`), 'utf8'));
}

export function migrateAll(log = console.log): Factory[] {
  const out: Factory[] = [];
  const games: [GameId, string | undefined][] = [
    ['satisfactory', join(REPO, 'recipes.tsv')],
    ['dsp', undefined],
  ];
  for (const [game, tsv] of games) {
    const dir = join(REPO, 'saved_chains', game);
    if (!existsSync(dir)) continue;
    const files = readdirSync(dir).filter((f) => f.endsWith('.json'));
    if (!files.length) continue;
    const data = loadData(game);
    const maps = tsv ? buildNameMaps(data, parseLegacyTsv(readFileSync(tsv, 'utf8'))) : { items: new Map(), recipes: new Map() };
    const outDir = join(PLANNER, 'factories', game);
    mkdirSync(outDir, { recursive: true });
    for (const f of files.sort()) {
      const chain: OldChain = JSON.parse(readFileSync(join(dir, f), 'utf8'));
      const { factory, problems } = migrateChain(chain, game, data, maps);
      writeFileSync(join(outDir, `${factory.id}.json`), JSON.stringify(factory, null, 2) + '\n');
      log(`${game}/${factory.id}  ${factory.name}${problems.length ? `  — couldn't map: ${problems.join('; ')}` : ''}`);
      out.push(factory);
    }
  }
  return out;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const done = migrateAll();
  console.log(`${done.length} chains migrated.`);
}
