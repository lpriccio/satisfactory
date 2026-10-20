<script lang="ts">
  // Ways out of a plan that can't balance.
  import { regenerate, resetPlan } from '../engine/edits.ts';
  import type { Editor } from '../lib/editor.svelte.ts';

  let { editor }: { editor: Editor } = $props();
  const canRegenerate = $derived(regenerate(editor.factory, editor.plan) !== editor.factory);
</script>

<span class="fix">
  <button
    disabled={!canRegenerate}
    title={canRegenerate
      ? 'Put the items that depend on the short item back on their standard recipes'
      : 'Everything involved already uses standard recipes'}
    onclick={() => editor.apply((f) => regenerate(f, editor.plan))}>Regenerate with standard recipes</button>
  <button
    title="Start the plan over from your targets: standard recipes, default imports, no line settings. Undo brings it back."
    onclick={() => editor.apply(resetPlan)}>Reset</button>
</span>

<style>
  .fix {
    display: inline-flex;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
