<script lang="ts">
  import { onMount } from 'svelte';
  import { router, GAMES, href } from './lib/router.svelte.ts';
  import { openStore, type Store } from './lib/storage.ts';
  import { loadGame, type GameSession } from './lib/game.svelte.ts';
  import { decodeFactory } from './lib/share.ts';
  import type { Factory, GameId } from './engine/types.ts';
  import Home from './ui/Home.svelte';
  import FactoryPage from './ui/FactoryPage.svelte';
  import WorldPage from './ui/WorldPage.svelte';
  import SettingsPage from './ui/SettingsPage.svelte';

  let store = $state<Store | null>(null);
  let session = $state<GameSession | null>(null);
  let error = $state('');
  let shared = $state<Factory | null>(null);

  const route = $derived(router.route);
  const game = $derived<GameId>(route.page === 'share' ? (shared?.game ?? 'satisfactory') : route.game);

  onMount(async () => {
    store = await openStore();
  });

  $effect(() => {
    document.documentElement.dataset.game = game;
  });

  // Load the shared factory first: its game decides which data to load.
  $effect(() => {
    if (route.page !== 'share') {
      shared = null;
      return;
    }
    decodeFactory(route.payload).then(
      (f) => (shared = f),
      () => (error = 'That share link is damaged or incomplete.'),
    );
  });

  $effect(() => {
    if (!store) return;
    const g = game;
    if (session?.game === g) return;
    error = '';
    loadGame(g, store).then(
      (s) => {
        if (game === g) session = s;
      },
      (e: Error) => (error = e.message),
    );
  });
</script>

<header>
  <a class="brand" href={href({ page: 'home', game })}>
    <svg viewBox="0 0 32 32" width="20" height="20" aria-hidden="true"><path d="M3 28V14l7 4v-4l7 4v-4l7 4V4h5v24z" fill="currentColor" /></svg>
    Factory Planner
  </a>
  <nav class="games" aria-label="Game">
    {#each GAMES as g (g.id)}
      <a href={href({ page: 'home', game: g.id })} class:active={g.id === game}>{g.name}</a>
    {/each}
  </nav>
  <span class="grow"></span>
  <a href={href({ page: 'settings', game })} class:active={route.page === 'settings'}>Settings</a>
  {#if store}
    <span class="storage muted" title={store.kind === 'files' ? 'Saving JSON files to planner/factories/' : 'No file server: saving in this browser only'}>
      {store.kind === 'files' ? 'files' : 'browser storage'}
    </span>
  {/if}
</header>

<main>
  {#if error}
    <p class="bad">{error}</p>
  {:else if !store || !session || session.game !== game}
    <p class="muted">Loading {GAMES.find((g) => g.id === game)?.name} data…</p>
  {:else}
    {#key session}
    {#if route.page === 'home'}
    <Home {session} {store} />
  {:else if route.page === 'factory'}
    {#key route.id}
      <FactoryPage {session} {store} id={route.id} view={route.view} />
    {/key}
  {:else if route.page === 'share'}
    {#if shared}
      <FactoryPage {session} {store} id={shared.id} view="flow" initial={shared} />
    {/if}
  {:else if route.page === 'world'}
    {#key route.id}
      <WorldPage {session} {store} id={route.id} />
    {/key}
  {:else if route.page === 'settings'}
    <SettingsPage {session} />
    {/if}
    {/key}
  {/if}
</main>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 20px;
    border-bottom: 1px solid var(--line);
    background: var(--panel);
    position: sticky;
    top: 0;
    z-index: 10;
    flex-wrap: wrap;
  }
  .brand {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    color: var(--text);
  }
  .brand svg {
    color: var(--accent);
  }
  .games {
    display: flex;
    gap: 4px;
  }
  header a {
    color: var(--muted);
    padding: 4px 8px;
    border-radius: var(--radius);
  }
  header a.active,
  header a:hover {
    color: var(--text);
    background: var(--panel-2);
    text-decoration: none;
  }
  .brand,
  .brand:hover {
    background: none !important;
  }
  .storage {
    font-size: 12px;
  }
  main {
    max-width: 1280px;
    margin: 0 auto;
    padding: 20px;
  }
  @media (max-width: 640px) {
    header,
    main {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
</style>
