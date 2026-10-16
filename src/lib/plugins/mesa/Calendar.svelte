<script lang="ts">
  // Calendário (Gantt) da Mesa: arraste barras para mover, bordas para ajustar,
  // tarefas "Sem data" e atrasadas para reagendar.
  import { addDays, diffDays, fmtDate, parseYmd, STATUS, statusOf, taskRange, today, type MesaCore, type Rec } from "./core";
  import Icon from "../../components/Icon.svelte";

  type Props = { core: MesaCore; tasks: Rec[]; projects: Rec[]; onedit: (n: Rec) => void; onchanged: () => void };
  let { core, tasks, projects, onedit, onchanged }: Props = $props();

  const ZOOM = { semana: { days: 14, w: 64, label: "2 semanas" }, mes: { days: 42, w: 32, label: "6 semanas" }, trimestre: { days: 91, w: 14, label: "Trimestre" } };
  const WD = ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"];
  const L = 260;

  let zoom = $state<keyof typeof ZOOM>("mes");
  let projectId = $state("");
  let showDone = $state(false);
  let z = $derived(ZOOM[zoom]);
  const t0 = today();
  const home = (n: number) => addDays(t0, -Math.min(7, Math.floor(n / 4)));
  let start = $state(home(42));
  let end = $derived(addDays(start, z.days - 1));
  let width = $derived(z.days * z.w);
  let dropping = $state(false);

  let days = $derived(Array.from({ length: z.days }, (_, k) => addDays(start, k)));
  let months = $derived.by(() => {
    const out: { label: string; left: number; width: number }[] = [];
    for (let i = 0; i < z.days; ) {
      const d = parseYmd(addDays(start, i));
      const span = Math.min(new Date(d.getFullYear(), d.getMonth() + 1, 0).getDate() - d.getDate() + 1, z.days - i);
      out.push({ label: span * z.w >= 60 ? d.toLocaleDateString("pt-BR", { month: "long", year: "numeric" }) : "", left: i * z.w, width: span * z.w });
      i += span;
    }
    return out;
  });

  let layout = $derived.by(() => {
    const visible = tasks.filter((n) => (showDone || n.data.state === "pending") && (!projectId || n.data.project === projectId));
    const groups: [string | null, string][] = [...projects.map((p) => [p.data.id, p.data.title] as [string, string]), [null, "Projeto a identificar"]];
    const out: { title: string; rows: { n: Rec; r: NonNullable<ReturnType<typeof taskRange>>; late?: boolean }[] }[] = [];
    const unscheduled: Rec[] = [];
    let outside = 0;
    for (const [pid, ptitle] of groups) {
      const list = visible.filter((n) => (n.data.project || null) === pid || (pid === null && n.data.project && !projects.some((p) => p.data.id === n.data.project)));
      const rows: (typeof out)[number]["rows"] = [];
      for (const n of list) {
        const r = taskRange(n);
        if (!r) unscheduled.push(n);
        else if (r.end < start && statusOf(n, t0)[0] === "overdue") rows.push({ n, r, late: true });
        else if (r.end < start || r.start > end) outside++;
        else rows.push({ n, r });
      }
      if (rows.length) {
        rows.sort((a, b) => (b.late ? 1 : 0) - (a.late ? 1 : 0) || a.r.start.localeCompare(b.r.start));
        out.push({ title: `${ptitle} (${rows.length})`, rows });
      }
    }
    return { groups: out, unscheduled, outside };
  });

  function shift(k: number) {
    start = addDays(start, k);
  }

  function setZoom(k: keyof typeof ZOOM) {
    zoom = k;
    start = home(ZOOM[k].days);
  }

  async function schedule(n: Rec, s: string, e: string) {
    await core.run(() => core.update(n, { start: s, deadline: "date", due: e }));
    onchanged();
  }

  function drag(node: HTMLElement, params: { n: Rec; r: { start: string; end: string } }) {
    let { n, r } = params;
    const down = (e: PointerEvent) => {
      if (e.button !== 0) return;
      const mode = (e.target as HTMLElement).closest(".is-start") ? "start" : (e.target as HTMLElement).closest(".is-end") ? "end" : "move";
      e.preventDefault();
      node.setPointerCapture(e.pointerId);
      node.classList.add("is-dragging");
      const x0 = e.clientX;
      const left0 = parseFloat(node.style.left);
      const width0 = parseFloat(node.style.width);
      const W = z.w;
      let delta = 0;
      let moved = false;
      const move = (ev: PointerEvent) => {
        if (Math.abs(ev.clientX - x0) > 3) moved = true;
        delta = Math.round((ev.clientX - x0) / W);
        if (mode === "move") node.style.left = left0 + delta * W + "px";
        else if (mode === "start") {
          const d = Math.min(delta, Math.round((width0 + 4) / W) - 1);
          node.style.left = left0 + d * W + "px";
          node.style.width = width0 - d * W + "px";
        } else node.style.width = Math.max(W - 4, width0 + delta * W) + "px";
      };
      const up = () => {
        node.removeEventListener("pointermove", move);
        node.removeEventListener("pointerup", up);
        node.classList.remove("is-dragging");
        if (!moved) {
          if (mode === "move") onedit(n);
          return;
        }
        if (!delta) {
          node.style.left = left0 + "px";
          node.style.width = width0 + "px";
          return;
        }
        let s = r.start;
        let en = r.end;
        if (mode !== "end") s = addDays(s, delta);
        if (mode !== "start") en = addDays(en, delta);
        if (s > en) mode === "start" ? (s = en) : (en = s);
        schedule(n, s, en);
      };
      node.addEventListener("pointermove", move);
      node.addEventListener("pointerup", up);
    };
    node.addEventListener("pointerdown", down);
    return {
      update(p: typeof params) {
        ({ n, r } = p);
      },
      destroy: () => node.removeEventListener("pointerdown", down),
    };
  }

  function onDrop(e: DragEvent) {
    dropping = false;
    const id = e.dataTransfer?.getData("text/mesa-task");
    const n = tasks.find((t) => t.data.id === id);
    const body = e.currentTarget as HTMLElement;
    const x = e.clientX - body.getBoundingClientRect().left + body.scrollLeft - L;
    if (!n || x < 0) return;
    e.preventDefault();
    const date = addDays(start, Math.min(z.days - 1, Math.floor(x / z.w)));
    schedule(n, date, date);
  }
