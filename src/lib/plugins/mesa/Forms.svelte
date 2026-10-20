<script lang="ts" module>
  import type { Rec } from "./core";
  export type FormKind = { type: "capture"; initial?: string; source?: string | null } | { type: "task"; task: Rec } | { type: "project"; project?: Rec };
</script>

<script lang="ts">
  // Formulários da Mesa: capturar tarefa, editar tarefa, criar/editar projeto.
  import { untrack } from "svelte";
  import { MONTH_RE, parseYmd, statusOf, today, validDate, validRecur, WEEKDAYS, type MesaCore, type Recur } from "./core";
  import { renderBlock } from "../../markdown/render";

  type Props = { core: MesaCore; form: FormKind; projects: Rec[]; onclose: () => void; onsaved: (created?: Rec) => void; onopenlink: (t: string) => void };
  let { core, form: formProp, projects, onclose, onsaved, onopenlink }: Props = $props();
  // O formulário começa com os valores do registro aberto (não acompanha mudanças externas)
  const form = untrack(() => formProp);

  let error = $state("");
  let saving = $state(false);

  // Captura
  let captureText = $state(form.type === "capture" ? (form.initial ?? "") : "");
  let captureProject = $state("");

  // Tarefa
  const t = form.type === "task" ? form.task.data : {};
  let title = $state(String(t.title ?? ""));
  let project = $state(String(t.project ?? ""));
  let taskState = $state(String(t.state ?? "pending"));
  let deadline = $state(String(t.deadline ?? "pending"));
  let due = $state(String(t.due ?? ""));
  let next = $state(String(t.next ?? ""));
  let blocked = $state(String(t.blocked ?? ""));
  let defer = $state(String(t.defer ?? ""));
  let start = $state(String(t.start ?? ""));
  let body = $state(form.type === "task" ? form.task.body : form.type === "project" ? (form.project?.body ?? "") : "");
  let editBody = $state(false);

  // Repetição (captura e tarefa)
  type RecurMode = "none" | "day" | "week" | "month" | "custom";
  const r0 = form.type === "task" ? validRecur(form.task.data.recur) : null;
  let recurMode = $state<RecurMode>(!r0 ? "none" : r0.every === 1 ? r0.unit : "custom");
  let recurEvery = $state(r0?.every ?? 2);
  let recurUnit = $state<Recur["unit"]>(r0?.unit ?? "week");
  let recurDays = $state<number[]>(r0?.days ?? []);
  let first = $state(today());
  let recur = $derived(
    recurMode === "none"
      ? null
      : validRecur({ unit: recurMode === "custom" ? recurUnit : recurMode, every: recurMode === "custom" ? recurEvery : 1, days: recurDays }),
  );
  $effect(() => {
    // Semanal sem dias marcados: começa no dia da semana da data da tarefa
    if (recur?.unit === "week" && !recurDays.length) {
      const base = form.type === "task" && validDate(due) ? due : first;
      recurDays = [parseYmd(validDate(base) ? base : today()).getDay()];
    }
  });

  $effect(() => {
    // Troca entre data e mês mantém o valor compatível
    if (deadline === "month" && /^\d{4}-\d{2}-\d{2}$/.test(due)) due = due.slice(0, 7);
    if (deadline === "date" && /^\d{4}-\d{2}$/.test(due)) due = due + "-01";
  });

  let badge = $derived(statusOf({ data: { ...t, state: taskState, blocked, deadline, due, defer, start } }));

  // Projeto
  const p = form.type === "project" ? form.project?.data : undefined;
  let pTitle = $state(String(p?.title ?? ""));
  let pState = $state(String(p?.state ?? "active"));
  let pRoots = $state(((p?.roots as string[]) ?? []).join("\n"));
  let pNotes = $state(((p?.notes as string[]) ?? []).join("\n"));
  let pAi = $state(p?.ai === true);

  const safe = (v: string) =>
    v
      .split("\n")
      .map((s) => s.trim().replace(/\\/g, "/").replace(/\/$/, ""))
      .filter(Boolean)
      .map((s) => {
        if (s.split("/").some((x) => !x || x === ".." || x === "." || x.startsWith("."))) throw Error("Use caminhos visíveis dentro do cofre.");
        return s;
      });

  async function save() {
    error = "";
    saving = true;
    let created: Rec | undefined;
    try {
      if (form.type === "capture") {
        created = await core.capture(captureText, captureProject || null, form.source ?? null, recur, first);
      } else if (form.type === "task") {
        if (!title.trim()) throw Error("Informe o título.");
        if (deadline === "date" && !validDate(due)) throw Error("Data inválida.");
        if (deadline === "month" && !MONTH_RE.test(due)) throw Error("Mês inválido.");
        if (defer && !validDate(defer)) throw Error("Data de adiamento inválida.");
        if (start && !validDate(start)) throw Error("Data de início inválida.");
        if (start && deadline === "date" && start > due) throw Error("O início está depois do prazo.");
        if (recur && deadline !== "date") throw Error("Tarefas que se repetem precisam de uma data de prazo.");
        const saved = await core.update(
          form.task,
          {
            title: title.trim(),
            project: project || null,
            state: taskState,
            deadline,
            due: ["date", "month"].includes(deadline) ? due : null,
            start: start || null,
            next,
            blocked,
            defer: defer || null,
            recur,
          },
          body,
        );
        if (taskState === "done" && form.task.data.state !== "done") await core.spawnNext(saved);
      } else {
        if (!pTitle.trim()) throw Error("Dê um nome ao projeto.");
        const extra = { state: pState, roots: safe(pRoots), notes: safe(pNotes), ai: pAi };
        if (form.project) await core.update(form.project, { title: pTitle, ...extra }, body);
        else await core.create("project", pTitle, extra, body);
      }
      onsaved(created);
      onclose();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
</script>

{#snippet recurFields()}
  <div class="mesa-field is-wide">
    <span>Repetir</span>
    <div class="mesa-recur">
      <select bind:value={recurMode}>
        <option value="none">Não repete</option>
        <option value="day">Diariamente</option>
        <option value="week">Semanalmente</option>
        <option value="month">Mensalmente</option>
        <option value="custom">Personalizado…</option>
      </select>
      {#if recurMode === "custom"}
        <span class="mesa-muted">a cada</span>
        <input class="sx-input mesa-recur-n" type="number" min="1" max="365" bind:value={recurEvery} />
        <select bind:value={recurUnit}>
          <option value="day">dia(s)</option>
          <option value="week">semana(s)</option>
          <option value="month">mês(es)</option>
        </select>
      {/if}
      {#if form.type === "capture" && recurMode !== "none"}
        <span class="mesa-muted">começando em</span>
        <input class="sx-input mesa-recur-date" type="date" bind:value={first} />
      {/if}
    </div>
    {#if recur?.unit === "week"}
      <div class="mesa-weekdays">
        {#each WEEKDAYS as d, i}
          <button
            type="button"
            class:is-on={recurDays.includes(i)}
            onclick={() => (recurDays = recurDays.includes(i) ? recurDays.filter((x) => x !== i) : [...recurDays, i].sort())}>{d}</button
          >
        {/each}
      </div>
    {/if}
    {#if recur}<span class="mesa-muted">Ao concluir, a próxima ocorrência é criada automaticamente.</span>{/if}
  </div>
{/snippet}

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="sx-overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="mesa-form" class:is-wide={form.type === "task"} role="dialog">
    <div class="mesa-form-title">
      {form.type === "capture" ? "Nova tarefa" : form.type === "task" ? "Detalhes da tarefa" : form.project ? "Editar projeto" : "Vincular projeto"}
      {#if form.type === "task"}<span class="mesa-status mesa-s-{badge[0]}">{badge[1]}</span>{/if}
    </div>

    {#if form.type === "capture"}
      <!-- svelte-ignore a11y_autofocus -->
      <textarea class="sx-input" rows="4" autofocus placeholder="Escreva do seu jeito. Ex.: prisma adicionar visão PJ" bind:value={captureText}></textarea>
      <label class="mesa-field">
        <span>Projeto (opcional)</span>
        <select bind:value={captureProject}>
          <option value="">A identificar</option>
          {#each projects as pr}<option value={pr.data.id}>{pr.data.title}</option>{/each}
        </select>
      </label>
      {@render recurFields()}
    {:else if form.type === "task"}
      <label class="mesa-field is-wide"><span>Título</span><input class="sx-input" bind:value={title} /></label>
      <div class="mesa-form-grid">
        <label class="mesa-field">
          <span>Projeto</span>
          <select bind:value={project}>
            <option value="">A identificar</option>
            {#each projects as pr}<option value={pr.data.id}>{pr.data.title}</option>{/each}
          </select>
        </label>
        <label class="mesa-field">
          <span>Estado</span>
          <select bind:value={taskState}>
            <option value="pending">Pendente</option>
            <option value="done">Concluída</option>
            <option value="cancelled">Cancelada</option>
          </select>
        </label>
        <label class="mesa-field">
          <span>Prazo</span>
          <select bind:value={deadline}>
            <option value="pending">A definir</option>
            <option value="none">Não precisa</option>
            <option value="date">Data definida</option>
            <option value="month">Mês definido</option>
          </select>
        </label>
        {#if deadline === "date" || deadline === "month"}
          <label class="mesa-field">
            <span>{deadline === "month" ? "Mês do prazo" : "Data do prazo"}</span>
            <input class="sx-input" type={deadline === "month" ? "month" : "date"} bind:value={due} />
          </label>
        {/if}
        <label class="mesa-field"><span>Início (calendário)</span><input class="sx-input" type="date" bind:value={start} /></label>
        <label class="mesa-field"><span>Adiar até</span><input class="sx-input" type="date" bind:value={defer} /></label>
        <label class="mesa-field is-wide"><span>Próximo passo</span><input class="sx-input" bind:value={next} /></label>
        <label class="mesa-field is-wide"><span>Bloqueio / depende de</span><input class="sx-input" bind:value={blocked} /></label>
        {@render recurFields()}
      </div>
      <div class="mesa-desc-head">
        <strong>Descrição</strong>
        <div class="sx-segmented sm">
          <button class:is-on={!editBody} onclick={() => (editBody = false)}>Visualizar</button>
          <button class:is-on={editBody} onclick={() => (editBody = true)}>Editar</button>
        </div>
      </div>
      {#if editBody}
        <textarea class="sx-input mesa-md-editor" rows="14" bind:value={body}></textarea>
      {:else}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="mesa-md mesa-md-view"
          title="Duplo clique para editar"
          ondblclick={() => (editBody = true)}
          onclick={(e) => {
            const a = (e.target as HTMLElement).closest<HTMLElement>("a.sx-wikilink");
            if (a?.dataset.target) {
              e.preventDefault();
              onclose();
              onopenlink(a.dataset.target);
            }
          }}
        >
          {#if body.trim()}{@html renderBlock(body)}{:else}<p class="mesa-muted">Sem descrição. Clique em Editar.</p>{/if}
        </div>
      {/if}
    {:else}
      <label class="mesa-field is-wide"><span>Nome</span><input class="sx-input" bind:value={pTitle} /></label>
      <label class="mesa-field">
        <span>Situação</span>
        <select bind:value={pState}>
          <option value="active">Ativo</option>
          <option value="paused">Pausado</option>
          <option value="archived">Arquivado</option>
        </select>
      </label>
      <label class="mesa-field is-wide"><span>Pastas existentes, uma por linha (só referência; não são movidas)</span><textarea class="sx-input" rows="3" bind:value={pRoots}></textarea></label>
      <label class="mesa-field is-wide"><span>Notas de contexto para a IA (caminho .md por linha)</span><textarea class="sx-input" rows="3" bind:value={pNotes}></textarea></label>
      <label class="sx-check"><input type="checkbox" bind:checked={pAi} /> Permitir envio destas notas à IA</label>
      <label class="mesa-field is-wide"><span>Objetivo, resultado esperado e onde você parou</span><textarea class="sx-input" rows="6" bind:value={body}></textarea></label>
    {/if}

    {#if error}<div class="mesa-warn">{error}</div>{/if}
    <div class="mesa-form-actions">
      <button class="btn-ghost" onclick={onclose}>Cancelar</button>
      <button class="btn-primary" disabled={saving} onclick={save}>{saving ? "Salvando…" : "Salvar"}</button>
    </div>
  </div>
</div>
