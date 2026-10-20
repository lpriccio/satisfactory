import { describe, expect, it } from 'vitest';
import { solveFactory } from '../src/engine/solve.ts';
import { combinePlans, worldToFactory } from '../src/engine/world.ts';
import { newWorld } from '../src/engine/factory.ts';
import { POWER, type Plan } from '../src/engine/types.ts';
import { factory, highs, miniSatisfactory } from './fixtures.ts';

const data = miniSatisfactory();
const machinesOf = (plan: Plan, recipe: string) =>
  plan.lines.filter((l) => l.recipe === recipe).reduce((a, l) => a + l.machines, 0);
const imported = (plan: Plan, item: string) => plan.items[item]?.imported ?? 0;

describe('target mode', () => {
  it('sizes a simple chain and its power', async () => {
    const f = factory(data, { targets: [{ item: 'IronPlate', rate: 20 }] });
    const plan = solveFactory(data, f, await highs());
    expect(plan.status).toBe('ok');
    expect(machinesOf(plan, 'IronPlate')).toBeCloseTo(1);
    expect(machinesOf(plan, 'IngotIron')).toBeCloseTo(1);
    expect(imported(plan, 'OreIron')).toBeCloseTo(30);
    expect(plan.powerMW).toBeCloseTo(8);
    expect(plan.footprint).toBe(54 + 80);
    expect(plan.foundations).toBeCloseTo(((54 + 80) * 4) / 64);
  });

  it('nets byproducts globally (Alclad 10/min)', async () => {
    // Hand-checked: scrap's water and alumina's silica offset demand elsewhere.
    const f = factory(data, { targets: [{ item: 'AluminumPlate', rate: 10 }] });
    const plan = solveFactory(data, f, await highs());
    expect(plan.status).toBe('ok');
    expect(machinesOf(plan, 'AluminumSheet')).toBeCloseTo(1 / 3);
    expect(machinesOf(plan, 'IngotAluminum')).toBeCloseTo(1 / 6);
    expect(machinesOf(plan, 'AluminumScrap')).toBeCloseTo(1 / 24);
    expect(machinesOf(plan, 'AluminaSolution')).toBeCloseTo(1 / 12);
    expect(machinesOf(plan, 'Silica')).toBeCloseTo(8.3333333 / 37.5);
    expect(imported(plan, 'OreBauxite')).toBeCloseTo(10);
    expect(imported(plan, 'Coal')).toBeCloseTo(5);
    expect(imported(plan, 'RawQuartz')).toBeCloseTo(5);
    expect(imported(plan, 'OreCopper')).toBeCloseTo(10 / 3);
    expect(imported(plan, 'Water')).toBeCloseTo(10);
    expect(plan.items.Silica.surplus).toBeCloseTo(0);
  });

  it('solves recipe loops instead of cutting them', async () => {
    const f = factory(data, {
      targets: [{ item: 'Plastic', rate: 60 }],
      recipeChoice: { Plastic: 'Alternate_RecycledPlastic', Rubber: 'Alternate_RecycledRubber' },
      imports: { items: ['LiquidFuel'], edges: [], exclude: [] },
    });
    const plan = solveFactory(data, f, await highs());
    expect(machinesOf(plan, 'Alternate_RecycledPlastic')).toBeCloseTo(4 / 3);
    expect(machinesOf(plan, 'Alternate_RecycledRubber')).toBeCloseTo(2 / 3);
    expect(imported(plan, 'LiquidFuel')).toBeCloseTo(120);
    expect(imported(plan, 'Plastic')).toBeCloseTo(0);
  });

  it('applies clock power exponent and somersloops', async () => {
    const f = factory(data, {
      targets: [{ item: 'IronIngot', rate: 75 }],
      lineSettings: { IngotIron: { clock: 2.5 } },
    });
    const plan = solveFactory(data, f, await highs());
    expect(machinesOf(plan, 'IngotIron')).toBeCloseTo(1);
    expect(plan.powerMW).toBeCloseTo(4 * Math.pow(2.5, 1.321928));

    const sloop = factory(data, {
      targets: [{ item: 'IronIngot', rate: 60 }],
      lineSettings: { IngotIron: { sloops: 1 } },
    });
    const p2 = solveFactory(data, sloop, await highs());
    expect(machinesOf(p2, 'IngotIron')).toBeCloseTo(1);
    expect(imported(p2, 'OreIron')).toBeCloseTo(30);
    expect(p2.powerMW).toBeCloseTo(16);
  });

  it('imports one consumer edge without importing the item everywhere', async () => {
    const f = factory(data, {
      targets: [
        { item: 'AluminumPlate', rate: 30 },
        { item: 'CopperIngot', rate: 15 },
      ],
      imports: { items: [], edges: ['CopperIngot>r:AluminumSheet'], exclude: [] },
    });
    const plan = solveFactory(data, f, await highs());
    // The sheet line's 10/min copper is imported; the 15/min target is smelted.
    expect(plan.items.CopperIngot.imported).toBeCloseTo(10);
    expect(plan.items.CopperIngot.produced).toBeCloseTo(15);
    expect(imported(plan, 'OreCopper')).toBeCloseTo(15);
    const edge = plan.edges.find((e) => e.item === 'CopperIngot' && e.to === 'r:AluminumSheet');
    expect(edge).toMatchObject({ from: 'import' });
    expect(edge!.rate).toBeCloseTo(10);
  });

  it('reports an item it cannot balance instead of failing', async () => {
    const f = factory(data, {
      targets: [{ item: 'IronIngot', rate: 30 }],
      imports: { items: [], edges: [], exclude: ['OreIron'] },
    });
    const plan = solveFactory(data, f, await highs());
    expect(plan.status).toBe('unbalanced');
    expect(plan.items.OreIron.unbalanced).toBeCloseTo(30);
    expect(plan.message).toContain('OreIron');
  });

  it('builds generators when grid power is not imported', async () => {
    const f = factory(data, {
      targets: [{ item: 'IronIngot', rate: 30 }],
      imports: { items: [], edges: [], exclude: [POWER] },
    });
    const plan = solveFactory(data, f, await highs());
    expect(plan.status).toBe('ok');
    expect(machinesOf(plan, 'Power_GeneratorCoal_Coal')).toBeCloseTo(4 / 75);
    expect(plan.powerMW).toBeCloseTo(0);
    expect(imported(plan, 'Coal')).toBeCloseTo((15 * 4) / 75);
  });
});

