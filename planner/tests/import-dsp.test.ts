import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { solveFactory } from '../src/engine/solve.ts';
import type { GameData } from '../src/engine/types.ts';
import { factory, highs } from './fixtures.ts';

const data: GameData = JSON.parse(readFileSync(new URL('../public/data/dsp.json', import.meta.url), 'utf8'));

describe('dsp data', () => {
  it('round-trips recipes from the TSV', () => {
    expect(data.recipes.CB).toMatchObject({
      time: 1,
      inputs: [
        { item: 'Iron', amount: 2 },
        { item: 'Copper', amount: 1 },
      ],
      outputs: [{ item: 'CB', amount: 2 }],
      buildings: ['producer'],
    });
    expect(data.recipes.Processor).toMatchObject({
      time: 3,
      inputs: [
        { item: 'CB', amount: 2 },
        { item: 'Micro', amount: 2 },
      ],
      outputs: [{ item: 'Processor', amount: 1 }],
    });
  });

  it('references only known items and buildings', () => {
    for (const r of Object.values(data.recipes)) {
      for (const b of r.buildings) expect(data.buildings[b], `${r.id} building ${b}`).toBeDefined();
      for (const s of [...r.inputs, ...r.outputs]) expect(data.items[s.item], `${r.id} item ${s.item}`).toBeDefined();
    }
    for (const id of data.defaultImports) expect(data.items[id].raw).toBe(true);
    expect(data.items.Processor.recipe).toBe('Processor');
  });

  it('solves a small target', async () => {
    // 1 Processor/min: 3 s recipe -> 1/20 machine; 2 CB + 2 Micro per processor.
    const plan = solveFactory(data, factory(data, { targets: [{ item: 'Processor', rate: 1 }] }), await highs());
    expect(plan.status).toBe('ok');
    const line = (id: string) => plan.lines.find((l) => l.recipe === id)!;
    expect(line('Processor').machines).toBeCloseTo(1 / 20);
    expect(plan.items.CB.produced).toBeCloseTo(2);
    expect(plan.items.Micro.produced).toBeCloseTo(2);
    expect(line('CB').machines).toBeCloseTo(2 / 120);

    const boosted = solveFactory(
      data,
      factory(data, { targets: [{ item: 'Processor', rate: 1 }], lineSettings: { Processor: { prodBonus: 0.25 } } }),
      await highs(),
    );
    expect(boosted.items.CB.consumed).toBeCloseTo(2 / 1.25);
  });
});
