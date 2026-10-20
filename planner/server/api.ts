// File-backed JSON API shared by the Vite dev server and server/serve.ts.
//
//   GET    /api/health
//   GET    /api/:game/factories            -> summaries
//   GET    /api/:game/factories/:id
//   PUT    /api/:game/factories/:id
//   DELETE /api/:game/factories/:id
//   (same for /worlds)
//   GET    /api/:game/settings
//   PUT    /api/:game/settings
//
// Layout on disk: factories/<game>/<id>.json, factories/<game>/worlds/<id>.json,
// factories/<game>/settings.json — plain JSON so saves diff and commit cleanly.

import { mkdir, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import type { IncomingMessage, ServerResponse } from 'node:http';
import { join } from 'node:path';

const GAMES = new Set(['satisfactory', 'factorio', 'dsp']);
const ID = /^[A-Za-z0-9_-]{1,80}$/;
const MAX_BODY = 4 * 1024 * 1024;

type Kind = 'factories' | 'worlds';

export function createApi(root: string) {
  const dir = (game: string, kind: Kind) => (kind === 'factories' ? join(root, game) : join(root, game, 'worlds'));

  async function list(game: string, kind: Kind) {
    const d = dir(game, kind);
    let names: string[] = [];
    try {
      names = (await readdir(d)).filter((n) => n.endsWith('.json') && n !== 'settings.json');
    } catch {
      return [];
    }
    const out = [];
    for (const n of names) {
      try {
        const doc = JSON.parse(await readFile(join(d, n), 'utf8'));
        out.push({
          id: doc.id ?? n.slice(0, -5),
          name: doc.name ?? 'Untitled',
          mode: doc.mode,
          targets: doc.targets,
          lines: Array.isArray(doc.lines) ? doc.lines.length : undefined,
          entries: Array.isArray(doc.entries) ? doc.entries.length : undefined,
          updatedAt: doc.updatedAt,
        });
      } catch {
        // A hand-broken file shouldn't hide the rest.
      }
    }
    return out;
  }

  async function writeJson(path: string, doc: unknown) {
    const tmp = `${path}.tmp`;
    await writeFile(tmp, JSON.stringify(doc, null, 2) + '\n', 'utf8');
    await rename(tmp, path);
  }

  async function body(req: IncomingMessage): Promise<unknown> {
    let size = 0;
    const chunks: Buffer[] = [];
    for await (const chunk of req) {
      size += (chunk as Buffer).length;
      if (size > MAX_BODY) throw Object.assign(new Error('Body too large'), { status: 413 });
      chunks.push(chunk as Buffer);
    }
    return JSON.parse(Buffer.concat(chunks).toString('utf8'));
  }

  function send(res: ServerResponse, status: number, data?: unknown) {
    res.statusCode = status;
    res.setHeader('Content-Type', 'application/json');
    res.setHeader('Cache-Control', 'no-store');
    res.end(data === undefined ? '' : JSON.stringify(data));
  }

  /** Connect-style middleware; calls next() for anything outside /api. */
  return async function handle(req: IncomingMessage, res: ServerResponse, next?: () => void) {
    const url = new URL(req.url ?? '/', 'http://local');
    if (!url.pathname.startsWith('/api/')) return next?.();
    const parts = url.pathname.split('/').filter(Boolean).slice(1).map(decodeURIComponent);
    try {
      if (parts[0] === 'health') return send(res, 200, { ok: true, root });
      const [game, kind, id] = parts;
      if (!game || !GAMES.has(game)) return send(res, 404, { error: 'unknown game' });

      if (kind === 'settings' && parts.length === 2) {
        const path = join(root, game, 'settings.json');
        if (req.method === 'GET') {
          try {
            return send(res, 200, JSON.parse(await readFile(path, 'utf8')));
          } catch {
            return send(res, 200, null);
          }
        }
        if (req.method === 'PUT') {
          await mkdir(join(root, game), { recursive: true });
          await writeJson(path, await body(req));
          return send(res, 204);
        }
        return send(res, 405, { error: 'method' });
      }

      if (kind !== 'factories' && kind !== 'worlds') return send(res, 404, { error: 'not found' });
      if (parts.length === 2) {
        if (req.method !== 'GET') return send(res, 405, { error: 'method' });
        return send(res, 200, await list(game, kind));
      }
      if (!id || !ID.test(id) || parts.length !== 3) return send(res, 400, { error: 'bad id' });
      const path = join(dir(game, kind), `${id}.json`);
      switch (req.method) {
        case 'GET':
          try {
            return send(res, 200, JSON.parse(await readFile(path, 'utf8')));
          } catch {
            return send(res, 404, { error: 'not found' });
          }
        case 'PUT': {
          const doc = (await body(req)) as { id?: string; game?: string };
          if (doc?.id !== id || doc?.game !== game) return send(res, 400, { error: 'id/game mismatch' });
          await mkdir(dir(game, kind), { recursive: true });
          await writeJson(path, doc);
          return send(res, 204);
        }
        case 'DELETE':
          await rm(path, { force: true });
          return send(res, 204);
        default:
          return send(res, 405, { error: 'method' });
      }
    } catch (err) {
      const status = (err as { status?: number }).status ?? (err instanceof SyntaxError ? 400 : 500);
      return send(res, status, { error: (err as Error).message });
    }
  };
}