describe('manual mode', () => {
  it('balances typed lines and imports the shortfall', async () => {
    const f = factory(data, {
      mode: 'manual',
      lines: [
        { id: 'a', recipe: 'IronPlate', machines: 2 },
        { id: 'b', recipe: 'IngotIron', machines: 1 },
      ],
    });
    const plan = solveFactory(data, f, await highs());
    expect(plan.items.IronPlate.surplus).toBeCloseTo(40);
    expect(plan.items.IronIngot.imported).toBeCloseTo(30);
    expect(plan.items.OreIron.imported).toBeCloseTo(30);
  });

  it('sizes unlocked lines to meet targets before importing', async () => {
    const f = factory(data, {
      mode: 'manual',
      targets: [{ item: 'IronPlate', rate: 40 }],
      lines: [
        { id: 'a', recipe: 'IronPlate', machines: 2 },
        { id: 'b', recipe: 'IngotIron', machines: null },
      ],
    });
    const plan = solveFactory(data, f, await highs());
    expect(machinesOf(plan, 'IngotIron')).toBeCloseTo(2);
    expect(plan.items.IronIngot.imported).toBeCloseTo(0);
  });
});

describe('world', () => {
  it('scales and sums plans, and flattening keeps every line', async () => {
    const lp = await highs();
    const a = factory(data, { name: 'plates', targets: [{ item: 'IronPlate', rate: 20 }] });
    const b = factory(data, { name: 'copper', targets: [{ item: 'CopperIngot', rate: 30 }] });
    const pa = solveFactory(data, a, lp);
    const pb = solveFactory(data, b, lp);
    const totals = combinePlans([
      { plan: pa, multiplier: 2, spacing: 4 },
      { plan: pb, multiplier: 1, spacing: 4 },
    ]);
    expect(totals.powerMW).toBeCloseTo(8 * 2 + 4);
    expect(totals.items.OreIron.imported).toBeCloseTo(60);

    let n = 0;
    const flat = worldToFactory(
      newWorld('satisfactory'),
      [
        { factory: a, plan: pa, multiplier: 2 },
        { factory: b, plan: pb, multiplier: 1 },
      ],
      () => `id${n++}`,
      'now',
    );
    expect(flat.lines).toHaveLength(3);
    const pf = solveFactory(data, flat, lp);
    expect(pf.powerMW).toBeCloseTo(totals.powerMW);
    expect(pf.items.IronPlate.surplus).toBeCloseTo(40);
  });
});
