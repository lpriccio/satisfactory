import loadHighs from 'highs';
import { newFactory, defaultSettings } from '../src/engine/factory.ts';
import type { LpSolver } from '../src/engine/solve.ts';
import type { Factory, GameData, Recipe } from '../src/engine/types.ts';

let solver: Promise<LpSolver> | undefined;
export function highs(): Promise<LpSolver> {
  return (solver ??= loadHighs() as unknown as Promise<LpSolver>);
}

const r = (
  id: string,
  time: number,
  inputs: [string, number][],
  outputs: [string, number][],
  building: string,
  extra: Partial<Recipe> = {},
): Recipe => ({
  id,
  name: id,
  time,
  inputs: inputs.map(([item, amount]) => ({ item, amount })),
  outputs: outputs.map(([item, amount]) => ({ item, amount })),
  buildings: [building],
  ...extra,
});

/** A small Satisfactory-shaped data set with numbers from the game. */
export function miniSatisfactory(): GameData {
  const recipes: Recipe[] = [
    r('IngotIron', 2, [['OreIron', 1]], [['IronIngot', 1]], 'SmelterMk1'),
    r('IronPlate', 6, [['IronIngot', 3]], [['IronPlate', 2]], 'ConstructorMk1'),
    r('IngotCopper', 2, [['OreCopper', 1]], [['CopperIngot', 1]], 'SmelterMk1'),
    r('AluminumSheet', 6, [['AluminumIngot', 3], ['CopperIngot', 1]], [['AluminumPlate', 3]], 'AssemblerMk1'),
    r('IngotAluminum', 4, [['AluminumScrap', 6], ['Silica', 5]], [['AluminumIngot', 4]], 'FoundryMk1'),
    r('AluminumScrap', 1, [['AluminaSolution', 4], ['Coal', 2]], [['AluminumScrap', 6], ['Water', 2]], 'OilRefinery'),
    r('AluminaSolution', 6, [['OreBauxite', 12], ['Water', 18]], [['AluminaSolution', 12], ['Silica', 5]], 'OilRefinery'),
    r('Silica', 8, [['RawQuartz', 3]], [['Silica', 5]], 'ConstructorMk1'),
    r('Alternate_RecycledPlastic', 12, [['Rubber', 6], ['LiquidFuel', 12]], [['Plastic', 12]], 'OilRefinery', { alternate: true }),
    r('Alternate_RecycledRubber', 12, [['Plastic', 6], ['LiquidFuel', 12]], [['Rubber', 12]], 'OilRefinery', { alternate: true }),
    r('Plastic', 6, [['LiquidOil', 3]], [['Plastic', 2], ['HeavyOilResidue', 1]], 'OilRefinery'),
    r('Rubber', 6, [['LiquidOil', 3]], [['Rubber', 2], ['HeavyOilResidue', 2]], 'OilRefinery'),
    r('Power_GeneratorCoal_Coal', 60, [['Coal', 15], ['Water', 45]], [], 'GeneratorCoal', { generatesMW: 75 }),
  ];
  const items = [
    'OreIron', 'IronIngot', 'IronPlate', 'OreCopper', 'CopperIngot', 'AluminumIngot', 'AluminumPlate',
    'AluminumScrap', 'AluminaSolution', 'Silica', 'Coal', 'Water', 'OreBauxite', 'RawQuartz',
    'Plastic', 'Rubber', 'LiquidFuel', 'LiquidOil', 'HeavyOilResidue',
  ];
  const raw = ['OreIron', 'OreCopper', 'Coal', 'Water', 'OreBauxite', 'RawQuartz', 'LiquidOil'];
  return {
    game: 'satisfactory',
    name: 'Satisfactory (test)',
    source: 'fixture',
    generatedAt: '2026-10-19',
    items: Object.fromEntries(items.map((id) => [id, { id, name: id, raw: raw.includes(id) }])),
    buildings: {
      SmelterMk1: { id: 'SmelterMk1', name: 'Smelter', powerMW: 4, powerExponent: 1.321928, sloopSlots: 1, footprint: 54 },
      ConstructorMk1: { id: 'ConstructorMk1', name: 'Constructor', powerMW: 4, powerExponent: 1.321928, sloopSlots: 1, footprint: 80 },
      AssemblerMk1: { id: 'AssemblerMk1', name: 'Assembler', powerMW: 15, powerExponent: 1.321928, sloopSlots: 2, footprint: 150 },
      FoundryMk1: { id: 'FoundryMk1', name: 'Foundry', powerMW: 16, powerExponent: 1.321928, sloopSlots: 2, footprint: 90 },
      OilRefinery: { id: 'OilRefinery', name: 'Refinery', powerMW: 30, powerExponent: 1.321928, sloopSlots: 2, footprint: 200 },
      GeneratorCoal: { id: 'GeneratorCoal', name: 'Coal Generator', powerMW: 0, footprint: 260 },
    },
    recipes: Object.fromEntries(recipes.map((x) => [x.id, x])),
    features: { clock: true, sloops: true, power: true, productivity: false, speedBonus: false, footprint: true, areaUnit: 'm²' },
    defaultImports: raw,
  };
}

export function factory(data: GameData, patch: Partial<Factory>): Factory {
  return { ...newFactory(data.game, defaultSettings()), ...patch };
}
