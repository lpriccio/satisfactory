<script lang="ts">
  import { addLine, moveLine, removeLine, updateLine } from '../engine/edits.ts';
  import { itemName } from '../engine/solve.ts';
  import { POWER } from '../engine/types.ts';
  import type { Editor } from '../lib/editor.svelte.ts';
  import { num } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';
  import LineControls from './LineControls.svelte';
  import Picker from './Picker.svelte';
  import { allRecipeOptions } from './helpers.ts';

  let { editor, session }: { editor: Editor; session: GameSession } = $props();
  const data = $derived(session.data);
  const options = $derived(allRecipeOptions(data));

  const plan = $derived(editor.plan);
  const f = $derived(editor.factory);
  // Typed lines in their saved order, then the solver's lines.
  const rows = $derived([
    ...f.lines.map((l) => plan.lines.find((p) => p.id === l.id)).filter((p) => p !== undefined),
    ...plan.lines.filter((p) => p.auto),
  ]);

  function ioText(rates: Record<string, number>, sign: 1 | -1) {
    return Object.entries(rates)
      .filter(([i, r]) => i !== POWER && r * sign > 1e-9)
      .map(([i, r]) => `${num(Math.abs(r))} ${itemName(data, i)}`)
      .join(', ');
  }
</script>

<section class="panel">
  <div class="row bar">
    <Picker {options} placeholder="Add a recipe line…" width="22rem"
      onpick={(id) => editor.apply((g) => addLine(g, id, g.mode === 'manual' ? 1 : 1))} />
    <span class="muted small">
      {#if f.mode === 'manual'}Lines you add start at 1 machine; edit the count, or Auto-size to let targets decide.{:else}Lines added here are locked extras; the solver adds the rest.{/if}
    </span>
  </div>
  <div class="scroll-x">
    <table>
      <thead>
        <tr>
          <th>Recipe</th>
          <th>Machines and settings</th>
          <th>In /min</th>
          <th>Out /min</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as line, i (line.id)}
          <tr class:auto={line.auto}>
            <td class="recipe">
              {#if line.auto}
                {data.recipes[line.recipe]?.name}
                <div class="muted small">solver</div>
              {:else}
                <Picker {options} value={line.recipe} width="15rem" onpick={(id) => editor.apply((g) => updateLine(g, line.id, { recipe: id }))} />
              {/if}
            </td>
            <td><LineControls {line} {editor} {session} compact /></td>
            <td class="io small">{ioText(line.rates, -1)}</td>
            <td class="io small">{ioText(line.rates, 1)}</td>
            <td class="actions">
              {#if !line.auto}
                <button class="ghost" aria-label="Move up" disabled={i === 0} onclick={() => editor.apply((g) => moveLine(g, line.id, -1))}>↑</button>
                <button class="ghost" aria-label="Move down" disabled={i >= f.lines.length - 1} onclick={() => editor.apply((g) => moveLine(g, line.id, 1))}>↓</button>
                <button class="ghost danger" aria-label="Delete line" onclick={() => editor.apply((g) => removeLine(g, line.id))}>✕</button>
              {/if}
            </td>
          </tr>
          {#if !line.auto && f.lines.find((l) => l.id === line.id)?.note}
            <tr class="note"><td colspan="5" class="muted small">{f.lines.find((l) => l.id === line.id)?.note}</td></tr>
          {/if}
        {/each}
        {#if !rows.length}
          <tr><td colspan="5" class="muted">No lines yet.</td></tr>
        {/if}
      </tbody>
    </table>
  </div>
</section>

<style>
  .bar {
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
  }
  .small {
    font-size: 12px;
  }
  .recipe {
    min-width: 12rem;
  }
  .io {
    max-width: 18rem;
  }
  tr.auto td {
    background: color-mix(in srgb, var(--panel-2) 50%, transparent);
  }
  tr.note td {
    border-top: none;
    padding-top: 0;
  }
  .actions {
    white-space: nowrap;
    text-align: right;
  }
</style>
