<script lang="ts">
  import { onDestroy } from 'svelte';
  import { newId, nowIso } from '../engine/factory.ts';
  import { itemName, solveFactory } from '../engine/solve.ts';
  import { POWER, type Factory, type Plan, type World } from '../engine/types.ts';
  import { combinePlans, netPosition, worldToFactory } from '../engine/world.ts';
  import { mw, num, signed } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';
  import { href, router } from '../lib/router.svelte.ts';
  import type { DocSummary, Store } from '../lib/storage.ts';
  import Picker from './Picker.svelte';

  let { session, store, id }: { session: GameSession; store: Store; id: string } = $props();
  const data = $derived(session.data);

  let world = $state.raw<World | null>(null);
  let missing = $state(false);
  let summaries = $state<DocSummary[]>([]);
  let factories = $state.raw<Record<string, Factory>>({});
  let timer: ReturnType<typeof setTimeout> | undefined;

  (async () => {
    const [w, list] = await Promise.all([store.getWorld(session.game, id), store.listFactories(session.game)]);
    summaries = list.sort((a, b) => a.name.localeCompare(b.name));
    if (!w) {
      missing = true;
      return;
    }
    world = w;
    await loadMembers();
  })();

  async function loadMembers() {
    const next = { ...factories };
    for (const e of world?.entries ?? []) {
      if (!next[e.factory]) {
        const f = await store.getFactory(session.game, e.factory);
        if (f) next[e.factory] = f;
      }
    }
    factories = next;
  }

  function update(next: World) {
    world = { ...next, updatedAt: nowIso() };
    clearTimeout(timer);
    timer = setTimeout(() => world && store.saveWorld(world), 400);
  }
  onDestroy(() => {
    clearTimeout(timer);
    if (world) void store.saveWorld(world);
  });

  const members = $derived.by(() => {
    if (!world) return [];
    const out: { factory: Factory; plan: Plan; multiplier: number }[] = [];
    for (const e of world.entries) {
      const f = factories[e.factory];
      if (f) out.push({ factory: f, plan: solveFactory(data, f, session.solver, session.settings), multiplier: e.multiplier });
    }
    return out;
  });
  const totals = $derived(combinePlans(members.map((m) => ({ plan: m.plan, multiplier: m.multiplier, spacing: m.factory.spacing }))));
  const position = $derived(
    netPosition(totals.items)
      .filter((r) => r.item !== POWER)
      .sort((a, b) => a.net - b.net),
  );

  async function add(factoryId: string) {
    if (!world) return;
    update({ ...world, entries: [...world.entries, { factory: factoryId, multiplier: 1 }] });
    await loadMembers();
  }

  async function saveAsFactory() {
    if (!world) return;
    const f = worldToFactory(world, members, newId, nowIso());
    await store.saveFactory(f);
    router.go({ page: 'factory', game: session.game, id: f.id, view: 'lines' });
  }
</script>

{#if missing}
  <p>That world doesn't exist. <a href={href({ page: 'home', game: session.game })}>Back</a></p>
{:else if world}
  <section class="row head">
    <a class="muted" href={href({ page: 'home', game: session.game })}>← Factories</a>
    <input class="name" value={world.name} aria-label="World name" oninput={(e) => update({ ...world!, name: e.currentTarget.value })} />
    <span class="grow"></span>
    <button disabled={!members.length} onclick={saveAsFactory} title="One manual factory holding every line, scaled by its multiplier">Save as one factory</button>
  </section>

  <section class="panel members">
    <h3>Factories in this world</h3>
    {#each world.entries as e, i (i)}
      {@const f = factories[e.factory]}
      <div class="row">
        <input type="number" min="0" step="any" style="width: 5.5rem" value={e.multiplier} aria-label="Multiplier"
          oninput={(ev) => update({ ...world!, entries: world!.entries.map((x, j) => (j === i ? { ...x, multiplier: Math.max(0, Number(ev.currentTarget.value) || 0) } : x)) })} />
        <span class="muted">×</span>
        {#if f}
          <a href={href({ page: 'factory', game: session.game, id: f.id, view: 'flow' })}>{f.name}</a>
        {:else}
          <span class="bad">missing factory {e.factory}</span>
        {/if}
        <span class="grow"></span>
        <button class="ghost danger" aria-label="Remove" onclick={() => update({ ...world!, entries: world!.entries.filter((_, j) => j !== i) })}>✕</button>
      </div>
    {/each}
    <Picker options={summaries.map((s) => ({ id: s.id, label: s.name }))} placeholder="Add a factory…" width="22rem" onpick={add} />
  </section>

  <section class="tiles">
    {#if data.features.power}
      <div class="tile panel"><span class="label">Power</span><span class="value">{mw(totals.powerMW)}</span></div>
    {/if}
    <div class="tile panel"><span class="label">Machines</span><span class="value">{num(totals.machines)}</span></div>
    {#if totals.foundations !== undefined}
      <div class="tile panel"><span class="label">Foundations</span><span class="value">{num(totals.foundations)}</span></div>
    {:else if data.features.footprint}
      <div class="tile panel"><span class="label">Floor</span><span class="value">{num(totals.footprint)} {data.features.areaUnit}</span></div>
    {/if}
  </section>

  <section class="panel">
    <div class="bar">
      <h2>Net position</h2>
      <p class="muted small">What the world sends out (+) or still needs (−) once factories feed each other. Raw resources show as needs.</p>
    </div>
    <div class="scroll-x">
      <table>
        <thead><tr><th>Item</th><th class="n">Net /min</th><th class="n">Out</th><th class="n">In</th></tr></thead>
        <tbody>
          {#each position as r (r.item)}
            {@const p = totals.items[r.item]}
            <tr>
              <td>{itemName(data, r.item)}</td>
              <td class="n" class:good={r.net > 0} class:bad={r.net < 0}>{signed(r.net)}</td>
              <td class="n">{num(p.target + p.surplus)}</td>
              <td class="n imp">{num(p.imported)}</td>
            </tr>
          {/each}
          {#if !position.length}<tr><td colspan="4" class="muted">Add factories to see the balance.</td></tr>{/if}
        </tbody>
      </table>
    </div>
  </section>
{:else}
  <p class="muted">Loading world…</p>
{/if}

<style>
  .head {
    margin-bottom: 14px;
  }
  .name {
    font-size: 20px;
    font-weight: 600;
    background: none;
    border-color: transparent;
    flex: 0 1 28rem;
  }
  .members {
    padding: 12px 14px;
    display: grid;
    gap: 8px;
    margin-bottom: 14px;
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
  .label {
    font-size: 12px;
    color: var(--muted);
  }
  .value {
    font-size: 22px;
    font-weight: 600;
    font-family: var(--mono);
  }
  .bar {
    padding: 12px 14px 0;
  }
  .small {
    font-size: 12px;
  }
</style>
