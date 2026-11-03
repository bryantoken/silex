<script lang="ts">
  // Sumário da nota aberta: árvore de cabeçalhos retrátil; clique leva ao cabeçalho.
  import { SvelteSet } from "svelte/reactivity";
  import Icon from "./Icon.svelte";
  import { parseHeadings, type Heading } from "../markdown/headings";

  type Props = { content: string; path: string; onjump: (h: Heading) => void };
  let { content, path, onjump }: Props = $props();

  type Node = { h: Heading; children: Node[] };

  // O texto muda a cada tecla: espera uma pausa curta antes de reler os cabeçalhos
  let source = $state("");
  $effect(() => {
    const c = content;
    const t = setTimeout(() => (source = c), source ? 250 : 0);
    return () => clearTimeout(t);
  });

  let tree = $derived.by(() => {
    const root: Node[] = [];
    const stack: Node[] = [];
    for (const h of parseHeadings(source)) {
      const node = { h, children: [] };
      while (stack.length && stack[stack.length - 1].h.level >= h.level) stack.pop();
      (stack.length ? stack[stack.length - 1].children : root).push(node);
      stack.push(node);
    }
    return root;
  });

  // Retraídos por nota (chave: linha + texto, sobrevive a edições em outras partes)
  const collapsedByPath = new Map<string, SvelteSet<string>>();
  let collapsed = $derived.by(() => {
    let set = collapsedByPath.get(path);
    if (!set) collapsedByPath.set(path, (set = new SvelteSet()));
    return set;
  });
  const keyOf = (h: Heading) => `${h.level}:${h.text}`;

  function setAll(fold: boolean) {
    collapsed.clear();
    if (!fold) return;
    const walk = (nodes: Node[]) => nodes.forEach((n) => n.children.length && (collapsed.add(keyOf(n.h)), walk(n.children)));
    walk(tree);
  }
</script>

<div class="sx-outline">
  <div class="sx-outline-tools">
    <span class="sx-outline-title">Sumário</span>
    <button class="icon-btn sm" title="Expandir tudo" onclick={() => setAll(false)}><Icon name="list" size={14} /></button>
    <button class="icon-btn sm" title="Recolher tudo" onclick={() => setAll(true)}><Icon name="collapse" size={14} /></button>
  </div>
  {#if !tree.length}
    <p class="sx-outline-empty">Esta nota não tem cabeçalhos.</p>
  {:else}
    <ul class="sx-outline-list">{@render items(tree)}</ul>
  {/if}
</div>

{#snippet items(nodes: Node[])}
  {#each nodes as n (n.h.from)}
    {@const key = keyOf(n.h)}
    {@const shut = collapsed.has(key)}
    <li>
      <div class="sx-outline-row" style:padding-left="{(n.h.level - 1) * 4}px">
        {#if n.children.length}
          <button
            class="sx-outline-chev"
            class:is-open={!shut}
            title={shut ? "Expandir" : "Recolher"}
            onclick={() => (shut ? collapsed.delete(key) : collapsed.add(key))}
          >
            <Icon name="chevron" size={12} stroke={2.2} />
          </button>
        {:else}
          <span class="sx-outline-chev is-leaf"></span>
        {/if}
        <button class="sx-outline-item" data-level={n.h.level} title={n.h.text} onclick={() => onjump(n.h)}>{n.h.text}</button>
      </div>
      {#if n.children.length && !shut}
        <ul>{@render items(n.children)}</ul>
      {/if}
    </li>
  {/each}
{/snippet}
