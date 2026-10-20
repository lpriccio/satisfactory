// Pure edits on a factory document. Each returns a new Factory; the UI keeps
// the old one for undo.

import { newId } from './factory.ts';
import { autoLineId, edgeKey, isDefaultImport } from './solve.ts';
import { POWER, type Factory, type GameData, type Line, type LineSettings, type Plan, type Target } from './types.ts';

function clone(f: Factory): Factory {
  return JSON.parse(JSON.stringify(f)) as Factory;
}

const without = <T>(list: T[], value: T) => list.filter((v) => v !== value);

function isDefaultKind(data: GameData, item: string): boolean {
  return item === POWER || data.defaultImports.includes(item);
}

/** Is `item` imported where `lineId` consumes it? */
export function occurrenceImported(data: GameData, f: Factory, item: string, lineId: string): boolean {
  return (
    f.imports.items.includes(item) ||
    isDefaultImport(data, f, item) ||
    f.imports.edges.includes(edgeKey(item, lineId))
  );
}

/** Is `item` imported everywhere it's consumed? */
export function itemImported(data: GameData, f: Factory, item: string): boolean {
  return f.imports.items.includes(item) || isDefaultImport(data, f, item);
}

function setItemImport(data: GameData, g: Factory, item: string, on: boolean) {
  g.imports.edges = g.imports.edges.filter((e) => !e.startsWith(`${item}>`));
  if (isDefaultKind(data, item)) {
    g.imports.exclude = on ? without(g.imports.exclude, item) : [...new Set([...g.imports.exclude, item])];
    g.imports.items = without(g.imports.items, item);
  } else {
    g.imports.items = on ? [...new Set([...g.imports.items, item])] : without(g.imports.items, item);
  }
}

/**
 * Click: toggle this one occurrence. Shift-click (`all`): set every occurrence
 * of the item to the clicked occurrence's new value.
 */
export function toggleImport(data: GameData, f: Factory, plan: Plan, item: string, lineId: string, all: boolean): Factory {
  const g = clone(f);
  const next = !occurrenceImported(data, f, item, lineId);
  if (all || isDefaultKind(data, item)) {
    // Raw items and power are imported or produced as a whole.
    setItemImport(data, g, item, next);
  } else if (g.imports.items.includes(item)) {
    // Imported everywhere -> produce here only: keep the other consumers imported per edge.
    g.imports.items = without(g.imports.items, item);
    for (const l of plan.lines) {
      if (l.id !== lineId && (l.rates[item] ?? 0) < 0) g.imports.edges.push(edgeKey(item, l.id));
    }
  } else {
    const key = edgeKey(item, lineId);
    g.imports.edges = next ? [...g.imports.edges, key] : without(g.imports.edges, key);
  }
  return g;
}

export function toggleItemImport(data: GameData, f: Factory, item: string): Factory {
  const g = clone(f);
  setItemImport(data, g, item, !itemImported(data, f, item));
  return g;
}

// --- targets ---------------------------------------------------------------

export function setTargets(f: Factory, targets: Target[]): Factory {
  const g = clone(f);
  g.targets = targets;
  return g;
}

// --- recipes ---------------------------------------------------------------

export function setRecipeChoice(f: Factory, item: string, recipe: string | undefined): Factory {
  const g = clone(f);
  if (recipe) g.recipeChoice[item] = recipe;
  else delete g.recipeChoice[item];
  return g;
}

// --- lines -----------------------------------------------------------------

function isAuto(lineId: string) {
  return lineId.startsWith('r:');
}

function renameEdges(g: Factory, from: string, to: string) {
  g.imports.edges = g.imports.edges.map((e) => (e.endsWith(`>${from}`) ? e.slice(0, -from.length) + to : e));
}

/** Change clock, sloops, building or bonuses on a line, auto or explicit. */
export function setLineSettings(f: Factory, lineId: string, recipe: string, patch: LineSettings): Factory {
  const g = clone(f);
  const clean = (s: LineSettings) => {
    for (const k of Object.keys(s) as (keyof LineSettings)[]) if (s[k] === undefined) delete s[k];
    return s;
  };
  if (isAuto(lineId)) {
    g.lineSettings[recipe] = clean({ ...(g.lineSettings[recipe] ?? {}), ...patch });
    if (!Object.keys(g.lineSettings[recipe]).length) delete g.lineSettings[recipe];
  } else {
    const line = g.lines.find((l) => l.id === lineId);
    if (line) Object.assign(line, patch), clean(line);
  }
  return g;
}

