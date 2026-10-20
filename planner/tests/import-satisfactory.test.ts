import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import type { GameData } from '../src/engine/types.ts';

const data: GameData = JSON.parse(
  readFileSync(new URL('../public/data/satisfactory.json', import.meta.url), 'utf8'),
);
const R = data.recipes;
const stacks = (s: { item: string; amount: number }[]) =>
  Object.fromEntries(s.map((x) => [x.item, x.amount]));

describe('satisfactory game data', () => {
  it('Iron Ingot: 1 ore → 1 ingot in 2 s at the smelter', () => {
    const r = R.IngotIron;
    expect(r.time).toBe(2);
    expect(stacks(r.inputs)).toEqual({ OreIron: 1 });
    expect(stacks(r.outputs)).toEqual({ IronIngot: 1 });
    expect(r.buildings).toEqual(['SmelterMk1']);
  });

  it('Alclad Aluminum Sheet: 3 aluminum + 1 copper ingot → 3 sheets in 6 s', () => {
    const r = R.AluminumSheet;
    expect(r.alternate).toBeUndefined();
    expect(r.time).toBe(6);
    expect(stacks(r.inputs)).toEqual({ AluminumIngot: 3, CopperIngot: 1 });
    expect(stacks(r.outputs)).toEqual({ AluminumPlate: 3 });
    expect(data.items.AluminumPlate.recipe).toBe('AluminumSheet');
  });

  it('byproducts survive import, fluids in m³', () => {
    expect(stacks(R.AluminumScrap.outputs)).toEqual({ AluminumScrap: 6, Water: 2 });
    expect(stacks(R.AluminaSolution.outputs)).toEqual({ AluminaSolution: 12, Silica: 5 });
  });

  it('generators: coal 45 m³/min water, nuclear 0.2 rods and 10 waste per minute', () => {
    const coal = R.Power_GeneratorCoal_Coal;
    expect(coal.generatesMW).toBe(75);
    expect(stacks(coal.inputs)).toEqual({ Coal: 15, Water: 45 });
    const nuke = R.Power_GeneratorNuclear_NuclearFuelRod;
    expect(nuke.generatesMW).toBe(2500);
    expect(stacks(nuke.inputs).NuclearFuelRod).toBeCloseTo(0.2);
    expect(stacks(nuke.inputs).Water).toBeCloseTo(240);
    expect(stacks(nuke.outputs).NuclearWaste).toBeCloseTo(10);
  });

  it('fuel energy: coal 300 MJ, liquid fuel 750 MJ/m³', () => {
    expect(data.items.Coal.energyMJ).toBe(300);
    expect(data.items.LiquidFuel.energyMJ).toBe(750);
  });

  it('constructor: 1 sloop slot, clock exponent ≈ 1.321928', () => {
    const b = data.buildings.ConstructorMk1;
    expect(b.sloopSlots).toBe(1);
    expect(b.powerExponent).toBeCloseTo(1.321928, 5);
    expect(b.powerMW).toBe(4);
    expect(b.footprint).toBe(80);
  });

  it('variable-power recipes carry a min/max', () => {
    expect(R.Plutonium.power).toEqual({ min: 250, max: 750 });
  });

  it('defaults: raw items imported, byproduct-only items not defaulted to their parent', () => {
    expect(data.defaultImports).toContain('Water');
    expect(data.defaultImports).toContain('OreIron');
    expect(data.items.Water.recipe).toBeUndefined();
    expect(data.items.CompactedCoal.recipe).toBe('Alternate_EnrichedCoal');
  });

  it('every reference resolves', () => {
    for (const r of Object.values(R)) {
      expect(r.buildings.length, r.id).toBeGreaterThan(0);
      for (const b of r.buildings) expect(data.buildings[b], `${r.id} → ${b}`).toBeDefined();
      for (const s of [...r.inputs, ...r.outputs]) {
        expect(data.items[s.item], `${r.id} → ${s.item}`).toBeDefined();
      }
    }
    for (const i of Object.values(data.items)) {
      if (i.recipe) expect(R[i.recipe], `${i.id} default`).toBeDefined();
    }
    for (const b of Object.values(data.buildings)) expect(b.footprint, b.id).toBeGreaterThan(0);
  });
});
