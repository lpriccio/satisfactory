import type { Factory, Line, Plan, PlanItem, World } from './types.ts';

export interface WorldTotals {
  items: Record<string, PlanItem>;
  powerMW: number;
  consumedMW: number;
  generatedMW: number;
  machines: number;
  footprint: number;
  foundations?: number;
}

/** Sum solved plans, each scaled by its multiplier. Plans are linear, so scaling is exact. */
export function combinePlans(parts: { plan: Plan; multiplier: number; spacing: number }[]): WorldTotals {
  const items: Record<string, PlanItem> = {};
  const totals: WorldTotals = { items, powerMW: 0, consumedMW: 0, generatedMW: 0, machines: 0, footprint: 0 };
  let foundations: number | undefined;
  for (const { plan, multiplier: k, spacing } of parts) {
    for (const p of Object.values(plan.items)) {
      const acc = (items[p.item] ??= { item: p.item, produced: 0, consumed: 0, imported: 0, surplus: 0, target: 0, unbalanced: 0 });
      acc.produced += p.produced * k;
      acc.consumed += p.consumed * k;
      acc.imported += p.imported * k;
      acc.surplus += p.surplus * k;
      acc.target += p.target * k;
      acc.unbalanced += p.unbalanced * k;
    }
    totals.powerMW += plan.powerMW * k;
    totals.consumedMW += plan.consumedMW * k;
    totals.generatedMW += plan.generatedMW * k;
    totals.machines += plan.machines * k;
    totals.footprint += plan.footprint * k;
    if (plan.foundations !== undefined) foundations = (foundations ?? 0) + (plan.footprint * k * spacing) / 64;
  }
  totals.foundations = foundations;
  return totals;
}

/**
 * Net position of the world once factories feed each other: an item one
 * factory exports (target or surplus) offsets another factory's import.
 */
export function netPosition(items: Record<string, PlanItem>): { item: string; net: number }[] {
  return Object.values(items)
    .map((p) => ({ item: p.item, net: p.target + p.surplus - p.imported }))
    .filter((r) => Math.abs(r.net) > 1e-6);
}

/** Flatten a world into one manual factory that keeps every line, scaled and locked. */
export function worldToFactory(
  world: World,
  members: { factory: Factory; plan: Plan; multiplier: number }[],
  newId: () => string,
  now: string,
): Factory {
  const lines: Line[] = [];
  for (const { factory, plan, multiplier } of members) {
    for (const pl of plan.lines) {
      if (pl.machines * multiplier < 1e-9) continue;
      lines.push({
        id: newId(),
        recipe: pl.recipe,
        machines: pl.machines * multiplier,
        ...stripUndefined(pl.settings),
        note: `from ${factory.name}${multiplier !== 1 ? ` ×${multiplier}` : ''}`,
      });
    }
  }
  const first = members[0]?.factory;
  return {
    schema: 1,
    id: newId(),
    game: world.game,
    name: `${world.name} (combined)`,
    notes: `Combined from: ${members.map((m) => `${m.factory.name} ×${m.multiplier}`).join(', ')}`,
    mode: 'manual',
    objective: 'min-raw',
    targets: [],
    recipeChoice: {},
    lineSettings: {},
    lines,
    imports: { items: [], edges: [], exclude: [] },
    spacing: first?.spacing ?? 4,
    createdAt: now,
    updatedAt: now,
  };
}

function stripUndefined<T extends object>(o: T): Partial<T> {
  return Object.fromEntries(Object.entries(o).filter(([, v]) => v !== undefined)) as Partial<T>;
}
