<script lang="ts">
  import { onDestroy } from 'svelte';
  import { newId } from '../engine/factory.ts';
  import { setMode, setTargets } from '../engine/edits.ts';
  import { POWER, type Factory, type Objective } from '../engine/types.ts';
  import { Editor } from '../lib/editor.svelte.ts';
  import { num, mw } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';
  import { router, href, type FactoryView } from '../lib/router.svelte.ts';
  import { encodeFactory } from '../lib/share.ts';
  import type { Store } from '../lib/storage.ts';
  import { wholeMachines } from '../engine/lineMath.ts';
  import Picker, { type Option } from './Picker.svelte';
  import FlowView from './FlowView.svelte';
  import FixActions from './FixActions.svelte';
  import LinesView from './LinesView.svelte';
  import BalanceView from './BalanceView.svelte';
  import BuildingsView from './BuildingsView.svelte';

  let {
    session,
    store,
    id,
    view,
    initial,
  }: { session: GameSession; store: Store; id: string; view: FactoryView; initial?: Factory } = $props();

  let editor = $state<Editor | null>(null);
  // Shared links have no factory URL to route views through.
  let localView = $state<FactoryView | null>(null);
  const shownView = $derived(localView ?? view);
  let notFound = $state(false);
  let toast = $state('');

  // A shared link opens read-only until you save a copy.
  (async () => {
    const f = initial ?? (await store.getFactory(session.game, id));
    if (!f) {
      notFound = true;
      return;
    }
    editor = new Editor({
      factory: f,
      data: session.data,
      settings: () => session.settings,
      solver: session.solver,
      store: initial ? null : store,
    });
  })();

  onDestroy(() => void editor?.flush());

  const data = $derived(session.data);
  const features = $derived(data.features);

  const itemOptions: Option[] = $derived([
    ...(features.power ? [{ id: POWER, label: 'Power (MW)', hint: 'generators' }] : []),
    ...Object.values(data.items)
      .map((i) => ({ id: i.id, label: i.name, hint: i.raw ? 'raw' : undefined }))
      .sort((a, b) => a.label.localeCompare(b.label)),
  ]);

  const VIEWS: { id: FactoryView; label: string }[] = [
    { id: 'flow', label: 'Flow' },
    { id: 'lines', label: 'Lines' },
    { id: 'balance', label: 'Balance' },
    { id: 'buildings', label: 'Buildings' },
  ];

  function flash(text: string) {
    toast = text;
    setTimeout(() => (toast = ''), 2500);
  }

  async function share() {
    if (!editor) return;
    const url = `${location.origin}${location.pathname}#/share/${await encodeFactory(editor.factory)}`;
    try {
      await navigator.clipboard.writeText(url);
      flash('Share link copied');
    } catch {
      prompt('Copy this link:', url);
    }
  }

  function exportJson() {
    if (!editor) return;
    const blob = new Blob([JSON.stringify(editor.factory, null, 2)], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${editor.factory.name.replace(/[^\w.-]+/g, '_')}.json`;
    a.click();
    URL.revokeObjectURL(a.href);
  }

  async function saveCopy() {
    if (!editor) return;
    const copy = { ...editor.factory, id: newId() };
    await store.saveFactory(copy);
    router.go({ page: 'factory', game: copy.game, id: copy.id, view: 'flow' });
  }

  function keydown(e: KeyboardEvent) {
    if (!editor || !(e.ctrlKey || e.metaKey)) return;
    const el = e.target as HTMLElement;
    if (el.closest('input, textarea, select')) return;
    const k = e.key.toLowerCase();
    if (k === 'z' && !e.shiftKey) {
      editor.undo();
      e.preventDefault();
    } else if ((k === 'z' && e.shiftKey) || k === 'y') {
      editor.redo();
      e.preventDefault();
    }
  }

  function setTarget(i: number, patch: Partial<{ item: string; rate: number }>) {
    editor!.apply((f) => setTargets(f, f.targets.map((t, j) => (j === i ? { ...t, ...patch } : t))), `target-${i}-${Object.keys(patch)}`);
  }
</script>

<svelte:window onkeydown={keydown} onpagehide={() => void editor?.flush()} />

{#if notFound}
  <p>That factory doesn't exist (or was deleted). <a href={href({ page: 'home', game: session.game })}>Back to the list</a></p>
{:else if editor}
  {@const f = editor.factory}
  {@const plan = editor.plan}
  <section class="head">
    <div class="row">
      <a class="muted" href={href({ page: 'home', game: session.game })}>← Factories</a>
      <input
        class="name"
        value={f.name}
        aria-label="Factory name"
        oninput={(e) => editor!.apply((g) => ({ ...g, name: e.currentTarget.value }), 'name')}
      />
      <span class="grow"></span>
      <span class="save muted" class:bad={editor.saved === 'error'} title={editor.saveError}>
        {#if editor.saved === 'readonly'}shared link · not saved{:else if editor.saved === 'error'}save failed{:else if editor.saved === 'saved'}saved{:else}saving…{/if}
      </span>
      <button class="ghost" disabled={!editor.canUndo} onclick={() => editor!.undo()} title="Undo (Ctrl+Z)">Undo</button>
      <button class="ghost" disabled={!editor.canRedo} onclick={() => editor!.redo()} title="Redo (Ctrl+Shift+Z)">Redo</button>
      {#if initial}
        <button class="primary" onclick={saveCopy}>Save a copy</button>
      {/if}
      <button onclick={share} title="Copy a link that carries this whole factory">Share link</button>
      <button onclick={exportJson}>Export JSON</button>
    </div>

    <div class="row controls">
      <div class="seg" role="group" aria-label="Mode">
        <button class:on={f.mode === 'target'} onclick={() => editor!.apply((g) => setMode(g, plan, 'target'))} title="The solver picks lines to meet your targets">Target</button>
        <button class:on={f.mode === 'manual'} onclick={() => editor!.apply((g) => setMode(g, plan, 'manual'))} title="You type the lines; switching freezes the current plan">Manual</button>
      </div>
      <label>
        Objective
        <select value={f.objective} onchange={(e) => editor!.apply((g) => ({ ...g, objective: e.currentTarget.value as Objective }))}>
          <option value="min-raw">Fewest raw inputs</option>
          <option value="min-power">Least power</option>
          <option value="min-machines">Fewest machines</option>
        </select>
      </label>
      {#if f.mode === 'target'}
        <label title="Let the solver choose among all standard recipes and the alternates unlocked in Settings">
          <input type="checkbox" checked={!!f.optimize} onchange={(e) => editor!.apply((g) => ({ ...g, optimize: e.currentTarget.checked }))} />
          Choose recipes for me
        </label>
      {/if}
      {#if features.clock}
        <label title="Foundations = floor area × spacing ÷ 64 m²">
          Spacing ×
          <input type="number" min="1" max="10" step="0.5" style="width: 4.5rem" value={f.spacing}
            onchange={(e) => editor!.apply((g) => ({ ...g, spacing: Number(e.currentTarget.value) || 4 }))} />
        </label>
      {/if}
    </div>
  </section>

  <section class="panel targets">
    <h3>Targets</h3>
    {#each f.targets as t, i (i)}
      <div class="row">
        <Picker options={itemOptions} value={t.item} placeholder="Item" onpick={(item) => setTarget(i, { item })} />
        <input type="number" min="0" step="any" style="width: 7rem" value={t.rate} aria-label="Rate per minute"
          oninput={(e) => setTarget(i, { rate: Math.max(0, Number(e.currentTarget.value) || 0) })} />
        <span class="muted">{t.item === POWER ? 'MW' : '/min'}</span>
        <button class="ghost danger" onclick={() => editor!.apply((g) => setTargets(g, g.targets.filter((_, j) => j !== i)))} aria-label="Remove target">✕</button>
      </div>
    {/each}
    <div class="row">
      <Picker options={itemOptions} placeholder={f.targets.length ? 'Add another target…' : 'What do you want to make?'}
        onpick={(item) => editor!.apply((g) => setTargets(g, [...g.targets, { item, rate: item === POWER ? 1000 : 10 }]))} />
      {#if f.mode === 'manual'}
        <span class="muted small">Optional in manual mode: lines left unlocked are sized to meet targets.</span>
      {/if}
    </div>
  </section>

  {#if plan.status !== 'ok' && plan.message}
    <div class="banner row" class:bad={plan.status === 'error'}>
      <span class="grow">{plan.message}</span>
      {#if plan.status === 'unbalanced' && f.mode === 'target'}<FixActions {editor} />{/if}
    </div>
  {/if}
  {#each plan.warnings as w}
    <p class="banner">{w}</p>
  {/each}

  <section class="tiles">
    {#if features.power}
      <div class="tile panel">
        <span class="label">Power</span>
        <span class="value">{mw(plan.powerMW)}</span>
        <span class="muted small">{num(plan.consumedMW)} drawn{plan.generatedMW ? ` · ${num(plan.generatedMW)} generated` : ''}</span>
      </div>
    {/if}
    <div class="tile panel">
      <span class="label">Machines</span>
      <span class="value">{num(plan.machines)}</span>
      <span class="muted small">{plan.lines.reduce((a, l) => a + wholeMachines(l.machines), 0)} whole · {plan.lines.length} lines</span>
    </div>
    {#if features.footprint}
      <div class="tile panel">
        <span class="label">{plan.foundations !== undefined ? 'Foundations' : 'Floor'}</span>
        <span class="value">{plan.foundations !== undefined ? num(plan.foundations) : `${num(plan.footprint)} ${features.areaUnit}`}</span>
        <span class="muted small">{num(plan.footprint)} {features.areaUnit}{plan.foundations !== undefined ? ` × ${f.spacing} ÷ 64` : ''}</span>
      </div>
    {/if}
    <div class="tile panel">
      <span class="label">Imports</span>
      <span class="value">{Object.values(plan.items).filter((p) => p.item !== POWER && p.imported > 1e-6).length}</span>
      <span class="muted small">items brought in</span>
    </div>
  </section>

  <nav class="tabs" aria-label="View">
    {#each VIEWS as v (v.id)}
      <a href={href({ page: 'factory', game: session.game, id: f.id, view: v.id })} class:active={v.id === shownView}
        onclick={(e) => {
          if (initial) {
            e.preventDefault();
            localView = v.id;
          }
        }}>{v.label}</a>
    {/each}
    <span class="grow"></span>
    <span class="muted small">solved in {num(editor.solveMs, 1)} ms</span>
  </nav>

  {#if shownView === 'flow'}
    <FlowView {editor} {session} />
  {:else if shownView === 'lines'}
    <LinesView {editor} {session} />
  {:else if shownView === 'balance'}
    <BalanceView {editor} {session} />
  {:else}
    <BuildingsView {editor} {session} />
  {/if}

  {#if f.notes}
    <details class="notes panel">
      <summary>Notes</summary>
      <textarea rows="6" value={f.notes} oninput={(e) => editor!.apply((g) => ({ ...g, notes: e.currentTarget.value }), 'notes')}></textarea>
    </details>
  {:else}
    <button class="ghost muted add-notes" onclick={() => editor!.apply((g) => ({ ...g, notes: ' ' }))}>+ Add notes</button>
  {/if}
  {#if toast}<div class="toast">{toast}</div>{/if}
{:else}
  <p class="muted">Loading factory…</p>
{/if}

<style>
  .head {
    display: grid;
    gap: 10px;
    margin-bottom: 14px;
  }
  .name {
    font-size: 20px;
    font-weight: 600;
    background: none;
    border-color: transparent;
    min-width: 12rem;
    flex: 0 1 28rem;
  }
  .name:hover {
    border-color: var(--line);
  }
  .save {
    font-size: 12px;
  }
  .controls {
    gap: 16px;
  }
  .controls label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .seg {
    display: inline-flex;
  }
  .seg button {
    border-radius: 0;
  }
  .seg button:first-child {
    border-radius: var(--radius) 0 0 var(--radius);
  }
  .seg button:last-child {
    border-radius: 0 var(--radius) var(--radius) 0;
    border-left: none;
  }
  .seg button.on {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: var(--accent);
  }
  .targets {
    padding: 12px 14px;
    display: grid;
    gap: 8px;
    margin-bottom: 14px;
  }
  .banner {
    padding: 8px 12px;
    border: 1px solid var(--warn);
    border-radius: var(--radius);
    color: var(--warn);
    margin: 0 0 12px;
  }
  .banner.bad {
    border-color: var(--bad);
    color: var(--bad);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(170px, 1fr));
    gap: 12px;
    margin-bottom: 14px;
  }
  .tile {
    padding: 10px 14px;
    display: grid;
  }
  .tile .label {
    font-size: 12px;
    color: var(--muted);
  }
  .tile .value {
    font-size: 22px;
    font-weight: 600;
    font-family: var(--mono);
  }
  .small {
    font-size: 12px;
  }
  .tabs {
    display: flex;
    align-items: center;
    gap: 4px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 14px;
  }
  .tabs a {
    padding: 8px 14px;
    color: var(--muted);
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
  }
  .tabs a.active {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .tabs a:hover {
    text-decoration: none;
    color: var(--text);
  }
  .notes {
    margin-top: 16px;
    padding: 10px 14px;
  }
  .notes textarea {
    width: 100%;
    margin-top: 8px;
    font-family: var(--mono);
    font-size: 12px;
  }
  .add-notes {
    margin-top: 12px;
  }
  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--panel-2);
    border: 1px solid var(--accent);
    padding: 8px 14px;
    border-radius: var(--radius);
  }
</style>
