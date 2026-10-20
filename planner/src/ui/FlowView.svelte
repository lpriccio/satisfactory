<script lang="ts">
  import { occurrenceImported, setRecipeChoice, toggleImport, toggleItemImport, updateLine } from '../engine/edits.ts';
  import { itemName } from '../engine/solve.ts';
  import { POWER, type PlanLine } from '../engine/types.ts';
  import type { Editor } from '../lib/editor.svelte.ts';
  import { num, rate } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';
  import LineControls from './LineControls.svelte';
  import FixActions from './FixActions.svelte';
  import { lineLabel, mainItem, orderLines, recipeOptionsFor } from './helpers.ts';

  let { editor, session }: { editor: Editor; session: GameSession } = $props();
  const data = $derived(session.data);

  const plan = $derived(editor.plan);
  const f = $derived(editor.factory);
  const ordered = $derived(orderLines(plan));
  const imports = $derived(
    Object.values(plan.items)
      .filter((p) => p.item !== POWER && p.imported > 1e-6)
      .sort((a, b) => b.imported - a.imported),
  );
  const surplus = $derived(Object.values(plan.items).filter((p) => p.item !== POWER && p.surplus > 1e-6));
  let collapsed = $state<Record<string, boolean>>({});

  function sources(line: PlanLine, item: string) {
    return plan.edges.filter((e) => e.to === line.id && e.item === item);
  }
  function sinks(line: PlanLine, item: string) {
    return plan.edges.filter((e) => e.from === line.id && e.item === item);
  }
  function io(line: PlanLine, sign: 1 | -1) {
    return Object.entries(line.rates)
      .filter(([item, r]) => item !== POWER && r * sign > 1e-9)
      .map(([item, r]) => ({ item, rate: Math.abs(r) }));
  }

  function jump(id: string) {
    if (['import', 'target', 'surplus'].includes(id)) return;
    collapsed[id] = false;
    document.getElementById(`line-${id}`)?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  function changeRecipe(line: PlanLine, recipe: string) {
    if (line.auto) editor.apply((g) => setRecipeChoice(g, mainItem(data, line), recipe));
    else editor.apply((g) => updateLine(g, line.id, { recipe }));
  }
</script>

<div class="flow">
  <aside class="panel side">
    <h3>Imports</h3>
    {#if !imports.length}
      <p class="muted small">Nothing imported.</p>
    {:else}
      <table>
        <tbody>
          {#each imports as p (p.item)}
            <tr>
              <td>{itemName(data, p.item)}</td>
              <td class="n imp">{num(p.imported)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
    {#if surplus.length}
      <h3 class="gap">Surplus</h3>
      <table>
        <tbody>
          {#each surplus as p (p.item)}
            <tr>
              <td>{itemName(data, p.item)}</td>
              <td class="n warn">{num(p.surplus)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
    <p class="muted small hint">
      Click <span class="chip import">import</span> on an input to import it for that line only. Shift-click to set every line using that item the same way.
    </p>
  </aside>

  <div class="lines">
    {#if !ordered.length}
      <div class="panel empty">
        {#if f.mode === 'target'}
          <p>Add a target above and the plan appears here.</p>
        {:else}
          <p>No lines yet. Add them in the <a href={`#/${session.game}/f/${encodeURIComponent(f.id)}/lines`}>Lines</a> view.</p>
        {/if}
      </div>
    {/if}
    {#each ordered as line (line.id)}
      {@const recipe = data.recipes[line.recipe]}
      {@const item = mainItem(data, line)}
      {@const options = recipeOptionsFor(data, item)}
      <article class="panel line" id={`line-${line.id}`} class:idle={line.machines < 1e-6}>
        <header>
          <button class="ghost fold" aria-expanded={!collapsed[line.id]} onclick={() => (collapsed[line.id] = !collapsed[line.id])}>
            {collapsed[line.id] ? '▸' : '▾'}
          </button>
          {#if options.length > 1 && !(line.auto && f.optimize)}
            <select class="recipe" value={line.recipe} onchange={(e) => changeRecipe(line, e.currentTarget.value)} aria-label="Recipe">
              {#each options as o (o.id)}
                <option value={o.id}>{o.label}{o.hint ? ` (${o.hint})` : ''}</option>
              {/each}
            </select>
          {:else}
            <span class="recipe-name">{recipe?.name ?? line.recipe}</span>
          {/if}
          {#if recipe?.alternate}<span class="chip">alt</span>{/if}
          {#if !line.auto}<span class="chip" title="Typed or locked by you">{line.locked ? 'locked' : 'sized'}</span>{/if}
          <span class="grow"></span>
          <LineControls {line} {editor} {session} />
        </header>
        {#if !collapsed[line.id]}
          <div class="io">
            <div>
              <h3>In</h3>
              {#each io(line, -1) as x (x.item)}
                {@const imported = occurrenceImported(data, f, x.item, line.id)}
                <div class="io-row">
                  <span class="item">{itemName(data, x.item)}</span>
                  <span class="mono">{rate(x.rate)}</span>
                  <span class="from">
                    {#each sources(line, x.item) as e (e.from)}
                      <button class="chip" class:import={e.from === 'import'} onclick={() => jump(e.from)} title={`${num(e.rate)}/min`}>
                        {lineLabel(data, plan, e.from)}
                      </button>
                    {/each}
                  </span>
                  <button
                    class="chip toggle"
                    class:import={imported}
                    aria-pressed={imported}
                    title="Click: import for this line only. Shift-click: every line using it."
                    onclick={(e) => editor.apply((g) => toggleImport(data, g, plan, x.item, line.id, e.shiftKey))}
                  >{imported ? 'imported' : 'import'}</button>
                </div>
              {/each}
            </div>
            <div>
              <h3>Out</h3>
              {#each io(line, 1) as x (x.item)}
                <div class="io-row">
                  <span class="item">{itemName(data, x.item)}</span>
                  <span class="mono">{rate(x.rate)}</span>
                  <span class="from">
                    {#each sinks(line, x.item) as e (e.to)}
                      <button class="chip" class:target={e.to === 'target'} class:surplus={e.to === 'surplus'} onclick={() => jump(e.to)} title={`${num(e.rate)}/min`}>
                        {lineLabel(data, plan, e.to)}
                      </button>
                    {/each}
                  </span>
                  <span></span>
                </div>
              {/each}
            </div>
          </div>
        {/if}
      </article>
    {/each}
  </div>
</div>

{#if Object.values(plan.items).some((p) => p.unbalanced > 1e-6)}
  <section class="panel unbalanced">
    <div class="row">
      <h3 class="grow">Can't balance</h3>
      {#if f.mode === 'target'}<FixActions {editor} />{/if}
    </div>
    {#each Object.values(plan.items).filter((p) => p.unbalanced > 1e-6) as p (p.item)}
      <div class="row">
        <span>{itemName(data, p.item)}</span>
        <span class="mono bad">{rate(p.unbalanced)} short</span>
        <button onclick={() => editor.apply((g) => toggleItemImport(data, g, p.item))}>Import it</button>
      </div>
    {/each}
  </section>
{/if}

<style>
  .flow {
    display: grid;
    grid-template-columns: 15rem minmax(0, 1fr);
    gap: 14px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .flow {
      grid-template-columns: 1fr;
    }
  }
  .side {
    padding: 12px;
    position: sticky;
    top: 64px;
    max-height: calc(100vh - 80px);
    overflow-y: auto;
  }
  .side td {
    padding: 3px 4px;
  }
  .gap {
    margin-top: 14px;
  }
  .hint {
    margin-top: 14px;
    line-height: 1.7;
  }
  .small {
    font-size: 12px;
  }
  .lines {
    display: grid;
    gap: 10px;
  }
  .empty {
    padding: 20px;
  }
  .line header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    flex-wrap: wrap;
  }
  .line.idle {
    opacity: 0.6;
  }
  .fold {
    padding: 2px 6px;
  }
  .recipe,
  .recipe-name {
    font-weight: 600;
    max-width: 22rem;
  }
  .io {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    padding: 4px 12px 12px 44px;
  }
  @media (max-width: 720px) {
    .io {
      grid-template-columns: 1fr;
      padding-left: 12px;
    }
  }
  .io h3 {
    margin-bottom: 4px;
  }
  .io-row {
    display: grid;
    grid-template-columns: minmax(7rem, 1fr) 6.5rem minmax(0, 1.4fr) auto;
    gap: 8px;
    align-items: center;
    padding: 3px 0;
    border-bottom: 1px dashed var(--line);
  }
  .io-row .mono {
    text-align: right;
    font-size: 13px;
  }
  .from {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .toggle {
    min-width: 5.5rem;
    justify-content: center;
    color: var(--muted);
  }
  .unbalanced {
    margin-top: 14px;
    padding: 12px;
    border-color: var(--bad);
    display: grid;
    gap: 6px;
  }
</style>
