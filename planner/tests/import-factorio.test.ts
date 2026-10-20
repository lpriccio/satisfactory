import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import type { GameData } from '../src/engine/types.ts';

const data: GameData = JSON.parse(readFileSync(new URL('../public/data/factorio.json', import.meta.url), 'utf8'));

describe('factorio game data', () => {
  it('has the electronic circuit recipe', () => {
    const r = data.recipes['electronic-circuit'];
    expect(r.time).toBe(0.5);
    expect(r.inputs).toEqual(
      expect.arrayContaining([
        { item: 'iron-plate', amount: 1 },
        { item: 'copper-cable', amount: 3 },
      ]),
    );
    expect(r.inputs).toHaveLength(2);
    expect(r.outputs).toEqual([{ item: 'electronic-circuit', amount: 1 }]);
    expect(r.buildings).toContain('assembling-machine-2');
    expect(r.allowProductivity).toBe(true);
  });

  it('has assembling machine 2 stats', () => {
    const b = data.buildings['assembling-machine-2'];
    expect(b.speed).toBe(0.75);
    expect(b.powerMW).toBeCloseTo(0.15);
    expect(b.footprint).toBe(9);
  });

  it('gives the foundry its built-in productivity', () => {
    expect(data.buildings.foundry.baseProductivity).toBe(0.5);
  });

  it('marks ores and water as raw default imports', () => {
    for (const id of ['iron-ore', 'water', 'crude-oil']) {
      expect(data.items[id].raw).toBe(true);
      expect(data.defaultImports).toContain(id);
    }
    expect(data.items.water.form).toBe('liquid');
  });

  it('applies probabilities and productivity exemptions', () => {
    const u = data.recipes['uranium-processing'];
    expect(u.outputs.find((s) => s.item === 'uranium-235')!.amount).toBeCloseTo(0.007);
    expect(u.outputs.find((s) => s.item === 'uranium-238')!.amount).toBeCloseTo(0.993);
    expect(data.recipes['kovarex-enrichment-process'].ignoredByProductivity).toEqual({ 'uranium-235': 40, 'uranium-238': 2 });
  });

  it('is internally consistent', () => {
    for (const r of Object.values(data.recipes)) {
      expect(r.buildings.length, r.id).toBeGreaterThan(0);
      for (const b of r.buildings) expect(data.buildings[b], `${r.id} -> ${b}`).toBeDefined();
      for (const s of [...r.inputs, ...r.outputs]) {
        expect(data.items[s.item], `${r.id} -> ${s.item}`).toBeDefined();
        expect(s.amount).toBeGreaterThan(0);
      }
      expect(r.time).toBeGreaterThan(0);
    }
    for (const i of Object.values(data.items)) {
      if (i.recipe) expect(data.recipes[i.recipe].outputs.some((s) => s.item === i.id), i.id).toBe(true);
      expect(i.name).not.toMatch(/__/);
    }
    for (const id of data.defaultImports) expect(data.items[id]).toBeDefined();
  });

  it('defaults to the same-name recipe', () => {
    expect(data.items['iron-plate'].recipe).toBe('iron-plate');
    expect(data.items['electronic-circuit'].recipe).toBe('electronic-circuit');
  });
});
