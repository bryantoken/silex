<script lang="ts">
  // Jev Organizer — tela de sugestões (reescrita do plugin Jev Organizer).
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import Icon from "../components/Icon.svelte";

  type Item = { id: string; kind: string; value: string; label: string; p: number; conf?: number | null };
  type Entry = { mtime: number; at: number; items: Item[] };
  type Progress = { running: boolean; done: number; total: number; errors: number; lastError: string };

  type Props = {
    activePath: string | null;
    high: number;
    hasKey: boolean;
    /** Abre a nota pelo caminho relativo ao cofre */
    onopen: (rel: string) => void;
    onnotify: (text: string, error?: boolean) => void;
    onchanged: () => void;
  };
  let { activePath, high, hasKey, onopen, onnotify, onchanged }: Props = $props();

  const KINDS: Record<string, string> = { move: "Mover", tags: "Tag", link: "Link", author: "Autor", date: "Data", aliases: "Alias" };
  const PAGE = 40;

  let suggestions = $state<Record<string, Entry>>({});
  let progress = $state<Progress | null>(null);
  let filter = $state("all");
  let limit = $state(PAGE);
  const sel = new SvelteSet<string>();

  const k = (path: string, it: Item) => `${path}\u0000${it.id}`;
  const pct = (p: number) => `${Math.round((p || 0) * 100)}%`;
  const folderOf = (p: string) => (p.includes("/") ? p.slice(0, p.lastIndexOf("/")) : "/");

  let entries = $derived(
    Object.entries(suggestions)
      .map(([path, e]) => [path, e.items.filter((i) => filter === "all" || (filter === "props" ? i.kind !== "move" : i.kind === filter))] as const)
      .filter(([, items]) => items.length)
      .sort((a, b) => a[0].localeCompare(b[0])),
  );
  let all = $derived(entries.flatMap(([path, items]) => items.map((it) => [path, it] as const)));
  let selectedCount = $derived(all.filter(([p, it]) => sel.has(k(p, it))).length);

  async function load() {
    const res = await invoke<{ suggestions: Record<string, Entry>; running: boolean }>("organizer_list");
    suggestions = res.suggestions;
  }

  async function scan(scope: "note" | "folder" | "vault", force = false) {
    if (!hasKey) return onnotify("Informe a API key da TypeSafe em Configurações › Jev Organizer.", true);
    let target: string | null = null;
    if (scope !== "vault") {
      if (!activePath || !activePath.endsWith(".md")) return onnotify("Abra uma nota primeiro.", true);
      target = scope === "note" ? activePath : activePath.replace(/[\\/][^\\/]*$/, "");
    }
    try {
      const r = await invoke<{ done: number; errors: number; lastError?: string; message?: string }>("organizer_scan", { scope: target, force });
      if (r.message) onnotify(r.message);
      else onnotify(`Jev: ${r.done - r.errors} nota(s) analisada(s)${r.errors ? `, ${r.errors} com erro (${r.lastError})` : ""}.`, !!r.errors);
    } catch (e) {
      onnotify(String(e), true);
    }
    await load();
  }

  function toggle(key: string, on: boolean) {
    if (on) sel.add(key);
    else sel.delete(key);
  }

  async function apply() {
    const map: Record<string, Item[]> = {};
    for (const [path, it] of all) if (sel.has(k(path, it))) (map[path] ??= []).push(it);
    sel.clear();
    try {
      const r = await invoke<{ ok: number; fail: number; lastError: string; log: string | null }>("organizer_apply", { selection: map });
      onnotify(
        `Jev: ${r.ok} ajuste(s) aplicado(s)${r.fail ? `, ${r.fail} não aplicado(s)${r.lastError ? ` (${r.lastError})` : ""}` : ""}.${r.log ? ` Registrado em ${r.log}.` : ""}`,
        !!r.fail,
      );
      onchanged();
    } catch (e) {
      onnotify(String(e), true);
    }
    await load();
  }

  async function dismiss(pairs: (readonly [string, Item])[]) {
    const map: Record<string, string[]> = {};
    for (const [path, it] of pairs) {
      (map[path] ??= []).push(it.id);
      sel.delete(k(path, it));
    }
    await invoke("organizer_dismiss", { items: map });
    await load();
  }

  onMount(() => {
    load();
    const un = listen<Progress>("organizer-progress", (e) => {
      progress = e.payload;
      if (e.payload.done % 3 === 0 || !e.payload.running) load();
    });
    return () => {
      un.then((f) => f());
    };
  });
</script>

