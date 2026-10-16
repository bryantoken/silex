<script lang="ts">
  // Painel de consumo de IA da Mesa: Claude/OpenAI (tokens) e Jev segregados,
  // por mês, dia da semana × hora, projeto e função.
  import { invoke } from "@tauri-apps/api/core";
  import { addDays, modelPrice, parseYmd, PURPOSE, today, ymd, type MesaCore, type Rec } from "./core";

  type Row = { at: string; engine: string; model?: string; purpose?: string; task?: string | null; proj?: Record<string, number>; in?: number; out?: number; cw?: number; cr?: number; q?: number; cost?: number | null };
  type Props = { core: MesaCore; all: Rec[] };
  let { core, all }: Props = $props();

  let rows = $state<Row[]>([]);
  let period = $state("30d");
  let engineSel = $state("all");
  let metric = $state<"cost" | "calls" | "tokens">("cost");

  $effect(() => {
    invoke<Row[]>("mesa_usage").then((r) => (rows = r));
  });

  const ENG: Record<string, string> = { claude: "Claude", openai: "OpenAI", jev: "Jev" };
  const fx = $derived(Number(core.settings.fx) || 0);
  const money = (usd: number) => {
    const v = fx > 0 ? usd * fx : usd;
    const digits = v === 0 ? 0 : v < 0.01 ? 4 : v < 0.1 ? 3 : 2;
    return new Intl.NumberFormat("pt-BR", { style: "currency", currency: fx > 0 ? "BRL" : "USD", minimumFractionDigits: digits, maximumFractionDigits: digits }).format(v);
  };
  const compact = (n: number) => new Intl.NumberFormat("pt-BR", { notation: "compact", maximumFractionDigits: 1 }).format(n);
  const eng = (r: Row) => (ENG[r.engine] ? r.engine : "claude");
  const tokens = (r: Row) => (Number(r.in) || 0) + (Number(r.out) || 0) + (Number(r.cw) || 0) + (Number(r.cr) || 0);
  const val = (r: Row) => (metric === "cost" ? core.costOf(r) : metric === "calls" ? 1 : tokens(r));
  const fmt = (v: number) => (metric === "cost" ? money(v) : metric === "calls" ? `${Math.round(v)} chamada(s)` : compact(v) + " tokens");
  const at = (r: Row) => new Date(r.at);
  const t0 = today();
  const thisMonth = t0.slice(0, 7);
  const SINCE: Record<string, string> = {
    "7d": addDays(t0, -6),
    "30d": addDays(t0, -29),
    mes: thisMonth + "-01",
    "90d": addDays(t0, -89),
    "12m": addDays(t0, -364),
    tudo: "0000",
  };
  const sum = (list: Row[], f: (r: Row) => number = (r) => core.costOf(r)) => list.reduce((a, r) => a + f(r), 0);

  let engines = $derived(["claude", "openai", "jev"].filter((e) => e !== "openai" || rows.some((r) => r.engine === "openai")));
  let inPeriod = $derived(rows.filter((r) => ymd(at(r)) >= (SINCE[period] ?? SINCE["30d"])));
  let shown = $derived(inPeriod.filter((r) => engineSel === "all" || eng(r) === engineSel));
  let month = $derived(rows.filter((r) => ymd(at(r)).startsWith(thisMonth)));
  const dom = Number(t0.slice(8, 10));
  const dim = new Date(Number(t0.slice(0, 4)), Number(t0.slice(5, 7)), 0).getDate();

  let months = $derived.by(() => {
    const ms: string[] = [];
    for (let i = 11; i >= 0; i--) {
      const d = parseYmd(thisMonth + "-01");
      d.setMonth(d.getMonth() - i);
      ms.push(ymd(d).slice(0, 7));
    }
    const vals = ms.map((m) => {
      const parts: Record<string, number> = {};
      for (const r of rows.filter((r) => (engineSel === "all" || eng(r) === engineSel) && ymd(at(r)).startsWith(m))) parts[eng(r)] = (parts[eng(r)] || 0) + val(r);
      return { m, parts, total: Object.values(parts).reduce((a, b) => a + b, 0) };
    });
    const max = Math.max(...vals.map((x) => x.total)) || 1;
    return { vals, max };
  });

  const WD = ["Seg", "Ter", "Qua", "Qui", "Sex", "Sáb", "Dom"];
  let heat = $derived.by(() => {
    const cells = Array.from({ length: 7 }, () => Array(24).fill(0));
    const counts = Array.from({ length: 7 }, () => Array(24).fill(0));
    for (const r of shown) {
      const d = at(r);
      const wd = (d.getDay() + 6) % 7;
      cells[wd][d.getHours()] += val(r);
      counts[wd][d.getHours()]++;
    }
    const max = Math.max(...cells.flat()) || 1;
    let peak: { v: number; w: number; h: number } | null = null;
    cells.forEach((row, w) => row.forEach((v, h) => v > 0 && (!peak || v > peak.v) && (peak = { v, w, h })));
    return { cells, counts, max, peak: peak as { v: number; w: number; h: number } | null };
  });

  function ranking(keyOf: (r: Row) => [string, number][], nameOf: (k: string) => string) {
    const agg = new Map<string, { total: number; parts: Record<string, number>; calls: number }>();
    for (const r of shown)
      for (const [k, w] of keyOf(r)) {
        const name = nameOf(k);
        const o = agg.get(name) || { total: 0, parts: {}, calls: 0 };
        const v = val(r) * w;
        o.total += v;
        o.parts[eng(r)] = (o.parts[eng(r)] || 0) + v;
        o.calls += w;
        agg.set(name, o);
      }
    const list = [...agg.entries()].sort((a, b) => b[1].total - a[1].total);
    const total = list.reduce((a, [, o]) => a + o.total, 0) || 1;
    return { list, total, max: list[0]?.[1].total || 1 };
  }
  let projTitle = $derived(new Map(all.filter((n) => n.data.kind === "project").map((x) => [x.data.id, x.data.title])));
  let taskTitle = $derived(new Map(all.filter((n) => n.data.kind === "task").map((n) => [n.data.id, n.data.title])));
  let byProject = $derived(ranking((r) => Object.entries(r.proj && typeof r.proj === "object" ? r.proj : { "": 1 }), (k) => projTitle.get(k) || "Outras tarefas"));
  let byPurpose = $derived(ranking((r) => [[r.purpose || "outro", 1]], (k) => PURPOSE[k] || k));
  let price = $derived(modelPrice(core.settings.model, core.settings));
