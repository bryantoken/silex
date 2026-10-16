<script lang="ts">
  /*
   * Canvas do Silex — editor de arquivos .canvas no formato aberto JSON Canvas
   * (o mesmo do Obsidian). Rolagem move; Ctrl+rolagem dá zoom; arraste o fundo
   * para mover; duplo clique cria cartão; arraste as bolinhas laterais para ligar.
   */
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import { renderBlock } from "../markdown/render";
  import Icon from "../components/Icon.svelte";

  type Side = "top" | "right" | "bottom" | "left";
  type CNode = {
    id: string;
    type: "text" | "file" | "link" | "group";
    x: number;
    y: number;
    width: number;
    height: number;
    color?: string;
    text?: string;
    file?: string;
    subpath?: string;
    url?: string;
    label?: string;
    styleAttributes?: { shape?: string; textAlign?: string; border?: string };
    [k: string]: unknown;
  };
  type CEdge = { id: string; fromNode: string; toNode: string; fromSide?: Side; toSide?: Side; fromEnd?: string; toEnd?: string; color?: string; label?: string; [k: string]: unknown };

  type Props = {
    path: string;
    vaultRoot: string;
    resolveFile: (name: string) => string | null;
    onopen: (path: string, newTab: boolean) => void;
    onopenlink: (target: string) => void;
    onnotify: (text: string, error?: boolean) => void;
  };
  let { path, vaultRoot, resolveFile, onopen, onopenlink, onnotify }: Props = $props();

  let nodes = $state<CNode[]>([]);
  let edges = $state<CEdge[]>([]);
  let extra: Record<string, unknown> = {};
  let loaded = $state(false);
  let wrap = $state<HTMLDivElement>();
  let scale = $state(1);
  let ox = $state(0);
  let oy = $state(0);
  let selected = $state(new Set<string>());
  let selectedEdge = $state<string | null>(null);
  let editing = $state<string | null>(null);
  let linking = $state<{ from: string; side: Side; x: number; y: number } | null>(null);
  let previews = $state<Record<string, string>>({});
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let dirty = false;

  const COLORS: Record<string, string> = { "1": "#fb464c", "2": "#e9973f", "3": "#e0de71", "4": "#44cf6e", "5": "#53dfdd", "6": "#a882ff" };
  const colorOf = (c?: string) => (c ? (COLORS[c] ?? c) : undefined);
  const uid = () => Array.from(crypto.getRandomValues(new Uint8Array(8)), (b) => b.toString(16).padStart(2, "0")).join("");
  const IMG = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;

  async function load() {
    try {
      const raw = await invoke<string>("read_note", { filePath: path });
      const data = raw.trim() ? JSON.parse(raw) : {};
      nodes = Array.isArray(data.nodes) ? data.nodes : [];
      edges = Array.isArray(data.edges) ? data.edges : [];
      const { nodes: _n, edges: _e, ...rest } = data;
      extra = rest;
      loaded = true;
      await tick();
      fit();
      loadPreviews();
    } catch (e) {
      onnotify(`Canvas inválido: ${e}`, true);
    }
  }

  function absOf(file: string) {
    return resolveFile(file) ?? `${vaultRoot}/${file}`.replace(/\//g, vaultRoot.includes("\\") ? "\\" : "/");
  }

  async function loadPreviews() {
    for (const n of nodes) {
      if (n.type !== "file" || !n.file || IMG.test(n.file) || previews[n.id] !== undefined) continue;
      if (!/\.md$/i.test(n.file)) continue;
      try {
        const md = await invoke<string>("read_note", { filePath: absOf(n.file) });
        previews[n.id] = renderBlock(md.replace(/^---\n[\s\S]*?\n---\n?/, "").slice(0, 3000));
      } catch {
        previews[n.id] = "";
      }
    }
  }

  function save() {
    dirty = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(flush, 600);
  }
  async function flush() {
    if (!dirty) return;
    dirty = false;
    // Formato JSON Canvas com tabulações, como o Obsidian grava
    const content = JSON.stringify({ ...extra, nodes, edges }, null, "\t");
    await invoke("write_text_file", { path, content }).catch((e) => onnotify(String(e), true));
  }

  function fit() {
    if (!wrap || !nodes.length) {
      scale = 1;
      ox = (wrap?.clientWidth ?? 800) / 2;
      oy = (wrap?.clientHeight ?? 600) / 2;
      return;
    }
    const minX = Math.min(...nodes.map((n) => n.x));
    const minY = Math.min(...nodes.map((n) => n.y));
    const maxX = Math.max(...nodes.map((n) => n.x + n.width));
    const maxY = Math.max(...nodes.map((n) => n.y + n.height));
    const w = wrap.clientWidth - 80;
    const h = wrap.clientHeight - 80;
    scale = Math.min(1.5, Math.max(0.1, Math.min(w / (maxX - minX || 1), h / (maxY - minY || 1))));
    ox = wrap.clientWidth / 2 - ((minX + maxX) / 2) * scale;
    oy = wrap.clientHeight / 2 - ((minY + maxY) / 2) * scale;
  }

  const toWorld = (cx: number, cy: number) => {
    const r = wrap!.getBoundingClientRect();
    return { x: (cx - r.left - ox) / scale, y: (cy - r.top - oy) / scale };
  };

  function anchor(n: CNode, side: Side) {
    switch (side) {
      case "top":
        return { x: n.x + n.width / 2, y: n.y };
      case "bottom":
        return { x: n.x + n.width / 2, y: n.y + n.height };
      case "left":
        return { x: n.x, y: n.y + n.height / 2 };
      default:
        return { x: n.x + n.width, y: n.y + n.height / 2 };
    }
  }

  function bestSides(a: CNode, b: CNode): [Side, Side] {
    const dx = b.x + b.width / 2 - (a.x + a.width / 2);
    const dy = b.y + b.height / 2 - (a.y + a.height / 2);
    if (Math.abs(dx) > Math.abs(dy)) return dx > 0 ? ["right", "left"] : ["left", "right"];
    return dy > 0 ? ["bottom", "top"] : ["top", "bottom"];
  }

  const DIR: Record<Side, [number, number]> = { top: [0, -1], bottom: [0, 1], left: [-1, 0], right: [1, 0] };

  function edgePath(e: CEdge) {
    const a = nodes.find((n) => n.id === e.fromNode);
    const b = nodes.find((n) => n.id === e.toNode);
    if (!a || !b) return null;
    const [fs, ts] = e.fromSide && e.toSide ? [e.fromSide, e.toSide] : bestSides(a, b);
    const p1 = anchor(a, fs);
    const p2 = anchor(b, ts);
    const d = Math.max(40, Math.hypot(p2.x - p1.x, p2.y - p1.y) / 2.5);
    const c1 = { x: p1.x + DIR[fs][0] * d, y: p1.y + DIR[fs][1] * d };
    const c2 = { x: p2.x + DIR[ts][0] * d, y: p2.y + DIR[ts][1] * d };
    const mid = { x: (p1.x + 3 * c1.x + 3 * c2.x + p2.x) / 8, y: (p1.y + 3 * c1.y + 3 * c2.y + p2.y) / 8 };
    return { d: `M${p1.x},${p1.y} C${c1.x},${c1.y} ${c2.x},${c2.y} ${p2.x},${p2.y}`, mid };
  }

  // ----- Interação -----

  let gesture: null | { kind: "pan"; x: number; y: number; ox: number; oy: number } | { kind: "move"; x: number; y: number; start: Map<string, { x: number; y: number }>; moved: boolean } | { kind: "resize"; id: string; x: number; y: number; w: number; h: number } = null;

  function childrenOf(group: CNode) {
    return nodes.filter((n) => n.id !== group.id && n.x >= group.x && n.y >= group.y && n.x + n.width <= group.x + group.width && n.y + n.height <= group.y + group.height);
  }

  function onBgDown(e: PointerEvent) {
    if (e.button !== 0 && e.button !== 1) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    if (!e.shiftKey) selected = new Set();
    selectedEdge = null;
    editing = null;
    gesture = { kind: "pan", x: e.clientX, y: e.clientY, ox, oy };
  }

  function onNodeDown(e: PointerEvent, n: CNode) {
    if (e.button !== 0 || editing === n.id) return;
    e.stopPropagation();
    wrap!.setPointerCapture(e.pointerId);
    selectedEdge = null;
    if (!selected.has(n.id)) selected = e.shiftKey ? new Set([...selected, n.id]) : new Set([n.id]);
    const ids = new Set(selected);
    for (const id of selected) {
      const g = nodes.find((x) => x.id === id);
      if (g?.type === "group") childrenOf(g).forEach((c) => ids.add(c.id));
    }
    gesture = { kind: "move", x: e.clientX, y: e.clientY, start: new Map(nodes.filter((x) => ids.has(x.id)).map((x) => [x.id, { x: x.x, y: x.y }])), moved: false };
  }

  function onResizeDown(e: PointerEvent, n: CNode) {
    e.stopPropagation();
    wrap!.setPointerCapture(e.pointerId);
    gesture = { kind: "resize", id: n.id, x: e.clientX, y: e.clientY, w: n.width, h: n.height };
  }

  function onLinkDown(e: PointerEvent, n: CNode, side: Side) {
    e.stopPropagation();
    wrap!.setPointerCapture(e.pointerId);
    const p = toWorld(e.clientX, e.clientY);
    linking = { from: n.id, side, x: p.x, y: p.y };
  }

  function onMove(e: PointerEvent) {
    if (linking) {
      const p = toWorld(e.clientX, e.clientY);
      linking = { ...linking, x: p.x, y: p.y };
      return;
    }
    if (!gesture) return;
    if (gesture.kind === "pan") {
      ox = gesture.ox + e.clientX - gesture.x;
      oy = gesture.oy + e.clientY - gesture.y;
    } else if (gesture.kind === "move") {
      const dx = (e.clientX - gesture.x) / scale;
      const dy = (e.clientY - gesture.y) / scale;
      if (Math.abs(dx) + Math.abs(dy) > 2) gesture.moved = true;
      for (const n of nodes) {
        const s = gesture.start.get(n.id);
        if (s) {
          n.x = Math.round(s.x + dx);
          n.y = Math.round(s.y + dy);
        }
      }
    } else {
      const n = nodes.find((x) => x.id === (gesture as { id: string }).id);
      if (n) {
        n.width = Math.max(60, Math.round(gesture.w + (e.clientX - gesture.x) / scale));
        n.height = Math.max(40, Math.round(gesture.h + (e.clientY - gesture.y) / scale));
      }
    }
  }

  function onUp(e: PointerEvent) {
    if (linking) {
      const p = toWorld(e.clientX, e.clientY);
      const target = [...nodes].reverse().find((n) => n.type !== "group" && p.x >= n.x && p.x <= n.x + n.width && p.y >= n.y && p.y <= n.y + n.height) ?? nodes.find((n) => p.x >= n.x && p.x <= n.x + n.width && p.y >= n.y && p.y <= n.y + n.height);
      if (target && target.id !== linking.from) {
        const from = nodes.find((n) => n.id === linking!.from)!;
        const [, toSide] = bestSides(from, target);
        edges.push({ id: uid(), fromNode: from.id, fromSide: linking.side, toNode: target.id, toSide });
        save();
      }
      linking = null;
      return;
    }
    if (gesture && gesture.kind !== "pan" && (gesture.kind === "resize" || gesture.moved)) save();
    gesture = null;
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      const r = wrap!.getBoundingClientRect();
      const mx = e.clientX - r.left;
      const my = e.clientY - r.top;
      const next = Math.min(3, Math.max(0.08, scale * Math.exp(-e.deltaY * 0.002)));
      ox = mx - ((mx - ox) * next) / scale;
      oy = my - ((my - oy) * next) / scale;
      scale = next;
    } else {
      ox -= e.deltaX;
      oy -= e.deltaY;
    }
  }

  function addText(at?: { x: number; y: number }) {
    const c = at ?? toWorld(wrap!.getBoundingClientRect().left + wrap!.clientWidth / 2, wrap!.getBoundingClientRect().top + wrap!.clientHeight / 2);
    const n: CNode = { id: uid(), type: "text", text: "", x: Math.round(c.x - 130), y: Math.round(c.y - 60), width: 260, height: 120 };
    nodes.push(n);
    selected = new Set([n.id]);
    editing = n.id;
    save();
  }

  function addGroup() {
    const c = toWorld(wrap!.getBoundingClientRect().left + wrap!.clientWidth / 2, wrap!.getBoundingClientRect().top + wrap!.clientHeight / 2);
    nodes.unshift({ id: uid(), type: "group", label: "Grupo", x: Math.round(c.x - 250), y: Math.round(c.y - 180), width: 500, height: 360 });
    save();
  }

  let fileQuery = $state("");
  let filePicker = $state(false);

  function addFile(rel: string) {
    const c = toWorld(wrap!.getBoundingClientRect().left + wrap!.clientWidth / 2, wrap!.getBoundingClientRect().top + wrap!.clientHeight / 2);
    const n: CNode = { id: uid(), type: "file", file: rel, x: Math.round(c.x - 200), y: Math.round(c.y - 150), width: 400, height: IMG.test(rel) ? 300 : 400 };
    nodes.push(n);
    filePicker = false;
    save();
    loadPreviews();
  }

  function remove() {
    if (selectedEdge) {
      edges = edges.filter((e) => e.id !== selectedEdge);
      selectedEdge = null;
    } else if (selected.size) {
      nodes = nodes.filter((n) => !selected.has(n.id));
      edges = edges.filter((e) => !selected.has(e.fromNode) && !selected.has(e.toNode));
      selected = new Set();
    } else return;
    save();
  }

  function setColor(c: string | undefined) {
    for (const n of nodes) if (selected.has(n.id)) n.color = c;
    for (const e of edges) if (e.id === selectedEdge) e.color = c;
    save();
  }

  function onKey(e: KeyboardEvent) {
    if (editing || (e.target as HTMLElement).matches("input, textarea")) return;
    if (e.key === "Delete" || e.key === "Backspace") {
      e.preventDefault();
      remove();
    }
  }

  let files = $state<string[]>([]);
  async function openPicker() {
    filePicker = !filePicker;
    if (!files.length) {
      const tree = await invoke<{ children: any[] }>("read_tree");
      const out: string[] = [];
      const walk = (ns: any[]) => ns.forEach((n) => (n.kind === "folder" ? walk(n.children) : out.push(n.path.slice(vaultRoot.length + 1).replace(/\\/g, "/"))));
      walk(tree.children);
      files = out.filter((f) => /\.(md|png|jpe?g|gif|webp|svg|pdf|canvas)$/i.test(f));
    }
  }

  onMount(() => {
    load();
    wrap?.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      wrap?.removeEventListener("wheel", onWheel);
      clearTimeout(saveTimer);
      flush();
    };
  });
