<script lang="ts">
  // Per-line knobs shared by the Flow and Lines views.
  import { lockLine, setLineSettings, unlockLine, updateLine } from '../engine/edits.ts';
  import type { PlanLine } from '../engine/types.ts';
  import type { Editor } from '../lib/editor.svelte.ts';
  import { num } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';
  import { wholeMachines } from '../engine/lineMath.ts';

  let { line, editor, session, compact = false }: { line: PlanLine; editor: Editor; session: GameSession; compact?: boolean } = $props();

  const data = $derived(session.data);
  const features = $derived(data.features);
  const recipe = $derived(data.recipes[line.recipe]);
  const building = $derived(data.buildings[line.building]);
  const slots = $derived(building?.sloopSlots ?? 0);
  const explicit = $derived(!line.auto);
  const s = $derived(line.settings);

  function settings(patch: Parameters<typeof setLineSettings>[3], key: string) {
    editor.apply((f) => setLineSettings(f, line.id, line.recipe, patch), `${line.id}-${key}`);
  }
</script>

<div class="controls" class:compact>
  <span class="machines" title={`${num(line.machines, 4)} machines`}>
    {#if explicit && line.locked}
      <input type="number" min="0" step="any" value={Number(line.machines.toFixed(4))} aria-label="Machines" style="width: 6rem"
        oninput={(e) => editor.apply((f) => updateLine(f, line.id, { machines: Math.max(0, Number(e.currentTarget.value) || 0) }), `${line.id}-machines`)} />
    {:else}
      <b class="mono">{num(line.machines)}</b>
    {/if}
    <span class="muted">× {building?.name ?? line.building}{wholeMachines(line.machines) !== line.machines ? ` (${wholeMachines(line.machines)})` : ''}</span>
  </span>

  {#if recipe && recipe.buildings.length > 1}
    <select value={line.building} aria-label="Building" onchange={(e) => settings({ building: e.currentTarget.value }, 'building')}>
      {#each recipe.buildings as b (b)}
        <option value={b}>{data.buildings[b]?.name ?? b}</option>
      {/each}
    </select>
  {/if}

  {#if features.clock}
    <label title="Clock speed (1–250%)">
      <input type="number" min="1" max="250" step="any" style="width: 5.2rem" value={Number(((s.clock ?? 1) * 100).toFixed(4))} aria-label="Clock %"
        onchange={(e) => {
          const v = Number(e.currentTarget.value);
          settings({ clock: v && v !== 100 ? Math.min(250, Math.max(1, v)) / 100 : undefined }, 'clock');
        }} />%
    </label>
  {/if}
  {#if features.sloops && slots > 0}
    <label title="Somersloops slotted: output ×(1 + n/{slots}), power ×(1 + n/{slots})²">
      <select value={s.sloops ?? 0} aria-label="Somersloops" onchange={(e) => settings({ sloops: Number(e.currentTarget.value) || undefined }, 'sloops')}>
        {#each Array.from({ length: slots + 1 }, (_, i) => i) as n (n)}
          <option value={n}>{n} sloop{n === 1 ? '' : 's'}</option>
        {/each}
      </select>
    </label>
  {/if}
  {#if features.speedBonus}
    <label title="Speed bonus from modules, beacons or proliferator">
      speed +<input type="number" step="any" style="width: 4.5rem" value={Number(((s.speedBonus ?? 0) * 100).toFixed(2))} aria-label="Speed bonus %"
        onchange={(e) => settings({ speedBonus: Number(e.currentTarget.value) / 100 || undefined }, 'speed')} />%
    </label>
  {/if}
  {#if features.productivity && (data.game !== 'factorio' || recipe?.allowProductivity)}
    <label title="Productivity bonus (outputs only)">
      prod +<input type="number" step="any" style="width: 4.5rem" value={Number(((s.prodBonus ?? 0) * 100).toFixed(2))} aria-label="Productivity bonus %"
        onchange={(e) => settings({ prodBonus: Number(e.currentTarget.value) / 100 || undefined }, 'prod')} />%
    </label>
  {/if}
  {#if features.power && !compact}
    <span class="mono muted" title="Power for the whole line">{line.powerMW < 0 ? '+' : ''}{num(Math.abs(line.powerMW))} MW</span>
  {/if}

  {#if editor.factory.mode === 'target'}
    {#if line.auto}
      <button class="ghost" title="Pin this line at its current machine count" onclick={() => editor.apply((f) => lockLine(f, editor.plan, line.id))}>Lock</button>
    {:else if line.locked}
      <button class="ghost" title="Let the solver size this line again" onclick={() => editor.apply((f) => unlockLine(f, line.id))}>Unlock</button>
    {/if}
  {:else if line.locked}
    <button class="ghost" title="Let the solver size this line to meet targets" onclick={() => editor.apply((f) => unlockLine(f, line.id))}>Auto-size</button>
  {:else}
    <button class="ghost" title="Fix the machine count" onclick={() => editor.apply((f) => updateLine(f, line.id, { machines: Number(line.machines.toFixed(4)) }))}>Lock</button>
  {/if}
</div>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .controls label {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    color: var(--muted);
  }
  .machines {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    min-width: 9rem;
  }
</style>
