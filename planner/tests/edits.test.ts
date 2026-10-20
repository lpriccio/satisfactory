import { describe, expect, it } from 'vitest';
import { lockLine, occurrenceImported, setMode, toggleImport, unlockLine } from '../src/engine/edits.ts';
import { solveFactory } from '../src/engine/solve.ts';
import { factory, highs, miniSatisfactory } from './fixtures.ts';

const data = miniSatisfactory();

// Two lines consume Copper Ingot: the Alclad sheet line and a locked second sheet line.
async function setup() {
  const f = factory(data, {
    targets: [{ item: 'AluminumPlate', rate: 60 }],
    lines: [{ id: 'extra', recipe: 'AluminumSheet', machines: 1 }],
  });
  const lp = await highs();
  return { f, lp, plan: solveFactory(data, f, lp) };
}

describe('import toggles (notes.md bug)', () => {
  it('click toggles only the clicked occurrence', async () => {
    const { f, lp, plan } = await setup();
    const g = toggleImport(data, f, plan, 'CopperIngot', 'extra', false);
    expect(occurrenceImported(data, g, 'CopperIngot', 'extra')).toBe(true);
    expect(occurrenceImported(data, g, 'CopperIngot', 'r:AluminumSheet')).toBe(false);
    const p2 = solveFactory(data, g, lp);
    expect(p2.items.CopperIngot.imported).toBeCloseTo(10);
  });

  it('shift-click sets every occurrence to the clicked one\'s new value', async () => {
    const { f, lp, plan } = await setup();
    // One occurrence already imported, the other not: shift-click the produced one.
    const one = toggleImport(data, f, plan, 'CopperIngot', 'extra', false);
    const all = toggleImport(data, one, solveFactory(data, one, lp), 'CopperIngot', 'r:AluminumSheet', true);
    expect(all.imports.items).toContain('CopperIngot');
    expect(all.imports.edges).toHaveLength(0);
    // Shift-click again on an imported occurrence turns them all off.
    const none = toggleImport(data, all, solveFactory(data, all, lp), 'CopperIngot', 'extra', true);
    expect(none.imports.items).not.toContain('CopperIngot');
    expect(occurrenceImported(data, none, 'CopperIngot', 'extra')).toBe(false);
  });

  it('click on an item imported everywhere produces it for that consumer only', async () => {
    const { f, lp } = await setup();
    const imported = { ...f, imports: { ...f.imports, items: ['CopperIngot'] } };
    const plan = solveFactory(data, imported, lp);
    const g = toggleImport(data, imported, plan, 'CopperIngot', 'extra', false);
    expect(occurrenceImported(data, g, 'CopperIngot', 'extra')).toBe(false);
    expect(occurrenceImported(data, g, 'CopperIngot', 'r:AluminumSheet')).toBe(true);
    const p2 = solveFactory(data, g, lp);
    expect(p2.items.CopperIngot.imported).toBeCloseTo(10);
    expect(p2.items.CopperIngot.produced).toBeCloseTo(10);
  });

  it('counts imported consumption once in the balance', async () => {
    const { f, lp } = await setup();
    const g = { ...f, imports: { ...f.imports, items: ['CopperIngot'] } };
    const p = solveFactory(data, g, lp);
    expect(p.items.CopperIngot.consumed).toBeCloseTo(20);
    expect(p.items.CopperIngot.imported).toBeCloseTo(20);
  });
});

describe('locking and modes', () => {
  it('lock keeps machines and edge imports; unlock restores the auto line', async () => {
    const { f, lp, plan } = await setup();
    const edged = toggleImport(data, f, plan, 'CopperIngot', 'r:AluminumSheet', false);
    const locked = lockLine(edged, solveFactory(data, edged, lp), 'r:IngotAluminum');
    const lockedId = locked.lines.find((l) => l.recipe === 'IngotAluminum')!.id;
    const p2 = solveFactory(data, locked, lp);
    expect(p2.lines.find((l) => l.id === lockedId)?.locked).toBe(true);
    const back = unlockLine(locked, lockedId);
    expect(back.lines.some((l) => l.recipe === 'IngotAluminum')).toBe(false);
    expect(solveFactory(data, back, lp).lines.some((l) => l.id === 'r:IngotAluminum')).toBe(true);
  });

  it('switching to manual freezes the plan', async () => {
    const { f, lp, plan } = await setup();
    const m = setMode(f, plan, 'manual');
    const p2 = solveFactory(data, m, lp);
    expect(p2.lines).toHaveLength(plan.lines.length);
    expect(p2.items.AluminumPlate.target).toBeCloseTo(60);
    expect(p2.items.AluminumPlate.produced).toBeCloseTo(plan.items.AluminumPlate.produced);
  });
});
