// Acceptance test for the T9 importer: imported build tabs must reproduce the
// workbook's own totals once solved by our engine.
import fs from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import type * as XLSX from 'xlsx';
import { solveFactory } from '../src/engine/solve.ts';
import { POWER, type GameData, type Plan } from '../src/engine/types.ts';
import {
  DEFAULT_WORKBOOK,
  buildRecipeMap,
  itemResolver,
  loadData,
  loadWorkbook,
  parseTab,
  readManualMap,
  readWorkbookRecipes,
  tabToFactory,
  type ParsedTab,
  type RecipeMapFile,
} from '../tools/import-t9.ts';
import { highs } from './fixtures.ts';

const haveWorkbook = fs.existsSync(DEFAULT_WORKBOOK);

let wb: XLSX.WorkBook;
let data: GameData;
let map: RecipeMapFile;

async function solved(name: string) {
  const tab = parseTab(wb.Sheets[name], name);
  if ('error' in tab) throw new Error(tab.error);
  const { factory, unmapped } = tabToFactory(tab, map, data, 'test');
  expect(unmapped).toEqual([]);
  return { tab, factory, plan: solveFactory(data, factory, await highs()) };
}

/**
 * Items whose workbook gross−/gross+ disagree with our consumed/produced by
 * more than 1%. Workbook rows are summed per game item, since it lists some
 * items under two names (MF and Modular Frame). Power is left out: the
 * workbook tracks draw in its own column, not as an item.
 */
function balanceMismatches(tab: ParsedTab, plan: Plan): string[] {
  const resolve = itemResolver(data);
  const wbById = new Map<string, { minus: number; plus: number; names: string[] }>();
  for (const [name, row] of tab.balance) {
    const id = resolve(name) ?? `?${name}`;
    if (id === POWER) continue;
    const acc = wbById.get(id) ?? { minus: 0, plus: 0, names: [] };
    acc.minus += row.minus;
    acc.plus += row.plus;
    acc.names.push(name);
    wbById.set(id, acc);
  }
  const close = (a: number, b: number) => Math.abs(a - b) <= Math.max(1e-6, 0.01 * Math.max(Math.abs(a), Math.abs(b)));
  const bad: string[] = [];
  for (const [id, wbRow] of wbById) {
    const ours = plan.items[id];
    const produced = ours?.produced ?? 0;
    const consumed = ours?.consumed ?? 0;
    if (!close(produced, wbRow.plus) || !close(consumed, -wbRow.minus)) {
      bad.push(`${wbRow.names.join('/')}: workbook −${-wbRow.minus}/+${wbRow.plus}, ours −${consumed}/+${produced}`);
    }
  }
  for (const [id, p] of Object.entries(plan.items)) {
    if (id !== POWER && !wbById.has(id) && (p.produced > 1e-6 || p.consumed > 1e-6)) bad.push(`${id}: not in workbook balance`);
  }
  return bad;
}

describe.skipIf(!haveWorkbook)('T9 import', () => {
  beforeAll(() => {
    wb = loadWorkbook();
    data = loadData();
    map = buildRecipeMap(data, readWorkbookRecipes(wb), readManualMap());
  }, 60_000);

  it('pasta reproduces the workbook size, foundations and item balance', async () => {
    const { tab, plan } = await solved('pasta');
    expect(tab.template).toBe('A');
    expect(plan.lines.map((l) => [l.recipe, l.machines])).toEqual([
      ['SpaceElevatorPart_9', 5], // Nuclear Pasta
      ['CopperDust', 10], // Copper Powder
      ['Alternate_PureCopperIngot', 80],
    ]);
    // Workbook: K1 = 21,360 m², "(1,335 foundations incl. support)" = size × 4 / 64.
    expect(tab.totalSize).toBe(21360);
    expect(plan.footprint).toBe(21360);
    expect(plan.foundations).toBeCloseTo(1335);
    expect(balanceMismatches(tab, plan)).toEqual([]);
  });

  it('babyhmf (template B, scale 6) matches the workbook item balance', async () => {
    // Template B sizes are size × fractional qty × 2.5; ours count whole
    // machines, so only the balance is compared here.
    const { tab, plan } = await solved('babyhmf');
    expect(tab.template).toBe('B');
    expect(tab.scale).toBe(6);
    expect(balanceMismatches(tab, plan)).toEqual([]);
  });

  it('mfg (template B with a SHARDED column) keeps production while splitting machines × clock', async () => {
    const { tab, factory, plan } = await solved('mfg');
    expect(tab.rows.length).toBeGreaterThan(0);
    for (const l of factory.lines) if (l.clock !== undefined) expect(l.clock).toBeLessThanOrEqual(2.5);
    expect(balanceMismatches(tab, plan)).toEqual([]);
  });
});

