// Converts Satisfactory's CommunityResources/Docs/en-US.json into
// public/data/satisfactory.json (GameData).
//
// Usage: npx tsx tools/import-satisfactory.ts [path/to/en-US.json]

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { Building, GameData, Item, Recipe, Stack } from '../src/engine/types.ts';
import { FOOTPRINTS } from './satisfactory-footprints.ts';

const DEFAULT_DOCS =
  '/mnt/pomni/SteamLibrary/steamapps/common/Satisfactory/CommunityResources/Docs/en-US.json';
const here = dirname(fileURLToPath(import.meta.url));
const OUT = resolve(here, '../public/data/satisfactory.json');

type Raw = Record<string, any>;

// Approximate Satisfactory 1.0 world maximum extraction rates (items or m³ per
// minute) used for scarcity weights. Water is effectively unlimited.
const WORLD_MAX: Record<string, number> = {
  OreIron: 92100,
  OreCopper: 36900,
  Stone: 69300,
  Coal: 42300,
  OreGold: 15000,
  RawQuartz: 13500,
  Sulfur: 10800,
  OreBauxite: 12300,
  OreUranium: 2100,
  SAM: 10200,
  LiquidOil: 12600,
  NitrogenGas: 12000,
};

const ITEM_CLASSES = new Set([
  'FGResourceDescriptor',
  'FGItemDescriptor',
  'FGEquipmentDescriptor',
  'FGItemDescriptorBiomass',
  'FGConsumableDescriptor',
  'FGAmmoTypeProjectile',
  'FGAmmoTypeSpreadshot',
  'FGAmmoTypeInstantHit',
  'FGItemDescriptorNuclearFuel',
  'FGPowerShardDescriptor',
  'FGItemDescriptorPowerBoosterFuel',
]);
const MANUFACTURER_CLASSES = new Set([
  'FGBuildableManufacturer',
  'FGBuildableManufacturerVariablePower',
]);
const GENERATOR_CLASSES = new Set(['FGBuildableGeneratorFuel', 'FGBuildableGeneratorNuclear']);

export function itemId(className: string): string {
  return className.replace(/_C$/, '').replace(/^Desc_/, '');
}
export function recipeId(className: string): string {
  return className.replace(/_C$/, '').replace(/^Recipe_/, '');
}
export function buildingId(className: string): string {
  return className.replace(/_C$/, '').replace(/^Build_/, '');
}

const num = (v: unknown): number => Number.parseFloat(String(v ?? '0')) || 0;

/** `((ItemClass="…/Desc_X.Desc_X_C'",Amount=4),…)` → [['Desc_X_C', 4], …] */
function parseStacks(s: string): [string, number][] {
  const out: [string, number][] = [];
  const re = /ItemClass="[^"]*?\.([A-Za-z0-9_]+)'?"\s*,\s*Amount=([\d.]+)/g;
  for (const m of s.matchAll(re)) out.push([m[1], Number.parseFloat(m[2])]);
  return out;
}

