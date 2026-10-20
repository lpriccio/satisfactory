<script lang="ts">
  import type { GameSettings } from '../engine/types.ts';
  import { itemName } from '../engine/solve.ts';
  import type { GameSession } from '../lib/game.svelte.ts';

  let { session }: { session: GameSession } = $props();
  const data = $derived(session.data);
  let filter = $state('');
  let status = $state('');

  const alternates = $derived(
    Object.values(data.recipes)
      .filter((r) => r.alternate)
      .sort((a, b) => a.name.localeCompare(b.name)),
  );
  const shownAlts = $derived(
    alternates.filter((r) => !filter || `${r.name} ${r.outputs.map((o) => itemName(data, o.item)).join(' ')}`.toLowerCase().includes(filter.toLowerCase())),
  );

  // Crafting categories that more than one building can run.
  const categories = $derived.by(() => {
    const map = new Map<string, Set<string>>();
    for (const r of Object.values(data.recipes)) {
      if (!r.category || r.buildings.length < 2) continue;
      const set = map.get(r.category) ?? new Set();
      r.buildings.forEach((b) => set.add(b));
      map.set(r.category, set);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  });

  async function save(patch: Partial<GameSettings>) {
    status = 'saving…';
    try {
      await session.saveSettings({ ...$state.snapshot(session.settings), ...patch });
      status = 'saved';
    } catch (e) {
      status = `save failed: ${(e as Error).message}`;
    }
  }

  function toggleAlt(id: string, on: boolean) {
    const set = new Set(session.settings.unlocked);
    if (on) set.add(id);
    else set.delete(id);
    save({ unlocked: [...set].sort() });
  }
</script>

<section class="row head">
  <h1>{data.name} settings</h1>
  <span class="muted small">{status}</span>
</section>

<section class="panel block">
  <h2>Defaults</h2>
  {#if data.features.clock}
    <label class="row">
      Foundation spacing for new factories
      <input type="number" min="1" max="10" step="0.5" style="width: 5rem" value={session.settings.spacing}
        onchange={(e) => save({ spacing: Number(e.currentTarget.value) || 4 })} />
      <span class="muted small">foundations = floor m² × spacing ÷ 64</span>
    </label>
  {/if}
  <p class="muted small">Data: {data.source} · generated {data.generatedAt?.slice(0, 10)}</p>
</section>

{#if alternates.length}
  <section class="panel block">
    <div class="row">
      <h2>Unlocked alternates</h2>
      <span class="muted">{session.settings.unlocked.length} of {alternates.length}</span>
      <span class="grow"></span>
      <input placeholder="Filter" bind:value={filter} aria-label="Filter alternates" />
      <button onclick={() => save({ unlocked: alternates.map((r) => r.id) })}>All</button>
      <button onclick={() => save({ unlocked: [] })}>None</button>
    </div>
    <p class="muted small">Used when a factory has “Choose recipes for me” on. You can always pick any recipe by hand.</p>
    <div class="alts">
      {#each shownAlts as r (r.id)}
        <label>
          <input type="checkbox" checked={session.settings.unlocked.includes(r.id)} onchange={(e) => toggleAlt(r.id, e.currentTarget.checked)} />
          {r.name}
          <span class="muted small">→ {r.outputs.map((o) => itemName(data, o.item)).join(', ')}</span>
        </label>
      {/each}
    </div>
  </section>
{/if}

{#if categories.length}
  <section class="panel block">
    <h2>Preferred building per category</h2>
    <p class="muted small">Lines use this unless you pick another building on the line.</p>
    <table>
      <tbody>
        {#each categories as [cat, buildings] (cat)}
          <tr>
            <td>{cat}</td>
            <td>
              <select value={session.settings.defaultBuilding[cat] ?? ''}
                onchange={(e) => {
                  const next = { ...session.settings.defaultBuilding };
                  if (e.currentTarget.value) next[cat] = e.currentTarget.value;
                  else delete next[cat];
                  save({ defaultBuilding: next });
                }}>
                <option value="">Fastest available</option>
                {#each [...buildings] as b (b)}
                  <option value={b}>{data.buildings[b]?.name ?? b}</option>
                {/each}
              </select>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
{/if}

<style>
  .head {
    margin-bottom: 14px;
  }
  .head h1 {
    font-size: 22px;
  }
  .block {
    padding: 14px;
    margin-bottom: 14px;
    display: grid;
    gap: 10px;
  }
  .small {
    font-size: 12px;
  }
  .alts {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(18rem, 1fr));
    gap: 4px 16px;
  }
  .alts label {
    display: flex;
    gap: 6px;
    align-items: baseline;
  }
</style>
