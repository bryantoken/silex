<script lang="ts">
  /*
   * Barra de formatação flutuante (vidro) sobre o editor. Os botões não tiram o
   * foco do editor (mousedown cancelado), então a seleção atual é preservada.
   *
   * Responsiva: mede o espaço livre no canvas (descontando a barra de status) e
   * mostra só as ferramentas que cabem; as demais vão para o menu "…".
   * Um ResizeObserver no canvas e na barra de status: sem medir a cada quadro.
   */
  import type { EditorView } from "@codemirror/view";
  import Icon from "./Icon.svelte";
  import * as ed from "../editor/commands";
  import { popupMenu, type MenuEntry } from "../menu";

  let { view }: { view: () => EditorView | undefined } = $props();

  type Tool = { icon: string; title: string; run: (v: EditorView, anchor?: HTMLElement) => void } | "sep";

  const HEADINGS = [1, 2, 3, 4, 5, 6];
  const headingItems = (v: EditorView): MenuEntry[] =>
    HEADINGS.map((n) => ({ text: `Título ${n}`, action: () => ed.toggleHeading(v, n) }));

  const above = (el: HTMLElement) => {
    const r = el.getBoundingClientRect();
    return { x: r.left, y: r.top - 6, above: true };
  };

  const tools: Tool[] = [
    { icon: "highlighter", title: "Realçar", run: (v) => ed.toggleWrap(v, "==") },
    { icon: "bold", title: "Negrito", run: (v) => ed.toggleWrap(v, "**") },
    { icon: "italic", title: "Itálico", run: (v) => ed.toggleWrap(v, "*") },
    { icon: "strike", title: "Tachado", run: (v) => ed.toggleWrap(v, "~~") },
    { icon: "code", title: "Código", run: (v) => ed.toggleWrap(v, "`") },
    "sep",
    { icon: "heading", title: "Título", run: (v, a) => popupMenu(headingItems(v), a ? above(a) : undefined) },
    { icon: "list", title: "Lista", run: (v) => ed.toggleLinePrefix(v, "bullet") },
    { icon: "list-ordered", title: "Lista numerada", run: (v) => ed.toggleLinePrefix(v, "number") },
    { icon: "square-check", title: "Tarefa (Ctrl+L)", run: (v) => (ed.setTaskStatus(v, null), v.focus()) },
    { icon: "quote", title: "Citação", run: (v) => ed.toggleLinePrefix(v, "quote") },
    "sep",
    { icon: "link", title: "Link para nota [[ ]]", run: ed.insertWikilink },
    { icon: "sheet", title: "Tabela", run: ed.insertTable },
    { icon: "code-block", title: "Bloco de código", run: ed.insertCodeBlock },
  ];

  // Larguras fixas (em px, iguais ao CSS): botão 30 + 1 de gap, separador 9 + 1
  const BTN = 31;
  const SEP = 10;
  const PAD = 12;
  const width = (t: Tool) => (t === "sep" ? SEP : BTN);
  const TOTAL = tools.reduce((n, t) => n + width(t), 0);

  let dock = $state<HTMLDivElement>();
  let fit = $state(tools.length);
  let visible = $derived.by(() => {
    const out = tools.slice(0, fit);
    while (out.at(-1) === "sep") out.pop();
    return out;
  });
  let hidden = $derived(tools.slice(fit));

  $effect(() => {
    const canvas = dock?.offsetParent as HTMLElement | null;
    if (!canvas) return;
    const measure = () => {
      const status = canvas.querySelector<HTMLElement>(".sx-status");
      // Centralizada: precisa de espaço livre dos dois lados da barra de status
      const avail = canvas.clientWidth - 2 * ((status?.offsetWidth ?? 0) + 20) - PAD;
      if (TOTAL <= avail) return void (fit = tools.length);
      let used = BTN; // botão "…"
      let n = 0;
      while (n < tools.length && used + width(tools[n]) <= avail) used += width(tools[n++]);
      fit = Math.max(1, n);
    };
    const ro = new ResizeObserver(measure);
    ro.observe(canvas);
    const status = canvas.querySelector(".sx-status");
    if (status) ro.observe(status);
    measure();
    return () => ro.disconnect();
  });

  function overflow(e: MouseEvent) {
    const v = view();
    if (!v) return;
    const entries: MenuEntry[] = [];
    for (const t of hidden) {
      if (t === "sep") {
        if (entries.length && entries.at(-1) !== "separator") entries.push("separator");
      } else if (t.icon === "heading") entries.push({ text: t.title, items: headingItems(v) });
      else entries.push({ text: t.title, action: () => t.run(v) });
    }
    popupMenu(entries, above(e.currentTarget as HTMLElement));
  }
</script>

<div class="sx-dock" role="toolbar" aria-label="Formatação" bind:this={dock}>
  {#each visible as tool}
    {#if tool === "sep"}
      <span class="sx-dock-sep"></span>
    {:else}
      <button
        class="sx-dock-btn"
        title={tool.title}
        aria-label={tool.title}
        onmousedown={(e) => e.preventDefault()}
        onclick={(e) => {
          const v = view();
          if (v) tool.run(v, e.currentTarget);
        }}
      >
        <Icon name={tool.icon} size={16} stroke={2} />
      </button>
    {/if}
  {/each}
  {#if hidden.some((t) => t !== "sep")}
    <button class="sx-dock-btn" title="Mais ferramentas" aria-label="Mais ferramentas" onmousedown={(e) => e.preventDefault()} onclick={overflow}>
      <Icon name="more" size={16} stroke={3} />
    </button>
  {/if}
</div>
