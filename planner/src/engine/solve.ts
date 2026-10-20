// Turns a factory document into a solved plan.
//
// Every recipe line is a variable (machine count). Each item gets one balance
// row: what lines make, minus what they eat, plus imports, minus surplus,
// equals the target. Byproducts, loops and shared intermediates therefore net
// out globally instead of being expanded branch by branch.

import { lineBuilding, machineRates, wholeMachines, type MachineRates } from './lineMath.ts';
import {
  POWER,
  type Factory,
  type GameData,
  type GameSettings,
  type Line,
  type LineSettings,
  type Plan,
  type PlanEdge,
  type PlanItem,
  type PlanLine,
  type Recipe,
} from './types.ts';

/** The slice of highs-js the engine needs. */
export interface LpSolver {
  solve(problem: string, options?: Record<string, unknown>): {
    Status: string;
    Columns: Record<string, { Primal: number }>;
  };
}

const EPS = 1e-6;
// Shortfalls are a last resort. One on an item with a recipe costs far more
// than one on an item without, so a gap shows up where it really is (the ore
// you excluded) rather than at the top of the chain.
const DEFICIT_COST = 1e5;
const DEFICIT_COST_PRODUCIBLE = 1e9;

interface WorkLine {
  line: Line;
  recipe: Recipe;
  auto: boolean;
  per: MachineRates;
  /** Variable name, or undefined when the machine count is locked. */
  col?: string;
  locked?: number;
}

// --- recipe lookups --------------------------------------------------------

const producerCache = new WeakMap<GameData, Map<string, string[]>>();

/** item id -> ids of recipes that output it, first-output recipes first. */
export function producers(data: GameData): Map<string, string[]> {
  let map = producerCache.get(data);
  if (map) return map;
  map = new Map();
  for (const r of Object.values(data.recipes)) {
    for (const [i, s] of r.outputs.entries()) {
      const list = map.get(s.item) ?? [];
      if (i === 0) list.unshift(r.id);
      else list.push(r.id);
      map.set(s.item, list);
    }
  }
  if (data.features.power) {
    const gens = Object.values(data.recipes).filter((r) => r.generatesMW).map((r) => r.id);
    if (gens.length) map.set(POWER, gens);
  }
  producerCache.set(data, map);
  return map;
}

export function defaultRecipe(data: GameData, item: string): string | undefined {
  const preset = data.items[item]?.recipe;
  if (preset && data.recipes[preset]) return preset;
  const list = producers(data).get(item) ?? [];
  return list.find((id) => !data.recipes[id].alternate) ?? list[0];
}

export function itemName(data: GameData, item: string): string {
  if (item === POWER) return 'Power (MW)';
  return data.items[item]?.name ?? item;
}

export function autoLineId(recipe: string): string {
  return `r:${recipe}`;
}

// --- import policy ---------------------------------------------------------

export function isDefaultImport(data: GameData, factory: Factory, item: string): boolean {
  if (factory.imports.exclude.includes(item)) return false;
  return item === POWER || data.defaultImports.includes(item);
}

export function isItemImported(data: GameData, factory: Factory, item: string): boolean {
  return factory.imports.items.includes(item) || isDefaultImport(data, factory, item);
}

export function edgeKey(item: string, lineId: string): string {
  return `${item}>${lineId}`;
}

// --- line set ---------------------------------------------------------------

function candidateRecipes(data: GameData, factory: Factory, settings: GameSettings | undefined, item: string): string[] {
  const all = producers(data).get(item) ?? [];
  if (!all.length) return [];
  if (factory.optimize) {
    const unlocked = new Set(settings?.unlocked ?? []);
    const pinned = factory.recipeChoice[item];
    const open = all.filter((id) => !data.recipes[id].alternate || unlocked.has(id) || id === pinned);
    return open.length ? open : all.slice(0, 1);
  }
  const chosen = factory.recipeChoice[item];
  if (chosen && data.recipes[chosen]) return [chosen];
  const def = defaultRecipe(data, item);
  return def ? [def] : [];
}

