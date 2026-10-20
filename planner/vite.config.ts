import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import { createApi } from './server/api.ts';

const factoriesDir = fileURLToPath(new URL('./factories', import.meta.url));

/** The JSON file API, mounted on the dev and preview servers. */
function fileApi(): Plugin {
  const api = createApi(factoriesDir);
  return {
    name: 'factory-file-api',
    configureServer(server) {
      server.middlewares.use((req, res, next) => void api(req, res, next));
      // Saves land in factories/; don't reload the page when they do.
      server.watcher.unwatch(factoriesDir);
    },
    configurePreviewServer(server) {
      server.middlewares.use((req, res, next) => void api(req, res, next));
    },
  };
}

export default defineConfig({
  // Relative base so the static build also works from GitHub Pages.
  base: './',
  plugins: [svelte(), fileApi()],
  server: { port: 5173, host: '127.0.0.1' },
  test: { include: ['tests/**/*.test.ts'] },
});