/** `("/Game/…/Build_X.Build_X_C","…")` → ['Build_X_C', …] */
function parseClassList(s: string): string[] {
  return (s ?? '')
    .replace(/^\(|\)$/g, '')
    .split(',')
    .map((p) => p.trim().replace(/"/g, ''))
    .filter(Boolean)
    .map((p) => p.split('.').pop()!);
}

function nativeName(n: string): string {
  return n.split('.').pop()!.replace(/'$/, '');
}

function stableStringify(value: unknown): string {
  const sort = (v: unknown): unknown => {
    if (Array.isArray(v)) return v.map(sort);
    if (v && typeof v === 'object') {
      return Object.fromEntries(
        Object.keys(v as object)
          .sort()
          .map((k) => [k, sort((v as Raw)[k])]),
      );
    }
    return v;
  };
  return JSON.stringify(sort(value), null, 1) + '\n';
}

export function importSatisfactory(docs: Raw[], source: string): GameData {
  const byClass = new Map<string, Raw[]>();
  for (const group of docs) {
    const k = nativeName(group.NativeClass);
    byClass.set(k, [...(byClass.get(k) ?? []), ...group.Classes]);
  }
  const of = (k: string) => byClass.get(k) ?? [];
  const skipped: { id: string; reason: string }[] = [];

  // Items
  const items: Record<string, Item> = {};
  const fluid = new Set<string>();
  for (const [cls, list] of byClass) {
    if (!ITEM_CLASSES.has(cls)) continue;
    for (const c of list) {
      const id = itemId(c.ClassName);
      const form =
        c.mForm === 'RF_LIQUID' ? 'liquid' : c.mForm === 'RF_GAS' ? 'gas' : 'solid';
      if (form !== 'solid') fluid.add(id);
      const item: Item = { id, name: c.mDisplayName, form };
      const energy = num(c.mEnergyValue) * (form === 'solid' ? 1 : 1000);
      if (energy > 0) item.energyMJ = energy;
      if (cls === 'FGResourceDescriptor') item.raw = true;
      items[id] = item;
    }
  }
  const amount = (id: string, a: number) => (fluid.has(id) ? a / 1000 : a);

  // Buildings
  const buildings: Record<string, Building> = {};
  const manufacturers = new Set<string>();
  for (const cls of MANUFACTURER_CLASSES) {
    for (const c of of(cls)) {
      const id = buildingId(c.ClassName);
      manufacturers.add(c.ClassName);
      const b: Building = {
        id,
        name: c.mDisplayName,
        powerMW: num(c.mPowerConsumption),
        powerExponent: num(c.mPowerConsumptionExponent) || undefined,
        sloopSlots: num(c.mProductionShardSlotSize) || undefined,
        footprint: FOOTPRINTS[id],
      };
      if (b.footprint === undefined) skipped.push({ id, reason: 'no footprint in hand table' });
      buildings[id] = b;
    }
  }

  // Recipes
  const recipes: Record<string, Recipe> = {};
  for (const c of of('FGRecipe')) {
    const id = recipeId(c.ClassName);
    const producedIn = parseClassList(c.mProducedIn).filter((p) => manufacturers.has(p));
    if (producedIn.length === 0) continue; // workbench, build gun, equipment workshop
    const toStacks = (s: string): Stack[] | null => {
      const out: Stack[] = [];
      for (const [cls, a] of parseStacks(s)) {
        const iid = itemId(cls);
        if (!items[iid]) {
          skipped.push({ id, reason: `unknown item ${cls}` });
          return null;
        }
        out.push({ item: iid, amount: amount(iid, a) });
      }
      return out;
    };
    const inputs = toStacks(c.mIngredients);
    const outputs = toStacks(c.mProduct);
    if (!inputs || !outputs) continue;
    if (outputs.length === 0) {
      skipped.push({ id, reason: 'no products' });
      continue;
    }
    const r: Recipe = {
      id,
      name: c.mDisplayName,
      time: num(c.mManufactoringDuration),
      inputs,
      outputs,
      buildings: producedIn.map(buildingId),
    };
    if (c.ClassName.includes('Alternate')) r.alternate = true;
    if (producedIn.some((p) => buildings[buildingId(p)].powerMW === 0)) {
      const min = num(c.mVariablePowerConsumptionConstant);
      r.power = { min, max: min + num(c.mVariablePowerConsumptionFactor) };
    }
    recipes[id] = r;
  }

  // Generators: one synthesized recipe per fuel.
  for (const cls of GENERATOR_CLASSES) {
    for (const g of of(cls)) {
      const gid = buildingId(g.ClassName);
      const mw = num(g.mPowerProduction);
      buildings[gid] = {
        id: gid,
        name: g.mDisplayName,
        powerMW: 0,
        powerExponent: num(g.mPowerConsumptionExponent) || undefined,
        footprint: FOOTPRINTS[gid],
      };
      if (FOOTPRINTS[gid] === undefined) skipped.push({ id: gid, reason: 'no footprint in hand table' });
      const ratio = num(g.mSupplementalToPowerRatio);
      for (const f of g.mFuel ?? []) {
        const fuel = itemId(f.mFuelClass);
        const rid = `Power_${gid}_${fuel}`;
        const energy = items[fuel]?.energyMJ;
        if (!energy) {
          skipped.push({ id: rid, reason: `fuel ${fuel} has no energy value` });
          continue;
        }
        const perMin = (mw * 60) / energy;
        const inputs: Stack[] = [{ item: fuel, amount: perMin }];
        if (f.mSupplementalResourceClass && ratio > 0) {
          inputs.push({
            item: itemId(f.mSupplementalResourceClass),
            amount: (mw * ratio * 60) / 1000,
          });
        }
        const outputs: Stack[] = [];
        if (f.mByproduct) {
          outputs.push({ item: itemId(f.mByproduct), amount: perMin * num(f.mByproductAmount) });
        }
        recipes[rid] = {
          id: rid,
          name: `${items[fuel].name} in ${g.mDisplayName}`,
          time: 60,
          inputs,
          outputs,
          buildings: [gid],
          generatesMW: mw,
        };
      }
    }
  }

  // Default recipe per produced item.
  const producers = new Map<string, Recipe[]>();
  for (const r of Object.values(recipes)) {
    if (r.generatesMW) continue;
    for (const o of r.outputs) producers.set(o.item, [...(producers.get(o.item) ?? []), r]);
  }
  for (const [iid, rs] of producers) {
    const item = items[iid];
    if (item.raw) continue;
    const notUnpack = rs.filter((r) => !r.name.startsWith('Unpackage'));
    const pool = notUnpack.length ? notUnpack : rs;
    const byId = (a: Recipe, b: Recipe) => a.id.localeCompare(b.id);
    const isPrimary = (r: Recipe) => r.outputs[0].item === iid;
    const primary = pool.filter((r) => !r.alternate && isPrimary(r)).sort(byId);
    // A recipe that makes the item as its main product (even an alternate)
    // beats one that only yields it as a byproduct: Compacted Coal should not
    // default to building Rocket Fuel.
    const pick =
      primary.find((r) => r.name === item.name) ??
      primary[0] ??
      pool.filter((r) => r.alternate && isPrimary(r)).sort(byId)[0] ??
      pool.filter((r) => !r.alternate).sort(byId)[0] ??
      pool.sort(byId)[0];
    item.recipe = pick.id;
  }

  const defaultImports = Object.values(items)
    .filter((i) => i.raw)
    .map((i) => i.id)
    .sort();
  const resourceWeights: Record<string, number> = {};
  for (const id of defaultImports) {
    resourceWeights[id] = WORLD_MAX[id] ? WORLD_MAX.OreIron / WORLD_MAX[id] : 0;
  }

  return {
    game: 'satisfactory',
    name: 'Satisfactory',
    source,
    generatedAt: new Date().toISOString(),
    items,
    buildings,
    recipes,
    features: {
      clock: true,
      sloops: true,
      power: true,
      productivity: false,
      speedBonus: false,
      footprint: true,
      areaUnit: 'm²',
    },
    defaultImports,
    resourceWeights,
    skipped,
  };
}

function main() {
  const path = process.argv[2] ?? DEFAULT_DOCS;
  const text = new TextDecoder('utf-16le').decode(readFileSync(path)).replace(/^﻿/, '');
  const data = importSatisfactory(JSON.parse(text), path);
  writeFileSync(OUT, stableStringify(data));
  const gens = Object.values(data.recipes).filter((r) => r.generatesMW).length;
  console.log(
    `items ${Object.keys(data.items).length}, recipes ${Object.keys(data.recipes).length - gens}, ` +
      `generator recipes ${gens}, buildings ${Object.keys(data.buildings).length}, skipped ${data.skipped!.length}`,
  );
  for (const s of data.skipped!) console.log(`  skipped ${s.id}: ${s.reason}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