/** Lines the solver may run: explicit lines plus, in target mode, one auto line per candidate recipe. */
export function buildLines(data: GameData, factory: Factory, settings?: GameSettings): { lines: Line[]; auto: Set<string> } {
  const lines: Line[] = factory.lines.filter((l) => data.recipes[l.recipe]);
  const auto = new Set<string>();
  if (factory.mode !== 'target') return { lines, auto };

  // A locked line is capacity you already have; the solver may still add an
  // auto line of the same recipe for demand beyond it. A free explicit line
  // already gives the solver that freedom.
  const haveRecipe = new Set(lines.filter((l) => typeof l.machines !== 'number').map((l) => l.recipe));
  const seen = new Set<string>();
  const queue: string[] = factory.targets.map((t) => t.item);
  for (const l of lines) for (const s of data.recipes[l.recipe].inputs) queue.push(s.item);
  if (data.features.power && !isItemImported(data, factory, POWER)) queue.push(POWER);

  while (queue.length) {
    const item = queue.shift()!;
    if (seen.has(item)) continue;
    seen.add(item);
    if (isItemImported(data, factory, item)) continue;
    for (const rid of candidateRecipes(data, factory, settings, item)) {
      if (haveRecipe.has(rid)) continue;
      haveRecipe.add(rid);
      const id = autoLineId(rid);
      auto.add(id);
      lines.push({ id, recipe: rid, ...(factory.lineSettings[rid] ?? {}) });
      for (const s of data.recipes[rid].inputs) queue.push(s.item);
      // A machine that draws power may need power produced locally.
      if (data.features.power && !isItemImported(data, factory, POWER)) queue.push(POWER);
    }
  }
  return { lines, auto };
}

// --- LP ---------------------------------------------------------------------

function num(n: number): string {
  if (!Number.isFinite(n)) throw new Error(`non-finite coefficient ${n}`);
  // Plain decimal keeps the LP text parseable; 12 significant digits is plenty.
  return Number(n.toPrecision(12)).toString();
}

function linear(terms: [number, string][]): string {
  const parts: string[] = [];
  for (const [c, v] of terms) {
    if (Math.abs(c) < 1e-12) continue;
    const sign = c < 0 ? '-' : '+';
    parts.push(`${parts.length || sign === '-' ? sign + ' ' : ''}${num(Math.abs(c))} ${v}`);
  }
  return parts.length ? parts.join(' ') : '0 dummy';
}

interface Weights {
  machine: (w: WorkLine) => number;
  imported: (item: string) => number;
  surplus: number;
}

function weights(data: GameData, factory: Factory): Weights {
  const raw = (item: string) => {
    if (item === POWER) return 0;
    const w = data.resourceWeights?.[item];
    return w === undefined ? 1 : w;
  };
  // Manual mode may import anything, but sized lines should be used before
  // importing an item the factory doesn't normally import.
  const fallback = (i: string) => factory.mode === 'manual' && !isItemImported(data, factory, i);
  switch (factory.objective) {
    case 'min-power':
      return {
        machine: (w) => 1e-3 + Math.max(0, w.per.powerMW) * 1e-3,
        imported: (i) => (fallback(i) ? 1e4 : i === POWER ? 1 : 1e-3 * raw(i)),
        surplus: 1e-5,
      };
    case 'min-machines':
      return {
        machine: () => 1,
        imported: (i) => (fallback(i) ? 1e4 : i === POWER ? 1e-4 : 1e-3 * raw(i)),
        surplus: 1e-5,
      };
    default:
      return {
        machine: () => 1e-3,
        imported: (i) => (fallback(i) ? 1e4 : i === POWER ? 1e-4 : raw(i)),
        surplus: 1e-5,
      };
  }
}

