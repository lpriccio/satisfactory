// Serves the built app (dist/) and the JSON file API on localhost.
//   npm run build && npm run serve        -> http://127.0.0.1:5174
import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createApi } from './api.ts';

const here = fileURLToPath(new URL('..', import.meta.url));
const dist = resolve(here, 'dist');
const api = createApi(resolve(here, 'factories'));
const port = Number(process.env.PORT ?? 5174);
const host = process.env.HOST ?? '127.0.0.1';

const TYPES: Record<string, string> = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.wasm': 'application/wasm',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

createServer(async (req, res) => {
  await api(req, res, async () => {
    const path = decodeURIComponent(new URL(req.url ?? '/', 'http://local').pathname);
    let file = normalize(join(dist, path));
    if (!file.startsWith(dist)) {
      res.statusCode = 403;
      return res.end();
    }
    try {
      if ((await stat(file)).isDirectory()) file = join(file, 'index.html');
    } catch {
      file = join(dist, 'index.html');
    }
    res.setHeader('Content-Type', TYPES[extname(file)] ?? 'application/octet-stream');
    createReadStream(file)
      .on('error', () => {
        res.statusCode = 404;
        res.end('Not found — run `npm run build` first.');
      })
      .pipe(res);
  });
}).listen(port, host, () => console.log(`Factory planner on http://${host}:${port}`));
