// Dyson Sphere Program has no data export, so recipes come from the
// hand-maintained TSV (Recipe, Item, Pair, Seconds, Item Count; negative
// count = input). Usage: npx tsx tools/import-dsp.ts [path/to/recipes_dsp.tsv]

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { GameData, Item, Recipe } from '../src/engine/types.ts';

const HERE = dirname(fileURLToPath(import.meta.url));
const DEFAULT_TSV = resolve(HERE, '../../recipes_dsp.tsv');
const OUT = resolve(HERE, '../public/data/dsp.json');

export function importDsp(tsv: string, source = 'recipes_dsp.tsv'): GameData {
  const lines = tsv.split(/\r?\n/);
  const header = lines[0].split('\t').map((h) => h.trim());
  const col = (name: string) => header.indexOf(name);
  const [cRecipe, cItem, cSeconds, cCount] = ['Recipe', 'Item', 'Seconds', 'Item Count'].map(col);
  if ([cRecipe, cItem, cSeconds, cCount].some((c) => c < 0)) throw new Error(`unexpected header: ${header.join(', ')}`);

  const skipped: { id: string; reason: string }[] = [];
  const recipes = new Map<string, Recipe>();
  for (const [n, line] of lines.slice(1).entries()) {
    const cells = line.split('\t').map((c) => c.trim());
    if (cells.every((c) => !c)) continue; // blank spacer rows
    const row = `row ${n + 2}`;
    const [name, item, secs, count] = [cells[cRecipe], cells[cItem], cells[cSeconds], cells[cCount]];
    if (!name || !item) {
      skipped.push({ id: row, reason: 'missing recipe or item name' });
      continue;
    }
    const time = Number(secs);
    const amount = Number(count);
    if (!Number.isFinite(time) || time <= 0) {
      skipped.push({ id: `${row} (${name})`, reason: `bad Seconds "${secs}"` });
      continue;
    }
    if (!Number.isFinite(amount) || amount === 0) {
      skipped.push({ id: `${row} (${name}/${item})`, reason: `bad or zero Item Count "${count}"` });
      continue;
    }
    let r = recipes.get(name);
    if (!r) {
      r = { id: name, name, time, inputs: [], outputs: [], buildings: ['producer'] };
      recipes.set(name, r);
    } else if (r.time !== time) {
      skipped.push({ id: `${row} (${name})`, reason: `Seconds ${time} differs from first row's ${r.time}; using ${r.time}` });
    }
    (amount < 0 ? r.inputs : r.outputs).push({ item, amount: Math.abs(amount) });
  }

  for (const r of [...recipes.values()]) {
    if (!r.outputs.length) {
      skipped.push({ id: r.id, reason: 'recipe has no outputs' });
      recipes.delete(r.id);
    }
  }

  const produced = new Map<string, string[]>();
  const items: Record<string, Item> = {};
  for (const r of recipes.values()) {
    for (const s of [...r.inputs, ...r.outputs]) items[s.item] ??= { id: s.item, name: s.item };
    for (const [i, s] of r.outputs.entries()) {
      const list = produced.get(s.item) ?? [];
      if (i === 0) list.unshift(r.id);
      else list.push(r.id);
      produced.set(s.item, list);
    }
  }
  const defaultImports: string[] = [];
  for (const item of Object.values(items)) {
    const makers = produced.get(item.id);
    if (!makers?.length) {
      item.raw = true;
      defaultImports.push(item.id);
    } else {
      item.recipe = makers.includes(item.id) ? item.id : makers[0];
    }
  }

  return {
    game: 'dsp',
    name: 'Dyson Sphere Program',
    source,
    generatedAt: new Date().toISOString(),
    items,
    buildings: { producer: { id: 'producer', name: 'Producer', powerMW: 0, speed: 1 } },
    recipes: Object.fromEntries(recipes),
    features: { clock: false, sloops: false, power: false, productivity: true, speedBonus: true, footprint: false },
    defaultImports: defaultImports.sort(),
    skipped,
  };
}

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

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const path = resolve(process.argv[2] ?? DEFAULT_TSV);
  const data = importDsp(readFileSync(path, 'utf8'), path);
  writeFileSync(OUT, JSON.stringify(sortKeys(data), null, 1) + '\n');
  console.log(
    `dsp: ${Object.keys(data.items).length} items, ${Object.keys(data.recipes).length} recipes, ` +
      `${data.defaultImports.length} raw, ${data.skipped?.length ?? 0} skipped -> ${OUT}`,
  );
  for (const s of data.skipped ?? []) console.log(`  skipped ${s.id}: ${s.reason}`);
}