export function solveFactory(
  data: GameData,
  factory: Factory,
  lp: LpSolver,
  settings?: GameSettings,
): Plan {
  const warnings: string[] = [];
  const { lines, auto } = buildLines(data, factory, settings);
  for (const l of factory.lines) if (!data.recipes[l.recipe]) warnings.push(`Unknown recipe "${l.recipe}" skipped.`);

  const work: WorkLine[] = lines.map((line, i) => {
    const recipe = data.recipes[line.recipe];
    const per = machineRates(data, recipe, line, settings?.defaultBuilding);
    const lockedCount = typeof line.machines === 'number' && Number.isFinite(line.machines) ? Math.max(0, line.machines) : undefined;
    return { line, recipe, auto: auto.has(line.id), per, locked: lockedCount, col: lockedCount === undefined ? `m${i}` : undefined };
  });

  // Items touched by the plan.
  const items = new Set<string>(factory.targets.map((t) => t.item));
  for (const w of work) for (const item of w.per.rates.keys()) items.add(item);

  if (!items.size) return { ...emptyPlan('', warnings), status: 'ok', message: undefined, foundations: data.features.clock ? 0 : undefined };

  const edgeImports = new Set(factory.imports.edges);
  const targetOf = new Map<string, number>();
  for (const t of factory.targets) targetOf.set(t.item, (targetOf.get(t.item) ?? 0) + t.rate);

  // Items nothing can make are imported unless the user excluded them, in
  // which case the plan reports them as unbalanced.
  const importable = (item: string) =>
    factory.mode === 'manual' ||
    isItemImported(data, factory, item) ||
    (!producers(data).get(item)?.length && !factory.imports.exclude.includes(item));

  const W = weights(data, factory);
  const objective: [number, string][] = [];
  const rows: string[] = [];
  const itemIndex = new Map<string, number>();
  [...items].forEach((item, k) => itemIndex.set(item, k));

  for (const w of work) if (w.col) objective.push([W.machine(w), w.col]);

  for (const item of items) {
    const k = itemIndex.get(item)!;
    const terms: [number, string][] = [];
    let rhs = targetOf.get(item) ?? 0;
    for (const w of work) {
      const rate = w.per.rates.get(item);
      if (!rate) continue;
      // An edge import cancels this line's own consumption of the item.
      const net = rate < 0 && edgeImports.has(edgeKey(item, w.line.id)) ? 0 : rate;
      if (w.col) terms.push([net, w.col]);
      else rhs -= net * (w.locked ?? 0);
    }
    if (importable(item)) {
      terms.push([1, `i${k}`]);
      objective.push([W.imported(item), `i${k}`]);
    } else {
      terms.push([1, `d${k}`]);
      // Raw items count as sources even when a recipe "makes" them: crude
      // oil's only recipe is unpackaging, which can't create oil.
      const source = isDefaultImport(data, { ...factory, imports: { ...factory.imports, exclude: [] } }, item) || !producers(data).get(item)?.length;
      objective.push([source ? DEFICIT_COST : DEFICIT_COST_PRODUCIBLE, `d${k}`]);
    }
    terms.push([-1, `s${k}`]);
    objective.push([W.surplus, `s${k}`]);
    rows.push(` b${k}: ${linear(terms)} = ${num(rhs)}`);
  }

  const problem = ['Minimize', ` obj: ${linear(objective)}`, 'Subject To', ...rows, 'End'].join('\n');

  let columns: Record<string, { Primal: number }>;
  try {
    const result = lp.solve(problem, { output_flag: false });
    if (result.Status !== 'Optimal') {
      return emptyPlan(`Solver stopped: ${result.Status}`, warnings);
    }
    columns = result.Columns;
  } catch (err) {
    return emptyPlan(`Solver error: ${(err as Error).message}`, warnings);
  }
  const value = (name: string) => {
    const v = columns[name]?.Primal ?? 0;
    return Math.abs(v) < 1e-9 ? 0 : v;
  };

  // --- assemble plan ---
  const planLines: PlanLine[] = [];
  for (const w of work) {
    const machines = w.col ? value(w.col) : (w.locked ?? 0);
    // Auto lines the solver left idle are noise; explicit lines always show.
    if (w.auto && machines < EPS) continue;
    const rates: Record<string, number> = {};
    for (const [item, r] of w.per.rates) rates[item] = r * machines;
    const settingsOut: LineSettings = {
      clock: w.line.clock,
      sloops: w.line.sloops,
      building: w.line.building,
      speedBonus: w.line.speedBonus,
      prodBonus: w.line.prodBonus,
    };
    planLines.push({
      id: w.line.id,
      recipe: w.recipe.id,
      building: lineBuilding(data, w.recipe, w.line, settings?.defaultBuilding).id,
      machines,
      locked: w.locked !== undefined,
      auto: w.auto,
      settings: settingsOut,
      rates,
      powerMW: w.per.powerMW * machines,
      footprint: w.per.footprint * wholeMachines(machines),
    });
  }

  const planItems: Record<string, PlanItem> = {};
  let unbalanced = false;
  for (const item of items) {
    const k = itemIndex.get(item)!;
    let produced = 0;
    let consumed = 0;
    let edgeImported = 0;
    for (const pl of planLines) {
      const r = pl.rates[item];
      if (!r) continue;
      if (r > 0) produced += r;
      else {
        consumed -= r;
        if (edgeImports.has(edgeKey(item, pl.id))) edgeImported -= r;
      }
    }
    const pi: PlanItem = {
      item,
      produced,
      consumed,
      imported: value(`i${k}`) + edgeImported,
      surplus: value(`s${k}`),
      target: targetOf.get(item) ?? 0,
      unbalanced: value(`d${k}`),
    };
    if (pi.unbalanced > EPS) unbalanced = true;
    planItems[item] = pi;
  }

  const consumedMW = planLines.reduce((a, l) => a + Math.max(0, l.powerMW), 0);
  const generatedMW = planLines.reduce((a, l) => a + Math.max(0, -l.powerMW), 0);
  const footprint = planLines.reduce((a, l) => a + l.footprint, 0);
  const missing = Object.values(planItems).filter((p) => p.unbalanced > EPS);

  return {
    status: unbalanced ? 'unbalanced' : 'ok',
    message: missing.length
      ? `Can't balance ${missing.map((p) => `${itemName(data, p.item)} (${round(p.unbalanced)}/min short)`).join(', ')}: nothing in this factory can supply ${missing.length > 1 ? 'them' : 'it'}, and ${missing.length > 1 ? "they aren't" : "it isn't"} imported.`
      : undefined,
    lines: planLines,
    items: planItems,
    edges: allocateEdges(planLines, planItems, edgeImports),
    powerMW: consumedMW - generatedMW,
    consumedMW,
    generatedMW,
    machines: planLines.reduce((a, l) => a + l.machines, 0),
    footprint,
    foundations: data.features.clock ? (footprint * factory.spacing) / 64 : undefined,
    warnings,
  };
}

