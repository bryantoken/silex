<script lang="ts">
  /*
   * Estrutura da janela:
   *
   *   .sx-app
   *   ├─ .sx-rail       trilho de ações
   *   ├─ .sx-sidebar    painéis Arquivos / Busca
   *   ├─ .sx-splitter   redimensiona a barra lateral
   *   └─ .sx-main
   *      ├─ .sx-tabbar  abas (cada uma com histórico próprio)
   *      └─ .sx-canvas  view ativa: nota, grafo, imagem, PDF, planilha...
   *
   * Sobreposições: seletor de cofres, paleta (Ctrl+P / Ctrl+O), configurações.
   */
  import "./styles/silex.css";
  import "./styles/components.css";
  import "./styles/plugins.css";
  import { invoke, convertFileSrc } from "@tauri-apps/api/core";
  import { openFile as openDialog } from "./lib/dialogs";
  import { openUrl, openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { SvelteSet } from "svelte/reactivity";
  import { onMount, tick } from "svelte";
  import type { EditorView } from "@codemirror/view";
  import Editor, { resetEditorStates, renameEditorState } from "./lib/editor/Editor.svelte";
  import type { EditorHost, BlockResult } from "./lib/editor/context";
  import * as ed from "./lib/editor/commands";
  import { TASK_STATUSES } from "./lib/editor/tasks";
  import { CALLOUT_TYPES } from "./lib/editor/callouts";
  import { popupMenu, type MenuEntry } from "./lib/menu";
  import { applyAppearance } from "./lib/appearance";
  import { fileKind, KIND_ICON } from "./lib/viewers/kinds";
  import Icon from "./lib/components/Icon.svelte";
  import Logo from "./lib/components/Logo.svelte";
  import Palette, { type PaletteItem } from "./lib/components/Palette.svelte";
  import Settings, {
    type VaultConfig,
    type PluginInfo,
    type Appearance,
    type ImportReport,
  } from "./lib/components/Settings.svelte";
  import VaultPicker, { type SavedVault } from "./lib/components/VaultPicker.svelte";
  import GraphView, { type GraphNote } from "./lib/components/GraphView.svelte";
  import MediaView from "./lib/viewers/MediaView.svelte";
  import PdfView from "./lib/viewers/PdfView.svelte";
  import SheetView from "./lib/viewers/SheetView.svelte";
  import CanvasView from "./lib/viewers/CanvasView.svelte";
  import ExcalidrawView from "./lib/viewers/ExcalidrawView.svelte";
  import { emptyDrawing } from "./lib/viewers/excalidrawFormat";
  import PromptModal, { type TplPrompt } from "./lib/components/PromptModal.svelte";
  import FormatDock from "./lib/components/FormatDock.svelte";
  import ExportModal from "./lib/components/ExportModal.svelte";
  import ChatPanel, { type ChatSettings } from "./lib/plugins/ChatPanel.svelte";
  import { chatStore } from "./lib/plugins/chat.svelte";
  import type { LinkTarget } from "./lib/editor/context";
  import JevHome from "./lib/plugins/JevHome.svelte";
  import Organizer from "./lib/plugins/Organizer.svelte";
  import Mesa from "./lib/plugins/mesa/Mesa.svelte";
  import type { Settings as MesaSettings } from "./lib/plugins/mesa/core";
  import { enabledCore, pluginSettings } from "./lib/plugins/registry";

  type FolderNode = { kind: "folder"; name: string; path: string; children: TreeNode[] };
  type FileNode = { kind: "file"; name: string; path: string; extension: string };
  type TreeNode = FolderNode | FileNode;
  type SearchHit = { path: string; title: string; score: number; snippet: { text: string; hl: boolean }[] };
  type Tab = { id: number; history: string[]; index: number };
  type Doc = { content: string; saved: string };

  const GRAPH = "silex:graph";
  const HOME = "silex:home";
  const ORGANIZER = "silex:organizer";
  const MESA = "silex:mesa";
  const VIEW_TITLES: Record<string, string> = { [GRAPH]: "Grafo", [HOME]: "Início", [ORGANIZER]: "Jev Organizer", [MESA]: "Mesa" };
  const VIEW_ICONS: Record<string, string> = { [GRAPH]: "graph", [HOME]: "search", [ORGANIZER]: "zap", [MESA]: "clipboard" };
  const isView = (p: string | null) => !!p && p.startsWith("silex:");
  const AUTOSAVE_MS = 1000;
  const SEARCH_DEBOUNCE_MS = 120;
  const DEFAULT_CONFIG: VaultConfig = {
    theme: "",
    snippets: [],
    plugins: [],
    accent_hue: 220,
    color_scheme: "dark",
    font_size: 16,
    line_width: 720,
    readable_line_width: true,
    core_plugins: null,
    plugin_settings: {},
  };

  // ---------- Estado ----------

  let vaults = $state<SavedVault[]>([]);
  let showVaults = $state(false);
  let booting = $state(true);

  let tree = $state<FolderNode | null>(null);
  let notes = $state<GraphNote[]>([]);
  let config = $state<VaultConfig>({ ...DEFAULT_CONFIG });
  let appearance = $state<Appearance>({ themes: [], snippets: [], dir: "" });
  let plugins = $state<PluginInfo[]>([]);
  const expanded = new SvelteSet<string>();
  let selectedFolder = $state<string | null>(null);
  let renaming = $state<string | null>(null);

  let nextTabId = 1;
  let tabs = $state<Tab[]>([{ id: nextTabId++, history: [], index: -1 }]);
  let activeId = $state(1);
  let docs = $state<Record<string, Doc>>({});
  let saving = $state(false);
  let toast = $state<{ text: string; error: boolean } | null>(null);

  let panel = $state<"files" | "search">("files");
  let sidebarOpen = $state(true);
  let sidebarWidth = $state(272);
  let palette = $state<"commands" | "files" | null>(null);
  let settingsOpen = $state(false);
  let livePreview = $state(true);

  let query = $state("");
  let hits = $state<SearchHit[]>([]);
  let searchMs = $state<number | null>(null);
  let searchInput = $state<HTMLInputElement>();
  let editor = $state<ReturnType<typeof Editor>>();
  let titleInput = $state<HTMLInputElement>();

  const saveTimers = new Map<string, ReturnType<typeof setTimeout>>();

  // ---------- Derivados ----------

  const sep = /[\\/]/;
  const pathOf = (t: Tab) => (t.index >= 0 ? t.history[t.index] : null);
  const baseName = (p: string) => p.split(sep).pop() ?? p;
  const stem = (p: string) => baseName(p).replace(/\.md$/i, "");
  const relative = (p: string) => (tree && p.startsWith(tree.path) ? p.slice(tree.path.length + 1) : p);
  const dirOf = (p: string) => p.replace(/[\\/][^\\/]*$/, "");
  const tabTitle = (p: string | null) =>
    !p ? "Nova aba" : isView(p) ? (VIEW_TITLES[p] ?? p) : fileKind(p) === "markdown" ? stem(p) : fileKind(p) === "excalidraw" ? baseName(p).replace(/\.excalidraw(\.md)?$/i, "") : baseName(p);

  let activeTab = $derived(tabs.find((t) => t.id === activeId) ?? tabs[0]);
  let activePath = $derived(activeTab ? pathOf(activeTab) : null);
  let activeKind = $derived(activePath && !isView(activePath) ? fileKind(activePath) : null);
  let activeDoc = $derived(activePath && activeKind === "markdown" ? docs[activePath] : undefined);

  let crumbs = $derived(activePath && !isView(activePath) && tree ? relative(activePath).split(sep).slice(0, -1) : []);

  // ---------- Plugins nativos ----------
  let core = $derived(enabledCore(config));
  let jevHomeCfg = $derived(pluginSettings<any>(config, "jev-home"));
  let organizerCfg = $derived(pluginSettings<any>(config, "jev-organizer"));
  let templaterCfg = $derived(pluginSettings<any>(config, "templater"));
  let excalidrawCfg = $derived(pluginSettings<any>(config, "excalidraw"));
  let exportCfg = $derived(pluginSettings<any>(config, "export"));
  let mesaCfg = $derived(pluginSettings<MesaSettings>(config, "mesa"));
  let chatCfg = $derived(pluginSettings<ChatSettings>(config, "chat"));

  // ---------- Chat ----------
  // O painel só existe enquanto está aberto (nada do chat fica em memória fechado)
  let chatMode = $state<"closed" | "popup" | "side">("closed");
  let chatWidth = $state(420);
  function lastChatMode(): "popup" | "side" {
    try {
      return localStorage.getItem("silex.chat.mode") === "side" ? "side" : "popup";
    } catch {
      return "popup";
    }
  }
  function setChatMode(m: "closed" | "popup" | "side") {
    chatMode = m;
    if (m !== "closed") {
      try {
        localStorage.setItem("silex.chat.mode", m);
      } catch {}
    }
  }
  const toggleChat = () => setChatMode(chatMode === "closed" ? lastChatMode() : "closed");

  function startChatResize(e: PointerEvent) {
    const startX = e.clientX;
    const startW = chatWidth;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => (chatWidth = Math.min(760, Math.max(320, startW - (ev.clientX - startX))));
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }
  let typesafeKey = $state(0);
  let settingsFocus = $state<string | null>(null);
  let promptState = $state<{ title: string; prompts: TplPrompt[]; resolve: (a: string[] | null) => void } | null>(null);
  let mesaCapture = $state<{ text: string; source: string | null; n: number } | null>(null);
  let pandoc = $state<string | null>(null);

  /** Notas abertas recentemente (para o Jev Home), por cofre */
  let recent = $state<string[]>([]);
  function touchRecent(path: string) {
    if (fileKind(path) !== "markdown") return;
    recent = [path, ...recent.filter((p) => p !== path)].slice(0, 30);
    try {
      localStorage.setItem(`silex.recent:${tree?.path}`, JSON.stringify(recent));
    } catch {}
  }
  let wordCount = $derived.by(() => {
    if (!activeDoc) return 0;
    const body = activeDoc.content.replace(/^---\n[\s\S]*?\n---\n?/, "");
    return (body.match(/\S+/g) ?? []).length;
  });

  let backlinks = $derived.by(() => {
    if (!activePath || activeKind !== "markdown") return 0;
    const target = stem(activePath).toLowerCase();
    return notes.filter(
      (n) => n.absolute_path !== activePath && n.links.some((l) => baseName(l).toLowerCase() === target),
    ).length;
  });

  let saveState = $derived(
    saving ? "saving" : activeDoc && activeDoc.content !== activeDoc.saved ? "dirty" : "saved",
  );

  /** Todos os arquivos do cofre, para resolver [[links]]/anexos e o seletor rápido. */
  let files = $derived.by(() => {
    const out: FileNode[] = [];
    const walk = (nodes: TreeNode[]) => {
      for (const n of nodes) n.kind === "folder" ? walk(n.children) : out.push(n);
    };
    if (tree) walk(tree.children);
    return out;
  });

  let fileIndex = $derived.by(() => {
    const byName = new Map<string, string>();
    const byRel = new Map<string, string>();
    for (const f of files) {
      const name = f.name.toLowerCase();
      if (!byName.has(name)) byName.set(name, f.path);
      byRel.set(relative(f.path).replace(/\\/g, "/").toLowerCase(), f.path);
    }
    return { byName, byRel };
  });

  /** Sugestões do [[autocompletar]]: nome curto quando único, senão o caminho relativo. Recentes primeiro. */
  let linkTargets = $derived.by(() => {
    const label = (f: FileNode) => (fileKind(f.path) === "markdown" ? stem(f.name) : f.name);
    const count = new Map<string, number>();
    for (const f of files) count.set(label(f).toLowerCase(), (count.get(label(f).toLowerCase()) ?? 0) + 1);
    const rank = new Map(recent.map((p, i) => [p, i]));
    const sorted = [...files].sort((a, b) => (rank.get(a.path) ?? 1e9) - (rank.get(b.path) ?? 1e9));
    const notesOnly: LinkTarget[] = [];
    const all: LinkTarget[] = [];
    const byPath = new Map<string, string>();
    for (const f of sorted) {
      const kind = fileKind(f.path);
      const rel = relative(f.path).replace(/\\/g, "/");
      const name = label(f);
      const t: LinkTarget = {
        label: name,
        link: count.get(name.toLowerCase()) === 1 ? name : kind === "markdown" ? rel.replace(/\.md$/i, "") : rel,
        detail: rel.includes("/") ? rel.slice(0, rel.lastIndexOf("/")) : "",
      };
      all.push(t);
      byPath.set(f.path, t.link);
      if (kind === "markdown" || kind === "excalidraw" || kind === "canvas") notesOnly.push(t);
    }
    return { notes: notesOnly, all, byPath };
  });

  /** Pastas do cofre (destinos de "Mover para…"). */
  let folders = $derived.by(() => {
    const out: string[] = [];
    const walk = (nodes: TreeNode[]) => {
      for (const n of nodes) if (n.kind === "folder") out.push(n.path), walk(n.children);
    };
    if (tree) walk(tree.children);
    return out;
  });

  let blockLangs = $derived(
    new Set([...plugins.filter((p) => p.enabled).flatMap((p) => p.code_blocks), ...(core.has("numerals") ? ["math", "numerals"] : [])]),
  );

  // ---------- Avisos ----------

  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function notify(text: string, error = false) {
    toast = { text, error };
    clearTimeout(toastTimer);
    if (!error) toastTimer = setTimeout(() => (toast = null), 3500);
  }
  const fail = (e: unknown) => notify(String(e), true);

  // ---------- Cofres ----------

  async function loadVault(path: string) {
    try {
      // Salva o que estiver pendente no cofre atual antes de trocar
      await Promise.all(Object.keys(docs).map(save));
      const opened = await invoke<{ tree: FolderNode; config: VaultConfig }>("open_vault", { vaultPath: path });
      tree = opened.tree;
      config = { ...DEFAULT_CONFIG, ...opened.config };
      expanded.clear();
      selectedFolder = null;
      docs = {};
      resetEditorStates();
      try {
        recent = JSON.parse(localStorage.getItem(`silex.recent:${opened.tree.path}`) ?? "[]");
      } catch {
        recent = [];
      }
      const cfgCore = enabledCore(config);
      const home = cfgCore.has("jev-home") && pluginSettings<any>(config, "jev-home").onStartup;
      tabs = [{ id: nextTabId, history: home ? [HOME] : [], index: home ? 0 : -1 }];
      activeId = nextTabId++;
      query = "";
      hits = [];
      toast = null;
      showVaults = false;
      vaults = await invoke<SavedVault[]>("list_vaults");
      await applyAppearance(config);
      [plugins, appearance] = await Promise.all([
        invoke<PluginInfo[]>("list_plugins"),
        invoke<Appearance>("list_appearance"),
      ]);
      typesafeKey = await invoke<number>("secret_len", { name: "typesafe" });
      notes = await invoke<GraphNote[]>("index_vault");
    } catch (e) {
      fail(e);
    }
  }

  async function pickVault() {
    const dir = await openDialog({ directory: true, title: "Abrir pasta como cofre" });
    if (typeof dir === "string") await loadVault(dir);
  }

  async function forgetVault(path: string) {
    vaults = await invoke<SavedVault[]>("forget_vault", { path });
  }

  async function refreshTree(reindex = true) {
    if (!tree) return;
    tree = await invoke<FolderNode>("read_tree");
    if (reindex) notes = await invoke<GraphNote[]>("index_vault");
    if (query) runSearch();
  }

  async function updateConfig(next: VaultConfig) {
    const mesaChanged = JSON.stringify(next.plugin_settings?.mesa) !== JSON.stringify(config.plugin_settings?.mesa);
    config = next;
    try {
      plugins = await invoke<PluginInfo[]>("save_vault_config", { config: next });
      // A Mesa também espelha a configuração em Mesa/Core/Configuracao.md (como o original)
      if (mesaChanged && enabledCore(next).has("mesa")) await invoke("mesa_save_settings", { settings: pluginSettings(next, "mesa") });
      await applyAppearance(next);
    } catch (e) {
      fail(e);
    }
  }

  // ---------- Árvore ----------

  function toggleFolder(path: string) {
    selectedFolder = path;
    if (expanded.has(path)) expanded.delete(path);
    else expanded.add(path);
  }

  /** Abre as pastas acima de `path` na árvore. */
  function reveal(path: string) {
    if (!tree) return;
    let dir = path;
    while ((dir = dirOf(dir)) && dir.length > tree.path.length) expanded.add(dir);
  }

  // ---------- Documentos ----------

  async function ensureDoc(path: string) {
    if (docs[path]) return;
    const content = await invoke<string>("read_note", { filePath: path });
    docs[path] = { content, saved: content };
  }

  async function save(path: string) {
    clearTimeout(saveTimers.get(path));
    saveTimers.delete(path);
    const doc = docs[path];
    if (!doc || doc.content === doc.saved) return;
    const snapshot = doc.content;
    saving = true;
    try {
      const note = await invoke<GraphNote>("save_note", { filePath: path, content: snapshot });
      doc.saved = snapshot;
      const i = notes.findIndex((n) => n.absolute_path === path);
      if (i >= 0) notes[i] = note;
      else notes.push(note);
    } catch (e) {
      fail(e);
    } finally {
      saving = false;
    }
  }

  /** Ao voltar para o app, recarrega notas abertas que mudaram no disco (OneDrive, outro editor...). */
  async function reloadChanged() {
    for (const [path, doc] of Object.entries(docs)) {
      if (doc.content !== doc.saved) continue; // não sobrescreve edição pendente
      const disk = await invoke<string>("read_note", { filePath: path }).catch(() => null);
      if (disk !== null && disk !== doc.saved && docs[path] === doc && doc.content === doc.saved) {
        doc.content = disk;
        doc.saved = disk;
      }
    }
  }

  function onEdit(path: string, value: string) {
    if (!docs[path]) return;
    docs[path].content = value;
    clearTimeout(saveTimers.get(path));
    saveTimers.set(path, setTimeout(() => save(path), AUTOSAVE_MS));
  }

  async function createNote(folder: string, name?: string) {
    const path = await invoke<string>("create_note", { folder, name: name ?? null });
    await refreshTree(false);
    notes = await invoke<GraphNote[]>("index_vault");
    reveal(path);
    // Templater: template da pasta em notas novas (se configurado)
    if (core.has("templater")) {
      const tpl = await invoke<string | null>("templater_for_folder", { target: path }).catch(() => null);
      if (tpl) {
        const r = await renderTemplate(tpl, path);
        if (r) {
          await invoke("save_note", { filePath: path, content: r.text }).catch(fail);
          delete docs[path];
          pendingCursor = r.cursor;
        }
      }
    }
    return path;
  }

  // ---------- Templater ----------

  let pendingCursor: number | null = null;

  /** Depois de aplicar um template com tp.file.cursor(), leva o cursor até lá. */
  function jumpToPendingCursor() {
    const pos = pendingCursor;
    pendingCursor = null;
    const v = view();
    if (pos === null || !v || !templaterCfg.auto_jump_to_cursor) return false;
    v.dispatch({ selection: { anchor: Math.min(pos, v.state.doc.length) }, scrollIntoView: true });
    v.focus();
    return true;
  }

  function askPrompts(title: string, prompts: TplPrompt[]): Promise<string[] | null> {
    return new Promise((resolve) => {
      promptState = { title, prompts, resolve };
      openPrompt = promptState;
    });
  }

  // Paleta e modal fecham (estado = null) antes de entregar a escolha; por isso a
  // promessa pendente fica numa variável comum, e não no estado reativo
  let openPrompt: { resolve: (a: string[] | null) => void } | null = null;
  function submitPrompts(answers: string[]) {
    openPrompt?.resolve(answers);
    openPrompt = null;
  }
  function closePrompts() {
    promptState = null;
    // fechar sem enviar cancela; enviar já resolveu (submit vem logo depois do fechamento)
    setTimeout(() => {
      openPrompt?.resolve(null);
      openPrompt = null;
    }, 0);
  }

  /** Renderiza um template para a nota alvo, perguntando o que tp.system.prompt pedir. */
  async function renderTemplate(template: string, target: string): Promise<{ text: string; cursor: number | null } | null> {
    let clipboard = "";
    try {
      clipboard = await navigator.clipboard.readText();
    } catch {}
    try {
      type R = { text: string; prompts: TplPrompt[]; cursor: number | null };
      let r = await invoke<R>("templater_render", { template, target, answers: [], clipboard });
      if (r.prompts.length) {
        const answers = await askPrompts(stem(template), r.prompts);
        if (!answers) return null;
        r = await invoke<R>("templater_render", { template, target, answers, clipboard });
      }
      return { text: r.text, cursor: r.cursor };
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async function pickTemplate(): Promise<string | null> {
    const list = await invoke<{ path: string; name: string; folder: string }[]>("templater_list").catch(() => []);
    if (!list.length) {
      notify("Nenhum template encontrado. Defina a pasta em Configurações › Plugins › Templater.", true);
      return null;
    }
    return new Promise((resolve) => {
      templatePick = { items: list, resolve };
      openTemplate = templatePick;
    });
  }
  let templatePick = $state<{ items: { path: string; name: string; folder: string }[]; resolve: (p: string | null) => void } | null>(null);
  let openTemplate: { resolve: (p: string | null) => void } | null = null;
  let templateItems = $derived(
    (templatePick?.items ?? []).map((t) => ({
      id: t.path,
      title: t.name,
      detail: t.folder,
      icon: "file",
      run: () => {
        openTemplate?.resolve(t.path);
        openTemplate = null;
      },
    })),
  );
  function closeTemplatePick() {
    templatePick = null;
    setTimeout(() => {
      openTemplate?.resolve(null);
      openTemplate = null;
    }, 0);
  }

  async function insertTemplate() {
    const v = view();
    if (!v || !activePath || activeKind !== "markdown") return notify("Abra uma nota para inserir o template.", true);
    const tpl = await pickTemplate();
    if (!tpl) return;
    const target = activePath;
    const r = await renderTemplate(tpl, target);
    if (!r) return;
    const from = v.state.selection.main.from;
    v.dispatch(v.state.replaceSelection(r.text));
    if (r.cursor !== null && templaterCfg.auto_jump_to_cursor) {
      const pos = from + r.cursor;
      v.dispatch({ selection: { anchor: pos } });
    }
    v.focus();
  }

  async function noteFromTemplate() {
    if (!tree) return;
    const tpl = await pickTemplate();
    if (!tpl) return;
    try {
      const path = await invoke<string>("create_note", { folder: selectedFolder ?? tree.path, name: null });
      await refreshTree(false);
      const r = await renderTemplate(tpl, path);
      if (r) await invoke("save_note", { filePath: path, content: r.text });
      notes = await invoke<GraphNote[]>("index_vault");
      reveal(path);
      pendingCursor = r?.cursor ?? null;
      await openFile(path);
      await tick();
      jumpToPendingCursor() || (titleInput?.focus(), titleInput?.select());
    } catch (e) {
      fail(e);
    }
  }

  async function replaceTemplatesInNote() {
    const v = view();
    if (!v || !activePath) return;
    await save(activePath);
    const r = await renderTemplate(activePath, activePath);
    if (r) ed.setContent(v, r.text);
  }

  // ---------- Exportar ----------

  let exportState = $state<{ note: string; format: string; label: string } | null>(null);

  async function exportNote(format: "pdf" | "html" | "docx" | "epub" | "odt" | "rtf" | "tex" | "pptx") {
    if (!activePath || activeKind !== "markdown") return notify("Abra uma nota para exportar.", true);
    await save(activePath);
    const note = activePath;
    try {
      if (format === "pdf") {
        // PDF pelo diálogo de impressão do sistema (escolha "Salvar como PDF")
        const html = await invoke<string>("export_html", { path: note });
        const frame = document.createElement("iframe");
        frame.style.cssText = "position:fixed;width:0;height:0;border:0;opacity:0";
        document.body.appendChild(frame);
        frame.srcdoc = html;
        await new Promise((r) => (frame.onload = r));
        await Promise.all([...frame.contentDocument!.images].map((img) => (img.complete ? null : new Promise((r) => (img.onload = img.onerror = r)))));
        frame.contentWindow!.focus();
        frame.contentWindow!.print();
        setTimeout(() => frame.remove(), 60_000);
        return;
      }
      const names: Record<string, string> = { html: "HTML", docx: "Word", epub: "EPUB", odt: "OpenDocument", rtf: "RTF", tex: "LaTeX", pptx: "PowerPoint" };
      exportState = { note, format, label: names[format] };
    } catch (e) {
      fail(e);
    }
  }

  // ---------- Canvas / Excalidraw ----------

  async function newCanvas() {
    if (!tree) return;
    try {
      const path = await invoke<string>("create_file", {
        folder: selectedFolder ?? tree.path,
        name: "Sem título",
        ext: "canvas",
        content: JSON.stringify({ nodes: [], edges: [] }, null, "	"),
      });
      await refreshTree(false);
      reveal(path);
      await openFile(path);
    } catch (e) {
      fail(e);
    }
  }

  async function newDrawing() {
    if (!tree) return;
    const folder = excalidrawCfg.folder?.trim() ? `${tree.path}${tree.path.includes("\\") ? "\\" : "/"}${excalidrawCfg.folder.trim()}` : tree.path;
    const stamp = new Date();
    const pad = (n: number) => String(n).padStart(2, "0");
    const name = `${excalidrawCfg.filenamePrefix ?? "Drawing "}${stamp.getFullYear()}-${pad(stamp.getMonth() + 1)}-${pad(stamp.getDate())} ${pad(stamp.getHours())}.${pad(stamp.getMinutes())}.${pad(stamp.getSeconds())}`;
    try {
      if (excalidrawCfg.folder?.trim()) await invoke("create_folder", { parent: tree.path, name: excalidrawCfg.folder.trim() }).catch(() => {});
      const path = await invoke<string>("create_file", { folder, name, ext: "excalidraw.md", content: emptyDrawing() });
      await refreshTree();
      reveal(path);
      await openFile(path);
    } catch (e) {
      fail(e);
    }
  }

  async function newNote(folder?: string) {
    if (!tree) return;
    try {
      const path = await createNote(folder ?? selectedFolder ?? tree.path);
      await openFile(path);
      await tick();
      jumpToPendingCursor() || (titleInput?.focus(), titleInput?.select());
    } catch (e) {
      fail(e);
    }
  }

  async function newFolder(parent?: string) {
    if (!tree) return;
    try {
      const path = await invoke<string>("create_folder", { parent: parent ?? selectedFolder ?? tree.path, name: null });
      await refreshTree(false);
      reveal(path);
      expanded.add(dirOf(path));
      selectedFolder = path;
      renaming = path;
    } catch (e) {
      fail(e);
    }
  }

  /** Atualiza abas, cache e estados do editor depois de renomear/mover. */
  function remapPaths(from: string, to: string) {
    const map = (p: string) => (p === from ? to : p.startsWith(from + "\\") || p.startsWith(from + "/") ? to + p.slice(from.length) : p);
    for (const t of tabs) t.history = t.history.map(map);
    const nextDocs: Record<string, Doc> = {};
    for (const [k, v] of Object.entries(docs)) {
      nextDocs[map(k)] = v;
      if (map(k) !== k) renameEditorState(k, map(k));
    }
    docs = nextDocs;
    for (const e of [...expanded]) {
      if (map(e) !== e) {
        expanded.delete(e);
        expanded.add(map(e));
      }
    }
    if (selectedFolder) selectedFolder = map(selectedFolder);
  }

  async function renamePath(path: string, rawName: string) {
    renaming = null;
    let name = rawName.trim();
    const isNote = fileKind(path) === "markdown";
    if (!name || name === (isNote ? stem(path) : baseName(path))) return;
    if (isNote && !/\.md$/i.test(name)) name += ".md";
    try {
      await save(path);
      const next = await invoke<string>("rename_path", { path, newName: name });
      remapPaths(path, next);
      await refreshTree();
    } catch (e) {
      fail(e);
    }
  }

  const isUnder = (p: string, root: string) => p === root || p.startsWith(root + "\\") || p.startsWith(root + "/");
  const canMoveTo = (path: string, folder: string) => dirOf(path) !== folder && !isUnder(folder, path);

  /** Move arquivo/pasta para outra pasta (arrastar e soltar, "Mover para…"). */
  async function movePath(path: string, folder: string) {
    if (!canMoveTo(path, folder)) return;
    try {
      // Grava edições pendentes antes: o salvamento automático escreveria no caminho antigo
      await Promise.all(Object.keys(docs).filter((k) => isUnder(k, path)).map(save));
      const next = await invoke<string>("move_path", { path, targetFolder: folder });
      remapPaths(path, next);
      const map = (p: string) => (isUnder(p, path) ? next + p.slice(path.length) : p);
      notes = notes.map((n) =>
        isUnder(n.absolute_path, path) ? { ...n, absolute_path: map(n.absolute_path), relative_path: relative(map(n.absolute_path)) } : n,
      );
      recent = recent.map(map);
      if (tree && folder !== tree.path) expanded.add(folder);
      reveal(next);
      await refreshTree(false);
      notify(`Movido para ${folder === tree?.path ? tree?.name : relative(folder).replace(/\\/g, "/")}`);
    } catch (e) {
      fail(e);
    }
  }

  // ---------- Arrastar e soltar na árvore ----------

  let dragPath = $state<string | null>(null);
  let dropFolder = $state<string | null>(null);
  let dragOpenTimer: ReturnType<typeof setTimeout> | undefined;

  function onTreeDragStart(e: DragEvent, node: TreeNode) {
    dragPath = node.path;
    const dt = e.dataTransfer!;
    dt.effectAllowed = "copyMove";
    // Solto no editor, vira link para o arquivo
    if (node.kind === "file") {
      const link = linkTargets.byPath.get(node.path) ?? stem(node.name);
      dt.setData("text/plain", fileKind(node.path) === "markdown" ? `[[${link}]]` : `![[${link}]]`);
    } else dt.setData("text/plain", node.name);
  }

  function onTreeDragOver(e: DragEvent, folder: string) {
    if (!dragPath || !canMoveTo(dragPath, folder)) return;
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer!.dropEffect = "move";
    if (dropFolder !== folder) {
      dropFolder = folder;
      // Pasta fechada abre depois de um instante com o item em cima
      clearTimeout(dragOpenTimer);
      if (!expanded.has(folder)) dragOpenTimer = setTimeout(() => dropFolder === folder && expanded.add(folder), 650);
    }
  }

  function onTreeDrop(e: DragEvent, folder: string) {
    const path = dragPath;
    if (!path) return;
    e.preventDefault();
    e.stopPropagation();
    endTreeDrag();
    movePath(path, folder);
  }

  function endTreeDrag() {
    clearTimeout(dragOpenTimer);
    dragPath = null;
    dropFolder = null;
  }

  let movePick = $state<string | null>(null);
  /** Destinos de "Mover para…". O caminho fica numa constante local: a paleta
   *  fecha (movePick = null) antes de executar o item escolhido. */
  let moveItems = $derived.by(() => {
    const from = movePick;
    const vault = tree;
    if (!from || !vault) return [];
    return [vault.path, ...folders]
      .filter((f) => canMoveTo(from, f))
      .map((f) => ({
        id: f,
        title: f === vault.path ? `${vault.name} (raiz)` : relative(f).replace(/\\/g, "/"),
        icon: "folder",
        run: () => movePath(from, f),
      }));
  });

  /** Confirmação no tema do app (sem diálogo do sistema). */
  let confirmState = $state<{ text: string; ok: string; resolve: (v: boolean) => void } | null>(null);
  function confirmIn(text: string, ok: string): Promise<boolean> {
    return new Promise((resolve) => (confirmState = { text, ok, resolve }));
  }
  function answerConfirm(v: boolean) {
    confirmState?.resolve(v);
    confirmState = null;
  }

  async function deletePath(path: string) {
    const isDir = !files.some((f) => f.path === path);
    const ok = await confirmIn(`Mover “${baseName(path)}” para a Lixeira?`, "Mover para a Lixeira");
    if (!ok) return;
    try {
      clearTimeout(saveTimers.get(path));
      await invoke("delete_path", { path });
      const gone = (p: string) => p === path || p.startsWith(path + "\\") || p.startsWith(path + "/");
      for (const t of [...tabs]) if (pathOf(t) && gone(pathOf(t)!)) closeTab(t.id);
      for (const t of tabs) {
        const cur = pathOf(t);
        t.history = t.history.filter((p) => !gone(p));
        t.index = cur ? t.history.indexOf(cur) : -1;
      }
      for (const k of Object.keys(docs)) if (gone(k)) delete docs[k];
      await refreshTree(isDir || fileKind(path) === "markdown");
    } catch (e) {
      fail(e);
    }
  }

  /** Resolve [[alvo]] como o Obsidian: caminho relativo ou nome do arquivo, com ou sem .md. */
  function resolveFile(name: string): string | null {
    const clean = name.replace(/\\/g, "/").replace(/^\.\//, "").trim().toLowerCase();
    const { byName, byRel } = fileIndex;
    const withMd = /\.[a-z0-9]{1,5}$/i.test(clean) ? clean : `${clean}.md`;
    return byRel.get(withMd) ?? byRel.get(clean) ?? byName.get(baseName(withMd)) ?? byName.get(baseName(clean)) ?? null;
  }

  async function followLink(target: string, newTab: boolean) {
    if (!tree) return;
    const clean = target.split("#")[0].trim();
    if (!clean) return;
    try {
      const path = resolveFile(clean) ?? (await createNote(tree.path, baseName(clean)));
      await openFile(path, newTab);
    } catch (e) {
      fail(e);
    }
  }

  // ---------- Busca ----------

  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let searchSeq = 0;

  async function runSearch() {
    const seq = ++searchSeq;
    const q = query.trim();
    if (!q || !tree) {
      hits = [];
      searchMs = null;
      return;
    }
    const t0 = performance.now();
    try {
      const result = await invoke<SearchHit[]>("search_vault", { query: q, limit: 100 });
      if (seq !== searchSeq) return;
      hits = result;
      searchMs = performance.now() - t0;
    } catch (e) {
      fail(e);
    }
  }

  function onQuery() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(runSearch, SEARCH_DEBOUNCE_MS);
  }

  async function openSearch() {
    sidebarOpen = true;
    panel = "search";
    await tick();
    searchInput?.focus();
    searchInput?.select();
  }

  // ---------- Abas ----------

  async function openFile(path: string, inNewTab = false) {
    touchRecent(path);
    if (!isView(path) && fileKind(path) === "markdown") {
      try {
        await ensureDoc(path);
      } catch (e) {
        fail(e);
        return;
      }
    }
    if (inNewTab) {
      const tab: Tab = { id: nextTabId++, history: [path], index: 0 };
      tabs.splice(tabs.findIndex((t) => t.id === activeId) + 1, 0, tab);
      activeId = tab.id;
      return;
    }
    const tab = activeTab;
    const current = pathOf(tab);
    if (current === path) return;
    if (current && !isView(current)) save(current);
    tab.history = [...tab.history.slice(0, tab.index + 1), path];
    tab.index = tab.history.length - 1;
  }

  /** Abre (ou foca) uma vista especial: grafo, Jev Home, Organizer, Mesa. */
  function openView(v: string, reuse = true) {
    const existing = reuse ? tabs.find((t) => pathOf(t) === v) : undefined;
    if (existing) activeId = existing.id;
    else openFile(v, pathOf(activeTab) !== null && pathOf(activeTab) !== HOME);
  }
  const openGraph = () => openView(GRAPH);

  /** Caminho absoluto a partir do relativo ao cofre ("pasta/nota.md"). */
  function absFromRel(rel: string) {
    const root = tree?.path ?? "";
    const win = root.includes("\\");
    return `${root}${win ? "\\" : "/"}${win ? rel.replace(/\//g, "\\") : rel}`;
  }

  function openPluginSettings(id: string) {
    settingsFocus = id;
    settingsOpen = true;
  }

  /** Botões das configurações dos plugins (testar chave, reindexar...). */
  async function pluginAction(plugin: string, action: string): Promise<string | void> {
    switch (action) {
      case "jev-test": {
        const model = (pluginSettings<any>(config, plugin).model as string) || "jev-latest";
        const v = await invoke<number>("jev_test", { model });
        typesafeKey = await invoke<number>("secret_len", { name: "typesafe" });
        return `Jev funcionando (${v.toFixed(2)}).`;
      }
      case "jev-test-mesa": {
        const v = await invoke<number>("jev_test", { model: mesaCfg.jevModel || "jev-latest" });
        return `Jev funcionando (${v.toFixed(2)}).`;
      }
      case "llm-test":
        await invoke("llm_test", { provider: mesaCfg.provider, model: mesaCfg.model });
        return "Conexão funcionando.";
      case "jev-home-reindex":
        return `${await invoke<number>("jev_home_reindex")} notas indexadas.`;
      case "organizer-clear":
        await invoke("organizer_clear");
        return "Sugestões apagadas.";
    }
  }

  function newTab() {
    // Jev Home: toda aba nova abre a página inicial, como num navegador
    const home = core.has("jev-home") && jevHomeCfg.newTab;
    const tab: Tab = { id: nextTabId++, history: home ? [HOME] : [], index: home ? 0 : -1 };
    tabs.push(tab);
    activeId = tab.id;
  }

  function closeTab(id: number) {
    const i = tabs.findIndex((t) => t.id === id);
    if (i < 0) return;
    const path = pathOf(tabs[i]);
    if (path && !isView(path)) save(path);
    tabs.splice(i, 1);
    if (tabs.length === 0) tabs.push({ id: nextTabId++, history: [], index: -1 });
    if (id === activeId) activeId = tabs[Math.min(i, tabs.length - 1)].id;
  }

  function cycleTab(dir: 1 | -1) {
    const i = tabs.findIndex((t) => t.id === activeId);
    activeId = tabs[(i + dir + tabs.length) % tabs.length].id;
  }

  async function navigate(dir: 1 | -1) {
    const tab = activeTab;
    const next = tab.index + dir;
    if (next < 0 || next >= tab.history.length) return;
    const current = pathOf(tab);
    if (current && !isView(current)) save(current);
    const target = tab.history[next];
    if (fileKind(target) === "markdown" && !isView(target)) await ensureDoc(target).catch(fail);
    tab.index = next;
  }

  let draggedTab: number | null = null;
  let dropTarget = $state<number | null>(null);
  function onTabDrop(targetId: number) {
    dropTarget = null;
    if (draggedTab === null || draggedTab === targetId) return;
    const [moved] = tabs.splice(
      tabs.findIndex((t) => t.id === draggedTab),
      1,
    );
    tabs.splice(
      tabs.findIndex((t) => t.id === targetId),
      0,
      moved,
    );
    draggedTab = null;
  }

  // ---------- Editor ----------

  const view = (): EditorView | undefined => editor?.getView();

  const editorHost: Omit<EditorHost, "path"> = {
    openLink: followLink,
    openExternal: (url) => {
      if (/^https?:\/\//i.test(url)) openUrl(url);
      else followLink(decodeURIComponent(url), false);
    },
    resolveFile,
    assetUrl: (p) => convertFileSrc(p),
    renderBlock: (lang, source, content) =>
      invoke<BlockResult>("render_block", { lang, source, path: activePath ?? "", content }),
    blockLangs: () => blockLangs,
    excelPaste: () => core.has("excel-paste"),
    taskMenu: (v, pos) =>
      popupMenu(
        TASK_STATUSES.map((s) => ({
          text: `[${s.char}]  ${s.label}`,
          checked: v.state.sliceDoc(pos, pos + 1) === s.char,
          action: () => v.dispatch({ changes: { from: pos, to: pos + 1, insert: s.char } }),
        })),
      ),
    linkTargets: (embed) => (embed ? linkTargets.all : linkTargets.notes),
    saveImage: async (data, ext) => {
      const note = activePath;
      if (!note || activeKind !== "markdown") return null;
      try {
        // Bytes crus no corpo da requisição (sem JSON); o Rust grava na pasta de anexos
        const name = await invoke<string>("save_attachment", data, {
          headers: { "x-note": encodeURIComponent(note), "x-ext": ext },
        });
        await refreshTree(false);
        return name;
      } catch (e) {
        fail(e);
        return null;
      }
    },
  };

  let showDock = $state(true);
  try {
    showDock = localStorage.getItem("silex.dock") !== "0";
  } catch {}
  function toggleDock() {
    showDock = !showDock;
    try {
      localStorage.setItem("silex.dock", showDock ? "1" : "0");
    } catch {}
  }

  function insertMenu(v: EditorView): MenuEntry[] {
    return [
      { text: "Tabela", action: () => ed.insertTable(v) },
      { text: "Bloco de código", action: () => ed.insertCodeBlock(v) },
      { text: "Link para nota", action: () => ed.insertWikilink(v) },
      { text: "Bloco rustview", action: () => ed.insertBlock(v, RUSTVIEW_SAMPLE) },
      "separator",
      ...CALLOUT_TYPES.map((c) => ({
        text: `Callout: ${c.title}`,
        action: () => ed.insertBlock(v, `> [!${c.type}] ${c.title}\n> `),
      })),
    ];
  }

  function editorMenu(v: EditorView) {
    popupMenu([
      "cut",
      "copy",
      "paste",
      "selectAll",
      "separator",
      {
        text: "Formatar",
        items: [
          { text: "Negrito", action: () => ed.toggleWrap(v, "**") },
          { text: "Itálico", action: () => ed.toggleWrap(v, "*") },
          { text: "Realce", action: () => ed.toggleWrap(v, "==") },
          { text: "Tachado", action: () => ed.toggleWrap(v, "~~") },
          { text: "Código", action: () => ed.toggleWrap(v, "`") },
          "separator",
          { text: "Lista", action: () => ed.toggleLinePrefix(v, "bullet") },
          { text: "Lista numerada", action: () => ed.toggleLinePrefix(v, "number") },
          { text: "Citação", action: () => ed.toggleLinePrefix(v, "quote") },
        ],
      },
      { text: "Inserir", items: insertMenu(v) },
      {
        text: "Tarefa",
        items: TASK_STATUSES.map((s) => ({ text: `[${s.char}]  ${s.label}`, action: () => ed.setTaskStatus(v, s.char) })),
      },
      "separator",
      { text: "Adicionar propriedade", action: () => ed.addProperty(v) },
    ]);
  }

  const RUSTVIEW_SAMPLE = [
    "```rustview",
    "let tasks = current().tasks;",
    'let feitas = count(tasks, |t| t.status == "x");',
    "header(3, `Tarefas: ${feitas}/${tasks.len()}`);",
    "```",
  ].join("\n");

  async function runPluginCommand(plugin: string, command: string) {
    const v = view();
    const path = activeKind === "markdown" ? activePath : null;
    try {
      const out = await invoke<{ actions: { type: string; text?: string; target?: string }[] }>("run_plugin_command", {
        plugin,
        command,
        note: { path: path ?? "", content: v?.state.doc.toString() ?? "", selection: v ? ed.selectionText(v) : "" },
      });
      for (const a of out.actions ?? []) {
        if (a.type === "replace_selection" && v) ed.replaceSelection(v, a.text ?? "");
        else if (a.type === "set_content" && v) ed.setContent(v, a.text ?? "");
        else if (a.type === "notice") notify(a.text ?? "");
        else if (a.type === "open" && a.target) followLink(a.target, false);
      }
    } catch (e) {
      fail(e);
    }
  }

  // ---------- Menus da árvore ----------

  function treeMenu(e: MouseEvent, node: TreeNode | null) {
    e.preventDefault();
    e.stopPropagation();
    if (!tree) return;
    const root = tree.path;
    if (!node) {
      popupMenu([
        { text: "Nova nota", action: () => newNote(root) },
        { text: "Nova pasta", action: () => newFolder(root) },
        "separator",
        { text: "Mostrar no Explorador", action: () => openPath(root) },
      ]);
      return;
    }
    const common: MenuEntry[] = [
      { text: "Renomear", action: () => (renaming = node.path) },
      { text: "Mover para…", action: () => (movePick = node.path) },
      { text: "Excluir", action: () => deletePath(node.path) },
      "separator",
      { text: "Mostrar no Explorador", action: () => revealItemInDir(node.path) },
      { text: "Copiar caminho", action: () => navigator.clipboard.writeText(node.path) },
    ];
    if (node.kind === "folder") {
      popupMenu([
        { text: "Nova nota aqui", action: () => newNote(node.path) },
        { text: "Nova pasta aqui", action: () => newFolder(node.path) },
        "separator",
        ...common,
      ]);
    } else {
      popupMenu([
        { text: "Abrir em nova aba", action: () => openFile(node.path, true) },
        ...(fileKind(node.path) === "other" ? [{ text: "Abrir no aplicativo padrão", action: () => openPath(node.path) }] : []),
        "separator",
        ...common,
      ]);
    }
  }

  function tabMenu(e: MouseEvent, tab: Tab) {
    e.preventDefault();
    const path = pathOf(tab);
    popupMenu([
      { text: "Fechar", action: () => closeTab(tab.id) },
      { text: "Fechar outras abas", action: () => tabs.filter((t) => t.id !== tab.id).forEach((t) => closeTab(t.id)) },
      ...(path && !isView(path)
        ? (["separator", { text: "Mostrar no Explorador", action: () => revealItemInDir(path) }] as MenuEntry[])
        : []),
    ]);
  }

  // ---------- Paleta ----------

  let commandItems = $derived.by((): PaletteItem[] => {
    const hasNote = activeKind === "markdown";
    const v = () => view();
    const items: (PaletteItem & { when?: boolean })[] = [
      { id: "note.new", title: "Criar nova nota", hotkey: "Ctrl N", icon: "edit", run: () => newNote() },
      { id: "folder.new", title: "Criar nova pasta", icon: "folder-plus", run: () => newFolder() },
      { id: "switcher", title: "Abrir arquivo rapidamente", hotkey: "Ctrl O", icon: "file", run: () => openPalette("files") },
      { id: "search", title: "Buscar no cofre", hotkey: "Ctrl Shift F", icon: "search", run: openSearch },
      { id: "graph", title: "Abrir visualização em grafo", hotkey: "Ctrl G", icon: "graph", run: openGraph },
      { id: "settings", title: "Abrir configurações", hotkey: "Ctrl ,", icon: "settings", run: () => (settingsOpen = true) },
      { id: "vaults", title: "Trocar de cofre…", icon: "vaults", run: () => (showVaults = true) },
      {
        id: "prop.add",
        title: "Adicionar propriedade à nota",
        icon: "list",
        when: hasNote,
        run: () => {
          const ev = v();
          if (ev) ed.addProperty(ev);
        },
      },
      {
        id: "note.rename",
        title: "Renomear nota atual",
        hotkey: "F2",
        icon: "pencil",
        when: hasNote,
        run: () => tick().then(() => (titleInput?.focus(), titleInput?.select())),
      },
      { id: "note.move", title: "Mover arquivo atual para…", icon: "folder-input", when: !!activeKind, run: () => activePath && (movePick = activePath) },
      { id: "dock", title: showDock ? "Ocultar barra de formatação" : "Mostrar barra de formatação", icon: "bold", when: hasNote, run: toggleDock },
      { id: "note.delete", title: "Excluir arquivo atual", icon: "trash", when: !!activeKind, run: () => activePath && deletePath(activePath) },
      { id: "note.reveal", title: "Revelar arquivo na árvore", icon: "folder", when: !!activeKind, run: () => {
          if (!activePath) return;
          sidebarOpen = true;
          panel = "files";
          reveal(activePath);
        } },
      { id: "source", title: livePreview ? "Mudar para modo fonte (Markdown cru)" : "Mudar para visualização ao vivo", hotkey: "Ctrl E", icon: "code", when: hasNote, run: () => (livePreview = !livePreview) },
      { id: "sidebar", title: "Alternar barra lateral", hotkey: "Ctrl B", icon: "sidebar", run: () => (sidebarOpen = !sidebarOpen) },
      { id: "tab.new", title: "Nova aba", hotkey: "Ctrl T", icon: "plus", run: newTab },
      { id: "tab.close", title: "Fechar aba", hotkey: "Ctrl W", icon: "x", run: () => closeTab(activeId) },
      { id: "vault.reload", title: "Recarregar cofre", icon: "refresh", run: () => refreshTree() },
      { id: "obsidian.import", title: "Importar temas e plugins do Obsidian", icon: "repeat", run: () => (settingsOpen = true) },
      { id: "task.toggle", title: "Alternar tarefa", hotkey: "Ctrl L", icon: "check", when: hasNote, run: () => {
          const ev = v();
          if (ev) ed.setTaskStatus(ev, null);
        } },
      ...TASK_STATUSES.map((s) => ({
        id: `task.${s.name}`,
        title: `Tarefa: marcar como ${s.label.toLowerCase()} [${s.char}]`,
        icon: "check",
        when: hasNote,
        run: () => {
          const ev = v();
          if (ev) ed.setTaskStatus(ev, s.char);
        },
      })),
      ...CALLOUT_TYPES.map((c) => ({
        id: `callout.${c.type}`,
        title: `Inserir callout: ${c.title}`,
        icon: "quote",
        when: hasNote,
        run: () => {
          const ev = v();
          if (ev) ed.insertBlock(ev, `> [!${c.type}] ${c.title}\n> `);
        },
      })),
      { id: "insert.rustview", title: "Inserir bloco rustview", icon: "code", when: hasNote, run: () => {
          const ev = v();
          if (ev) ed.insertBlock(ev, RUSTVIEW_SAMPLE);
        } },
      { id: "insert.table", title: "Inserir tabela", icon: "sheet", when: hasNote, run: () => {
          const ev = v();
          if (ev) ed.insertTable(ev);
        } },
      { id: "line.up", title: "Mover linha para cima", hotkey: "Ctrl Shift ↑", icon: "back", when: hasNote, run: () => {
          const ev = v();
          if (ev) ed.moveLines(ev, -1, livePreview);
        } },
      { id: "line.down", title: "Mover linha para baixo", hotkey: "Ctrl Shift ↓", icon: "forward", when: hasNote, run: () => {
          const ev = v();
          if (ev) ed.moveLines(ev, 1, livePreview);
        } },
      // ----- Plugins nativos -----
      { id: "jev-home.open", title: "Jev Home: abrir página inicial", icon: "search", when: core.has("jev-home"), run: () => openView(HOME) },
      { id: "jev-home.open-tab", title: "Jev Home: abrir página inicial em nova aba", icon: "search", when: core.has("jev-home"), run: () => openFile(HOME, true) },
      { id: "jev-home.reindex", title: "Jev Home: reindexar notas", icon: "refresh", when: core.has("jev-home"), run: async () => notify(`${await invoke<number>("jev_home_reindex")} notas indexadas.`) },
      { id: "organizer.open", title: "Jev Organizer: abrir sugestões", icon: "zap", when: core.has("jev-organizer"), run: () => openView(ORGANIZER) },
      { id: "organizer.note", title: "Jev Organizer: analisar nota atual", icon: "zap", when: core.has("jev-organizer") && hasNote, run: async () => {
          openView(ORGANIZER);
          const r = await invoke<any>("organizer_scan", { scope: activePath, force: true }).catch(fail);
          if (r) notify(r.message ?? `Jev: ${r.done - r.errors} nota(s) analisada(s).`);
        } },
      { id: "organizer.vault", title: "Jev Organizer: analisar cofre", icon: "zap", when: core.has("jev-organizer"), run: async () => {
          openView(ORGANIZER);
          const r = await invoke<any>("organizer_scan", { scope: null, force: false }).catch(fail);
          if (r) notify(r.message ?? `Jev: ${r.done - r.errors} nota(s) analisada(s).`);
        } },
      { id: "mesa.open", title: "Mesa: abrir mesa", icon: "clipboard", when: core.has("mesa"), run: () => openView(MESA) },
      { id: "mesa.capture", title: "Mesa: capturar tarefa", icon: "plus", when: core.has("mesa"), run: () => {
          openView(MESA);
          mesaCapture = { text: "", source: null, n: Date.now() };
        } },
      { id: "mesa.capture-sel", title: "Mesa: capturar seleção como tarefa", icon: "plus", when: core.has("mesa") && hasNote, run: () => {
          const ev = v();
          const text = ev ? ed.selectionText(ev) : "";
          const source = activePath ? relative(activePath).replace(/\\/g, "/") : null;
          openView(MESA);
          mesaCapture = { text, source, n: Date.now() };
        } },
      { id: "tpl.insert", title: "Templater: inserir template", icon: "file", when: core.has("templater") && hasNote, run: insertTemplate },
      { id: "tpl.new", title: "Templater: criar nota a partir de template", icon: "file", when: core.has("templater"), run: noteFromTemplate },
      { id: "tpl.replace", title: "Templater: substituir templates na nota atual", icon: "refresh", when: core.has("templater") && hasNote, run: replaceTemplatesInNote },
      { id: "export.pdf", title: "Exportar: PDF", icon: "pdf", when: core.has("export") && hasNote, run: () => exportNote("pdf") },
      { id: "export.html", title: "Exportar: HTML", icon: "file", when: core.has("export") && hasNote, run: () => exportNote("html") },
      { id: "export.docx", title: "Exportar: Word (.docx)", icon: "file", when: core.has("export") && hasNote, run: () => exportNote("docx") },
      ...(["epub", "odt", "rtf", "tex", "pptx"] as const).map((f) => ({
        id: `export.${f}`,
        title: `Exportar: ${f.toUpperCase()} (Pandoc)`,
        icon: "file",
        when: core.has("export") && hasNote && !!pandoc,
        run: () => exportNote(f),
      })),
      { id: "chat.toggle", title: "Chat: abrir/fechar", hotkey: "Ctrl J", icon: "chat", when: core.has("chat"), run: toggleChat },
      { id: "chat.popup", title: "Chat: abrir em janela flutuante", icon: "popup", when: core.has("chat"), run: () => setChatMode("popup") },
      { id: "chat.side", title: "Chat: abrir no painel lateral", icon: "panel-right", when: core.has("chat"), run: () => setChatMode("side") },
      { id: "canvas.new", title: "Criar novo canvas", icon: "graph", when: core.has("canvas"), run: newCanvas },
      { id: "excalidraw.new", title: "Excalidraw: criar novo desenho", icon: "pencil", when: core.has("excalidraw"), run: newDrawing },
      { id: "numerals.insert", title: "Numerals: inserir bloco de cálculo", icon: "target", when: core.has("numerals") && hasNote, run: () => {
          const ev = v();
          if (ev) ed.insertBlock(ev, "```math\nvalor = 1500\nvalor * 12\n@prev + 10%\n```", 8);
        } },
      ...plugins
        .filter((p) => p.enabled)
        .flatMap((p) =>
          p.commands.map((c) => ({
            id: `plugin.${p.id}.${c.id}`,
            title: `${p.name}: ${c.name}`,
            icon: "zap",
            run: () => runPluginCommand(p.id, c.id),
          })),
        ),
    ];
    return items.filter((i) => i.when !== false);
  });

  let fileItems = $derived(
    files.map((f) => ({
      id: f.path,
      title: fileKind(f.path) === "markdown" ? stem(f.name) : f.name,
      detail: relative(dirOf(f.path)) === relative(f.path) ? "" : relative(dirOf(f.path)),
      icon: KIND_ICON[fileKind(f.path)],
      run: ({ ctrl }: { ctrl: boolean }) => openFile(f.path, ctrl),
    })),
  );

  function openPalette(mode: "commands" | "files") {
    if (!tree && mode === "files") return;
    palette = mode;
  }

  // ---------- Configurações ----------

  async function installPlugin() {
    const file = await openDialog({ title: "Instalar plugin", filters: [{ name: "Plugin do Silex", extensions: ["wasm"] }] });
    if (typeof file !== "string") return;
    try {
      plugins = await invoke<PluginInfo[]>("install_plugin", { source: file });
      const installed = plugins.find((p) => p.file === baseName(file));
      if (installed && !installed.error && !config.plugins.includes(installed.id)) {
        await updateConfig({ ...config, plugins: [...config.plugins, installed.id] });
      }
      notify(installed?.error ? `Plugin inválido: ${installed.error}` : `Plugin “${installed?.name}” instalado.`, !!installed?.error);
    } catch (e) {
      fail(e);
    }
  }

  async function importObsidian(): Promise<ImportReport | null> {
    try {
      const report = await invoke<ImportReport>("import_obsidian");
      appearance = await invoke<Appearance>("list_appearance");
      return report;
    } catch (e) {
      fail(e);
      return null;
    }
  }

  // ---------- Barra lateral ----------

  function autofocusBtn(node: HTMLButtonElement) {
    requestAnimationFrame(() => node.focus());
  }

  function autofocus(node: HTMLInputElement) {
    requestAnimationFrame(() => {
      node.focus();
      node.select();
    });
  }

  function startResize(e: PointerEvent) {
    const startX = e.clientX;
    const startW = sidebarWidth;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const move = (ev: PointerEvent) => {
      sidebarWidth = Math.min(560, Math.max(200, startW + ev.clientX - startX));
    };
    const up = () => {
      handle.removeEventListener("pointermove", move);
      handle.removeEventListener("pointerup", up);
    };
    handle.addEventListener("pointermove", move);
    handle.addEventListener("pointerup", up);
  }

  // ---------- Atalhos ----------

  function onKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    const key = e.key.toLowerCase();
    const typing = (e.target as HTMLElement)?.matches?.("input, textarea, select");
    if (palette || settingsOpen || showVaults || movePick || confirmState || exportState) return;
    if (mod && e.key === "Tab") {
      e.preventDefault();
      cycleTab(e.shiftKey ? -1 : 1);
    } else if (mod && e.shiftKey && key === "f") {
      e.preventDefault();
      openSearch();
    } else if (mod && key === "p") {
      e.preventDefault();
      openPalette("commands");
    } else if (mod && key === "o") {
      e.preventDefault();
      openPalette("files");
    } else if (mod && key === "g") {
      e.preventDefault();
      openGraph();
    } else if (mod && e.key === ",") {
      e.preventDefault();
      settingsOpen = true;
    } else if (mod && key === "s") {
      e.preventDefault();
      if (activeDoc && activePath) save(activePath);
    } else if (mod && key === "n") {
      e.preventDefault();
      newNote();
    } else if (mod && key === "t") {
      e.preventDefault();
      newTab();
    } else if (mod && key === "w") {
      e.preventDefault();
      closeTab(activeId);
    } else if (mod && key === "e") {
      e.preventDefault();
      livePreview = !livePreview;
    } else if (mod && key === "j" && core.has("chat")) {
      e.preventDefault();
      toggleChat();
    } else if (mod && key === "b") {
      e.preventDefault();
      sidebarOpen = !sidebarOpen;
    } else if (e.key === "F2" && !typing) {
      e.preventDefault();
      if (activeKind === "markdown") titleInput?.focus();
      else if (activePath && !isView(activePath)) renaming = activePath;
    } else if (e.altKey && e.key === "ArrowLeft") {
      e.preventDefault();
      navigate(-1);
    } else if (e.altKey && e.key === "ArrowRight") {
      e.preventDefault();
      navigate(1);
    }
  }

  /** Menu de contexto nativo também em campos de texto; o menu HTML do webview fica desativado. */
  function onContextMenu(e: MouseEvent) {
    if (e.defaultPrevented) return;
    e.preventDefault();
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, [contenteditable]")) popupMenu(["cut", "copy", "paste", "separator", "selectAll"]);
  }

  onMount(() => {
    (async () => {
      try {
        vaults = await invoke<SavedVault[]>("list_vaults");
        if (vaults.length) await loadVault(vaults[0].path);
        else showVaults = true;
      } catch (e) {
        fail(e);
        showVaults = true;
      } finally {
        booting = false;
      }
    })();
    const flush = () => Object.keys(docs).forEach(save);
    // Arquivo solto fora do editor: sem isso o webview navegaria até ele
    const noFileDrop = (e: DragEvent) => {
      if (!e.defaultPrevented && e.dataTransfer?.types.includes("Files")) e.preventDefault();
    };
    window.addEventListener("beforeunload", flush);
    window.addEventListener("focus", reloadChanged);
    window.addEventListener("dragover", noFileDrop);
    window.addEventListener("drop", noFileDrop);
    return () => {
      window.removeEventListener("beforeunload", flush);
      window.removeEventListener("focus", reloadChanged);
      window.removeEventListener("dragover", noFileDrop);
      window.removeEventListener("drop", noFileDrop);
    };
  });
</script>

<svelte:window onkeydown={onKeydown} oncontextmenu={onContextMenu} />

{#snippet treeItems(nodes: TreeNode[])}
  {#each nodes as node (node.path)}
    {#if renaming === node.path}
      {@const isNote = node.kind === "file" && fileKind(node.path) === "markdown"}
      <input
        class="sx-tree-rename"
        value={isNote ? stem(node.name) : node.name}
        spellcheck="false"
        onkeydown={(e) => {
          if (e.key === "Enter") renamePath(node.path, e.currentTarget.value);
          if (e.key === "Escape") renaming = null;
          e.stopPropagation();
        }}
        onblur={(e) => renaming === node.path && renamePath(node.path, e.currentTarget.value)}
        use:autofocus
      />
      {#if node.kind === "folder" && expanded.has(node.path)}
        <div class="sx-tree-children">{@render treeItems(node.children)}</div>
      {/if}
    {:else if node.kind === "folder"}
      {@const open = expanded.has(node.path)}
      <button
        class="sx-tree-row"
        class:is-selected={selectedFolder === node.path}
        class:is-drop-target={dropFolder === node.path}
        class:is-dragging={dragPath === node.path}
        aria-expanded={open}
        draggable="true"
        onclick={() => toggleFolder(node.path)}
        oncontextmenu={(e) => treeMenu(e, node)}
        ondragstart={(e) => onTreeDragStart(e, node)}
        ondragover={(e) => onTreeDragOver(e, node.path)}
        ondrop={(e) => onTreeDrop(e, node.path)}
        ondragend={endTreeDrag}
      >
        <span class="chevron" class:is-open={open}><Icon name="chevron" size={14} stroke={2} /></span>
        <span class="label">{node.name}</span>
      </button>
      {#if open}
        <div class="sx-tree-children">{@render treeItems(node.children)}</div>
      {/if}
    {:else}
      {@const kind = fileKind(node.path)}
      <button
        class="sx-tree-row"
        class:is-active={node.path === activePath}
        class:is-muted={kind === "other"}
        class:is-dragging={dragPath === node.path}
        title={relative(node.path)}
        draggable="true"
        onclick={(e) => openFile(node.path, e.ctrlKey || e.metaKey)}
        onauxclick={(e) => e.button === 1 && openFile(node.path, true)}
        oncontextmenu={(e) => treeMenu(e, node)}
        ondragstart={(e) => onTreeDragStart(e, node)}
        ondragover={(e) => onTreeDragOver(e, dirOf(node.path))}
        ondrop={(e) => onTreeDrop(e, dirOf(node.path))}
        ondragend={endTreeDrag}
      >
        <span class="file-dot"><Icon name={KIND_ICON[kind]} size={kind === "markdown" ? 14 : 13} stroke={kind === "markdown" ? 4 : 1.8} /></span>
        <span class="label">{kind === "markdown" || kind === "excalidraw" ? tabTitle(node.path) : node.name}</span>
        {#if kind === "excalidraw"}<span class="ext">desenho</span>{:else if kind !== "markdown" && node.extension}<span class="ext">{node.extension}</span>{/if}
      </button>
    {/if}
  {/each}
{/snippet}

{#if booting}
  <div class="sx-boot"><Logo size={64} /></div>
{:else}
  <div class="sx-app">
    <nav class="sx-rail" aria-label="Ações">
      <button class="sx-rail-brand" title="Cofres" onclick={() => (showVaults = true)}><Logo size={28} /></button>
      <button
        class="icon-btn"
        class:is-on={sidebarOpen && panel === "files"}
        title="Arquivos (Ctrl+B)"
        onclick={() => (sidebarOpen && panel === "files" ? (sidebarOpen = false) : ((sidebarOpen = true), (panel = "files")))}
      >
        <Icon name="sidebar" />
      </button>
      <button class="icon-btn" class:is-on={sidebarOpen && panel === "search"} title="Buscar no cofre (Ctrl+Shift+F)" onclick={openSearch}>
        <Icon name="search" />
      </button>
      <button class="icon-btn" title="Grafo (Ctrl+G)" disabled={!tree} onclick={openGraph}><Icon name="graph" /></button>
      {#if core.has("jev-home")}
        <button class="icon-btn" class:is-on={activePath === HOME} title="Jev Home" disabled={!tree} onclick={() => openView(HOME)}><Icon name="target" /></button>
      {/if}
      {#if core.has("mesa")}
        <button class="icon-btn" class:is-on={activePath === MESA} title="Mesa" disabled={!tree} onclick={() => openView(MESA)}><Icon name="clipboard" /></button>
      {/if}
      {#if core.has("chat")}
        <button class="icon-btn sx-rail-chat" class:is-on={chatMode !== "closed"} title={chatStore.running ? "Chat — respondendo em segundo plano" : "Chat (Ctrl+J)"} onclick={toggleChat}>
          <Icon name="chat" />{#if chatStore.running && chatMode === "closed"}<i class="sx-rail-badge"></i>{/if}
        </button>
      {/if}
      {#if core.has("jev-organizer")}
        <button class="icon-btn" class:is-on={activePath === ORGANIZER} title="Jev Organizer" disabled={!tree} onclick={() => openView(ORGANIZER)}><Icon name="zap" /></button>
      {/if}
      <button class="icon-btn" title="Paleta de comandos (Ctrl+P)" onclick={() => openPalette("commands")}><Icon name="command" /></button>
      <button class="icon-btn" title="Nova nota (Ctrl+N)" disabled={!tree} onclick={() => newNote()}><Icon name="edit" /></button>
      <div class="sx-rail-spacer"></div>
      <button class="icon-btn" title="Trocar de cofre" onclick={() => (showVaults = true)}><Icon name="vaults" /></button>
      <button class="icon-btn" title="Configurações (Ctrl+,)" disabled={!tree} onclick={() => (settingsOpen = true)}><Icon name="settings" /></button>
    </nav>

    {#if sidebarOpen}
      <aside class="sx-sidebar" style:width="{sidebarWidth}px">
        <div class="sx-sidebar-head">
          <div class="sx-segmented" role="tablist">
            <button role="tab" aria-selected={panel === "files"} class:is-on={panel === "files"} onclick={() => (panel = "files")}>Arquivos</button>
            <button role="tab" aria-selected={panel === "search"} class:is-on={panel === "search"} onclick={openSearch}>Busca</button>
          </div>
        </div>

        {#if !tree}
          <div class="sx-sidebar-empty">
            <p>Escolha uma pasta com suas notas Markdown para começar.</p>
            <button class="btn-primary" onclick={pickVault}>Abrir cofre</button>
          </div>
        {:else if panel === "files"}
          <div class="sx-sidebar-tools">
            <button
              class="sx-vault-name"
              class:is-selected={selectedFolder === null}
              class:is-drop-target={dropFolder === tree.path}
              title={tree.path}
              onclick={() => (selectedFolder = null)}
              oncontextmenu={(e) => treeMenu(e, null)}
              ondragover={(e) => tree && onTreeDragOver(e, tree.path)}
              ondrop={(e) => tree && onTreeDrop(e, tree.path)}
            >
              {tree.name}
            </button>
            <div class="tools">
              <button class="icon-btn sm" title="Nova nota" onclick={() => newNote()}><Icon name="edit" size={15} /></button>
              <button class="icon-btn sm" title="Nova pasta" onclick={() => newFolder()}><Icon name="folder-plus" size={15} /></button>
              <button class="icon-btn sm" title="Recarregar" onclick={() => refreshTree()}><Icon name="refresh" size={15} /></button>
              <button class="icon-btn sm" title="Recolher tudo" onclick={() => expanded.clear()}><Icon name="collapse" size={15} /></button>
            </div>
          </div>
          <div
            class="sx-sidebar-body"
            class:is-drop-target={dropFolder === tree.path}
            role="tree"
            tabindex="-1"
            oncontextmenu={(e) => treeMenu(e, null)}
            ondragover={(e) => tree && onTreeDragOver(e, tree.path)}
            ondragleave={(e) => !e.currentTarget.contains(e.relatedTarget as Node) && (dropFolder = null)}
            ondrop={(e) => tree && onTreeDrop(e, tree.path)}
          >
            {@render treeItems(tree.children)}
          </div>
        {:else}
          <div class="sx-search-box">
            <Icon name="search" size={15} />
            <input
              bind:this={searchInput}
              bind:value={query}
              oninput={onQuery}
              onkeydown={(e) => e.key === "Enter" && hits[0] && openFile(hits[0].path, e.ctrlKey || e.metaKey)}
              placeholder="Buscar no cofre…"
              spellcheck="false"
            />
          </div>
          {#if !query}
            <div class="sx-search-help">
              <code>gato preto</code> todos os termos · <code>"frase exata"</code><br />
              <code>"gato preto"~5</code> termos a até 5 palavras<br />
              <code>gato OR cão</code> · <code>-excluir</code> · <code>title:plano</code>
            </div>
          {/if}
          <div class="sx-sidebar-body">
            {#if query && searchMs !== null}
              <div class="sx-search-meta">{hits.length} resultado{hits.length === 1 ? "" : "s"} · {searchMs.toFixed(0)} ms</div>
            {/if}
            {#each hits as hit (hit.path)}
              <button
                class="sx-result"
                class:is-active={hit.path === activePath}
                onclick={(e) => openFile(hit.path, e.ctrlKey || e.metaKey)}
                onauxclick={(e) => e.button === 1 && openFile(hit.path, true)}
              >
                <div class="sx-result-title">{hit.title}</div>
                <div class="sx-result-path">{relative(hit.path)}</div>
                {#if hit.snippet.length}
                  <div class="sx-result-snippet">
                    {#each hit.snippet as seg}{#if seg.hl}<mark>{seg.text}</mark>{:else}{seg.text}{/if}{/each}
                  </div>
                {/if}
              </button>
            {/each}
          </div>
        {/if}
      </aside>
      <div class="sx-splitter" role="separator" aria-orientation="vertical" onpointerdown={startResize}></div>
    {/if}

    <main class="sx-main" style:padding-left={sidebarOpen ? "0" : "var(--sx-gap)"}>
      <div class="sx-tabbar" role="tablist">
        {#each tabs as tab (tab.id)}
          {@const path = pathOf(tab)}
          <div
            class="sx-tab"
            class:is-active={tab.id === activeId}
            class:is-drop-target={dropTarget === tab.id}
            role="tab"
            tabindex="0"
            aria-selected={tab.id === activeId}
            draggable="true"
            title={path && !isView(path) ? relative(path) : tabTitle(path)}
            onclick={() => (activeId = tab.id)}
            onkeydown={(e) => e.key === "Enter" && (activeId = tab.id)}
            onauxclick={(e) => e.button === 1 && closeTab(tab.id)}
            oncontextmenu={(e) => tabMenu(e, tab)}
            ondragstart={() => (draggedTab = tab.id)}
            ondragover={(e) => {
              e.preventDefault();
              dropTarget = tab.id;
            }}
            ondragleave={() => dropTarget === tab.id && (dropTarget = null)}
            ondrop={() => onTabDrop(tab.id)}
          >
            {#if path && isView(path)}<Icon name={VIEW_ICONS[path] ?? "file"} size={13} />{:else if path && fileKind(path) !== "markdown"}<Icon name={KIND_ICON[fileKind(path)]} size={13} />{/if}
            <span class="title">{tabTitle(path)}</span>
            <button
              class="close"
              aria-label="Fechar aba"
              onclick={(e) => {
                e.stopPropagation();
                closeTab(tab.id);
              }}
            >
              <Icon name="x" size={13} stroke={2} />
            </button>
          </div>
        {/each}
        <button class="icon-btn sm" title="Nova aba (Ctrl+T)" onclick={newTab}><Icon name="plus" size={15} stroke={2} /></button>
        <div class="sx-tabbar-fill" data-tauri-drag-region></div>
      </div>

      <section class="sx-canvas">
        <header class="sx-view-head">
          <div class="sx-view-nav">
            <button class="icon-btn sm" title="Voltar (Alt+←)" disabled={activeTab.index <= 0} onclick={() => navigate(-1)}>
              <Icon name="back" size={16} stroke={2} />
            </button>
            <button
              class="icon-btn sm"
              title="Avançar (Alt+→)"
              disabled={activeTab.index >= activeTab.history.length - 1}
              onclick={() => navigate(1)}
            >
              <Icon name="forward" size={16} stroke={2} />
            </button>
          </div>
          <div class="sx-crumbs">
            {#each crumbs as part}<span>{part}</span><span>/</span>{/each}
            {#if activePath}<span class="current">{tabTitle(activePath)}</span>{/if}
          </div>
        </header>

        {#if activePath === GRAPH}
          <GraphView {notes} activePath={tabs.map(pathOf).find((p) => p && !isView(p)) ?? null} onopen={(p) => openFile(p, true)} />
        {:else if activePath === HOME}
          <JevHome
            vaultName={tree?.name ?? ""}
            settings={jevHomeCfg}
            hasKey={typesafeKey > 0}
            recent={recent
              .filter((p) => files.some((f) => f.path === p))
              .slice(0, 7)
              .map((p) => ({ abs: p, title: stem(p), folder: relative(dirOf(p)).replace(/\\/g, "/") || "/" }))}
            onopen={(p, n) => openFile(p, n || jevHomeCfg.openIn === "tab")}
            onsettings={() => openPluginSettings("jev-home")}
          />
        {:else if activePath === ORGANIZER}
          <Organizer
            activePath={tabs.map(pathOf).find((p) => p && !isView(p)) ?? null}
            high={organizerCfg.high}
            hasKey={typesafeKey > 0}
            onopen={(rel) => tree && openFile(absFromRel(rel), true)}
            onnotify={notify}
            onchanged={() => refreshTree()}
          />
        {:else if activePath === MESA}
          <Mesa
            settings={mesaCfg}
            files={files.map((f) => ({ path: f.path, rel: relative(f.path).replace(/\\/g, "/"), name: stem(f.name) }))}
            onopen={(rel, n) => openFile(absFromRel(rel), n)}
            onopenlink={(t) => followLink(t, true)}
            onnotify={notify}
            onsettings={() => openPluginSettings("mesa")}
            captureRequest={mesaCapture}
          />
        {:else if activePath && activeKind === "canvas"}
          {#key activePath}
            <CanvasView
              path={activePath}
              vaultRoot={tree?.path ?? ""}
              {resolveFile}
              onopen={(p, n) => openFile(p, n)}
              onopenlink={(t) => followLink(t, true)}
              onnotify={notify}
            />
          {/key}
        {:else if activePath && activeKind === "excalidraw"}
          {#key activePath}
            <ExcalidrawView path={activePath} compress={excalidrawCfg.compress !== false} {resolveFile} onnotify={notify} />
          {/key}
        {:else if activePath && activeKind === "markdown" && activeDoc}
          {@const path = activePath}
          <div class="sx-doc">
            {#key path}
              <input
                class="sx-doc-title"
                bind:this={titleInput}
                value={stem(path)}
                spellcheck="false"
                aria-label="Título da nota"
                onkeydown={(e) => {
                  if (e.key === "Enter" || e.key === "ArrowDown") {
                    e.preventDefault();
                    e.currentTarget.blur();
                    editor?.focus();
                  }
                  if (e.key === "Escape") {
                    e.currentTarget.value = stem(path);
                    e.currentTarget.blur();
                  }
                }}
                onblur={(e) => renamePath(path, e.currentTarget.value)}
              />
            {/key}
            <Editor
              bind:this={editor}
              {path}
              content={activeDoc.content}
              host={editorHost}
              {livePreview}
              onchange={onEdit}
              onblur={save}
              oncontextmenu={editorMenu}
              onexitup={() => {
                titleInput?.focus();
                titleInput?.setSelectionRange(titleInput.value.length, titleInput.value.length);
              }}
            />
          </div>
          {#if showDock}
            <FormatDock {view} />
          {/if}
          <div class="sx-status">
            <button class="sx-status-item" title="Alternar modo fonte (Ctrl+E)" onclick={() => (livePreview = !livePreview)}>
              {livePreview ? "Ao vivo" : "Fonte"}
            </button>
            <span class="sx-status-item" title="Backlinks"><Icon name="link" size={12} stroke={2} /> {backlinks}</span>
            <span class="sx-status-item">{wordCount} palavra{wordCount === 1 ? "" : "s"}</span>
            <span class="sx-status-item" title={saveState === "saved" ? "Salvo" : saveState === "saving" ? "Salvando…" : "Alterações pendentes"}>
              <span class="sx-status-dot" class:is-dirty={saveState === "dirty"} class:is-saving={saveState === "saving"}></span>
              {saveState === "saved" ? "Salvo" : saveState === "saving" ? "Salvando" : "Editado"}
            </span>
          </div>
        {:else if activePath && (activeKind === "image" || activeKind === "video" || activeKind === "audio")}
          <MediaView path={activePath} />
        {:else if activePath && activeKind === "pdf"}
          <PdfView path={activePath} />
        {:else if activePath && activeKind === "sheet"}
          <SheetView path={activePath} />
        {:else if activePath && activeKind === "other"}
          {@const path = activePath}
          <div class="sx-empty">
            <div class="sx-empty-card">
              <Icon name="file" size={40} stroke={1.4} />
              <h2>{baseName(path)}</h2>
              <p>O Silex ainda não visualiza este tipo de arquivo.</p>
              <div class="sx-empty-actions">
                <button onclick={() => openPath(path)}>Abrir no aplicativo padrão</button>
                <button onclick={() => revealItemInDir(path)}>Mostrar no Explorador</button>
              </div>
            </div>
          </div>
        {:else}
          <div class="sx-empty">
            <div class="sx-empty-card">
              <Logo size={52} />
              <h2>{tree ? "Nenhuma nota aberta" : "Bem-vindo ao Silex"}</h2>
              <p>{tree ? "Escolha uma nota na barra lateral ou use a paleta de comandos." : "Abra uma pasta de notas Markdown para começar."}</p>
              <div class="sx-empty-actions">
                {#if tree}
                  <button onclick={() => newNote()}>Nova nota <kbd>Ctrl N</kbd></button>
                  <button onclick={() => openPalette("files")}>Abrir arquivo <kbd>Ctrl O</kbd></button>
                  <button onclick={() => openPalette("commands")}>Paleta de comandos <kbd>Ctrl P</kbd></button>
                  <button onclick={openGraph}>Grafo <kbd>Ctrl G</kbd></button>
                {:else}
                  <button onclick={pickVault}>Abrir cofre</button>
                {/if}
              </div>
            </div>
          </div>
        {/if}

        {#if toast}
          <button class="sx-toast" class:is-error={toast.error} title="Clique para dispensar" onclick={() => (toast = null)}>{toast.text}</button>
        {/if}
      </section>
    </main>

    {#if chatMode === "side" && core.has("chat")}
      <div class="sx-splitter" role="separator" aria-orientation="vertical" onpointerdown={startChatResize}></div>
      <aside class="sx-chat-side" style:width="{chatWidth}px">
        {@render chatPanel("side")}
      </aside>
    {/if}
  </div>
  {#if chatMode === "popup" && core.has("chat")}
    {@render chatPanel("popup")}
  {/if}
{/if}

{#snippet chatPanel(mode: "popup" | "side")}
  <ChatPanel
    {mode}
    settings={chatCfg}
    notePath={activeKind === "markdown" ? activePath : null}
    noteTitle={activeKind === "markdown" && activePath ? stem(activePath) : null}
    onmode={setChatMode}
    onclose={() => setChatMode("closed")}
    notes={linkTargets.notes}
    onopenlink={(t) => followLink(t, false)}
    onopenpath={(rel) => openFile(absFromRel(rel))}
    onerror={fail}
    onopenurl={(u) => /^https?:\/\//i.test(u) && openUrl(u)}
  />
{/snippet}

{#if templatePick}
  <Palette items={templateItems} placeholder="Escolha um template…" onclose={closeTemplatePick} />
{/if}

{#if movePick && tree}
  <Palette
    items={moveItems}
    placeholder={`Mover “${baseName(movePick)}” para…`}
    onclose={() => (movePick = null)}
  />
{/if}

{#if exportState}
  {@const ex = exportState}
  <ExportModal
    note={ex.note}
    format={ex.format}
    label={ex.label}
    defaultName={stem(ex.note)}
    openAfter={exportCfg.openAfter !== false}
    ondone={(out, open) => {
      notify(`Exportado: ${baseName(out)}`);
      if (open) openPath(out).catch(fail);
    }}
    onerror={fail}
    onclose={() => (exportState = null)}
  />
{/if}

{#if confirmState}
  <div
    class="sx-overlay"
    role="presentation"
    onmousedown={(e) => e.target === e.currentTarget && answerConfirm(false)}
    onkeydown={(e) => e.key === "Escape" && answerConfirm(false)}
  >
    <form
      class="sx-prompt"
      onsubmit={(e) => {
        e.preventDefault();
        answerConfirm(true);
      }}
    >
      <div class="sx-prompt-title">Excluir</div>
      <p class="sx-confirm-text">{confirmState.text}</p>
      <div class="mesa-form-actions">
        <button type="button" class="btn-ghost" onclick={() => answerConfirm(false)}>Cancelar</button>
        <button type="submit" class="btn-primary is-danger" use:autofocusBtn>{confirmState.ok}</button>
      </div>
    </form>
  </div>
{/if}

{#if promptState}
  <PromptModal title={promptState.title} prompts={promptState.prompts} onsubmit={submitPrompts} onclose={closePrompts} />
{/if}

{#if palette === "commands"}
  <Palette items={commandItems} placeholder="Digite um comando…" onclose={() => (palette = null)} />
{:else if palette === "files"}
  <Palette
    items={fileItems}
    placeholder="Abrir arquivo…"
    fallback={(q) =>
      tree ? { id: `create:${q}`, title: `Criar nota “${q}”`, icon: "plus", run: ({ ctrl }) => createNote(tree!.path, q).then((p) => openFile(p, ctrl)).catch(fail) } : null}
    onclose={() => (palette = null)}
  />
{/if}

{#if settingsOpen && tree}
  <Settings
    {config}
    {appearance}
    {plugins}
    vaultName={tree.name}
    onconfig={updateConfig}
    oninstall={installPlugin}
    onopenplugins={() => invoke<string>("plugins_dir").then(openPath).catch(fail)}
    onopenthemes={() => appearance.dir && openPath(appearance.dir).catch(fail)}
    onimport={importObsidian}
    onvaults={() => ((settingsOpen = false), (showVaults = true))}
    onclose={async () => {
      settingsOpen = false;
      settingsFocus = null;
      typesafeKey = await invoke<number>("secret_len", { name: "typesafe" });
    }}
    onaction={pluginAction}
    focusPlugin={settingsFocus}
  />
{/if}

{#if showVaults || (!booting && !tree)}
  <VaultPicker
    {vaults}
    current={tree?.path ?? null}
    onopen={loadVault}
    onforget={forgetVault}
    onpick={pickVault}
    onclose={tree ? () => (showVaults = false) : undefined}
  />
{/if}
