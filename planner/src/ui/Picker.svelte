<script lang="ts" module>
  export interface Option {
    id: string;
    label: string;
    hint?: string;
  }
</script>

<script lang="ts">
  let {
    options,
    value = '',
    placeholder = 'Search…',
    onpick,
    width = '16rem',
  }: {
    options: Option[];
    value?: string;
    placeholder?: string;
    onpick: (id: string) => void;
    width?: string;
  } = $props();

  const listId = `picker-${Math.random().toString(36).slice(2, 9)}`;
  let query = $state('');
  let open = $state(false);
  let active = $state(0);
  let input: HTMLInputElement;

  const current = $derived(options.find((o) => o.id === value));
  const matches = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return options.slice(0, 60);
    const words = q.split(/\s+/);
    const hits = options.filter((o) => {
      const hay = `${o.label} ${o.hint ?? ''} ${o.id}`.toLowerCase();
      return words.every((w) => hay.includes(w));
    });
    // Prefix matches on the label first.
    hits.sort((a, b) => Number(!a.label.toLowerCase().startsWith(q)) - Number(!b.label.toLowerCase().startsWith(q)));
    return hits.slice(0, 60);
  });

  function pick(o: Option) {
    onpick(o.id);
    open = false;
    query = '';
    input?.blur();
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      active = Math.min(active + 1, matches.length - 1);
      open = true;
      e.preventDefault();
    } else if (e.key === 'ArrowUp') {
      active = Math.max(active - 1, 0);
      e.preventDefault();
    } else if (e.key === 'Enter' && open && matches[active]) {
      pick(matches[active]);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      open = false;
      query = '';
      input?.blur();
    }
  }
</script>

<div class="picker" style:width>
  <input
    bind:this={input}
    bind:value={query}
    placeholder={current?.label ?? placeholder}
    class:has-value={!!current}
    onfocus={() => ((open = true), (active = 0))}
    onblur={() => setTimeout(() => (open = false), 150)}
    oninput={() => ((open = true), (active = 0))}
    onkeydown={keydown}
    aria-label={placeholder}
    role="combobox"
    aria-expanded={open}
    aria-controls={listId}
  />
  {#if open && matches.length}
    <ul role="listbox" id={listId}>
      {#each matches as o, i (o.id)}
        <li role="option" aria-selected={i === active}>
          <button type="button" class:active={i === active} onmousedown={(e) => (e.preventDefault(), pick(o))}>
            <span>{o.label}</span>
            {#if o.hint}<span class="muted hint">{o.hint}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    max-width: 100%;
  }
  input {
    width: 100%;
  }
  input.has-value::placeholder {
    color: var(--text);
  }
  ul {
    position: absolute;
    z-index: 20;
    top: calc(100% + 2px);
    left: 0;
    min-width: 100%;
    width: max-content;
    max-width: min(32rem, 90vw);
    max-height: 18rem;
    overflow-y: auto;
    margin: 0;
    padding: 4px;
    list-style: none;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }
  li button {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    border: none;
    background: none;
    text-align: left;
    padding: 4px 8px;
    border-radius: 4px;
  }
  li button.active,
  li button:hover {
    background: var(--panel-2);
  }
  .hint {
    font-size: 12px;
  }
</style>
