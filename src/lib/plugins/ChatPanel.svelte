<script lang="ts" module>
  export type ChatSettings = {
    provider: string;
    anthropicModel: string;
    openaiModel: string;
    geminiModel: string;
    claudeModel: string;
    codexModel: string;
    includeNote: boolean;
    showThinking: boolean;
  };
</script>

<script lang="ts">
  /*
   * Chat de IA (plugin nativo). O trabalho pesado fica no Rust (chat.rs) e o
   * estado em chat.svelte.ts: este componente só desenha. Fechar não para uma
   * resposta em andamento.
   *
   * Dois modos: janela flutuante ("popup") e painel lateral direito ("side").
   */
  import { tick } from "svelte";
  import Icon from "../components/Icon.svelte";
  import { renderBlock } from "../markdown/render";
  import { popupMenu } from "../menu";
  import { fuzzy } from "../fuzzy";
  import { openFiles } from "../dialogs";
  import type { LinkTarget } from "../editor/context";
  import { chatStore as store, type Msg, type Permission, type Summary } from "./chat.svelte";

  type Props = {
    mode: "popup" | "side";
    settings: ChatSettings;
    notePath: string | null;
    noteTitle: string | null;
    notes: readonly LinkTarget[];
    onmode: (m: "popup" | "side") => void;
    onclose: () => void;
    onopenlink: (target: string) => void;
    onopenpath: (rel: string) => void;
    onopenurl: (url: string) => void;
    onerror: (e: unknown) => void;
  };
  let { mode, settings, notePath, noteTitle, notes, onmode, onclose, onopenlink, onopenpath, onopenurl, onerror }: Props = $props();

  const PROVIDERS: [string, string][] = [
    ["anthropic", "Anthropic"],
    ["openai", "OpenAI"],
    ["gemini", "Gemini"],
    ["claude-code", "Claude Code"],
    ["codex", "Codex"],
  ];
  const PERMS: [Permission, string, string][] = [
    ["read", "Só leitura", "Lê o cofre; não altera nada"],
    ["edit", "Editar", "Cria e edita arquivos no cofre"],
    ["full", "Total", "Também roda comandos, sem pedir"],
  ];
  const providerName = (p: string) => PROVIDERS.find((x) => x[0] === p)?.[1] ?? p;
  const isCli = (p: string) => p === "claude-code" || p === "codex";
  const defaultModel = (p: string) =>
    ({ anthropic: settings.anthropicModel, openai: settings.openaiModel, gemini: settings.geminiModel, "claude-code": settings.claudeModel, codex: settings.codexModel })[p] ?? "";

  let showList = $state(false);
  let draft = $state("");
  let withNote = $state(true);
  let renaming = $state<string | null>(null);
  let dragging = $state(false);
  let scroller = $state<HTMLDivElement>();
  let inputEl = $state<HTMLTextAreaElement>();

  let chat = $derived(store.chat);
  let provider = $derived(chat?.provider ?? settings.provider ?? "anthropic");
  let live = $derived(chat ? store.live[chat.id] : undefined);
  let error = $derived(chat ? store.errors[chat.id] : undefined);

  $effect(() => {
    withNote = settings.includeNote !== false;
  });

  $effect(() => {
    store.init().then(scrollDown).catch(onerror);
    tick().then(() => inputEl?.focus());
  });

  async function scrollDown() {
    await tick();
    if (scroller) scroller.scrollTop = scroller.scrollHeight;
  }

  const run = (p: Promise<unknown>) => p.catch(onerror);

  async function openChat(id: string) {
    await run(store.open(id));
    showList = false;
    await scrollDown();
    inputEl?.focus();
  }

  async function newChat(p = provider) {
    await run(store.create(p, defaultModel(p)));
    showList = false;
    inputEl?.focus();
  }

  // Resposta em andamento: o Markdown é re-renderizado no máximo uma vez por quadro
  let liveHtml = $state("");
  let frame = 0;
  $effect(() => {
    const t = live?.text ?? "";
    live?.thinking;
    live?.steps.length;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      liveHtml = t ? renderBlock(t) : "";
      if (scroller && scroller.scrollHeight - scroller.scrollTop - scroller.clientHeight < 140) scroller.scrollTop = scroller.scrollHeight;
    });
  });

  async function send(text = draft, permission?: Permission) {
    if (live || (!text.trim() && !store.pending.length)) return;
    if (text === draft) draft = "";
    if (inputEl) inputEl.style.height = "auto";
    const p = store.send(text, withNote ? notePath : null, permission ?? store.permission, provider, defaultModel(provider));
    await scrollDown();
    await p;
    await scrollDown();
  }

  /** O CLI quis fazer algo que a permissão atual não deixava: libera e continua. */
  function grant(p: Permission) {
    store.setPermission(p);
    send("Permissão concedida. Continue de onde parou e conclua a tarefa.", p);
  }

  // ---------- [[ notas ]] no campo de texto ----------

  let refQuery = $state<string | null>(null);
  let refIndex = $state(0);
  let refs = $derived.by(() => {
    if (refQuery === null) return [];
    if (!refQuery) return notes.slice(0, 8);
    const out: { t: LinkTarget; s: number }[] = [];
    for (const t of notes) {
      const m = fuzzy(refQuery, t.label);
      if (m) out.push({ t, s: m.score });
    }
    return out
      .sort((a, b) => b.s - a.s)
      .slice(0, 8)
      .map((x) => x.t);
  });

  function updateRef() {
    const el = inputEl;
    if (!el) return;
    const before = el.value.slice(0, el.selectionStart);
    const m = /\[\[([^[\]\n|#]*)$/.exec(before);
    refQuery = m ? m[1] : null;
    refIndex = 0;
  }

  function pickRef(t: LinkTarget) {
    const el = inputEl!;
    const pos = el.selectionStart;
    const before = draft.slice(0, pos).replace(/\[\[([^[\]\n|#]*)$/, "");
    const after = draft.slice(pos).replace(/^\]\]/, "");
    const insert = `[[${t.link}]]`;
    draft = before + insert + after;
    refQuery = null;
    tick().then(() => {
      el.focus();
      el.selectionStart = el.selectionEnd = before.length + insert.length;
    });
  }

  function onkey(e: KeyboardEvent) {
    if (refQuery !== null && refs.length) {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        refIndex = (refIndex + (e.key === "ArrowDown" ? 1 : -1) + refs.length) % refs.length;
        return;
      }
      if (e.key === "Enter" || e.key === "Tab") {
        e.preventDefault();
        pickRef(refs[refIndex]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        refQuery = null;
        return;
      }
    }
    if (e.key === "Enter" && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      send();
    }
  }

  function grow(el: HTMLTextAreaElement) {
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }

  // ---------- Anexos ----------

  async function attachFiles(files: File[]) {
    for (const f of files) await run(store.attachBytes(f));
  }

  function onpaste(e: ClipboardEvent) {
    const files = [...(e.clipboardData?.files ?? [])];
    if (!files.length) return;
    e.preventDefault();
    attachFiles(files);
  }

  function ondrop(e: DragEvent) {
    dragging = false;
    const files = [...(e.dataTransfer?.files ?? [])];
    if (files.length) {
      e.preventDefault();
      attachFiles(files);
      return;
    }
    // Arquivo arrastado da árvore do Silex chega como [[link]]
    const text = e.dataTransfer?.getData("text/plain");
    if (text) {
      e.preventDefault();
      draft = `${draft}${draft && !draft.endsWith(" ") ? " " : ""}${text} `;
      inputEl?.focus();
    }
  }

  async function pickFiles() {
    const paths = await openFiles("Anexar ao chat").catch(() => []);
    await run(store.attachPaths(paths));
    inputEl?.focus();
  }

  const KIND_ICON: Record<string, string> = { image: "image", pdf: "pdf", sheet: "sheet", doc: "file", text: "file" };
  const kindOf = (path: string) => {
    const ext = path.split(".").pop()?.toLowerCase() ?? "";
    if (/^(png|jpe?g|gif|webp)$/.test(ext)) return "image";
    if (ext === "pdf") return "pdf";
    if (/^(xlsx|xlsm|xls|xlsb|ods|csv)$/.test(ext)) return "sheet";
    return "file";
  };
  const baseName = (p: string) => p.split("/").pop() ?? p;

  // ---------- Diversos ----------

  /** Links das respostas: [[notas]] abrem no Silex; URLs, no navegador. */
  function onclick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    if (a.dataset.target) onopenlink(a.dataset.target);
    else if (a.getAttribute("href")) onopenurl(a.getAttribute("href")!);
  }

  function itemMenu(e: MouseEvent, s: Summary) {
    e.preventDefault();
    e.stopPropagation();
    popupMenu([
      { text: "Renomear", action: () => (renaming = s.id) },
      { text: "Excluir", action: () => run(store.remove(s.id)) },
    ]);
  }

  function headerMenu() {
    const c = chat;
    if (!c) return;
    popupMenu([
      { text: "Nova conversa", action: () => newChat(c.provider) },
      { text: "Renomear", action: () => ((showList = true), (renaming = c.id)) },
      { text: "Copiar conversa", action: () => navigator.clipboard.writeText(c.messages.map((m) => `**${m.role === "user" ? "Você" : "IA"}:** ${m.content}`).join("\n\n")) },
      "separator",
      { text: "Excluir conversa", action: () => run(store.remove(c.id)) },
    ]);
  }

  function permMenu(e: MouseEvent) {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    popupMenu(
      PERMS.map(([id, label, desc]) => ({ text: `${label} — ${desc}`, checked: store.permission === id, action: () => store.setPermission(id) })),
      { x: r.left, y: r.top - 6, above: true },
    );
  }

  const when = (t: number) => {
    const d = new Date(t);
    return d.toDateString() === new Date().toDateString()
      ? d.toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })
      : d.toLocaleDateString("pt-BR", { day: "2-digit", month: "short" });
  };
  const k = (n: number) => (n >= 1000 ? `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k` : String(n));
  const usageText = (m: Msg) =>
    m.usage
      ? `↑ ${k(m.usage.input)} · ↓ ${k(m.usage.output)} tokens${m.usage.cost ? ` · US$ ${m.usage.cost.toFixed(m.usage.cost < 0.01 ? 4 : 2)}` : ""}`
      : "";

  function autofocus(node: HTMLInputElement) {
    requestAnimationFrame(() => (node.focus(), node.select()));
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && mode === "popup" && !showList && refQuery === null && document.activeElement === inputEl && onclose()} />

{#snippet thinking(text: string, open: boolean)}
  <details class="sx-think" {open}>
    <summary><Icon name="zap" size={12} stroke={2} /> Raciocínio</summary>
    <div class="sx-think-body">{text}</div>
  </details>
{/snippet}

{#snippet steps(list: string[])}
  <div class="sx-steps">
    {#each list as s}<div class="sx-step"><Icon name="chevron" size={11} stroke={2.4} /><span>{s}</span></div>{/each}
  </div>
{/snippet}

<div
  class="sx-chat"
  class:is-popup={mode === "popup"}
  class:is-side={mode === "side"}
  class:is-dragging={dragging}
  role="dialog"
  tabindex="-1"
  aria-label="Chat"
  ondragover={(e) => {
    e.preventDefault();
    dragging = true;
  }}
  ondragleave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && (dragging = false)}
  {ondrop}
>
  <header class="sx-chat-head">
    <button class="sx-chat-title" title="Conversas" onclick={() => ((showList = !showList), store.refresh())}>
      <span class="label">{chat?.title ?? "Nova conversa"}</span>
      <Icon name="chevron" size={12} stroke={2.2} />
    </button>
    <div class="sx-chat-tools">
      <button class="icon-btn sm" title="Nova conversa" onclick={() => newChat()}><Icon name="plus" size={15} stroke={2} /></button>
      {#if chat}
        <button class="icon-btn sm" title="Mais opções" onclick={headerMenu}><Icon name="more" size={15} stroke={3} /></button>
      {/if}
      <button class="icon-btn sm" title={mode === "popup" ? "Abrir no painel lateral" : "Janela flutuante"} onclick={() => onmode(mode === "popup" ? "side" : "popup")}>
        <Icon name={mode === "popup" ? "panel-right" : "popup"} size={15} />
      </button>
      <button class="icon-btn sm" title="Ocultar (Ctrl+J) — respostas continuam em segundo plano" onclick={onclose}><Icon name="x" size={15} stroke={2} /></button>
    </div>
  </header>

  {#if showList}
    <div class="sx-chat-list">
      <button class="sx-chat-new" onclick={() => newChat()}><Icon name="plus" size={14} stroke={2} /> Nova conversa</button>
      {#each store.list as s (s.id)}
        {#if renaming === s.id}
          <input
            class="sx-input sx-chat-rename"
            value={s.title}
            use:autofocus
            onkeydown={(e) => {
              if (e.key === "Enter") (renaming = null), run(store.rename(s.id, e.currentTarget.value));
              if (e.key === "Escape") renaming = null;
              e.stopPropagation();
            }}
            onblur={(e) => renaming === s.id && ((renaming = null), run(store.rename(s.id, e.currentTarget.value)))}
          />
        {:else}
          <div class="sx-chat-item" class:is-active={chat?.id === s.id}>
            <button class="sx-chat-open" onclick={() => openChat(s.id)} ondblclick={() => (renaming = s.id)} oncontextmenu={(e) => itemMenu(e, s)}>
              <span class="t">{#if store.live[s.id]}<i class="sx-chat-busy"></i>{/if}{s.title}</span>
              <span class="m">{providerName(s.provider)} · {when(s.updated)}</span>
            </button>
            <button class="icon-btn sm" title="Renomear" onclick={() => (renaming = s.id)}><Icon name="pencil" size={13} /></button>
            <button class="icon-btn sm" title="Excluir" onclick={() => run(store.remove(s.id))}><Icon name="trash" size={13} /></button>
          </div>
        {/if}
      {:else}
        <p class="sx-chat-empty">Nenhuma conversa ainda.</p>
      {/each}
    </div>
  {:else}
    <div class="sx-chat-body" bind:this={scroller} role="presentation" {onclick}>
      {#if !chat?.messages.length && !live}
        <div class="sx-chat-hello">
          <div class="h">Como posso ajudar?</div>
          <p>{providerName(provider)}{noteTitle && withNote ? ` · com a nota “${noteTitle}”` : ""}</p>
          <p class="tip">Digite <code>[[</code> para citar notas · cole ou arraste arquivos</p>
        </div>
      {/if}
      {#each chat?.messages ?? [] as m, i}
        {#if m.role === "user"}
          <div class="sx-msg is-user">
            {#if m.attachments?.length}
              <div class="sx-attach-row">
                {#each m.attachments as a}
                  <button class="sx-attach" title={a} onclick={() => onopenpath(a)}><Icon name={kindOf(a)} size={12} /><span>{baseName(a)}</span></button>
                {/each}
              </div>
            {/if}
            {#if m.content}<div>{m.content}</div>{/if}
          </div>
        {:else}
          <div class="sx-msg is-ai">
            {#if m.thinking && settings.showThinking !== false}{@render thinking(m.thinking, false)}{/if}
            {#if m.steps?.length}{@render steps(m.steps)}{/if}
            <div class="sx-md">{@html renderBlock(m.content)}</div>
            {#if m.denied?.length && i === (chat?.messages.length ?? 0) - 1 && !live}
              <div class="sx-grant">
                <div class="q"><Icon name="key" size={13} /> Precisa de permissão para:</div>
                {#each m.denied as d}<div class="d">{d}</div>{/each}
                <div class="a">
                  <button class="btn-primary" onclick={() => grant("edit")}>Permitir edição e continuar</button>
                  <button class="btn-ghost" onclick={() => grant("full")}>Permitir tudo</button>
                </div>
              </div>
            {/if}
            {#if m.usage}<div class="sx-usage">{usageText(m)}</div>{/if}
          </div>
        {/if}
      {/each}
      {#if live}
        <div class="sx-msg is-ai">
          {#if live.thinking && settings.showThinking !== false}{@render thinking(live.thinking, !live.text)}{/if}
          {#if live.steps.length}{@render steps(live.steps)}{/if}
          {#if liveHtml}<div class="sx-md">{@html liveHtml}</div>{:else}<span class="sx-chat-dots"><i></i><i></i><i></i></span>{/if}
        </div>
      {/if}
      {#if error}
        <div class="sx-chat-error">{error}</div>
      {/if}
    </div>

    <footer class="sx-chat-foot">
      {#if refQuery !== null && refs.length}
        <div class="sx-refs" role="listbox">
          {#each refs as t, i}
            <button
              class="sx-ref"
              class:is-active={i === refIndex}
              role="option"
              aria-selected={i === refIndex}
              onmousedown={(e) => e.preventDefault()}
              onmousemove={() => (refIndex = i)}
              onclick={() => pickRef(t)}
            >
              <span class="t">{t.label}</span>{#if t.detail}<span class="d">{t.detail}</span>{/if}
            </button>
          {/each}
        </div>
      {/if}
      <div class="sx-chat-box">
        {#if store.pending.length}
          <div class="sx-attach-row">
            {#each store.pending as a, i}
              <span class="sx-attach is-pending" title={a.path}>
                <Icon name={KIND_ICON[a.kind] ?? "file"} size={12} /><span>{a.name}</span>
                <button title="Remover" onclick={() => store.pending.splice(i, 1)}><Icon name="x" size={11} stroke={2.4} /></button>
              </span>
            {/each}
          </div>
        {/if}
        <div class="sx-chat-input">
          <button class="icon-btn sm" title="Anexar arquivos (ou cole/arraste)" onclick={pickFiles}><Icon name="plus" size={16} stroke={2} /></button>
          <textarea
            bind:this={inputEl}
            bind:value={draft}
            rows="1"
            placeholder="Pergunte qualquer coisa… ([[ cita uma nota)"
            onkeydown={onkey}
            oninput={(e) => (grow(e.currentTarget), updateRef())}
            onclick={updateRef}
            {onpaste}
            spellcheck="false"
          ></textarea>
          {#if live && chat}
            <button class="sx-chat-send" title="Parar" onclick={() => store.stop(chat.id)}><Icon name="stop" size={14} stroke={2.4} /></button>
          {:else}
            <button class="sx-chat-send" title="Enviar (Enter)" disabled={!draft.trim() && !store.pending.length} onclick={() => send()}><Icon name="send" size={15} stroke={2.4} /></button>
          {/if}
        </div>
      </div>
      <div class="sx-chat-opts">
        <select
          value={provider}
          disabled={!!live}
          onchange={(e) => {
            const p = e.currentTarget.value;
            chat ? run(store.setProvider(p, defaultModel(p))) : newChat(p);
          }}
        >
          {#each PROVIDERS as [id, label]}<option value={id}>{label}</option>{/each}
        </select>
        {#if isCli(provider)}
          <button class="sx-chat-perm" class:is-open={store.permission !== "read"} title="Permissões do CLI no cofre" onclick={permMenu}>
            <Icon name="key" size={11} /> {PERMS.find((p) => p[0] === store.permission)?.[1]}
          </button>
        {/if}
        {#if notePath}
          <label class="sx-chat-note" title="Enviar a nota aberta como contexto">
            <input type="checkbox" bind:checked={withNote} />
            <span>{noteTitle}</span>
          </label>
        {/if}
      </div>
    </footer>
  {/if}
</div>
