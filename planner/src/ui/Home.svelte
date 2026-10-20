<script lang="ts">
  import { newFactory, newId, newWorld, normalizeFactory } from '../engine/factory.ts';
  import { itemName } from '../engine/solve.ts';
  import type { Factory } from '../engine/types.ts';
  import type { GameSession } from '../lib/game.svelte.ts';
  import { router } from '../lib/router.svelte.ts';
  import type { DocSummary, Store } from '../lib/storage.ts';
  import { num, when } from '../lib/format.ts';

  let { session, store }: { session: GameSession; store: Store } = $props();

  let factories = $state<DocSummary[]>([]);
  let worlds = $state<DocSummary[]>([]);
  let filter = $state('');
  let loading = $state(true);
  let message = $state('');

  async function refresh() {
    [factories, worlds] = await Promise.all([store.listFactories(session.game), store.listWorlds(session.game)]);
    factories.sort((a, b) => (b.updatedAt ?? '').localeCompare(a.updatedAt ?? ''));
    loading = false;
  }
  $effect(() => {
    void session.game;
    refresh();
  });

  const shown = $derived(
    factories.filter((f) => !filter || `${f.name} ${f.targets?.map((t) => itemName(session.data, t.item)).join(' ')}`.toLowerCase().includes(filter.toLowerCase())),
  );

  async function create(mode: Factory['mode']) {
    const f = newFactory(session.game, session.settings, mode === 'manual' ? 'New build' : 'New factory');
    f.mode = mode;
    await store.saveFactory(f);
    router.go({ page: 'factory', game: session.game, id: f.id, view: mode === 'manual' ? 'lines' : 'flow' });
  }

  async function createWorld() {
    const w = newWorld(session.game);
    await store.saveWorld(w);
    router.go({ page: 'world', game: session.game, id: w.id });
  }

  async function duplicate(id: string) {
    const f = await store.getFactory(session.game, id);
    if (!f) return;
    const copy = { ...f, id: newId(), name: `${f.name} copy` };
    await store.saveFactory(copy);
    await refresh();
  }

  async function remove(kind: 'factory' | 'world', d: DocSummary) {
    if (!confirm(`Delete “${d.name}”? This removes its file.`)) return;
    if (kind === 'factory') await store.deleteFactory(session.game, d.id);
    else await store.deleteWorld(session.game, d.id);
    await refresh();
  }

  async function importJson(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    try {
      const doc = JSON.parse(await file.text());
      if (doc.game !== session.game) throw new Error(`That file is a ${doc.game ?? 'unknown'} factory.`);
      const f = normalizeFactory({ ...doc, id: doc.id ?? newId() });
      await store.saveFactory(f);
      message = `Imported “${f.name}”.`;
      await refresh();
    } catch (err) {
      message = `Couldn't import: ${(err as Error).message}`;
    }
  }

  function targetsText(d: DocSummary): string {
    if (!d.targets?.length) return d.lines !== undefined ? `${d.lines} line${d.lines === 1 ? '' : 's'}` : '';
    return d.targets.map((t) => `${num(t.rate)} ${itemName(session.data, t.item)}`).join(', ');
  }
</script>

<section class="head row">
  <h1>{session.data.name}</h1>
  <span class="muted">{Object.keys(session.data.recipes).length} recipes · {Object.keys(session.data.items).length} items</span>
  <span class="grow"></span>
  <button class="primary" onclick={() => create('target')}>New factory</button>
  <button onclick={() => create('manual')} title="Type recipe lines yourself, like a workbook tab">New build (manual)</button>
  <label class="file">
    <input type="file" accept="application/json,.json" onchange={importJson} />
    <span>Import JSON…</span>
  </label>
</section>
{#if message}<p class="muted">{message}</p>{/if}

<div class="grid">
  <section class="panel list">
    <div class="row bar">
      <h2>Factories</h2>
      <span class="muted">{factories.length}</span>
      <span class="grow"></span>
      <input placeholder="Filter" bind:value={filter} aria-label="Filter factories" />
    </div>
    {#if loading}
      <p class="muted pad">Loading…</p>
    {:else if !shown.length}
      <p class="muted pad">{factories.length ? 'Nothing matches.' : 'No factories yet. Start one above.'}</p>
    {:else}
      <table>
        <tbody>
          {#each shown as d (d.id)}
            <tr>
              <td class="name">
                <a href={`#/${session.game}/f/${encodeURIComponent(d.id)}/flow`}>{d.name}</a>
                <div class="muted small">{targetsText(d)}</div>
              </td>
              <td class="muted small">{d.mode === 'manual' ? 'manual' : 'target'}</td>
              <td class="muted small when">{when(d.updatedAt)}</td>
              <td class="actions">
                <button class="ghost" onclick={() => duplicate(d.id)}>Duplicate</button>
                <button class="ghost danger" onclick={() => remove('factory', d)}>Delete</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section class="panel list">
    <div class="row bar">
      <h2>Worlds</h2>
      <span class="grow"></span>
      <button onclick={createWorld}>New world</button>
    </div>
    <p class="muted pad small">A world adds up several factories with multipliers and shows what's left over or short.</p>
    {#if worlds.length}
      <table>
        <tbody>
          {#each worlds as d (d.id)}
            <tr>
              <td class="name"><a href={`#/${session.game}/w/${encodeURIComponent(d.id)}`}>{d.name}</a></td>
              <td class="muted small">{d.entries ?? 0} factor{d.entries === 1 ? 'y' : 'ies'}</td>
              <td class="actions"><button class="ghost danger" onclick={() => remove('world', d)}>Delete</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
</div>

<style>
  .head {
    margin-bottom: 16px;
  }
  .head h1 {
    font-size: 22px;
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(0, 1fr);
    gap: 16px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
  .bar {
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
  }
  .pad {
    padding: 0 14px;
  }
  .small {
    font-size: 12px;
  }
  .name {
    width: 100%;
  }
  .when,
  .actions {
    white-space: nowrap;
  }
  .actions {
    text-align: right;
  }
  .file input {
    display: none;
  }
  .file span {
    display: inline-block;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    padding: 4px 10px;
    cursor: pointer;
    background: var(--panel-2);
  }
  .file span:hover {
    border-color: var(--accent);
  }
  tr:last-child td {
    border-bottom: none;
  }
</style>