<div class="jo">
  <div class="jo-head">
    <h2>Jev Organizer</h2>
    <div class="jo-row">
      <button class="btn-ghost" onclick={() => scan("note")}>Analisar nota atual</button>
      <button class="btn-ghost" onclick={() => scan("folder")}>Analisar pasta atual</button>
      <button class="btn-primary" onclick={() => scan("vault")}>Analisar cofre</button>
      <button class="btn-ghost" title="Ignora análises anteriores" onclick={() => scan("vault", true)}>Reanalisar tudo</button>
    </div>
  </div>
  {#if !hasKey}<div class="jo-warn">Sem API key: informe a chave da TypeSafe em Configurações › Plugins › Jev Organizer.</div>{/if}
  {#if progress?.running}
    <div class="jo-progress">
      <span>Analisando {progress.done}/{progress.total}{progress.errors ? ` · ${progress.errors} erro(s)` : ""}…</span>
      <div class="jo-bar"><div class="jo-bar-fill" style:width="{progress.total ? (progress.done / progress.total) * 100 : 0}%"></div></div>
      <button class="btn-ghost sm" onclick={() => invoke("organizer_cancel")}>Cancelar</button>
    </div>
  {:else if progress?.lastError}
    <div class="jo-warn">Último erro: {progress.lastError}</div>
  {/if}

  <div class="jo-toolbar">
    <select bind:value={filter} onchange={() => (limit = PAGE)}>
      {#each [["all", "Tudo"], ["move", "Mover"], ["props", "Propriedades"], ["tags", "Tags"], ["link", "Links"], ["author", "Autores"], ["date", "Datas"], ["aliases", "Aliases"]] as [v, label]}
        <option value={v}>{label}</option>
      {/each}
    </select>
    <button class="btn-ghost sm" onclick={() => all.forEach(([p, it]) => sel.add(k(p, it)))}>Selecionar tudo</button>
    <button class="btn-ghost sm" onclick={() => all.filter(([, it]) => it.p >= high).forEach(([p, it]) => sel.add(k(p, it)))}>
      Selecionar ≥ {pct(high)}
    </button>
    <button class="btn-ghost sm" onclick={() => sel.clear()}>Limpar seleção</button>
    <button class="btn-primary sm" disabled={!selectedCount} onclick={apply}>Aplicar selecionados ({selectedCount})</button>
    <button class="btn-ghost sm is-danger" disabled={!selectedCount} onclick={() => dismiss(all.filter(([p, it]) => sel.has(k(p, it))))}>Descartar selecionados</button>
    <span class="jo-muted">{entries.length} nota(s) · {all.length} sugestão(ões)</span>
  </div>

  {#if !entries.length}
    <div class="jo-empty">{progress?.running ? "As sugestões aparecem aqui conforme o Jev analisa." : "Nenhuma sugestão pendente. Analise uma nota, uma pasta ou o cofre."}</div>
  {:else}
    <div class="jo-list">
      {#each entries.slice(0, limit) as [path, items] (path)}
        {@const keys = items.map((it) => k(path, it))}
        {@const n = keys.filter((x) => sel.has(x)).length}
        <div class="jo-card">
          <div class="jo-card-head">
            <input
              type="checkbox"
              checked={n === keys.length}
              indeterminate={n > 0 && n < keys.length}
              onchange={(e) => {
                const on = e.currentTarget.checked;
                keys.forEach((x) => (on ? sel.add(x) : sel.delete(x)));
              }}
            />
            <button class="jo-title" onclick={() => onopen(path)}>
              {path.split("/").pop()?.replace(/\.md$/, "")}
            </button>
            <span class="jo-muted">{folderOf(path)}</span>
            <button class="icon-btn sm" title="Descartar sugestões desta nota" onclick={() => dismiss(items.map((it) => [path, it] as const))}>
              <Icon name="x" size={13} stroke={2} />
            </button>
          </div>
          {#each items as it (it.id)}
            {@const key = k(path, it)}
            <div class="jo-item">
              <input type="checkbox" checked={sel.has(key)} onchange={(e) => toggle(key, e.currentTarget.checked)} />
              <span class="jo-kind jo-kind-{it.kind}">{KINDS[it.kind] ?? it.kind}</span>
              <span class="jo-value">{it.kind === "move" ? "→ " + it.label : it.label}</span>
              <span
                class="jo-conf jo-{it.p >= high ? 'high' : it.p >= 0.6 ? 'mid' : 'low'}"
                title={typeof it.conf === "number" ? `Probabilidade ${pct(it.p)} · confiança da escolha ${pct(it.conf)}` : `Probabilidade ${pct(it.p)}`}
              >
                <span class="jo-conf-track"><span class="jo-conf-fill" style:width="{Math.round(it.p * 100)}%"></span></span>
                {pct(it.p)}
              </span>
              <button class="icon-btn sm" title="Descartar sugestão" onclick={() => dismiss([[path, it]])}><Icon name="x" size={12} stroke={2} /></button>
            </div>
          {/each}
        </div>
      {/each}
    </div>
    {#if limit < entries.length}
      <button class="btn-ghost jo-more" onclick={() => (limit += PAGE)}>Mostrar mais ({limit} de {entries.length} notas)</button>
    {/if}
  {/if}
</div>
