<script lang="ts">
  // Histórico legível da Mesa: agrupa por dia e descreve o que mudou; permite desfazer.
  import { invoke } from "@tauri-apps/api/core";
  import { parse as parseYaml } from "yaml";
  import { diffDays, fmtDate, STATUS, today, validDate, ymd, type MesaCore, type Rec } from "./core";
  import Icon from "../../components/Icon.svelte";

  type Props = { core: MesaCore; all: Rec[]; onedit: (n: Rec) => void; onchanged: () => void };
  let { core, all, onedit, onchanged }: Props = $props();

  type J = { file: string; path: string; before?: string; after: string; at: string };
  let rows = $state<J[]>([]);
  let total = $state(0);
  let limit = $state(40);

  async function load() {
    const r = await invoke<{ rows: J[]; total: number }>("mesa_history", { limit });
    rows = r.rows;
    total = r.total;
  }
  $effect(() => {
    limit;
    load();
  });

  const parse = (raw?: string) => {
    if (!raw) return null;
    const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/);
    if (!m) return null;
    try {
      return { data: parseYaml(m[1]) as Record<string, any>, body: raw.slice(m[0].length) };
    } catch {
      return null;
    }
  };
  const byPath = $derived(new Map(all.map((n) => [n.path, n])));
  const pt = (id: string) => (!id ? "sem projeto" : all.find((n) => n.data.kind === "project" && n.data.id === id)?.data.title || "projeto removido");
  const ICON: Record<string, string> = { task: "check", project: "folder", day: "clipboard", suggestion: "zap" };

  function describe(before: ReturnType<typeof parse>, after: ReturnType<typeof parse>) {
    const B = before?.data || {};
    const A = after?.data || {};
    const kind = A.kind || B.kind;
    const out: { label: string; from?: string; to?: string }[] = [];
    if (!before) return [{ label: "Criada" }];
    const IGNORE = new Set(["revision", "updated", "todayPrev", "base", "proposal", "created", "id", "kind", "date"]);
    const same = (k: string) => JSON.stringify(B[k] ?? null) === JSON.stringify(A[k] ?? null);
    const dt = (v: unknown) => (validDate(String(v || "")) ? fmtDate(String(v)) : "—");
    const due = (x: Record<string, any>) => (x.deadline === "date" ? dt(x.due) : x.deadline === "month" ? "mês " + (x.due || "?") : x.deadline === "none" ? "sem prazo" : "a definir");
    const cut = (v: unknown) => {
      const t = String(v || "").replace(/\s+/g, " ").trim();
      return t.length > 80 ? t.slice(0, 79) + "…" : t;
    };
    const P: Record<string, string> = { active: "Ativo", paused: "Pausado", archived: "Arquivado" };
    let dueDone = false;
    for (const k of [...new Set([...Object.keys(B), ...Object.keys(A)])].filter((k) => !IGNORE.has(k) && !same(k))) {
      if (k === "state" && kind === "suggestion")
        out.push({ label: ({ accepted: "Sugestão aplicada", dismissed: "Sugestão descartada", pending: "Sugestão reaberta" } as Record<string, string>)[A.state] || "Sugestão alterada" });
      else if (k === "state" && kind === "project") out.push({ label: "Situação", from: P[B.state] || "—", to: P[A.state] || "—" });
      else if (k === "state") {
        const closed = (s: string) => s === "done" || s === "cancelled";
        out.push(
          closed(A.state) || closed(B.state)
            ? { label: A.state === "done" ? "Concluída" : A.state === "cancelled" ? "Cancelada" : "Reaberta" }
            : { label: "Estado", from: STATUS[B.state] || "—", to: STATUS[A.state] || "—" },
        );
      } else if (k === "archived") out.push({ label: A.archived ? "Arquivada" : "Restaurada do arquivo" });
      else if (k === "title") out.push({ label: "Título", from: cut(B.title), to: cut(A.title) });
      else if (k === "project") out.push({ label: "Projeto", from: pt(B.project), to: pt(A.project) });
      else if (k === "deadline" || k === "due") {
        if (!dueDone) out.push({ label: "Prazo", from: due(B), to: due(A) });
        dueDone = true;
      } else if (k === "start") out.push({ label: "Início", from: dt(B.start), to: dt(A.start) });
      else if (k === "defer") out.push(A.defer ? { label: "Adiada até", to: dt(A.defer) } : { label: "Adiamento removido" });
      else if (k === "next") out.push(A.next ? { label: "Próximo passo", to: cut(A.next) } : { label: "Próximo passo removido" });
      else if (k === "blocked") out.push(A.blocked ? { label: "Bloqueada", to: cut(A.blocked) } : { label: "Desbloqueada" });
      else if (k === "tasks") {
        const b = new Set(B.tasks || []);
        const a = new Set(A.tasks || []);
        const add = [...a].filter((x) => !b.has(x)).length;
        const rem = [...b].filter((x) => !a.has(x)).length;
        out.push({ label: "Tarefas de hoje", to: [add ? `+${add} adicionada(s)` : "", rem ? `−${rem} removida(s)` : ""].filter(Boolean).join(" · ") || "reordenadas" });
      } else if (k === "silenced") {
        if (A.silenced) out.push({ label: "Dia marcado sem tarefas" });
      } else if (k === "notes" || k === "roots" || k === "ai") {
        if (!out.some((x) => x.label === "Contexto de IA atualizado")) out.push({ label: "Contexto de IA atualizado" });
      } else out.push({ label: k + " alterado" });
    }
    if ((before.body || "") !== (after?.body || ""))
      out.push({ label: /^## Enriquecimento\s*$/m.test(after?.body || "") && !/^## Enriquecimento\s*$/m.test(before.body || "") ? "Enriquecimento aplicado" : "Descrição editada" });
    return out.length ? out : [{ label: "Sem mudanças visíveis" }];
  }

  let view = $derived(
    rows.map((j) => {
      const before = parse(j.before);
      const after = parse(j.after);
      const d = after?.data || before?.data || {};
      const when = new Date(j.at);
      return { j, d, when, day: ymd(when), current: byPath.get(j.path), changes: describe(before, after) };
    }),
  );

  async function undo(file: string) {
    await core.run(() => core.undo(file));
    onchanged();
    load();
  }
</script>

<div class="mesa-section-head">
  <div class="mesa-section-sub">Tudo o que a Mesa alterou nas suas notas. “Desfazer” só aparece enquanto a nota estiver exatamente como ficou depois da alteração.</div>
</div>
{#if !rows.length}
  <div class="mesa-gantt-empty">Nenhuma alteração registrada ainda.</div>
{:else}
  <div class="mesa-hist">
    {#each view as v, i (v.j.file)}
      {#if i === 0 || view[i - 1].day !== v.day}
        {@const diff = diffDays(v.day, today())}
        <div class="mesa-hist-day">{diff === 0 ? "Hoje" : diff === 1 ? "Ontem" : v.when.toLocaleDateString("pt-BR", { weekday: "long", day: "numeric", month: "long" })}</div>
      {/if}
      <div class="mesa-hist-row">
        <div class="mesa-hist-time">{v.when.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}</div>
        <div class="mesa-hist-icon"><Icon name={ICON[v.d.kind] || "file"} size={14} /></div>
        <div class="mesa-hist-main">
          {#if v.current && v.d.kind === "task" && !v.current.data.archived}
            <button class="mesa-hist-title is-link" onclick={() => onedit(v.current!)}>{v.d.title || "(sem título)"}</button>
          {:else}
            <div class="mesa-hist-title">
              {v.d.kind === "day" ? "Plano do dia " + (validDate(String(v.d.date || "")) ? fmtDate(String(v.d.date)) : "") : v.d.title || "(sem título)"}
            </div>
          {/if}
          <div class="mesa-hist-changes">
            {#each v.changes as ch}
              <div class="mesa-hist-change">
                <span class="mesa-hist-label">{ch.label}</span>
                {#if ch.from !== undefined}<span class="mesa-hist-from">{ch.from}</span>{/if}
                {#if ch.from !== undefined && ch.to !== undefined}<span class="mesa-hist-arrow">→</span>{/if}
                {#if ch.to !== undefined}<span class="mesa-hist-to">{ch.to}</span>{/if}
              </div>
            {/each}
          </div>
        </div>
        <div class="mesa-hist-side">
          {#if v.current && v.current.raw === v.j.after}
            <button class="btn-ghost sm" onclick={() => undo(v.j.file)}><Icon name="back" size={12} stroke={2} /> Desfazer</button>
          {:else}
            <span class="mesa-hist-note">{v.current ? "alterada depois" : "não está mais na Mesa"}</span>
          {/if}
        </div>
      </div>
    {/each}
  </div>
  {#if total > limit}
    <button class="btn-ghost mesa-more" onclick={() => (limit += 40)}>Mostrar mais ({total - limit} restantes)</button>
  {/if}
{/if}
