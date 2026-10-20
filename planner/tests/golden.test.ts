// Known builds against the real game data in public/data, hand-checked.
import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { canProduce, solveFactory } from '../src/engine/solve.ts';
import { regenerate, resetPlan } from '../src/engine/edits.ts';
import type { GameData, Plan } from '../src/engine/types.ts';
import { factory, highs } from './fixtures.ts';

const load = (game: string): GameData => JSON.parse(readFileSync(new URL(`../public/data/${game}.json`, import.meta.url), 'utf8'));
const sat = load('satisfactory');
const machines = (plan: Plan, recipe: string) => plan.lines.filter((l) => l.recipe === recipe).reduce((a, l) => a + l.machines, 0);
const imported = (plan: Plan, item: string) => plan.items[item]?.imported ?? 0;

describe('Satisfactory golden builds', () => {
  it('Alclad 10/min nets scrap water and alumina silica', async () => {
    const plan = solveFactory(sat, factory(sat, { targets: [{ item: 'AluminumPlate', rate: 10 }] }), await highs());
    expect(plan.status).toBe('ok');
    expect(machines(plan, 'AluminumSheet')).toBeCloseTo(1 / 3, 4);
    expect(imported(plan, 'OreBauxite')).toBeCloseTo(10, 4);
    expect(imported(plan, 'Coal')).toBeCloseTo(5, 4);
    expect(imported(plan, 'RawQuartz')).toBeCloseTo(5, 4);
    expect(imported(plan, 'OreCopper')).toBeCloseTo(10 / 3, 4);
    expect(imported(plan, 'Water')).toBeCloseTo(10, 4);
  });

  it('Recycled plastic/rubber loop solves exactly', async () => {
    const f = factory(sat, {
      targets: [{ item: 'Plastic', rate: 60 }],
      recipeChoice: { Plastic: 'Alternate_Plastic_1', Rubber: 'Alternate_RecycledRubber' },
      imports: { items: ['LiquidFuel'], edges: [], exclude: [] },
    });
    const plan = solveFactory(sat, f, await highs());
    expect(machines(plan, 'Alternate_Plastic_1')).toBeCloseTo(4 / 3, 4);
    expect(machines(plan, 'Alternate_RecycledRubber')).toBeCloseTo(2 / 3, 4);
    // 6 fuel per 12 s cycle in game 1.0: 30/min per machine × 2 machines.
    expect(imported(plan, 'LiquidFuel')).toBeCloseTo(60, 4);
  });

  it('20 iron plates: 1 constructor + 1 smelter, 8 MW', async () => {
    const plan = solveFactory(sat, factory(sat, { targets: [{ item: 'IronPlate', rate: 20 }] }), await highs());
    expect(machines(plan, 'IronPlate')).toBeCloseTo(1, 6);
    expect(machines(plan, 'IngotIron')).toBeCloseTo(1, 6);
    expect(plan.powerMW).toBeCloseTo(8, 6);
  });

  it('nuclear power: a self-powered plant burns rods and makes waste', async () => {
    const f = factory(sat, { targets: [{ item: '@power', rate: 2500 }] });
    f.imports.exclude = ['@power'];
    f.imports.items = ['NuclearFuelRod'];
    f.recipeChoice = { '@power': Object.keys(sat.recipes).find((id) => id.startsWith('Power_GeneratorNuclear_NuclearFuelRod'))! };
    const plan = solveFactory(sat, f, await highs());
    expect(plan.status).toBe('ok');
    expect(imported(plan, 'NuclearFuelRod')).toBeCloseTo(0.2, 4);
    expect(plan.items.NuclearWaste.surplus).toBeCloseTo(10, 4);
    expect(plan.generatedMW).toBeCloseTo(2500, 4);
  });
});

