<script lang="ts" module>
  export type PaletteItem = {
    id: string;
    title: string;
    detail?: string;
    hotkey?: string;
    icon?: string;
    run: (e: { ctrl: boolean }) => void;
  };
</script>

<script lang="ts">
  import { tick } from "svelte";
  import { fuzzy, highlightParts } from "../fuzzy";
  import Icon from "./Icon.svelte";

  type Props = {
    items: PaletteItem[];
    placeholder: string;
    /** Item extra quando nada corresponde (ex.: "Criar nota …") */
    fallback?: (query: string) => PaletteItem | null;
    onclose: () => void;
  };
  let { items, placeholder, fallback, onclose }: Props = $props();

  let query = $state("");
  let selected = $state(0);
  let input = $state<HTMLInputElement>();
  let list = $state<HTMLDivElement>();

  const MAX = 80;

  let results = $derived.by(() => {
    if (!query.trim()) return items.slice(0, MAX).map((item) => ({ item, indices: [] as number[] }));
    const scored = [];
    for (const item of items) {
      const m = fuzzy(query, item.title);
      if (m) scored.push({ item, indices: m.indices, score: m.score });
    }
    scored.sort((a, b) => b.score - a.score);
    const out = scored.slice(0, MAX);
    const extra = fallback?.(query.trim());
    if (extra && !scored.some((s) => s.item.title.toLowerCase() === query.trim().toLowerCase())) {
      out.push({ item: extra, indices: [], score: -1e9 });
    }
    return out;
  });

  $effect(() => {
    query;
    selected = 0;
  });

  $effect(() => {
    tick().then(() => input?.focus());
  });

  function run(i: number, ctrl = false) {
    const r = results[i];
    if (!r) return;
    onclose();
    r.item.run({ ctrl });
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selected = (selected + 1) % Math.max(1, results.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selected = (selected - 1 + results.length) % Math.max(1, results.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(selected, e.ctrlKey || e.metaKey);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    }
  }

  $effect(() => {
    const el = list?.querySelector<HTMLElement>(`[data-i="${selected}"]`);
    el?.scrollIntoView({ block: "nearest" });
  });
</script>

<div class="sx-overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sx-palette" role="dialog" aria-label={placeholder}>
    <input bind:this={input} bind:value={query} {placeholder} spellcheck="false" {onkeydown} />
    <div class="sx-palette-list" bind:this={list} role="listbox">
      {#each results as r, i (r.item.id)}
        <button
          class="sx-palette-item"
          class:is-selected={i === selected}
          data-i={i}
          role="option"
          aria-selected={i === selected}
          onmousemove={() => (selected = i)}
          onclick={(e) => run(i, e.ctrlKey || e.metaKey)}
        >
          {#if r.item.icon}<Icon name={r.item.icon} size={15} />{/if}
          <span class="title">
            {#each highlightParts(r.item.title, r.indices) as part}{#if part.hl}<mark>{part.text}</mark>{:else}{part.text}{/if}{/each}
          </span>
          {#if r.item.detail}<span class="detail">{r.item.detail}</span>{/if}
          {#if r.item.hotkey}<kbd>{r.item.hotkey}</kbd>{/if}
        </button>
      {:else}
        <div class="sx-palette-empty">Nada encontrado.</div>
      {/each}
    </div>
    <div class="sx-palette-foot">
      <span><kbd>↑↓</kbd> navegar</span>
      <span><kbd>Enter</kbd> executar</span>
      <span><kbd>Ctrl Enter</kbd> nova aba</span>
      <span><kbd>Esc</kbd> fechar</span>
    </div>
  </div>
</div>
