import { POWER, type Building, type GameData, type LineSettings, type Recipe } from './types.ts';

export const MIN_CLOCK = 0.01;
export const MAX_CLOCK = 2.5;

export interface MachineRates {
  /** Per-minute net flow of one machine by item, including POWER. */
  rates: Map<string, number>;
  /** MW per machine; negative for generators. */
  powerMW: number;
  footprint: number;
  building: Building;
}

const UNKNOWN_BUILDING: Building = { id: '?', name: 'Unknown building', powerMW: 0 };

export function lineBuilding(data: GameData, recipe: Recipe, settings: LineSettings, preferred?: Record<string, string>): Building {
  const chosen = settings.building && recipe.buildings.includes(settings.building) ? settings.building : undefined;
  const byCategory = recipe.category ? preferred?.[recipe.category] : undefined;
  const id = chosen ?? (byCategory && recipe.buildings.includes(byCategory) ? byCategory : recipe.buildings[0]);
  return (id && data.buildings[id]) || UNKNOWN_BUILDING;
}

export function clampClock(clock: number | undefined): number {
  if (clock === undefined || !Number.isFinite(clock)) return 1;
  return Math.min(MAX_CLOCK, Math.max(MIN_CLOCK, clock));
}

/** Somersloop amplification factor: 1 + filled / slots. */
export function amplification(building: Building, sloops: number | undefined): number {
  const slots = building.sloopSlots ?? 0;
  if (!slots || !sloops) return 1;
  return 1 + Math.min(Math.max(0, Math.round(sloops)), slots) / slots;
}

/** What one machine running `recipe` with `settings` makes, eats and draws per minute. */
export function machineRates(
  data: GameData,
  recipe: Recipe,
  settings: LineSettings,
  preferred?: Record<string, string>,
): MachineRates {
  const building = lineBuilding(data, recipe, settings, preferred);
  const rates = new Map<string, number>();
  const add = (item: string, rate: number) => rates.set(item, (rates.get(item) ?? 0) + rate);
  let powerMW: number;

  if (data.features.clock) {
    // Satisfactory: output scales with clock and sloops; power with clock^exp × amp².
    const clock = clampClock(settings.clock);
    const amp = amplification(building, settings.sloops);
    const cyclesPerMin = (60 / recipe.time) * clock;
    for (const s of recipe.inputs) add(s.item, -s.amount * cyclesPerMin);
    for (const s of recipe.outputs) add(s.item, s.amount * cyclesPerMin * amp);
    if (recipe.generatesMW) {
      powerMW = -recipe.generatesMW * clock;
    } else {
      const base = recipe.power ? (recipe.power.min + recipe.power.max) / 2 : building.powerMW;
      powerMW = base * Math.pow(clock, building.powerExponent ?? 1) * amp * amp;
    }
  } else {
    // Factorio / DSP: crafting speed and productivity bonuses.
    const speed = (building.speed ?? 1) * (1 + (settings.speedBonus ?? 0));
    const cyclesPerMin = (60 / recipe.time) * speed;
    const prodAllowed = data.game !== 'factorio' || recipe.allowProductivity;
    // Built-in productivity (foundry, EM plant) also needs allow_productivity.
    const prod = prodAllowed ? 1 + (building.baseProductivity ?? 0) + (settings.prodBonus ?? 0) : 1;
    for (const s of recipe.inputs) add(s.item, -s.amount * cyclesPerMin);
    for (const s of recipe.outputs) {
      const ignored = Math.min(s.amount, recipe.ignoredByProductivity?.[s.item] ?? 0);
      add(s.item, ((s.amount - ignored) * prod + ignored) * cyclesPerMin);
    }
    powerMW = recipe.generatesMW ? -recipe.generatesMW * speed : building.powerMW;
  }

  if (data.features.power && powerMW !== 0) add(POWER, -powerMW);
  return { rates, powerMW, footprint: building.footprint ?? 0, building };
}

/** Whole machines needed to house a fractional count. */
export function wholeMachines(machines: number): number {
  return Math.ceil(machines - 1e-6);
}