describe('optimizer scale', () => {
  it('chooses among every unlocked alternate quickly', async () => {
    const lp = await highs();
    const unlocked = Object.values(sat.recipes).filter((r) => r.alternate).map((r) => r.id);
    const f = factory(sat, { targets: [{ item: 'SpaceElevatorPart_9', rate: 1 }], optimize: true });
    const t0 = performance.now();
    const plan = solveFactory(sat, f, lp, { unlocked, defaultBuilding: {}, spacing: 4 });
    const ms = performance.now() - t0;
    expect(plan.status).toBe('ok');
    expect(plan.lines.length).toBeGreaterThan(5);
    expect(ms).toBeLessThan(500);
  });

  it('Factorio: a rocket part solves with every recipe open', async () => {
    const fac = load('factorio');
    const f = factory(fac, { targets: [{ item: 'rocket-part', rate: 1 }], optimize: true });
    const t0 = performance.now();
    const plan = solveFactory(fac, f, await highs());
    expect(plan.status).toBe('ok');
    expect(performance.now() - t0).toBeLessThan(500);
  });
});

describe("can't balance", () => {
  // The case from real use: Plastic AI Limiter chosen, crude oil import turned off.
  const stuck = () =>
    factory(sat, {
      targets: [{ item: 'CircuitBoardHighSpeed', rate: 24 }],
      recipeChoice: { CircuitBoardHighSpeed: 'Alternate_AILimiter_Plastic' },
      imports: { items: [], edges: [], exclude: ['LiquidOil'] },
    });

  it('blames the raw item, not the target, even with a packaging loop', async () => {
    const plan = solveFactory(sat, stuck(), await highs());
    expect(plan.status).toBe('unbalanced');
    expect(plan.items.LiquidOil.unbalanced).toBeGreaterThan(0);
    expect(plan.items.CircuitBoardHighSpeed.unbalanced).toBeCloseTo(0);
    expect(plan.message).toContain('Crude Oil');
  });

  it('regenerate puts dependent items back on standard recipes', async () => {
    const lp = await highs();
    const f = stuck();
    const g = regenerate(f, solveFactory(sat, f, lp));
    expect(g.recipeChoice).toEqual({});
    const plan = solveFactory(sat, g, lp);
    expect(plan.status).toBe('ok');
    expect(plan.lines.some((l) => l.recipe === 'AILimiter')).toBe(true);
    // Nothing left to reset: same object back.
    expect(regenerate(g, plan)).toBe(g);
  });
});

describe('reset', () => {
  it('starts the plan over but keeps targets', async () => {
    const f = factory(sat, {
      name: 'mine',
      targets: [{ item: 'CircuitBoardHighSpeed', rate: 24 }],
      recipeChoice: { CircuitBoardHighSpeed: 'Alternate_AILimiter_Plastic' },
      lineSettings: { AILimiter: { clock: 2 } },
      lines: [{ id: 'x', recipe: 'Quickwire', machines: 3 }],
      imports: { items: ['Wire'], edges: ['Plastic>r:Alternate_AILimiter_Plastic'], exclude: ['LiquidOil'] },
    });
    const g = resetPlan(f);
    expect(g.name).toBe('mine');
    expect(g.targets).toEqual(f.targets);
    expect(g.recipeChoice).toEqual({});
    expect(g.lineSettings).toEqual({});
    expect(g.lines).toEqual([]);
    expect(g.imports).toEqual({ items: [], edges: [], exclude: [] });
    expect(solveFactory(sat, g, await highs()).status).toBe('ok');
  });
});

describe('canProduce', () => {
  it('knows crude oil can only be imported but iron ore can be converted', async () => {
    const lp = await highs();
    expect(canProduce(sat, 'LiquidOil', lp)).toBe(false);
    // Scrap's water byproduct needs more water upstream, so water is import-only too.
    expect(canProduce(sat, 'Water', lp)).toBe(false);
    expect(canProduce(sat, 'OreIron', lp)).toBe(true);
    expect(canProduce(sat, '@power', lp)).toBe(true);
    expect(canProduce(sat, 'IronPlate', lp)).toBe(true);
  });
});