</script>

<div class="mesa-filters mesa-cal-toolbar">
  <button class="btn-ghost sm" aria-label="Período anterior" onclick={() => shift(-Math.ceil(z.days / 2))}>‹</button>
  <button class="btn-ghost sm" onclick={() => (start = home(z.days))}>Hoje</button>
  <button class="btn-ghost sm" aria-label="Próximo período" onclick={() => shift(Math.ceil(z.days / 2))}>›</button>
  <span class="mesa-cal-range">{fmtDate(start)} – {fmtDate(end)}</span>
  <div class="sx-segmented sm">
    {#each Object.entries(ZOOM) as [k, v]}
      <button class:is-on={k === zoom} onclick={() => setZoom(k as keyof typeof ZOOM)}>{v.label}</button>
    {/each}
  </div>
  <select bind:value={projectId}>
    <option value="">Todos os projetos</option>
    {#each projects as p}<option value={p.data.id}>{p.data.title}</option>{/each}
  </select>
  <label class="sx-check"><input type="checkbox" bind:checked={showDone} /> Mostrar encerradas</label>
</div>
<div class="mesa-legend">
  {#each Object.entries(STATUS) as [k, v]}<span class="mesa-status mesa-s-{k}">{v}</span>{/each}
</div>

<div
  class="mesa-gantt"
  class:is-drop={dropping}
  role="grid"
  tabindex="-1"
  ondragover={(e) => {
    if (e.dataTransfer?.types.includes("text/mesa-task")) {
      e.preventDefault();
      dropping = true;
    }
  }}
  ondragleave={() => (dropping = false)}
  ondrop={onDrop}
>
  <div class="mesa-gantt-head">
    <div class="mesa-gantt-row">
      <div class="mesa-gantt-label"></div>
      <div class="mesa-gantt-track mesa-gantt-months" style:width="{width}px">
        {#each months as m}<div class="mesa-gantt-month" style:left="{m.left}px" style:width="{m.width}px">{m.label}</div>{/each}
      </div>
    </div>
    <div class="mesa-gantt-row">
      <div class="mesa-gantt-label">Tarefa</div>
      <div class="mesa-gantt-track mesa-gantt-days" style:width="{width}px">
        {#each days as s, k}
          {@const d = parseYmd(s)}
          <div
            class="mesa-gantt-day"
            class:is-today={s === t0}
            class:is-weekend={d.getDay() % 6 === 0}
            style:left="{k * z.w}px"
            style:width="{z.w}px"
            title={d.toLocaleDateString("pt-BR", { weekday: "long", day: "numeric", month: "long" })}
          >
            {#if z.w >= 24 || d.getDay() === 1 || d.getDate() === 1}{z.w >= 56 ? `${WD[d.getDay()]} ${d.getDate()}` : d.getDate()}{/if}
          </div>
        {/each}
      </div>
    </div>
  </div>
  <div class="mesa-gantt-body">
    <div class="mesa-gantt-cols" style:left="{L}px" style:width="{width}px">
      {#each days as s, k}
        {#if s === t0}<div class="mesa-gantt-today" style:left="{k * z.w + z.w / 2 - 1}px"></div>
        {:else if parseYmd(s).getDay() % 6 === 0}<div class="mesa-gantt-weekend" style:left="{k * z.w}px" style:width="{z.w}px"></div>{/if}
      {/each}
    </div>
    {#each layout.groups as g}
      <div class="mesa-gantt-row mesa-gantt-group"><div class="mesa-gantt-label">{g.title}</div><div class="mesa-gantt-track" style:width="{width}px"></div></div>
      {#each g.rows as { n, r, late } (n.data.id)}
        {@const [key, st] = statusOf(n)}
        <div class="mesa-gantt-row">
          <div class="mesa-gantt-label">
            <span class="mesa-dot mesa-s-{key}"></span>
            <button class="mesa-gantt-title" title={n.data.title} onclick={() => onedit(n)}>{n.data.title}</button>
          </div>
          <div class="mesa-gantt-track" style:width="{width}px">
            {#if late}
              {@const ago = diffDays(r.end, t0)}
              <div
                class="mesa-gantt-late mesa-s-overdue"
                role="button"
                tabindex="0"
                draggable="true"
                title="{n.data.title}&#10;Prazo {fmtDate(r.end)} (fora da janela) · arraste para um dia para reagendar"
                onclick={() => onedit(n)}
                onkeydown={(e) => e.key === "Enter" && onedit(n)}
                ondragstart={(e) => e.dataTransfer?.setData("text/mesa-task", n.data.id)}
              >
                <Icon name="back" size={12} stroke={2} /> venceu {fmtDate(r.end)} · há {ago} dia{ago === 1 ? "" : "s"}
              </div>
            {:else}
              {@const si = diffDays(start, r.start)}
              {@const ei = diffDays(start, r.end)}
              {@const a = Math.max(0, si)}
              {@const b = Math.min(z.days - 1, ei)}
              <div
                class="mesa-gantt-bar mesa-s-{key}"
                class:is-month={r.month}
                class:cut-start={si < 0}
                class:cut-end={ei > z.days - 1}
                style:left="{a * z.w + 2}px"
                style:width="{(b - a + 1) * z.w - 4}px"
                title="{n.data.title}&#10;{fmtDate(r.start)} → {fmtDate(r.end)} · {st}{r.month ? ' (mês)' : ''}&#10;Arraste para mover · bordas para ajustar · clique para detalhes"
                use:drag={{ n, r }}
              >
                {#if (b - a + 1) * z.w >= 48}<span class="mesa-gantt-bar-text">{n.data.title}</span>{/if}
                {#if si >= 0}<div class="mesa-gantt-handle is-start"></div>{/if}
                {#if ei <= z.days - 1}<div class="mesa-gantt-handle is-end"></div>{/if}
              </div>
            {/if}
          </div>
        </div>
      {/each}
    {:else}
      <div class="mesa-gantt-empty">
        Nenhuma tarefa com data neste período.{layout.unscheduled.length ? " Arraste uma tarefa de “Sem data” para cá." : ""}
      </div>
    {/each}
  </div>
</div>
{#if layout.outside}<p class="mesa-muted">{layout.outside} tarefa(s) com data fora deste período.</p>{/if}
{#if layout.unscheduled.length}
  <h3>Sem data ({layout.unscheduled.length})</h3>
  <p class="mesa-muted">Arraste para um dia do calendário ou clique para definir datas.</p>
  <div class="mesa-chips">
    {#each layout.unscheduled as n (n.data.id)}
      <div
        class="mesa-chip"
        role="button"
        tabindex="0"
        draggable="true"
        title={n.data.title}
        onclick={() => onedit(n)}
        onkeydown={(e) => e.key === "Enter" && onedit(n)}
        ondragstart={(e) => e.dataTransfer?.setData("text/mesa-task", n.data.id)}
      >
        <span class="mesa-dot mesa-s-{statusOf(n)[0]}"></span>{n.data.title}
      </div>
    {/each}
  </div>
{/if}
