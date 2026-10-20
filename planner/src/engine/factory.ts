import type { Factory, GameId, GameSettings, World } from './types.ts';

export function newId(): string {
  const bytes = new Uint8Array(8);
  // Node 18 (tools, tests) has no global crypto; ids only need to be unlikely to collide.
  if (globalThis.crypto?.getRandomValues) globalThis.crypto.getRandomValues(bytes);
  else for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  return Array.from(bytes, (b) => (b % 36).toString(36)).join('') + Date.now().toString(36).slice(-4);
}

export function nowIso(): string {
  return new Date().toISOString();
}

export function defaultSettings(): GameSettings {
  return { unlocked: [], defaultBuilding: {}, spacing: 4 };
}

export function newFactory(game: GameId, settings: GameSettings, name = 'New factory'): Factory {
  const now = nowIso();
  return {
    schema: 1,
    id: newId(),
    game,
    name,
    mode: 'target',
    objective: 'min-raw',
    targets: [],
    recipeChoice: {},
    lineSettings: {},
    lines: [],
    imports: { items: [], edges: [], exclude: [] },
    spacing: settings.spacing,
    createdAt: now,
    updatedAt: now,
  };
}

export function newWorld(game: GameId, name = 'World'): World {
  const now = nowIso();
  return { schema: 1, id: newId(), game, name, entries: [], createdAt: now, updatedAt: now };
}

/** Fill fields older or hand-written files may lack, so the rest of the app can trust the shape. */
export function normalizeFactory(raw: Partial<Factory> & { id: string; game: GameId }): Factory {
  const now = nowIso();
  return {
    schema: 1,
    name: 'Untitled',
    mode: 'target',
    objective: 'min-raw',
    targets: [],
    recipeChoice: {},
    lineSettings: {},
    lines: [],
    spacing: 4,
    createdAt: now,
    updatedAt: now,
    ...raw,
    imports: { items: [], edges: [], exclude: [], ...(raw.imports ?? {}) },
  } as Factory;
}
