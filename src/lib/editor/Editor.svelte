<script module lang="ts">
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView, keymap, drawSelection, placeholder } from "@codemirror/view";
  import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
  import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
  import { syntaxHighlighting, indentOnInput, bracketMatching } from "@codemirror/language";
  import { markdown, markdownLanguage } from "@codemirror/lang-markdown";
  import { yamlFrontmatter } from "@codemirror/lang-yaml";
  import { languages } from "@codemirror/language-data";
  import { autocompletion, type CompletionContext, type CompletionResult } from "@codemirror/autocomplete";
  import { invoke } from "@tauri-apps/api/core";
  import { Highlight } from "../markdown/extensions";
  import { hostFacet, revealExtensions, type EditorHost } from "./context";
  import { inlinePreview } from "./inline";
  import { blockPreview } from "./blocks";
  import { moveLines, selectCodeBlock, setTaskStatus } from "./commands";
  import { silexHighlight, silexEditorTheme } from "./theme";
  import { frontmatterRange } from "./widgets";

  /** Callbacks do componente montado. As extensões ficam no módulo e sempre leem daqui,
   *  então estados guardados em cache nunca chamam callbacks de um componente destruído. */
  type Bridge = {
    path: string;
    host: Omit<EditorHost, "path">;
    livePreview: boolean;
    onchange: (path: string, content: string) => void;
    onblur?: (path: string) => void;
    oncontextmenu?: (view: EditorView, e: MouseEvent) => void;
    onexitup?: () => void;
  };
  let bridge: Bridge | null = null;

  const preview = new Compartment();
  const previewExtensions = [inlinePreview, blockPreview];

  const editorHost: EditorHost = {
    path: () => bridge?.path ?? "",
    openLink: (t, n) => bridge?.host.openLink(t, n),
    openExternal: (u) => bridge?.host.openExternal(u),
    resolveFile: (n) => bridge?.host.resolveFile(n) ?? null,
    assetUrl: (p) => bridge?.host.assetUrl(p) ?? p,
    renderBlock: (l, s, c) => bridge?.host.renderBlock(l, s, c) ?? Promise.resolve(null),
    blockLangs: () => bridge?.host.blockLangs() ?? new Set(),
    excelPaste: () => bridge?.host.excelPaste() ?? false,
    taskMenu: (v, p) => bridge?.host.taskMenu(v, p),
    linkTargets: (e) => bridge?.host.linkTargets(e) ?? [],
    saveImage: (d, x) => bridge?.host.saveImage(d, x) ?? Promise.resolve(null),
  };

  /** [[ sugere notas e ![[ sugere qualquer arquivo; filtra (fuzzy) conforme se digita. */
  function wikiCompletions(ctx: CompletionContext): CompletionResult | null {
    const m = ctx.matchBefore(/!?\[\[[^[\]\n|#]*$/);
    if (!m) return null;
    const embed = m.text.startsWith("!");
    // Pula um "]]" que já esteja logo depois do cursor
    const close = ctx.state.sliceDoc(ctx.pos, ctx.pos + 2) === "]]" ? 2 : 0;
    return {
      from: m.from + (embed ? 3 : 2),
      validFor: /^[^[\]\n|#]*$/,
      options: editorHost.linkTargets(embed).map((t) => ({
        label: t.label,
        detail: t.detail,
        apply: (view: EditorView, _c: unknown, from: number, to: number) => {
          const insert = `${t.link}]]`;
          view.dispatch({ changes: { from, to: to + close, insert }, selection: { anchor: from + insert.length } });
        },
      })),
    };
  }

  const IMAGE_EXT: Record<string, string> = {
    "image/png": "png",
    "image/jpeg": "jpg",
    "image/gif": "gif",
    "image/webp": "webp",
    "image/bmp": "bmp",
    "image/svg+xml": "svg",
    "image/avif": "avif",
  };
  const ONLY_IMAGE_HTML = /^\s*(<meta[^>]*>)?\s*(<html>\s*<body>)?\s*(<!--StartFragment-->)?\s*<img[^>]*>\s*(<!--EndFragment-->)?\s*(<\/body>\s*<\/html>)?\s*$/i;

  /** Imagem na área de transferência (print, "copiar imagem")? Texto que vem com imagem (Word, Excel) continua texto. */
  function clipboardImage(data: DataTransfer | null): File | null {
    const file = data ? [...data.files].find((f) => IMAGE_EXT[f.type]) : undefined;
    if (!data || !file) return null;
    return !data.getData("text/plain").trim() || ONLY_IMAGE_HTML.test(data.getData("text/html")) ? file : null;
  }

  const extensions = [
    hostFacet.of(editorHost),
    history(),
    drawSelection(),
    indentOnInput(),
    bracketMatching(),
    highlightSelectionMatches(),
    EditorView.lineWrapping,
    placeholder("Comece a escrever…"),
    yamlFrontmatter({
      content: markdown({ base: markdownLanguage, codeLanguages: languages, extensions: [Highlight] }),
    }),
    syntaxHighlighting(silexHighlight),
    silexEditorTheme,
    revealExtensions,
    autocompletion({ override: [wikiCompletions], icons: false, maxRenderedOptions: 50 }),
    keymap.of([
      { key: "Mod-Enter", run: (v) => (setTaskStatus(v, null), true) },
      // Ctrl+L cria a tarefa (ou alterna [ ] ↔ [x]), como no Obsidian
      { key: "Mod-l", run: (v) => (setTaskStatus(v, null), true) },
      { key: "Mod-Shift-ArrowUp", run: (v) => moveLines(v, -1, !!bridge?.livePreview) },
      { key: "Mod-Shift-ArrowDown", run: (v) => moveLines(v, 1, !!bridge?.livePreview) },
      { key: "Mod-a", run: selectCodeBlock },
      {
        key: "ArrowUp",
        run: (v) => {
          const sel = v.state.selection.main;
          if (!sel.empty || !bridge?.onexitup) return false;
          const fm = bridge.livePreview ? frontmatterRange(v.state) : null;
          const firstBody = fm ? Math.min(fm.end + 1, v.state.doc.length) : 0;
          const line = v.state.doc.lineAt(sel.head);
          if (line.from > firstBody) return false;
          // Só sai se o cursor estiver na primeira linha visual (texto pode quebrar)
          const top = v.coordsAtPos(line.from)?.top;
          const here = v.coordsAtPos(sel.head)?.top;
          if (top !== undefined && here !== undefined && here - top > 4) return false;
          bridge.onexitup();
          return true;
        },
      },
      indentWithTab,
      ...defaultKeymap,
      ...historyKeymap,
      ...searchKeymap,
    ]),
    EditorView.updateListener.of((u) => {
      if (u.docChanged && bridge) bridge.onchange(bridge.path, u.state.doc.toString());
    }),
    EditorView.domEventHandlers({
      blur: () => {
        if (bridge) bridge.onblur?.(bridge.path);
      },
      contextmenu: (e, v) => {
        if (!bridge?.oncontextmenu) return false;
        e.preventDefault();
        bridge.oncontextmenu(v, e);
        return true;
      },
      drop: (e, v) => {
        // Arquivos soltos do Explorer: imagens viram anexos; o resto é ignorado
        // (o CodeMirror inseriria o conteúdo binário como texto)
        const dropped = [...(e.dataTransfer?.files ?? [])];
        if (!dropped.length) return false;
        e.preventDefault();
        const pos = v.posAtCoords({ x: e.clientX, y: e.clientY }) ?? v.state.selection.main.head;
        (async () => {
          const names: string[] = [];
          for (const f of dropped.filter((f) => IMAGE_EXT[f.type])) {
            const name = await editorHost.saveImage(new Uint8Array(await f.arrayBuffer()), IMAGE_EXT[f.type]);
            if (name) names.push(`![[${name}]]`);
          }
          if (names.length && v.dom.isConnected) {
            const insert = names.join("\n");
            const at = Math.min(pos, v.state.doc.length);
            v.dispatch({ changes: { from: at, insert }, selection: { anchor: at + insert.length } });
          }
        })();
        return true;
      },
      paste: (e, v) => {
        // Células copiadas do Excel viram tabela Markdown (port do "Excel to Markdown Table")
        const text = e.clipboardData?.getData("text/plain") ?? "";
        if (bridge?.host.excelPaste() && text.includes("\t") && text.includes("\n")) {
          e.preventDefault();
          invoke<string | null>("tsv_to_markdown", { text }).then((md) => {
            v.dispatch(v.state.replaceSelection(md ?? text));
          });
          return true;
        }
        // Imagem colada vira anexo do cofre (gravado pelo Rust) + ![[embed]]
        const image = clipboardImage(e.clipboardData);
        if (!image) return false;
        e.preventDefault();
        image
          .arrayBuffer()
          .then((buf) => editorHost.saveImage(new Uint8Array(buf), IMAGE_EXT[image.type]))
          .then((name) => {
            if (name && v.dom.isConnected) v.dispatch(v.state.replaceSelection(`![[${name}]]`), { scrollIntoView: true });
          });
        return true;
      },
    }),
  ];

  // Um EditorState por arquivo: preserva desfazer/refazer e cursor ao trocar de aba
  const states = new Map<string, EditorState>();

  export function resetEditorStates() {
    states.clear();
  }

  export function renameEditorState(from: string, to: string) {
    const s = states.get(from);
    if (s) {
      states.delete(from);
      states.set(to, s);
    }
  }

  function stateFor(p: string, text: string, live: boolean) {
    const mode = preview.reconfigure(live ? previewExtensions : []);
    const cached = states.get(p);
    // Se o arquivo mudou por fora (recarregado do disco), descarta o estado antigo
    if (cached && cached.doc.toString() === text) return cached.update({ effects: mode }).state;
    return EditorState.create({ doc: text, extensions: [extensions, preview.of(live ? previewExtensions : [])] });
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";

  type Props = Omit<Bridge, "livePreview"> & { content: string; livePreview?: boolean };
  let { path, content, host, onchange, onblur, oncontextmenu, onexitup, livePreview = true }: Props = $props();

  let hostEl: HTMLDivElement;
  let view: EditorView | undefined;
  let currentPath = "";

  // Mantém a ponte apontando para as props atuais deste componente
  $effect.pre(() => {
    bridge = { path, host, livePreview, onchange, onblur, oncontextmenu, onexitup };
  });

  $effect(() => {
    const p = path;
    untrack(() => {
      if (!view) {
        currentPath = p;
        view = new EditorView({ state: stateFor(p, content, livePreview), parent: hostEl });
      } else if (p !== currentPath) {
        states.set(currentPath, view.state);
        currentPath = p;
        view.setState(stateFor(p, content, livePreview));
      }
    });
  });

  // Modo fonte ↔ visualização ao vivo
  $effect(() => {
    const on = livePreview;
    untrack(() => view?.dispatch({ effects: preview.reconfigure(on ? previewExtensions : []) }));
  });

  // Conteúdo alterado fora do editor (ex.: arquivo mudou no disco)
  $effect(() => {
    const text = content;
    untrack(() => {
      if (view && path === currentPath && view.state.doc.toString() !== text) {
        view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
      }
    });
  });

  $effect(() => () => {
    if (view) {
      states.set(currentPath, view.state);
      view.destroy();
      view = undefined;
    }
  });

  export function focus() {
    view?.focus();
  }

  export function getView() {
    return view;
  }
</script>

<div class="editor-host" bind:this={hostEl}></div>

<style>
  .editor-host {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .editor-host :global(.cm-editor) {
    flex: 1;
  }
</style>
