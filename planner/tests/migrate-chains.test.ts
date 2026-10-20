import { readdirSync, readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { normalizeFactory } from '../src/engine/factory.ts';
import { solveFactory } from '../src/engine/solve.ts';
import type { Factory, GameData, GameId } from '../src/engine/types.ts';
import { highs } from './fixtures.ts';

const root = new URL('../', import.meta.url);
const read = (rel: string) => JSON.parse(readFileSync(new URL(rel, root), 'utf8'));

// Only the factories migrated from saved_chains (they keep the old chain's
// uuid); other importers write to the same folders.
const UUID = /[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/;
const migrated: { game: GameId; factory: Factory }[] = [];
for (const game of ['satisfactory', 'dsp'] as GameId[]) {
  const oldIds = new Set(
    readdirSync(new URL(`../saved_chains/${game}/`, root)).map((f) => f.match(UUID)?.[0]).filter(Boolean),
  );
  for (const f of readdirSync(new URL(`factories/${game}/`, root)).filter((f) => f.endsWith('.json'))) {
    if (oldIds.has(f.slice(0, -5))) migrated.push({ game, factory: read(`factories/${game}/${f}`) });
  }
}
const dataFor = new Map<GameId, GameData>([
  ['satisfactory', read('public/data/satisfactory.json')],
  ['dsp', read('public/data/dsp.json')],
]);

describe('migrated chains', () => {
  it('covers every old saved chain', () => {
    const old = (game: string) => readdirSync(new URL(`../saved_chains/${game}/`, root)).filter((f) => f.endsWith('.json')).length;
    expect(migrated.filter((m) => m.game === 'satisfactory')).toHaveLength(old('satisfactory'));
    expect(migrated.filter((m) => m.game === 'dsp')).toHaveLength(old('dsp'));
  });

  it.each(migrated.map((m) => [m.factory.name, m] as const))('%s references only existing ids and solves', async (_, { game, factory: raw }) => {
    const data = dataFor.get(game)!;
    const factory = normalizeFactory(raw);
    expect(factory.mode).toBe('target');
    for (const t of factory.targets) expect(data.items[t.item], t.item).toBeDefined();
    for (const [item, recipe] of Object.entries(factory.recipeChoice)) {
      expect(data.items[item], item).toBeDefined();
      expect(data.recipes[recipe]?.outputs.some((s) => s.item === item), `${recipe} makes ${item}`).toBe(true);
    }
    for (const recipe of Object.keys(factory.lineSettings)) expect(data.recipes[recipe], recipe).toBeDefined();
    for (const item of factory.imports.items) expect(data.items[item], item).toBeDefined();
    const plan = solveFactory(data, factory, await highs());
    expect(['ok', 'unbalanced']).toContain(plan.status);
  });

  it('keeps Alclad 1 at 10 Alclad Aluminum Sheet per minute', () => {
    const alclad = migrated.find((m) => m.factory.name === 'Alclad 1')!.factory;
    expect(alclad.id).toBe('a0ca37fe-2588-4157-aa80-522e6507a148');
    expect(alclad.targets).toEqual([{ item: 'AluminumPlate', rate: 10 }]);
  });

  it('carries DSP productivity as a line bonus', () => {
    const purple = migrated.find((m) => m.factory.name === 'Purple Science 1')!.factory;
    expect(purple.lineSettings['Purple Science']).toEqual({ prodBonus: 0.25 });
    expect(purple.recipeChoice.Tubes).toBe('Tubes+');
  });
});