export function addLine(f: Factory, recipe: string, machines: number | null = 1): Factory {
  const g = clone(f);
  g.lines.push({ id: newId(), recipe, machines });
  return g;
}

export function updateLine(f: Factory, lineId: string, patch: Partial<Line>): Factory {
  const g = clone(f);
  const line = g.lines.find((l) => l.id === lineId);
  if (line) Object.assign(line, patch);
  return g;
}

export function removeLine(f: Factory, lineId: string): Factory {
  const g = clone(f);
  g.lines = g.lines.filter((l) => l.id !== lineId);
  g.imports.edges = g.imports.edges.filter((e) => !e.endsWith(`>${lineId}`));
  return g;
}

export function moveLine(f: Factory, lineId: string, delta: number): Factory {
  const g = clone(f);
  const i = g.lines.findIndex((l) => l.id === lineId);
  const j = i + delta;
  if (i < 0 || j < 0 || j >= g.lines.length) return f;
  [g.lines[i], g.lines[j]] = [g.lines[j], g.lines[i]];
  return g;
}

/** Target mode: pin an auto line at its current machine count. */
export function lockLine(f: Factory, plan: Plan, lineId: string): Factory {
  const pl = plan.lines.find((l) => l.id === lineId);
  if (!pl || !isAuto(lineId)) return f;
  const g = clone(f);
  const id = newId();
  g.lines.push({ id, recipe: pl.recipe, machines: Number(pl.machines.toFixed(6)), ...(g.lineSettings[pl.recipe] ?? {}) });
  renameEdges(g, lineId, id);
  return g;
}

/** Target mode: let the solver size a locked line again. */
export function unlockLine(f: Factory, lineId: string): Factory {
  const g = clone(f);
  const line = g.lines.find((l) => l.id === lineId);
  if (!line) return f;
  if (g.mode === 'manual') {
    line.machines = null;
    return g;
  }
  g.lines = g.lines.filter((l) => l.id !== lineId);
  const { clock, sloops, building, speedBonus, prodBonus } = line;
  const settings = Object.fromEntries(
    Object.entries({ clock, sloops, building, speedBonus, prodBonus }).filter(([, v]) => v !== undefined),
  );
  if (Object.keys(settings).length) g.lineSettings[line.recipe] = settings;
  renameEdges(g, lineId, autoLineId(line.recipe));
  return g;
}

/** Switch modes. Going to manual freezes the current plan into locked lines. */
export function setMode(f: Factory, plan: Plan, mode: Factory['mode']): Factory {
  if (f.mode === mode) return f;
  const g = clone(f);
  g.mode = mode;
  if (mode === 'manual') {
    const lines: Line[] = [];
    for (const pl of plan.lines) {
      const existing = f.lines.find((l) => l.id === pl.id);
      if (existing) {
        lines.push(existing);
        continue;
      }
      const id = newId();
      lines.push({ id, recipe: pl.recipe, machines: Number(pl.machines.toFixed(6)), ...(f.lineSettings[pl.recipe] ?? {}) });
      renameEdges(g, pl.id, id);
    }
    g.lines = lines;
  }
  return g;
}

/**
 * After a "can't balance": put every item that depends on a short item back
 * on its standard recipe, so the plan is re-filled from defaults. Walks from
 * each short item to the lines that consume it and on to what they make.
 * Returns the factory unchanged when there is nothing to reset.
 */
export function regenerate(f: Factory, plan: Plan): Factory {
  const short = Object.values(plan.items).filter((p) => p.unbalanced > 1e-6).map((p) => p.item);
  const seen = new Set<string>(short);
  const queue = [...short];
  while (queue.length) {
    const item = queue.shift()!;
    for (const l of plan.lines) {
      if ((l.rates[item] ?? 0) >= 0) continue;
      for (const [out, r] of Object.entries(l.rates)) {
        if (r > 0 && !seen.has(out)) {
          seen.add(out);
          queue.push(out);
        }
      }
    }
  }
  const reset = Object.keys(f.recipeChoice).filter((item) => seen.has(item));
  if (!reset.length) return f;
  const g = clone(f);
  for (const item of reset) delete g.recipeChoice[item];
  return g;
}

/**
 * Start the plan over: standard recipes, default imports, no per-line
 * settings or locked lines. Targets, name, notes, mode, objective and spacing
 * stay. Undo brings everything back.
 */
export function resetPlan(f: Factory): Factory {
  const g = clone(f);
  g.recipeChoice = {};
  g.lineSettings = {};
  g.optimize = false;
  g.imports = { items: [], edges: [], exclude: [] };
  if (g.mode === 'target') g.lines = [];
  return g;
}
