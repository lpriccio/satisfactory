import { defaultRecipe, itemName, producers } from '../engine/solve.ts';
import type { GameData, Plan, PlanLine } from '../engine/types.ts';
import type { Option } from './Picker.svelte';

export function lineLabel(data: GameData, plan: Plan, id: string): string {
  if (id === 'import') return 'import';
  if (id === 'target') return 'target';
  if (id === 'surplus') return 'surplus';
  const l = plan.lines.find((x) => x.id === id);
  return l ? (data.recipes[l.recipe]?.name ?? l.recipe) : id;
}

/** The item a line is "for": its recipe's first output. */
export function mainItem(data: GameData, line: PlanLine): string {
  return data.recipes[line.recipe]?.outputs[0]?.item ?? '';
}

/** Recipes able to make an item, as picker options with an alternate hint. */
export function recipeOptionsFor(data: GameData, item: string): Option[] {
  const def = defaultRecipe(data, item);
  return (producers(data).get(item) ?? []).map((id) => {
    const r = data.recipes[id];
    const byproduct = r.outputs[0]?.item !== item;
    const hint = [id === def ? 'default' : r.alternate ? 'alternate' : '', byproduct ? `byproduct of ${itemName(data, r.outputs[0]?.item)}` : '']
      .filter(Boolean)
      .join(' · ');
    return { id, label: r.name, hint: hint || undefined };
  });
}

let allRecipeCache: { data: GameData; options: Option[] } | undefined;
export function allRecipeOptions(data: GameData): Option[] {
  if (allRecipeCache?.data === data) return allRecipeCache.options;
  const options = Object.values(data.recipes)
    .map((r) => ({
      id: r.id,
      label: r.name,
      hint: [r.alternate ? 'alternate' : '', r.outputs.map((o) => itemName(data, o.item)).join(', ')].filter(Boolean).join(' · '),
    }))
    .sort((a, b) => a.label.localeCompare(b.label));
  allRecipeCache = { data, options };
  return options;
}

/**
 * Order lines from the targets down: a line's depth is one more than the
 * deepest line it feeds. Loops are cut after one pass per line.
 */
export function orderLines(plan: Plan): PlanLine[] {
  const depth = new Map<string, number>();
  for (const e of plan.edges) if (e.to === 'target' || e.to === 'surplus') depth.set(e.from, 0);
  const feeds = new Map<string, Set<string>>();
  for (const e of plan.edges) {
    if (e.from === 'import' || e.to === 'target' || e.to === 'surplus') continue;
    if (!feeds.has(e.to)) feeds.set(e.to, new Set());
    feeds.get(e.to)!.add(e.from);
  }
  for (let pass = 0; pass < plan.lines.length; pass++) {
    let changed = false;
    for (const [consumer, suppliers] of feeds) {
      const d = depth.get(consumer);
      if (d === undefined) continue;
      for (const s of suppliers) {
        if ((depth.get(s) ?? -1) < d + 1 && d + 1 <= plan.lines.length) {
          depth.set(s, d + 1);
          changed = true;
        }
      }
    }
    if (!changed) break;
  }
  const max = Math.max(0, ...depth.values()) + 1;
  return [...plan.lines].sort((a, b) => (depth.get(a.id) ?? max) - (depth.get(b.id) ?? max));
}
