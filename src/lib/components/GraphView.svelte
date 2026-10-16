<script lang="ts" module>
  export type GraphNote = { title: string; relative_path: string; absolute_path: string; links: string[]; tags: string[] };
</script>

<script lang="ts">
  /*
   * Grafo de notas com layout por forças (repulsão + molas + gravidade),
   * desenhado em canvas. Arraste para mover, roda do mouse para zoom,
   * clique abre a nota (Ctrl+clique em nova aba).
   */
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";

  type Props = {
    notes: GraphNote[];
    activePath: string | null;
    onopen: (path: string, newTab: boolean) => void;
  };
  let { notes, activePath, onopen }: Props = $props();

  let canvas: HTMLCanvasElement;
  let wrap: HTMLDivElement;
  let filter = $state("");
  let showOrphans = $state(true);
  let stats = $state({ nodes: 0, edges: 0 });

  type G = {
    ids: string[];
    titles: string[];
    x: Float32Array;
    y: Float32Array;
    vx: Float32Array;
    vy: Float32Array;
    deg: Uint16Array;
    edges: Uint32Array; // pares [a, b]
    neighbors: Set<number>[];
  };
  let g: G | null = null;
  // Para testes/diagnóstico
  $effect(() => {
    (window as unknown as { __graph?: () => G | null }).__graph = () => g;
  });

  function buildGraph(): G {
    const byKey = new Map<string, number>();
    const list = notes;
    list.forEach((n, i) => {
      byKey.set(n.title.toLowerCase(), i);
      byKey.set(n.relative_path.replace(/\\/g, "/").replace(/\.md$/i, "").toLowerCase(), i);
    });
    const pairs = new Set<string>();
    const edgeList: number[] = [];
    list.forEach((n, i) => {
      for (const l of n.links) {
        const key = l.replace(/\\/g, "/").replace(/\.md$/i, "").toLowerCase();
        const j = byKey.get(key) ?? byKey.get(key.split("/").pop()!);
        if (j === undefined || j === i) continue;
        const id = i < j ? `${i}:${j}` : `${j}:${i}`;
        if (pairs.has(id)) continue;
        pairs.add(id);
        edgeList.push(i, j);
      }
    });
    const deg = new Uint16Array(list.length);
    const neighbors = list.map(() => new Set<number>());
    for (let e = 0; e < edgeList.length; e += 2) {
      const [a, b] = [edgeList[e], edgeList[e + 1]];
      deg[a]++;
      deg[b]++;
      neighbors[a].add(b);
      neighbors[b].add(a);
    }
    // Posições iniciais em espiral (determinístico, converge rápido)
    const n = list.length;
    const x = new Float32Array(n);
    const y = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const r = 12 * Math.sqrt(i + 1);
      const a = i * 2.399963;
      x[i] = Math.cos(a) * r;
      y[i] = Math.sin(a) * r;
    }
    stats = { nodes: n, edges: edgeList.length / 2 };
    return {
      ids: list.map((l) => l.absolute_path),
      titles: list.map((l) => l.title),
      x,
      y,
      vx: new Float32Array(n),
      vy: new Float32Array(n),
      deg,
      edges: Uint32Array.from(edgeList),
      neighbors,
    };
  }

  // Câmera
  let scale = 1;
  let ox = 0;
  let oy = 0;
  let alpha = 1;
  let hover = -1;
  let dragNode = -1;
  let raf = 0;

  const visible = (i: number) => showOrphans || g!.deg[i] > 0;

  const MAX_SPEED = 25;

  /**
   * Um passo da simulação. Molas com força 1/min(grau) e viés pelo grau (como no
   * d3-force): notas com muitos links não acumulam força e o sistema não oscila.
   * Antes, hubs somavam a força de todas as arestas, a simulação divergia para
   * infinito/NaN e os nós sumiam da tela.
   */
  function tick() {
    if (!g) return;
    const { x, y, vx, vy, edges, deg } = g;
    const n = x.length;
    const k = alpha;
    // Repulsão (O(n²) com corte de distância)
    for (let i = 0; i < n; i++) {
      if (!visible(i)) continue;
      for (let j = i + 1; j < n; j++) {
        if (!visible(j)) continue;
        let dx = x[i] - x[j];
        let dy = y[i] - y[j];
        let d2 = dx * dx + dy * dy;
        if (d2 > 160000) continue;
        if (d2 < 1) {
          dx = (Math.random() - 0.5) * 2;
          dy = (Math.random() - 0.5) * 2;
          d2 = dx * dx + dy * dy || 1;
        }
        const f = (-30 * k) / d2;
        vx[i] -= dx * f;
        vy[i] -= dy * f;
        vx[j] += dx * f;
        vy[j] += dy * f;
      }
    }
    // Molas
    for (let e = 0; e < edges.length; e += 2) {
      const a = edges[e];
      const b = edges[e + 1];
      const dx = x[b] + vx[b] - x[a] - vx[a];
      const dy = y[b] + vy[b] - y[a] - vy[a];
      const d = Math.sqrt(dx * dx + dy * dy) || 1;
      const strength = 1 / Math.max(1, Math.min(deg[a], deg[b]));
      const l = ((d - 40) / d) * k * strength;
      const bias = deg[a] / (deg[a] + deg[b]);
      vx[b] -= dx * l * bias;
      vy[b] -= dy * l * bias;
      vx[a] += dx * l * (1 - bias);
      vy[a] += dy * l * (1 - bias);
    }
    // Gravidade ao centro, atrito, limite de velocidade e proteção contra NaN
    for (let i = 0; i < n; i++) {
      if (i === dragNode) {
        vx[i] = vy[i] = 0;
        continue;
      }
      vx[i] -= x[i] * 0.01 * k;
      vy[i] -= y[i] * 0.01 * k;
      vx[i] *= 0.6;
      vy[i] *= 0.6;
      const sp = Math.hypot(vx[i], vy[i]);
      if (sp > MAX_SPEED) {
        vx[i] *= MAX_SPEED / sp;
        vy[i] *= MAX_SPEED / sp;
      }
      x[i] += vx[i];
      y[i] += vy[i];
      if (!Number.isFinite(x[i]) || !Number.isFinite(y[i])) {
        x[i] = (Math.random() - 0.5) * 100;
        y[i] = (Math.random() - 0.5) * 100;
        vx[i] = vy[i] = 0;
      }
    }
    alpha = Math.max(0, alpha * 0.985);
  }

  function radius(i: number) {
    return 3 + Math.sqrt(g!.deg[i]) * 1.6;
  }

  function css(name: string) {
    return getComputedStyle(wrap).getPropertyValue(name).trim() || "#888";
  }

  function draw() {
    if (!g) return;
    const ctx = canvas.getContext("2d")!;
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.width / dpr;
    const h = canvas.height / dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.translate(w / 2 + ox, h / 2 + oy);
    ctx.scale(scale, scale);

    const q = filter.trim().toLowerCase();
    const focus = hover >= 0 ? hover : -1;
    const lit = (i: number) =>
      (focus < 0 || i === focus || g!.neighbors[focus].has(i)) && (!q || g!.titles[i].toLowerCase().includes(q));

    const colLine = css("--sx-border-strong");
    const colAccent = css("--sx-accent");
    const colNode = css("--sx-text-muted");
    const colFaint = css("--sx-text-faint");
    const colText = css("--sx-text");

    ctx.lineWidth = 1 / scale;
    for (let e = 0; e < g.edges.length; e += 2) {
      const a = g.edges[e];
      const b = g.edges[e + 1];
      if (!visible(a) || !visible(b)) continue;
      const on = focus >= 0 && (a === focus || b === focus);
      ctx.strokeStyle = on ? colAccent : colLine;
      ctx.globalAlpha = on ? 0.9 : lit(a) && lit(b) ? 0.55 : 0.12;
      ctx.beginPath();
      ctx.moveTo(g.x[a], g.y[a]);
      ctx.lineTo(g.x[b], g.y[b]);
      ctx.stroke();
    }

    for (let i = 0; i < g.x.length; i++) {
      if (!visible(i)) continue;
      const on = lit(i);
      ctx.globalAlpha = on ? 1 : 0.2;
      ctx.fillStyle = g.ids[i] === activePath || i === focus ? colAccent : g.deg[i] ? colNode : colFaint;
      ctx.beginPath();
      ctx.arc(g.x[i], g.y[i], radius(i), 0, Math.PI * 2);
      ctx.fill();
    }

    // Rótulos: com zoom, no foco/vizinhos e nas notas mais conectadas
    ctx.font = `${12 / scale}px ${css("--sx-font-ui")}`;
    ctx.textAlign = "center";
    for (let i = 0; i < g.x.length; i++) {
      if (!visible(i)) continue;
      const near = focus >= 0 && (i === focus || g.neighbors[focus].has(i));
      if (!(near || scale > 1.4 || (scale > 0.7 && g.deg[i] > 6) || g.ids[i] === activePath)) continue;
      if (!lit(i) && !near) continue;
      ctx.globalAlpha = near || g.ids[i] === activePath ? 1 : 0.75;
      ctx.fillStyle = colText;
      ctx.fillText(g.titles[i], g.x[i], g.y[i] + radius(i) + 12 / scale);
    }
    ctx.globalAlpha = 1;
  }

  /** Enquadra todos os nós visíveis na tela. */
  function fit() {
    if (!g || !wrap) return;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (let i = 0; i < g.x.length; i++) {
      if (!visible(i)) continue;
      minX = Math.min(minX, g.x[i]);
      maxX = Math.max(maxX, g.x[i]);
      minY = Math.min(minY, g.y[i]);
      maxY = Math.max(maxY, g.y[i]);
    }
    if (!Number.isFinite(minX)) return;
    const w = wrap.clientWidth - 80;
    const h = wrap.clientHeight - 80;
    scale = Math.min(3, Math.max(0.1, Math.min(w / (maxX - minX || 1), h / (maxY - minY || 1))));
    ox = (-(minX + maxX) / 2) * scale;
    oy = (-(minY + maxY) / 2) * scale;
  }

  let fitted = false;
  let userMoved = false;

  function loop() {
    if (alpha > 0.01) tick();
    // Enquadra uma vez quando o layout assenta (se o usuário ainda não mexeu)
    if (!fitted && alpha < 0.3 && !userMoved) {
      fit();
      fitted = true;
    }
    draw();
    raf = requestAnimationFrame(loop);
  }

  function toWorld(e: MouseEvent) {
    const r = canvas.getBoundingClientRect();
    return { x: (e.clientX - r.left - r.width / 2 - ox) / scale, y: (e.clientY - r.top - r.height / 2 - oy) / scale };
  }

  function pick(e: MouseEvent) {
    if (!g) return -1;
    const p = toWorld(e);
    let best = -1;
    let bestD = Infinity;
    for (let i = 0; i < g.x.length; i++) {
      if (!visible(i)) continue;
      const d = (g.x[i] - p.x) ** 2 + (g.y[i] - p.y) ** 2;
      const r = radius(i) + 4 / scale;
      if (d < r * r && d < bestD) {
        best = i;
        bestD = d;
      }
    }
    return best;
  }

  let panning: { x: number; y: number; ox: number; oy: number; moved: boolean } | null = null;

  function onpointerdown(e: PointerEvent) {
    canvas.setPointerCapture(e.pointerId);
    const i = pick(e);
    if (i >= 0) {
      dragNode = i;
      alpha = Math.max(alpha, 0.3);
    }
    panning = { x: e.clientX, y: e.clientY, ox, oy, moved: false };
  }

  function onpointermove(e: PointerEvent) {
    if (panning && Math.hypot(e.clientX - panning.x, e.clientY - panning.y) > 3) panning.moved = true;
    if (dragNode >= 0 && g) {
      const p = toWorld(e);
      g.x[dragNode] = p.x;
      g.y[dragNode] = p.y;
      alpha = Math.max(alpha, 0.3);
    } else if (panning) {
      userMoved = true;
      ox = panning.ox + e.clientX - panning.x;
      oy = panning.oy + e.clientY - panning.y;
    } else {
      hover = pick(e);
      canvas.style.cursor = hover >= 0 ? "pointer" : "grab";
    }
  }

  function onpointerup(e: PointerEvent) {
    if (panning && !panning.moved && dragNode >= 0 && g) onopen(g.ids[dragNode], e.ctrlKey || e.metaKey);
    dragNode = -1;
    panning = null;
  }

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    userMoved = true;
    const r = canvas.getBoundingClientRect();
    const mx = e.clientX - r.left - r.width / 2;
    const my = e.clientY - r.top - r.height / 2;
    const factor = Math.exp(-e.deltaY * 0.0015);
    const next = Math.min(6, Math.max(0.1, scale * factor));
    ox = mx - ((mx - ox) * next) / scale;
    oy = my - ((my - oy) * next) / scale;
    scale = next;
  }

  function resize() {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = wrap.clientWidth * dpr;
    canvas.height = wrap.clientHeight * dpr;
    canvas.style.width = `${wrap.clientWidth}px`;
    canvas.style.height = `${wrap.clientHeight}px`;
  }

  $effect(() => {
    notes;
    g = buildGraph();
    alpha = 1;
    fitted = false;
  });

  $effect(() => {
    showOrphans;
    alpha = Math.max(alpha, 0.5);
  });

  onMount(() => {
    // wheel não-passivo para o preventDefault funcionar (zoom sem rolar a página)
    canvas.addEventListener("wheel", onwheel, { passive: false });
    const ro = new ResizeObserver(resize);
    ro.observe(wrap);
    resize();
    // Escala inicial para caber a espiral
    scale = Math.min(1, 380 / (12 * Math.sqrt(notes.length + 1)));
    raf = requestAnimationFrame(loop);
    return () => {
      cancelAnimationFrame(raf);
      canvas.removeEventListener("wheel", onwheel);
      ro.disconnect();
    };
  });
</script>

<div class="sx-graph" bind:this={wrap}>
  <canvas
    bind:this={canvas}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointerleave={() => (hover = -1)}
  ></canvas>
  <div class="sx-graph-panel">
    <div class="sx-graph-search">
      <Icon name="search" size={14} />
      <input bind:value={filter} placeholder="Filtrar notas…" spellcheck="false" />
    </div>
    <label class="sx-graph-toggle"><input type="checkbox" bind:checked={showOrphans} /> Notas sem links</label>
    <div class="sx-graph-stats">
      {stats.nodes} notas · {stats.edges} links
      <button class="sx-link-btn" onclick={fit}>Centralizar</button>
    </div>
  </div>
</div>
