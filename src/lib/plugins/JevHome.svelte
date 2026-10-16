<script lang="ts" module>
  export type JevDoc = { abs: string; path: string; title: string; folder: string; p?: number };
  export type JevResult = {
    query: string;
    items: JevDoc[];
    none: number;
    confidence?: number;
    compared: number;
    total: number;
    broad: boolean;
    calls: number;
    ms: number;
    cached?: boolean;
    snippet?: { segments: { text: string; hl: boolean }[]; pre: boolean; post: boolean } | null;
  };
</script>

<script lang="ts">
  /*
   * Jev Home — página inicial (reescrita do plugin Jev Home).
   * Digitação → busca local instantânea em Rust; Enter → o Jev (TypeSafe) compara
   * as candidatas e diz qual é a nota, com a probabilidade.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount, tick } from "svelte";
  import Icon from "../components/Icon.svelte";

  type Settings = { autoAsk: boolean; autoDelay: number; maxSimilar: number; minSimilar: number; high: number };
  type Props = {
    vaultName: string;
    settings: Settings;
    recent: { abs: string; title: string; folder: string }[];
    hasKey: boolean;
    onopen: (path: string, newTab: boolean) => void;
    onsettings: () => void;
  };
  let { vaultName, settings, recent, hasKey, onopen, onsettings }: Props = $props();

  let query = $state("");
  let mode = $state<"idle" | "local" | "asking" | "jev" | "error">("idle");
  let local = $state<JevDoc[]>([]);
  let result = $state<JevResult | null>(null);
  let error = $state("");
  let sel = $state(-1);
  let input = $state<HTMLInputElement>();
  let seq = 0;
  let localTimer: ReturnType<typeof setTimeout> | undefined;
  let autoTimer: ReturnType<typeof setTimeout> | undefined;
  let elapsed = $state(0);
  let ticker: ReturnType<typeof setInterval> | undefined;

  const pct = (p: number) => {
    const n = Math.round((p || 0) * 100);
    return n === 0 && p > 0 ? "<1%" : `${n}%`;
  };
  const level = (p: number) => (p >= settings.high ? "high" : p >= 0.5 ? "mid" : "low");
  const fold = (s: string) => s.normalize("NFD").replace(/\p{M}+/gu, "").toLowerCase().replace(/\s+/g, " ").trim();

  let items = $derived.by((): JevDoc[] => {
    if (!query.trim()) return recent.map((r) => ({ ...r, path: r.abs }));
    if (mode === "jev" && result) return [result.items[0], ...similar].filter(Boolean);
    return local;
  });
  let similar = $derived(
    result ? result.items.slice(1).filter((x) => (x.p ?? 0) >= settings.minSimilar).slice(0, settings.maxSimilar) : [],
  );

  async function runLocal() {
    const q = query.trim();
    if (!q) return;
    const res = await invoke<JevDoc[]>("jev_home_local", { query: q, limit: 8 }).catch(() => []);
    if (query.trim() === q && mode !== "jev" && mode !== "asking") {
      local = res;
      mode = "local";
    }
  }

  function onInput() {
    sel = -1;
    clearTimeout(localTimer);
    clearTimeout(autoTimer);
    const q = query.trim();
    if (mode === "asking") seq++;
    if (!q) {
      mode = "idle";
      local = [];
      result = null;
      return;
    }
    if (mode === "jev" && result && fold(result.query) === fold(q)) return;
    mode = "local";
    localTimer = setTimeout(runLocal, 90);
    if (settings.autoAsk && hasKey && q.length >= 3) autoTimer = setTimeout(() => ask(), Math.max(300, settings.autoDelay));
  }

  async function ask(forceBroad = false) {
    const q = query.trim();
    if (!q) return input?.focus();
    clearTimeout(autoTimer);
    const my = ++seq;
    mode = "asking";
    error = "";
    sel = -1;
    elapsed = 0;
    const t0 = performance.now();
    clearInterval(ticker);
    ticker = setInterval(() => (elapsed = (performance.now() - t0) / 1000), 100);
    try {
      const res = await invoke<JevResult>("jev_home_find", { query: q, forceBroad, recent: recent.map((r) => r.abs) });
      if (my !== seq) return;
      result = res;
      mode = "jev";
    } catch (e) {
      if (my !== seq) return;
      error = String(e);
      mode = "error";
    } finally {
      clearInterval(ticker);
    }
  }

  function open(doc: JevDoc | undefined, e?: KeyboardEvent | MouseEvent) {
    if (!doc) return;
    onopen(doc.abs, !!(e && (e.ctrlKey || e.metaKey)));
  }

  function onKey(e: KeyboardEvent) {
    if (e.isComposing) return;
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      if (!items.length) return;
      e.preventDefault();
      const n = items.length;
      sel = e.key === "ArrowDown" ? (sel + 1) % n : sel <= 0 ? n - 1 : sel - 1;
      tick().then(() => document.querySelector(".jh-row.is-selected, .jh-top.is-selected")?.scrollIntoView({ block: "nearest" }));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (sel >= 0 && items[sel]) return open(items[sel], e);
      if (!query.trim()) return;
      if (mode === "jev" && result && fold(result.query) === fold(query) && result.items[0]) return open(result.items[0], e);
      if (mode !== "asking") ask();
    } else if (e.key === "Escape" && query) {
      e.preventDefault();
      query = "";
      onInput();
    }
  }

  onMount(() => {
    tick().then(() => input?.focus());
    return () => {
      clearInterval(ticker);
      clearTimeout(localTimer);
      clearTimeout(autoTimer);
    };
  });
</script>

{#snippet row(doc: JevDoc, i: number, prob?: number)}
  <button
    class="jh-row"
    class:is-selected={sel === i}
    onclick={(e) => open(doc, e)}
    onauxclick={(e) => e.button === 1 && onopen(doc.abs, true)}
  >
    <span class="jh-row-main">
      <span class="jh-row-title">{doc.title}</span>
      {#if doc.folder && doc.folder !== "/"}<span class="jh-row-folder">{doc.folder}</span>{/if}
    </span>
    {#if prob !== undefined}
      <span class="jh-meter jh-{level(prob)}" aria-label="{pct(prob)} de probabilidade">
        <span class="jh-meter-track"><span class="jh-meter-fill" style:width="{Math.min(100, prob * 100)}%"></span></span>
        <span class="jh-meter-pct">{pct(prob)}</span>
      </span>
    {/if}
  </button>
{/snippet}

<div class="jh" class:has-query={!!query.trim()}>
  <div class="jh-stage">
    <div class="jh-vault">{vaultName}</div>
    <div class="jh-bar">
      <Icon name="search" size={18} />
      <input
        bind:this={input}
        bind:value={query}
        oninput={onInput}
        onkeydown={onKey}
        placeholder="Descreva a nota que você procura"
        aria-label="Descreva a nota que você procura"
        spellcheck="false"
        autocomplete="off"
      />
      {#if query}
        <button class="icon-btn sm" aria-label="Limpar busca" onclick={() => ((query = ""), onInput(), input?.focus())}>
          <Icon name="x" size={14} stroke={2} />
        </button>
      {/if}
      <button class="btn-primary" disabled={!query.trim() || mode === "asking"} onclick={() => ask()}>Perguntar</button>
    </div>
    <div class="jh-hint">
      {#if !query.trim()}
        Escreva do seu jeito: parte do título, o assunto ou um detalhe de que você lembra. Enter pergunta ao Jev.
      {:else if mode === "jev"}
        Enter abre a primeira. Setas escolhem outra. Ctrl+Enter abre em nova aba.
      {:else if mode !== "asking"}
        Enter pergunta ao Jev qual é a nota.
      {/if}
    </div>

    <div class="jh-results" role="listbox" aria-label="Notas encontradas">
      {#if !query.trim()}
        {#if !hasKey}
          <div class="jh-notice">
            Para o Jev responder, adicione a API key da TypeSafe nas configurações.
            <button class="sx-link-btn" onclick={onsettings}>Abrir configurações</button>
          </div>
        {/if}
        {#if recent.length}
          <div class="jh-section">Abertas recentemente</div>
          {#each recent as r, i (r.abs)}{@render row({ ...r, path: r.abs }, i)}{/each}
        {/if}
      {:else if mode === "jev" && result}
        {@const top = result.items[0]}
        {#if !top}
          <div class="jh-muted">O Jev não devolveu nenhuma nota.</div>
        {:else}
          {@const shown = [top, ...similar]}
          {@const rest = Math.max(0, 1 - result.none - shown.reduce((t, x) => t + (x.p ?? 0), 0))}
          <div class="jh-strip" role="img" aria-label="Divisão da probabilidade">
            {#each shown as x, k}
              <div class="jh-seg {k === 0 ? `jh-seg-top jh-${level(x.p ?? 0)}` : 'jh-seg-alt'}" style:flex-grow={x.p ?? 0} title="{x.title}: {pct(x.p ?? 0)}"></div>
            {/each}
            {#if rest >= 0.005}<div class="jh-seg jh-seg-rest" style:flex-grow={rest} title="Outras notas comparadas: {pct(rest)}"></div>{/if}
            {#if result.none >= 0.005}<div class="jh-seg jh-seg-none" style:flex-grow={result.none} title="Nenhuma destas: {pct(result.none)}"></div>{/if}
          </div>
          {#if result.none > (top.p ?? 0)}
            <div class="jh-verdict">
              O Jev acha mais provável que a nota não esteja entre as {result.compared} comparadas ({pct(result.none)}). Melhor palpite:
            </div>
          {/if}
          <button class="jh-top jh-{level(top.p ?? 0)}" class:is-selected={sel === 0} onclick={(e) => open(top, e)}>
            <span class="jh-top-main">
              <span class="jh-top-title">{top.title}</span>
              {#if top.folder !== "/"}<span class="jh-folder">{top.folder}</span>{/if}
            </span>
            <span class="jh-score" title={typeof result.confidence === "number" ? `Confiança do Jev na escolha: ${pct(result.confidence)}` : ""}>
              <span class="jh-score-pct">{pct(top.p ?? 0)}</span>
              <span class="jh-score-label">de chance de ser esta</span>
            </span>
            {#if result.snippet}
              <span class="jh-snippet">
                {#if result.snippet.pre}…{/if}{#each result.snippet.segments as s}{#if s.hl}<mark>{s.text}</mark>{:else}{s.text}{/if}{/each}{#if result.snippet.post}…{/if}
              </span>
            {/if}
          </button>
          {#if similar.length}
            <div class="jh-section">Parecidas</div>
            {#each similar as x, i (x.abs)}{@render row(x, i + 1, x.p ?? 0)}{/each}
          {/if}
          {#if result.none >= 0.01}
            <div class="jh-row jh-none-row">
              <span class="jh-row-main"><span class="jh-row-title">Nenhuma destas</span></span>
              <span class="jh-meter jh-none">
                <span class="jh-meter-track"><span class="jh-meter-fill" style:width="{result.none * 100}%"></span></span>
                <span class="jh-meter-pct">{pct(result.none)}</span>
              </span>
            </div>
          {/if}
          <div class="jh-foot">
            O Jev comparou {result.compared} de {result.total} notas{result.broad ? " depois de ler todos os títulos" : ""}, em
            {(result.ms / 1000).toFixed(1).replace(".", ",")} s{result.cached ? " (resposta já guardada)" : ""}.
            {#if !result.broad && result.total > result.compared}
              <button class="sx-link-btn" onclick={() => ask(true)}>Procurar em todas as notas</button>
            {/if}
          </div>
        {/if}
      {:else}
        {#if mode === "error"}
          <div class="jh-error">
            {error}
            <div class="jh-actions">
              <button class="sx-link-btn" onclick={() => ask()}>Tentar de novo</button>
              {#if /API key|401/.test(error)}<button class="sx-link-btn" onclick={onsettings}>Abrir configurações</button>{/if}
            </div>
          </div>
        {/if}
        {#if mode === "asking"}
          <div class="jh-asking"><div class="jh-line"></div><div class="jh-muted">Comparando notas… {elapsed.toFixed(1).replace(".", ",")} s</div></div>
        {/if}
        <div class:jh-dim={mode === "asking"}>
          {#if local.length}
            <div class="jh-section">Com essas palavras</div>
            {#each local as d, i (d.abs)}{@render row(d, i)}{/each}
          {:else if mode !== "asking"}
            <div class="jh-muted">Nenhuma nota tem essas palavras. O Jev ainda pode achar pelo sentido: aperte Enter.</div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>