</script>

<div class="mesa-filters mesa-ai-toolbar">
  {#each [[[["7d", "7 dias"], ["30d", "30 dias"], ["mes", "Este mês"], ["90d", "90 dias"], ["12m", "12 meses"], ["tudo", "Tudo"]], "period"], [[["all", "Tudo"], ...engines.map((e) => [e, ENG[e]])], "engine"], [[["cost", fx > 0 ? "R$" : "US$"], ["calls", "Chamadas"], ["tokens", "Tokens"]], "metric"]] as [opts, which]}
    <div class="sx-segmented sm">
      {#each opts as [k, label]}
        <button
          class:is-on={(which === "period" ? period : which === "engine" ? engineSel : metric) === k}
          onclick={() => (which === "period" ? (period = k) : which === "engine" ? (engineSel = k) : (metric = k as typeof metric))}
        >
          {label}
        </button>
      {/each}
    </div>
  {/each}
</div>

{#if !rows.length}
  <div class="mesa-gantt-empty">Nenhuma chamada registrada ainda. A partir de agora, cada uso do Claude e do Jev pela Mesa aparece aqui.</div>
{:else}
  <div class="mesa-kpis">
    <div class="mesa-kpi"><div class="mesa-kpi-label">Total no período</div><div class="mesa-kpi-value">{money(sum(inPeriod))}</div><div class="mesa-kpi-sub">{inPeriod.length} chamada(s)</div></div>
    {#each engines as e}
      {@const list = inPeriod.filter((r) => eng(r) === e)}
      {@const cost = sum(list)}
      <div class="mesa-kpi is-{e}">
        <div class="mesa-kpi-label">{ENG[e]}</div>
        <div class="mesa-kpi-value">{e === "jev" && !cost && list.length && !(Number(core.settings.jevPrice) > 0) ? "sem preço" : money(cost)}</div>
        <div class="mesa-kpi-sub">
          {e === "jev" ? `${list.length} chamada(s) · ${sum(list, (r) => Number(r.q) || 0)} pergunta(s)` : `${list.length} chamada(s) · ${compact(sum(list, tokens))} tokens`}
        </div>
      </div>
    {/each}
    <div class="mesa-kpi"><div class="mesa-kpi-label">Este mês</div><div class="mesa-kpi-value">{money(sum(month))}</div><div class="mesa-kpi-sub">projeção {money((sum(month) / dom) * dim)}</div></div>
  </div>

  <div class="mesa-ai-grid">
    <div class="mesa-ai-card is-wide">
      <div class="mesa-ai-card-head"><div class="mesa-ai-card-title">Por mês</div><div class="mesa-ai-card-sub">Últimos 12 meses</div></div>
      <div class="mesa-bars">
        {#each months.vals as x}
          <div class="mesa-bar-col" class:is-current={x.m === thisMonth}>
            <div class="mesa-bar-value">{x.total ? fmt(x.total).replace(/ chamada\(s\)| tokens/, "") : ""}</div>
            <div class="mesa-bar-stack" title={engines.filter((e) => x.parts[e]).map((e) => `${ENG[e]}: ${fmt(x.parts[e])}`).join("\n") || "Sem uso"}>
              {#each engines as e}{#if x.parts[e]}<div class="mesa-bar-seg is-{e}" style:height="{(x.parts[e] / months.max) * 100}%"></div>{/if}{/each}
            </div>
            <div class="mesa-bar-label">{parseYmd(x.m + "-01").toLocaleDateString("pt-BR", { month: "short" }).replace(".", "")}</div>
          </div>
        {/each}
      </div>
      <div class="mesa-ai-legend">{#each engines as e}<span class="is-{e}">{ENG[e]}</span>{/each}</div>
    </div>

    <div class="mesa-ai-card is-wide">
      <div class="mesa-ai-card-head"><div class="mesa-ai-card-title">Quando o consumo acontece</div><div class="mesa-ai-card-sub">Dia da semana × hora, no período</div></div>
      <div class="mesa-heat">
        <div></div>
        {#each { length: 24 } as _, h}<div class="mesa-heat-hour">{h % 3 === 0 ? String(h).padStart(2, "0") : ""}</div>{/each}
        {#each WD as d, w}
          <div class="mesa-heat-day">{d}</div>
          {#each heat.cells[w] as v, h}
            <div
              class="mesa-heat-cell"
              class:has-value={v > 0}
              style:--mesa-heat={v > 0 ? Math.round(15 + (v / heat.max) * 85) : 0}
              title="{d} {String(h).padStart(2, '0')}h · {fmt(v)} · {heat.counts[w][h]} chamada(s)"
            ></div>
          {/each}
        {/each}
      </div>
      <div class="mesa-heat-foot">
        {heat.peak ? `Pico: ${WD[heat.peak.w]} ${String(heat.peak.h).padStart(2, "0")}h (${fmt(heat.peak.v)})` : "Sem uso no período"}
      </div>
    </div>

    {#each [["Por projeto", "Custo das chamadas dividido entre as tarefas envolvidas", byProject], ["Por função", "Qual recurso da Mesa consome", byPurpose]] as [title, sub, rank]}
      {@const r = rank as ReturnType<typeof ranking>}
      <div class="mesa-ai-card">
        <div class="mesa-ai-card-head"><div class="mesa-ai-card-title">{title}</div><div class="mesa-ai-card-sub">{sub}</div></div>
        {#each r.list as [name, o]}
          <div class="mesa-rank" title={engines.filter((e) => o.parts[e]).map((e) => `${ENG[e]}: ${fmt(o.parts[e])}`).join(" · ")}>
            <div class="mesa-rank-top"><span class="mesa-rank-name">{name}</span><span class="mesa-rank-val">{fmt(o.total)} · {Math.round((o.total / r.total) * 100)}%</span></div>
            <div class="mesa-rank-track">{#each engines as e}{#if o.parts[e]}<div class="mesa-bar-seg is-{e}" style:width="{(o.parts[e] / r.max) * 100}%"></div>{/if}{/each}</div>
          </div>
        {:else}
          <div class="mesa-muted">Sem uso no período.</div>
        {/each}
      </div>
    {/each}

    <div class="mesa-ai-card is-wide">
      <div class="mesa-ai-card-head"><div class="mesa-ai-card-title">Últimas chamadas</div><div class="mesa-ai-card-sub">As 15 mais recentes no filtro</div></div>
      <div class="mesa-calls">
        {#each shown.slice(-15).reverse() as r}
          <div class="mesa-call">
            <span class="mesa-call-when">{at(r).toLocaleString("pt-BR", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })}</span>
            <span class="mesa-call-eng is-{eng(r)}">{ENG[eng(r)]}</span>
            <span class="mesa-call-what">{PURPOSE[r.purpose ?? ""] || r.purpose || "—"}{r.task && taskTitle.get(r.task) ? " · " + taskTitle.get(r.task) : ""}</span>
            <span class="mesa-call-tok">{eng(r) === "jev" ? `${r.q || 0} perg.` : compact(tokens(r)) + " tok"}</span>
            <span class="mesa-call-cost">{money(core.costOf(r))}</span>
          </div>
        {:else}
          <div class="mesa-muted">Sem chamadas no filtro.</div>
        {/each}
      </div>
    </div>
  </div>
{/if}
<div class="mesa-ai-foot">
  Valores estimados. {core.providerName}: tokens × US$ {price.in}/{price.out} por milhão (entrada/saída). Jev: custo informado pela API ou preço por
  pergunta das configurações. {fx > 0 ? `Convertido para real a ${fx}. ` : ""}Só conta chamadas feitas pela Mesa.
</div>