function round(n: number): string {
  return Number(n.toFixed(2)).toString();
}

function emptyPlan(message: string, warnings: string[]): Plan {
  return {
    status: 'error',
    message,
    lines: [],
    items: {},
    edges: [],
    powerMW: 0,
    consumedMW: 0,
    generatedMW: 0,
    machines: 0,
    footprint: 0,
    warnings,
  };
}

/**
 * Split each item's supply across its consumers in proportion: every consumer
 * gets the same mix of producers and imports. Edge imports go straight from
 * 'import' to their consumer. Targets and surplus appear as consumers
 * 'target' and 'surplus'.
 */
export function allocateEdges(lines: PlanLine[], items: Record<string, PlanItem>, edgeImports: Set<string>): PlanEdge[] {
  const edges: PlanEdge[] = [];
  for (const pi of Object.values(items)) {
    const item = pi.item;
    const sources: [string, number][] = [];
    const sinks: [string, number][] = [];
    let edgeImported = 0;
    for (const l of lines) {
      const r = l.rates[item];
      if (!r) continue;
      if (r > 0) sources.push([l.id, r]);
      else if (edgeImports.has(edgeKey(item, l.id))) {
        edges.push({ item, from: 'import', to: l.id, rate: -r });
        edgeImported -= r;
      } else sinks.push([l.id, -r]);
    }
    const pooledImport = pi.imported - edgeImported;
    if (pooledImport > EPS) sources.push(['import', pooledImport]);
    if (pi.target > EPS) sinks.push(['target', pi.target]);
    if (pi.surplus > EPS) sinks.push(['surplus', pi.surplus]);
    const supply = sources.reduce((a, [, r]) => a + r, 0);
    if (supply < EPS) continue;
    for (const [to, need] of sinks) {
      for (const [from, have] of sources) {
        const rate = (need * have) / supply;
        if (rate > EPS) edges.push({ item, from, to, rate });
      }
    }
  }
  return edges;
}

const producibleCache = new WeakMap<GameData, Map<string, boolean>>();

/**
 * Can anything actually make `item` from other sources? Crude oil "has" a
 * recipe (unpackaging) that needs crude oil, so the answer there is no. Tested
 * by asking the solver for 1/min of the item with its import turned off and
 * every recipe open.
 */
export function canProduce(data: GameData, item: string, lp: LpSolver): boolean {
  let cache = producibleCache.get(data);
  if (!cache) producibleCache.set(data, (cache = new Map()));
  const known = cache.get(item);
  if (known !== undefined) return known;
  let ok = false;
  if (producers(data).get(item)?.length) {
    const now = new Date(0).toISOString();
    const probe: Factory = {
      schema: 1,
      id: 'probe',
      game: data.game,
      name: 'probe',
      mode: 'target',
      objective: 'min-raw',
      targets: [{ item, rate: 1 }],
      recipeChoice: {},
      optimize: true,
      lineSettings: {},
      lines: [],
      imports: { items: [], edges: [], exclude: [item] },
      spacing: 4,
      createdAt: now,
      updatedAt: now,
    };
    const unlocked = Object.values(data.recipes).filter((r) => r.alternate).map((r) => r.id);
    ok = solveFactory(data, probe, lp, { unlocked, defaultBuilding: {}, spacing: 4 }).status === 'ok';
  }
  cache.set(item, ok);
  return ok;
}