</script>

<svelte:window onkeydown={onKey} />

<div class="cv">
  <div class="cv-toolbar">
    <button class="btn-ghost sm" onclick={() => addText()}><Icon name="plus" size={13} stroke={2} /> Cartão</button>
    <div class="cv-picker">
      <button class="btn-ghost sm" onclick={openPicker}><Icon name="file" size={13} /> Nota ou mídia</button>
      {#if filePicker}
        <div class="cv-picker-pop">
          <!-- svelte-ignore a11y_autofocus -->
          <input class="sx-input" placeholder="Buscar arquivo…" bind:value={fileQuery} autofocus />
          <div class="cv-picker-list">
            {#each files.filter((f) => f.toLowerCase().includes(fileQuery.toLowerCase())).slice(0, 60) as f}
              <button onclick={() => addFile(f)}>{f}</button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
    <button class="btn-ghost sm" onclick={addGroup}><Icon name="sidebar" size={13} /> Grupo</button>
    {#if selected.size || selectedEdge}
      <div class="cv-colors">
        <button class="cv-color" title="Sem cor" aria-label="Sem cor" onclick={() => setColor(undefined)}></button>
        {#each Object.entries(COLORS) as [k, c]}<button class="cv-color" style:background={c} aria-label="Cor {k}" onclick={() => setColor(k)}></button>{/each}
      </div>
      <button class="btn-ghost sm is-danger" onclick={remove}><Icon name="trash" size={13} /> Excluir</button>
    {/if}
    <span class="cv-spacer"></span>
    <button class="btn-ghost sm" onclick={fit}>Caber na tela</button>
    <span class="cv-zoom">{Math.round(scale * 100)}%</span>
  </div>

  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="cv-stage"
    bind:this={wrap}
    onpointerdown={onBgDown}
    onpointermove={onMove}
    onpointerup={onUp}
    ondblclick={(e) => e.target === e.currentTarget && addText(toWorld(e.clientX, e.clientY))}
  >
    {#if loaded}
      <div class="cv-world" style:transform="translate({ox}px, {oy}px) scale({scale})">
        <svg class="cv-edges" overflow="visible">
          <defs>
            <marker id="cv-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
              <path d="M0,0 L10,5 L0,10 z" fill="context-stroke" />
            </marker>
          </defs>
          {#each edges as e (e.id)}
            {@const p = edgePath(e)}
            {#if p}
              <g class="cv-edge" class:is-selected={selectedEdge === e.id} style:--edge={colorOf(e.color) ?? "var(--sx-text-faint)"}>
                <path class="cv-edge-hit" d={p.d} onpointerdown={(ev) => (ev.stopPropagation(), (selectedEdge = e.id), (selected = new Set()))} />
                <path
                  class="cv-edge-line"
                  d={p.d}
                  marker-end={(e.toEnd ?? "arrow") === "arrow" ? "url(#cv-arrow)" : undefined}
                  marker-start={e.fromEnd === "arrow" ? "url(#cv-arrow)" : undefined}
                />
                {#if e.label}<text class="cv-edge-label" x={p.mid.x} y={p.mid.y}>{e.label}</text>{/if}
              </g>
            {/if}
          {/each}
          {#if linking}
            {@const from = nodes.find((n) => n.id === linking!.from)}
            {#if from}
              {@const a = anchor(from, linking.side)}
              <path class="cv-edge-line is-draft" d="M{a.x},{a.y} L{linking.x},{linking.y}" marker-end="url(#cv-arrow)" />
            {/if}
          {/if}
        </svg>

        {#each nodes as n (n.id)}
          {@const color = colorOf(n.color)}
          {@const shape = n.styleAttributes?.shape}
          <div
            class="cv-node cv-{n.type}"
            class:is-selected={selected.has(n.id)}
            class:is-diamond={shape === "diamond"}
            class:is-ellipse={shape === "ellipse" || shape === "circle"}
            class:is-pill={shape === "pill"}
            style:left="{n.x}px"
            style:top="{n.y}px"
            style:width="{n.width}px"
            style:height="{n.height}px"
            style:--node={color ?? "var(--sx-border-strong)"}
            style:text-align={n.styleAttributes?.textAlign ?? undefined}
            role="button"
            tabindex="-1"
            onpointerdown={(e) => onNodeDown(e, n)}
            ondblclick={(e) => {
              e.stopPropagation();
              if (n.type === "text") editing = n.id;
              else if (n.type === "file" && n.file) onopen(absOf(n.file), true);
              else if (n.type === "group") {
                const l = prompt("Nome do grupo", n.label ?? "");
                if (l !== null) ((n.label = l), save());
              }
            }}
          >
            {#if n.type === "group"}
              <div class="cv-group-label">{n.label ?? ""}</div>
            {:else if n.type === "text"}
              {#if editing === n.id}
                <!-- svelte-ignore a11y_autofocus -->
                <textarea
                  class="cv-edit"
                  autofocus
                  value={n.text ?? ""}
                  onpointerdown={(e) => e.stopPropagation()}
                  oninput={(e) => ((n.text = e.currentTarget.value), save())}
                  onblur={() => (editing = null)}
                  onkeydown={(e) => e.key === "Escape" && (editing = null)}
                ></textarea>
              {:else}
                <!-- svelte-ignore a11y_click_events_have_key_events -->
                <div
                  class="cv-md"
                  onclick={(e) => {
                    const a = (e.target as HTMLElement).closest<HTMLElement>("a.sx-wikilink");
                    if (a?.dataset.target) onopenlink(a.dataset.target);
                  }}
                >
                  {@html renderBlock(n.text ?? "")}
                </div>
              {/if}
            {:else if n.type === "file" && n.file}
              <div class="cv-file-head"><Icon name={IMG.test(n.file) ? "image" : "file"} size={12} /> {n.file.split("/").pop()}</div>
              {#if IMG.test(n.file)}
                <img class="cv-img" src={convertFileSrc(absOf(n.file))} alt={n.file} draggable="false" />
              {:else if previews[n.id]}
                <div class="cv-md">{@html previews[n.id]}</div>
              {/if}
            {:else if n.type === "link"}
              <div class="cv-file-head"><Icon name="link" size={12} /> {n.url}</div>
              <button class="sx-link-btn" onpointerdown={(e) => e.stopPropagation()} onclick={() => n.url && window.open(n.url)}>Abrir link</button>
            {/if}
            {#if selected.has(n.id) && editing !== n.id}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <div class="cv-resize" onpointerdown={(e) => onResizeDown(e, n)}></div>
            {/if}
            {#if n.type !== "group"}
              {#each ["top", "right", "bottom", "left"] as const as side}
                <!-- svelte-ignore a11y_no_static_element_interactions -->
                <div class="cv-handle is-{side}" onpointerdown={(e) => onLinkDown(e, n, side)}></div>
              {/each}
            {/if}
          </div>
        {/each}
      </div>
    {/if}
    {#if loaded && !nodes.length}<div class="cv-empty">Canvas vazio. Dê um duplo clique para criar um cartão.</div>{/if}
  </div>
</div>
