<script lang="ts">
  import { wholeMachines } from '../engine/lineMath.ts';
  import type { PlanLine } from '../engine/types.ts';
  import type { Editor } from '../lib/editor.svelte.ts';
  import { num, pct } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';

  let { editor, session }: { editor: Editor; session: GameSession } = $props();
  const data = $derived(session.data);
  const features = $derived(data.features);

  const plan = $derived(editor.plan);
  const groups = $derived.by(() => {
    const by = new Map<string, PlanLine[]>();
    for (const l of plan.lines) {
      if (l.machines < 1e-9) continue;
      by.set(l.building, [...(by.get(l.building) ?? []), l]);
    }
    return [...by.entries()]
      .map(([building, lines]) => ({
        building,
        name: data.buildings[building]?.name ?? building,
        lines: lines.sort((a, b) => b.machines - a.machines),
        machines: lines.reduce((a, l) => a + l.machines, 0),
        whole: lines.reduce((a, l) => a + wholeMachines(l.machines), 0),
        power: lines.reduce((a, l) => a + l.powerMW, 0),
        footprint: lines.reduce((a, l) => a + l.footprint, 0),
      }))
      .sort((a, b) => b.whole - a.whole);
  });
  const spacing = $derived(editor.factory.spacing);
</script>

<section class="panel scroll-x">
  <table>
    <thead>
      <tr>
        <th>Building / recipe</th>
        <th class="n">Machines</th>
        <th class="n">Whole</th>
        {#if features.clock}<th class="n">Clock</th><th class="n">Sloops</th>{/if}
        {#if features.power}<th class="n">MW</th>{/if}
        {#if features.footprint}<th class="n">Floor {features.areaUnit}</th>{/if}
        {#if plan.foundations !== undefined}<th class="n">Foundations</th>{/if}
      </tr>
    </thead>
    <tbody>
      {#each groups as g (g.building)}
        <tr class="group">
          <td>{g.name}</td>
          <td class="n">{num(g.machines)}</td>
          <td class="n">{g.whole}</td>
          {#if features.clock}<td></td><td></td>{/if}
          {#if features.power}<td class="n">{num(g.power)}</td>{/if}
          {#if features.footprint}<td class="n">{num(g.footprint)}</td>{/if}
          {#if plan.foundations !== undefined}<td class="n">{num((g.footprint * spacing) / 64)}</td>{/if}
        </tr>
        {#each g.lines as l (l.id)}
          <tr>
            <td class="indent">{data.recipes[l.recipe]?.name ?? l.recipe}</td>
            <td class="n">{num(l.machines)}</td>
            <td class="n">{wholeMachines(l.machines)}</td>
            {#if features.clock}<td class="n">{pct(l.settings.clock)}</td><td class="n">{l.settings.sloops ?? ''}</td>{/if}
            {#if features.power}<td class="n">{num(l.powerMW)}</td>{/if}
            {#if features.footprint}<td class="n">{num(l.footprint)}</td>{/if}
            {#if plan.foundations !== undefined}<td class="n">{num((l.footprint * spacing) / 64)}</td>{/if}
          </tr>
        {/each}
      {/each}
      <tr class="total">
        <td>Total</td>
        <td class="n">{num(plan.machines)}</td>
        <td class="n">{groups.reduce((a, g) => a + g.whole, 0)}</td>
        {#if features.clock}<td></td><td></td>{/if}
        {#if features.power}<td class="n">{num(plan.powerMW)}</td>{/if}
        {#if features.footprint}<td class="n">{num(plan.footprint)}</td>{/if}
        {#if plan.foundations !== undefined}<td class="n">{num(plan.foundations)}</td>{/if}
      </tr>
    </tbody>
  </table>
</section>

<style>
  tr.group td {
    font-weight: 600;
    background: var(--panel-2);
  }
  .indent {
    padding-left: 24px;
  }
  tr.total td {
    font-weight: 700;
    border-top: 2px solid var(--line-strong);
    border-bottom: none;
  }
</style>
