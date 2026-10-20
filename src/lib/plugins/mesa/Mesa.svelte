<script lang="ts">
  /*
   * Mesa — reescrita do plugin Mesa do Obsidian. Dados em Markdown na pasta
   * Mesa/ do cofre (mesmo formato), núcleo de arquivos e IA em Rust.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick, untrack } from "svelte";
  import { compactMarkdown, eligible, fmtDate, MesaCore, prioLabel, prioLevel, recurLabel, statusOf, taskRange, today, type Rec, type Settings } from "./core";
  import Forms, { type FormKind } from "./Forms.svelte";
  import Calendar from "./Calendar.svelte";
  import Usage from "./Usage.svelte";
  import History from "./History.svelte";
  import Icon from "../../components/Icon.svelte";
  import { renderBlock } from "../../markdown/render";

  type Props = {
    settings: Settings;
    files: { path: string; rel: string; name: string }[];
    onopen: (rel: string, newTab: boolean) => void;
    onopenlink: (target: string) => void;
    onnotify: (text: string, error?: boolean) => void;
    onsettings: () => void;
    captureRequest?: { text: string; source: string | null; n: number } | null;
  };
  let { settings, files, onopen, onopenlink, onnotify, onsettings, captureRequest = null }: Props = $props();

  let version = $state(0);
  // Configurações iniciais; o $effect abaixo mantém o núcleo em dia
  const core = untrack(() => new MesaCore(settings, () => (version++, load()), (t, e) => onnotify(t, e)));
  let all = $state<Rec[]>([]);
  let ready = $state(false);
  let tab = $state("Mesa");
  let query = $state("");
  let projectId = $state("");
  let triageOnly = $state(false);
  let form = $state<FormKind | null>(null);
  let autoDay = "";

  const TABS = ["Mesa", "Projetos", "Pendências", "Calendário", "Sugestões", "Concluídas", "Arquivadas", "Histórico", "IA"];

  async function load() {
    try {
      all = await core.all();
    } catch (e) {
      onnotify(String(e), true);
    }
  }

  $effect(() => {
    core.settings = settings;
  });

  $effect(() => {
    if (captureRequest) form = { type: "capture", initial: captureRequest.text, source: captureRequest.source };
  });

  let projects = $derived(all.filter((n) => n.data.kind === "project"));
  let tasks = $derived(all.filter((n) => n.data.kind === "task" && !n.data.archived));
  let archived = $derived(all.filter((n) => n.data.kind === "task" && n.data.archived));
  let suggestions = $derived(
    all.filter(
      (n) =>
        n.data.kind === "suggestion" &&
        n.data.state === "pending" &&
        (n.data.type === "enrich"
          ? tasks.some((t) => t.path === n.data.target && t.data.state === "pending")
          : n.data.date === today() && (n.data.ids || []).some((id: string) => tasks.some((t) => t.data.id === id && t.data.state === "pending"))),
    ),
  );
  let enrichReady = $derived(new Set(suggestions.filter((s) => s.data.type === "enrich").map((s) => s.data.target)));
  let load_ = $derived(version >= 0 ? core.dayLoad(all) : core.dayLoad(all));
  let dayRec = $derived(all.find((n) => n.data.kind === "day" && n.data.date === today()));
  let planned = $derived<string[]>(dayRec?.data.tasks || []);
  let selected = $derived([
    ...load_.committed.filter((n) => planned.includes(n.data.id)),
    ...load_.committed.filter((n) => !planned.includes(n.data.id)).sort((a, b) => (taskRange(a)?.end || "").localeCompare(taskRange(b)?.end || "")),
  ]);
  // Fixadas ficam sempre na Mesa (fora da lista de hoje para não repetir)
  let pinned = $derived(version >= 0 ? core.ordered(tasks.filter((n) => n.data.pinned && n.data.state === "pending"), "pinned") : []);
  let todayList = $derived(version >= 0 ? core.ordered(selected.filter((n) => !n.data.pinned), "today") : []);
  // Capturadas nas últimas 48 h aparecem no topo até entrarem no fluxo (fixar, hoje)
  const since = () => new Date(Date.now() - 48 * 3600e3).toISOString();
  let fresh = $derived(
    tasks
      .filter((n) => n.data.state === "pending" && !n.data.pinned && !selected.includes(n) && String(n.data.created || "") >= since())
      .sort((a, b) => String(b.data.created).localeCompare(String(a.data.created)))
      .slice(0, 8),
  );
  let nextUp = $derived(
    version < 0
      ? []
      : tasks
      .filter((n) => eligible(n, projects) && !selected.includes(n) && !n.data.pinned && !fresh.includes(n))
      .map((n, i) => [n, core.prioOf(n) ?? -1, i] as const)
      .sort((a, b) => b[1] - a[1] || a[2] - b[2])
      .map(([n]) => n)
      .slice(0, 5),
  );
  const triage = (n: Rec) => !n.data.project || n.data.deadline === "pending";
  const newest = (k: string) => (a: Rec, b: Rec) => String(b.data[k] || b.data.created || "").localeCompare(String(a.data[k] || a.data.created || ""));
  let filtered = $derived(
    (tab === "Arquivadas"
      ? [...archived].sort(newest("archived"))
      : tab === "Concluídas"
        ? [...tasks].sort(newest("updated"))
        : [...tasks].sort((a, b) => Number(!!b.data.pinned) - Number(!!a.data.pinned) || newest("created")(a, b))
    ).filter(
      (n) =>
        (tab === "Arquivadas" || (tab === "Concluídas" ? n.data.state !== "pending" : n.data.state === "pending")) &&
        (!triageOnly || tab !== "Pendências" || triage(n)) &&
        (!projectId || n.data.project === projectId) &&
        (n.data.title + " " + n.body).toLowerCase().includes(query.toLowerCase()),
    ),
  );

  $effect(() => {
    if (ready && all.length) core.prioritize(all);
  });

  async function act(fn: () => Promise<unknown>) {
    await core.run(fn);
  }

  onMount(async () => {
    try {
      await invoke("mesa_init");
      await core.loadLocal();
      await core.loadKeys();
      await load();
      ready = true;
      if (settings.daily && settings.jev && core.hasJev && autoDay !== today()) {
        autoDay = today();
        await core.run(() => core.suggestDay(true));
      }
    } catch (e) {
      onnotify(String(e), true);
    }
  });

  // ----- Dá pra fazer hoje -----
  let quick = $derived(version >= 0 ? core.loadQuick() : null);
  let quickItems = $derived(
    quick ? quick.items.filter((i: any) => !quick.dismissed.includes(i.id)).map((i: any) => [i, load_.candidates.find((n) => n.data.id === i.id)] as const).filter(([, n]: any) => n) : [],
  );
  $effect(() => {
    if (!ready || !settings.quick) return;
    const used = load_.committed.length;
    if (used < settings.capacity && load_.candidates.length && !quick && !core.quickBusy && !core.quickError && core.quickTried !== today() && (core.hasAi || (settings.jev && core.hasJev))) {
      core.quickSuggest(all);
    }
  });

  const projectOf = (n: Rec) => projects.find((p) => p.data.id === n.data.project);

  // ----- Ver sugestão: abre a aba e destaca a proposta da tarefa -----
  let focusSugg = $state<string | null>(null);
  function showSuggestion(n: Rec) {
    focusSugg = suggestions.find((s) => s.data.type === "enrich" && s.data.target === n.path)?.data.id ?? null;
    tab = "Sugestões";
  }
  $effect(() => {
    const id = focusSugg;
    if (tab !== "Sugestões" || !id) return;
    tick().then(() => document.getElementById("mesa-sugg-" + id)?.scrollIntoView({ block: "center", behavior: "smooth" }));
    const t = setTimeout(() => focusSugg === id && (focusSugg = null), 3000);
    return () => clearTimeout(t);
  });

  // ----- Recém-criada: destaca na lista -----
  let flashId = $state<string | null>(null);
  function created(n?: Rec) {
    load();
    if (!n) return;
    if (!["Mesa", "Pendências"].includes(tab)) tab = "Mesa";
    flashId = n.data.id;
    setTimeout(() => flashId === n.data.id && (flashId = null), 2500);
  }

  // ----- Arrastar para reordenar (Fixadas e Hoje) -----
  let grab = $state<string | null>(null);
  let drag = $state<{ key: string; id: string } | null>(null);
  let dropAt = $state<{ id: string; after: boolean } | null>(null);
  function dragOver(e: DragEvent, key: string, n: Rec) {
    if (!drag || drag.key !== key || drag.id === n.data.id) return;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dropAt = { id: n.data.id, after: e.clientY > r.top + r.height / 2 };
  }
  function drop(key: string, list: Rec[]) {
    const d = drag;
    const at = dropAt;
    drag = dropAt = grab = null;
    if (!d || !at || d.key !== key) return;
    const ids = list.map((n) => n.data.id).filter((id) => id !== d.id);
    ids.splice(ids.indexOf(at.id) + (at.after ? 1 : 0), 0, d.id);
    core.reorder(key, ids);
    version++;
  }
</script>

{#snippet card(n: Rec, inToday: boolean | "cal" = false, dragKey: string | null = null, list: Rec[] = [])}
  {@const [statusKey, statusLabel] = statusOf(n)}
  {@const prio = n.data.state === "pending" && !n.data.archived && version >= 0 ? core.prioOf(n) : undefined}
  {@const range = taskRange(n)}
  {@const recur = recurLabel(n.data.recur)}
  {@const sub = [n.data.next, n.data.blocked ? "Bloqueada: " + n.data.blocked : "", n.data.defer ? "Adiada até " + fmtDate(String(n.data.defer)) : ""].filter(Boolean).join(" · ")}
  {@const pending = n.data.state === "pending" && !n.data.archived}
  {@const ready_ = enrichReady.has(n.path)}
  {@const busy = version >= 0 && core.enriching.has(n.data.id)}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="mesa-row mesa-s-{statusKey}"
    class:is-pinned={n.data.pinned && !n.data.archived}
    class:is-closed={n.data.state !== "pending"}
    class:is-flash={flashId === n.data.id}
    class:is-dragging={drag?.id === n.data.id}
    class:drop-before={dropAt?.id === n.data.id && !dropAt?.after}
    class:drop-after={dropAt?.id === n.data.id && !!dropAt?.after}
    draggable={!!dragKey && grab === n.data.id}
    ondragstart={(e) => dragKey && ((drag = { key: dragKey, id: n.data.id }), e.dataTransfer?.setData("text/plain", n.data.title))}
    ondragover={(e) => dragKey && dragOver(e, dragKey, n)}
    ondrop={(e) => (e.preventDefault(), dragKey && drop(dragKey, list))}
    ondragend={() => (drag = dropAt = grab = null)}
  >
    {#if dragKey}
      <span class="mesa-grip" title="Arraste para reordenar" onpointerdown={() => (grab = n.data.id)} onpointerup={() => !drag && (grab = null)}>
        <Icon name="grip" size={14} stroke={3} />
      </span>
    {/if}
    <label class="mesa-check" title={n.data.state === "done" ? "Reabrir" : "Concluir"}>
      <input type="checkbox" checked={n.data.state === "done"} onchange={(e) => act(() => core.setState(n, e.currentTarget.checked ? "done" : "pending"))} />
      <span><Icon name="check" size={11} stroke={3} /></span>
    </label>
    <div class="mesa-row-main">
      <button class="mesa-task-title" title={n.data.title} onclick={() => (form = { type: "task", task: n })}>
        {#if n.data.pinned && !n.data.archived}<span class="mesa-row-pin"><Icon name="pin" size={11} stroke={2.2} /></span>{/if}{n.data.title}
      </button>
      {#if sub}<div class="mesa-row-sub" title={sub}>{sub}</div>{/if}
    </div>
    <span class="mesa-row-proj" class:is-empty={!projectOf(n)} title={projectOf(n)?.data.title || "Projeto a identificar"}>{projectOf(n)?.data.title || "—"}</span>
    <span class="mesa-row-date" title={range && range.start !== range.end && !range.month ? `${fmtDate(range.start)} → ${fmtDate(range.end)}` : ""}>
      {#if recur}<span class="mesa-row-recur" title={recur}><Icon name="repeat" size={11} stroke={2} /></span>{/if}
      {n.data.deadline === "pending" ? "a definir" : range ? (range.month ? "Mês " + String(n.data.due).slice(5) : fmtDate(range.end)) : "—"}
    </span>
    <span class="mesa-row-status"><i class="mesa-dot"></i>{statusLabel}</span>
    {#if prio !== undefined}
      <span class="mesa-row-prio mesa-prio-{prioLevel(prio)}" title="Prioridade pelo Jev: {prio.toFixed(1).replace('.', ',')} de 3"><i class="mesa-dot"></i>{prioLabel(prio)}</span>
    {:else}
      <span class="mesa-row-prio"></span>
    {/if}
    <div class="mesa-row-actions">
      {#if pending}
        <button class="icon-btn sm" class:is-on={n.data.pinned} title={n.data.pinned ? "Desafixar da Mesa" : "Fixar na Mesa"} onclick={() => act(() => core.togglePin(n))}>
          <Icon name="pin" size={14} />
        </button>
        {#if inToday === "cal"}
          <button class="icon-btn sm is-on" disabled title="No calendário para hoje · ajuste as datas para tirar"><Icon name="sun" size={14} /></button>
        {:else}
          <button class="icon-btn sm" class:is-on={inToday} title={inToday ? "Tirar de hoje" : "Fazer hoje"} onclick={() => act(() => (inToday ? core.removeToday(n) : core.acceptToday(n)))}>
            <Icon name="sun" size={14} />
          </button>
        {/if}
        <button
          class="icon-btn sm mesa-enrich"
          class:is-ready={ready_}
          class:is-busy={busy}
          disabled={busy || (!ready_ && !core.hasAi)}
          title={ready_ ? "Ver sugestão" : busy ? "A IA está preparando a proposta" : core.hasAi ? "Enriquecer com IA (uma chamada)" : "Informe a API key em Configurações › Mesa"}
          onclick={() => (ready_ ? showSuggestion(n) : act(() => core.enrich(n)))}
        >
          <Icon name="zap" size={14} />
        </button>
      {/if}
      <button class="icon-btn sm" title="Abrir nota" onclick={() => onopen(n.path, true)}><Icon name="external" size={14} /></button>
      {#if n.data.archived}
        <button class="icon-btn sm" title="Restaurar" onclick={() => act(() => core.archive(n, false))}><Icon name="undo" size={14} /></button>
      {:else}
        <button class="icon-btn sm" title="Arquivar" onclick={() => act(() => core.archive(n))}><Icon name="archive" size={14} /></button>
      {/if}
    </div>
  </div>
{/snippet}

<div class="mesa">
  <div class="mesa-header">
    <h2>Mesa</h2>
    <button class="btn-primary sm" onclick={() => (form = { type: "capture" })}><Icon name="plus" size={13} stroke={2} /> Capturar</button>
    <button class="icon-btn sm" title="Configurações da Mesa" onclick={onsettings}><Icon name="settings" size={15} /></button>
  </div>
  <div class="mesa-nav">
    {#each TABS as t}
      <button class:is-active={tab === t} onclick={() => (tab = t)}>
        {t}{#if t === "Sugestões" && suggestions.length}<span class="mesa-bubble">{suggestions.length > 99 ? "99+" : suggestions.length}</span>{/if}
      </button>
    {/each}
  </div>

  {#if settings.quick && ready}
    {@const used = load_.committed.length}
    {@const ratio = Math.min(1, used / settings.capacity)}
    <div class="mesa-quick" class:is-loading={core.quickBusy}>
      <div class="mesa-quick-head">
        <Icon name="zap" size={14} />
        <span class="mesa-quick-title">Dá pra fazer hoje</span>
        <div class="mesa-quick-meter" title="{used} tarefa(s) comprometida(s) hoje{load_.overdue ? `, ${load_.overdue} atrasada(s)` : ''} · capacidade {settings.capacity}">
          <div class="mesa-quick-meter-track"><div class="mesa-quick-meter-bar {ratio >= 1 ? 'is-full' : ratio >= 0.6 ? 'is-mid' : 'is-low'}" style:width="{Math.max(4, ratio * 100)}%"></div></div>
          <span class="mesa-quick-meter-text">Carga {used}/{settings.capacity}</span>
        </div>
        <div class="mesa-quick-tools">
          {#if quick}<span class="mesa-quick-meter-text">{quick.engine === "jev" ? "via Jev" : "via " + core.providerName}</span>{/if}
          {#if used < settings.capacity && load_.candidates.length && !core.quickBusy}
            <button class="icon-btn sm" class:is-stale={quick && quick.fp !== core.quickFingerprint(load_)} title="Gerar novas sugestões" onclick={() => core.quickSuggest(all, true)}>
              <Icon name="refresh" size={13} />
            </button>
          {/if}
        </div>
      </div>
      <div class="mesa-quick-body">
        {#if !core.hasAi && !(settings.jev && core.hasJev)}
          <div class="mesa-quick-note">Informe a API key em Configurações › Mesa para receber sugestões.</div>
        {:else if used >= settings.capacity}
          <div class="mesa-quick-note">Seu dia já tem {used} tarefa(s) comprometida(s){load_.overdue ? ` (${load_.overdue} atrasada(s))` : ""}. Sem sugestões extras para não sobrecarregar.</div>
        {:else if !load_.candidates.length}
          <div class="mesa-quick-note">Nada disponível para adiantar agora.</div>
        {:else if core.quickBusy}
          <div class="mesa-quick-note">Analisando a carga do dia…</div>
        {:else if !quick}
          <div class="mesa-quick-note">{core.quickError || "Analisando a carga do dia…"}</div>
        {:else if quick.abstain || !quickItems.length}
          <div class="mesa-quick-note">{quick.reason || (quick.items.length ? "Você já tratou as sugestões de hoje." : "Nada extra por agora — siga com o que já está planejado.")}</div>
        {:else}
          {#if quick.reason}<div class="mesa-quick-reason">{quick.reason}</div>{/if}
          {#each quickItems as [i, n]}
            <div class="mesa-quick-item">
              <div class="mesa-quick-main">
                <button class="mesa-quick-task" onclick={() => (form = { type: "task", task: n! })}>{n!.data.title}</button>
                {#if i.why}<div class="mesa-quick-why">{i.why}</div>{/if}
              </div>
              {#if i.effort}<span class="mesa-quick-effort">{i.effort}</span>{/if}
              <button class="btn-ghost sm" onclick={() => act(() => core.acceptToday(n!))}>Fazer hoje</button>
              <button class="icon-btn sm" aria-label="Dispensar sugestão" onclick={() => (core.saveQuick({ ...quick, dismissed: [...quick.dismissed, i.id] }), version++)}>
                <Icon name="x" size={12} stroke={2} />
              </button>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}

  {#if !ready}
    <div class="mesa-muted">Carregando a Mesa…</div>
  {:else if tab === "Mesa"}
    {#if pinned.length}
      <h3><Icon name="pin" size={13} /> Fixadas</h3>
      <div class="mesa-list">{#each pinned as n (n.data.id)}{@render card(n, planned.includes(n.data.id) ? true : selected.includes(n) ? "cal" : false, "pinned", pinned)}{/each}</div>
    {/if}
    {#if fresh.length}
      <h3>Novas</h3>
      <div class="mesa-list">{#each fresh as n (n.data.id)}{@render card(n)}{/each}</div>
    {/if}
    <h3>Hoje · {fmtDate(today())}</h3>
    {#if !selected.length}
      <p>{planned.length ? "O que você planejou já foi encerrado." : "Escolha uma entrega para começar. O restante pode esperar fora desta mesa."}</p>
      <div class="mesa-actions">
        <button class="btn-ghost" onclick={() => act(() => core.suggestDay())}>Sugerir com IA</button>
        <button class="btn-ghost" onclick={() => act(() => core.plan([], true))}>Hoje sem tarefas</button>
      </div>
    {/if}
    {#if todayList.length}
      <div class="mesa-list">{#each todayList as n (n.data.id)}{@render card(n, planned.includes(n.data.id) ? true : "cal", "today", todayList)}{/each}</div>
    {:else if selected.length}
      <p class="mesa-muted">As tarefas de hoje estão em Fixadas.</p>
    {/if}
    {#if planned.length}<button class="btn-ghost sm" onclick={() => act(() => core.clearToday())}>Limpar planejamento de hoje</button>{/if}
    <h3>Próximos passos disponíveis</h3>
    <div class="mesa-list">{#each nextUp as n (n.data.id)}{@render card(n)}{/each}</div>
    <p class="mesa-muted">{tasks.filter((n) => n.data.state === "pending").length} pendências guardadas. Veja todas em Pendências.</p>
  {:else if tab === "Projetos"}
    {@const t0 = today()}
    {@const LABEL = { active: "Ativo", paused: "Pausado", archived: "Arquivado" } as Record<string, string>}
    <div class="mesa-section-head">
      <div class="mesa-section-sub">{projects.filter((p) => (p.data.state || "active") === "active").length} ativo(s) · {projects.length} no total</div>
      <button class="btn-ghost sm" onclick={() => (form = { type: "project" })}>+ Vincular projeto</button>
    </div>
    {#if !projects.length}
      <div class="mesa-gantt-empty">Nenhum projeto ainda. Vincule um para agrupar tarefas e dar contexto à IA.</div>
    {/if}
    {#each [["Ativos", "active"], ["Pausados", "paused"], ["Arquivados", "archived"]] as [name, key]}
      {@const list = projects.filter((p) => (LABEL[p.data.state] ? p.data.state : "active") === key).sort((a, b) => String(a.data.title).localeCompare(String(b.data.title), "pt-BR"))}
      {#if list.length}
        <div class="mesa-group-label">{name} · {list.length}</div>
        <div class="mesa-proj-grid">
          {#each list as p (p.data.id)}
            {@const mine = tasks.filter((t) => t.data.project === p.data.id)}
            {@const pending = mine.filter((t) => t.data.state === "pending")}
            {@const overdue = pending.filter((t) => statusOf(t, t0)[0] === "overdue").length}
            {@const done = mine.filter((t) => t.data.state === "done").length}
            {@const total = pending.length + done}
            {@const pct = total ? Math.round((done / total) * 100) : 0}
            {@const nextDue = pending.map((t) => taskRange(t)?.end).filter((e) => e && e >= t0).sort()[0]}
            {@const desc = compactMarkdown(p.body || "").replace(/^#+\s*/gm, "").replace(/[*_`>]/g, "").trim()}
            {@const docs = files.filter((f) => (p.data.roots || []).some((r: string) => f.rel === r || f.rel.startsWith(r + "/")) || (p.data.notes || []).includes(f.rel))}
            <div class="mesa-proj is-{key}">
              <div class="mesa-proj-top">
                <button class="mesa-proj-title" onclick={() => ((projectId = p.data.id), (tab = "Pendências"))}>{p.data.title}</button>
                <span class="mesa-pill is-{key}">{LABEL[key]}</span>
              </div>
              <div class="mesa-proj-desc" class:is-empty={!desc}>{desc || "Sem descrição. Edite para registrar objetivo e onde parou."}</div>
              <div class="mesa-proj-stats">
                <div class="mesa-proj-stat"><div class="mesa-proj-num">{pending.length}</div><div class="mesa-proj-lbl">pendentes</div></div>
                <div class="mesa-proj-stat" class:is-alert={overdue}><div class="mesa-proj-num">{overdue}</div><div class="mesa-proj-lbl">atrasadas</div></div>
                <div class="mesa-proj-stat"><div class="mesa-proj-num">{done}</div><div class="mesa-proj-lbl">concluídas</div></div>
              </div>
              <div class="mesa-proj-progress"><div class="mesa-proj-track"><div class="mesa-proj-fill" style:width="{pct}%"></div></div><span>{total ? `${pct}%` : "—"}</span></div>
              <div class="mesa-proj-meta">
                {#if nextDue}<span>Próximo prazo {fmtDate(nextDue)}</span>{/if}
                {#if p.data.ai === true && (p.data.notes || []).length}<span>IA lê {(p.data.notes || []).length} nota(s)</span>{/if}
              </div>
              <div class="mesa-proj-foot">
                <button class="btn-ghost sm" onclick={() => ((projectId = p.data.id), (tab = "Pendências"))}><Icon name="check" size={12} /> Tarefas</button>
                <button class="btn-ghost sm" onclick={() => (form = { type: "project", project: p })}><Icon name="pencil" size={12} /> Editar</button>
                {#if docs.length}
                  <details class="mesa-proj-docs">
                    <summary class="btn-ghost sm"><Icon name="folder" size={12} /> Docs ({docs.length})</summary>
                    <div>{#each docs.slice(0, 200) as f}<button class="sx-link-btn" title={f.rel} onclick={() => onopen(f.rel, true)}>{f.name}</button>{/each}</div>
                  </details>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    {/each}
  {:else if tab === "Pendências" || tab === "Concluídas" || tab === "Arquivadas"}
    <div class="mesa-filters">
      <input class="sx-input mesa-search" type="search" placeholder="Buscar tarefas" bind:value={query} />
      <select bind:value={projectId}>
        <option value="">Todos os projetos</option>
        {#each projects as p}<option value={p.data.id}>{p.data.title}</option>{/each}
      </select>
      {#if tab === "Pendências"}
        <label class="sx-check">
          <input type="checkbox" bind:checked={triageOnly} /> Só a triar (sem projeto ou prazo) · {tasks.filter((n) => n.data.state === "pending" && triage(n)).length}
        </label>
      {/if}
    </div>
    <div class="mesa-list">{#each filtered as n (n.data.id)}{@render card(n)}{:else}<p>Nenhuma tarefa nesta seleção.</p>{/each}</div>
  {:else if tab === "Calendário"}
    <Calendar {core} {tasks} {projects} onedit={(n) => (form = { type: "task", task: n })} onchanged={load} />
  {:else if tab === "Sugestões"}
    <div class="mesa-sugg-bar mesa-muted"><Icon name="zap" size={13} /> Enriquecimento só sob demanda: use o botão Enriquecer em cada tarefa.</div>
    {#each suggestions as s (s.data.id)}
      <div class="mesa-card" id="mesa-sugg-{s.data.id}" class:is-focus={focusSugg === s.data.id}>
        <h4>{s.data.title}</h4>
        <p>{s.body}</p>
        {#if s.data.type === "enrich"}
          {@const p = s.data.proposal}
          {@const target = tasks.find((n) => n.path === s.data.target)}
          <div class="mesa-proposal">
            {#if p?.title}<div><strong>Título:</strong> {p.title}</div>{/if}
            {#if p?.next}<div><strong>Próximo passo:</strong> {p.next}</div>{/if}
            <div><strong>Projeto:</strong> {projects.find((x) => x.data.id === p?.project)?.data.title || "Mantém o atual"}</div>
            {#if p?.description}<div class="mesa-md">{@html renderBlock(String(p.description))}</div>{/if}
            {#if Array.isArray(p?.gaps) && p.gaps.length}
              <strong>A esclarecer</strong>
              <ul>{#each p.gaps as g}<li>{g}</li>{/each}</ul>
            {/if}
          </div>
          <div class="mesa-actions">
            {#if target && target.raw !== s.data.base}
              <p class="mesa-warn">A tarefa mudou depois desta sugestão. Gere novamente para aplicar.</p>
              <button class="btn-ghost sm" onclick={() => act(() => core.enrich(target))}>Gerar novamente</button>
            {:else}
              <button class="btn-primary sm" onclick={() => act(() => core.applyEnrich(s, tasks, projects))}>Aplicar enriquecimento</button>
            {/if}
            <button class="btn-ghost sm" onclick={() => act(() => core.update(s, { state: "dismissed" }))}>Descartar</button>
          </div>
        {:else}
          <div class="mesa-actions">
            {#each s.data.ids || [] as id}
              {@const t = tasks.find((n) => n.data.id === id && n.data.state === "pending")}
              {#if t}
                <button
                  class="btn-ghost sm"
                  onclick={() =>
                    act(async () => {
                      const fresh = await core.all();
                      const current = fresh.find((n) => n.data.id === t.data.id);
                      if (!current || !eligible(current, fresh.filter((n) => n.data.kind === "project"))) throw Error("Tarefa indisponível.");
                      await core.acceptToday(t);
                      await core.update(s, { state: "accepted" });
                    })}
                >
                  Hoje: {t.data.title}
                </button>
              {/if}
            {/each}
            <button class="btn-ghost sm" onclick={() => act(() => core.update(s, { state: "dismissed" }))}>Descartar</button>
          </div>
        {/if}
      </div>
    {:else}
      <div class="mesa-gantt-empty">Nenhuma sugestão pendente. Peça um enriquecimento em qualquer tarefa e a proposta aparece aqui.</div>
    {/each}
  {:else if tab === "Histórico"}
    <History {core} {all} onedit={(n) => (form = { type: "task", task: n })} onchanged={load} />
  {:else}
    <Usage {core} {all} />
  {/if}
</div>

{#if form}
  <Forms {core} {form} {projects} onclose={() => (form = null)} onsaved={created} {onopenlink} />
{/if}
