<script lang="ts">
  import { itemImported, toggleItemImport } from '../engine/edits.ts';
  import { itemName } from '../engine/solve.ts';
  import { POWER, type PlanItem } from '../engine/types.ts';
  import type { Editor } from '../lib/editor.svelte.ts';
  import { num, signed } from '../lib/format.ts';
  import type { GameSession } from '../lib/game.svelte.ts';

  let { editor, session }: { editor: Editor; session: GameSession } = $props();
  const data = $derived(session.data);

  type Col = 'item' | 'produced' | 'consumed' | 'imported' | 'surplus' | 'target' | 'net';
  let sortBy = $state<Col>('item');
  let desc = $state(false);
  let hideZero = $state(true);

  const plan = $derived(editor.plan);
  const rows = $derived.by(() => {
    const list = Object.values(plan.items).filter(
      (p) => p.item !== POWER && (!hideZero || p.produced + p.consumed + p.imported + p.target > 1e-6),
    );
    const key = (p: PlanItem) =>
      sortBy === 'item' ? itemName(data, p.item).toLowerCase() : sortBy === 'net' ? p.produced - p.consumed : p[sortBy];
    list.sort((a, b) => {
      const x = key(a);
      const y = key(b);
      const c = typeof x === 'string' ? x.localeCompare(y as string) : (x as number) - (y as number);
      return desc ? -c : c;
    });
    return list;
  });

  function sort(c: Col) {
    if (sortBy === c) desc = !desc;
    else {
      sortBy = c;
      desc = c !== 'item';
    }
  }

  const COLS: { id: Col; label: string; title: string }[] = [
    { id: 'produced', label: 'Produced', title: 'Made by lines in this factory' },
    { id: 'consumed', label: 'Consumed', title: 'Eaten by lines in this factory' },
    { id: 'net', label: 'Net', title: 'Produced − consumed' },
    { id: 'imported', label: 'Imported', title: 'Brought in from outside' },
    { id: 'target', label: 'Target', title: 'Sent out as the factory output' },
    { id: 'surplus', label: 'Surplus', title: 'Made but not used: byproducts or rounding' },
  ];
</script>

<section class="panel">
  <div class="row bar">
    <label><input type="checkbox" bind:checked={hideZero} /> Hide items with no flow</label>
    <span class="grow"></span>
    <span class="muted small">All figures per minute. The import toggle applies to every line using the item.</span>
  </div>
  <div class="scroll-x">
    <table>
      <thead>
        <tr>
          <th><button class="ghost sort" onclick={() => sort('item')}>Item {sortBy === 'item' ? (desc ? '↓' : '↑') : ''}</button></th>
          {#each COLS as c (c.id)}
            <th class="n"><button class="ghost sort" title={c.title} onclick={() => sort(c.id)}>{c.label} {sortBy === c.id ? (desc ? '↓' : '↑') : ''}</button></th>
          {/each}
          <th>Import</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as p (p.item)}
          {@const net = p.produced - p.consumed}
          {@const imported = itemImported(data, editor.factory, p.item)}
          <tr>
            <td>
              {itemName(data, p.item)}
              {#if p.unbalanced > 1e-6}<span class="bad small"> · {num(p.unbalanced)} short</span>{/if}
            </td>
            <td class="n">{num(p.produced)}</td>
            <td class="n">{num(p.consumed)}</td>
            <td class="n" class:good={net > 1e-6} class:bad={net < -1e-6}>{signed(net)}</td>
            <td class="n imp">{p.imported > 1e-6 ? num(p.imported) : ''}</td>
            <td class="n">{p.target > 1e-6 ? num(p.target) : ''}</td>
            <td class="n warn">{p.surplus > 1e-6 ? num(p.surplus) : ''}</td>
            <td>
              <button class="chip" class:import={imported} aria-pressed={imported} onclick={() => editor.apply((g) => toggleItemImport(data, g, p.item))}>
                {imported ? 'imported' : 'import'}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
</section>

<style>
  .bar {
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }
  .small {
    font-size: 12px;
  }
  .sort {
    padding: 0;
    font-weight: 600;
    color: var(--muted);
    font-size: 12px;
  }
</style>
