import { EditorView, WidgetType } from "@codemirror/view";
import type { EditorState, Text } from "@codemirror/state";
import { parseDocument, isScalar, type Document } from "yaml";
import { host, type BlockResult, type InlineNode, type ViewNode } from "./context";
import { renderInline, safeColor } from "../markdown/render";
import { popupMenu } from "../menu";
import { svgIcon } from "../icons";
import { taskStatus, TASK_LINE } from "./tasks";
import { calloutKind } from "./callouts";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls?: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}

/**
 * Invólucro de todo widget de bloco. O espaçamento é padding, nunca margin:
 * o CodeMirror mede a altura do elemento sem as margens, e cada pixel de
 * margem desalinhava cliques e setas nas linhas abaixo.
 */
function shell(inner: HTMLElement, cls = ""): HTMLElement {
  const outer = el("div", `sx-block-shell ${cls}`.trim());
  outer.append(inner);
  return outer;
}

/** Links dentro de widgets: [[wikilinks]] abrem notas, URLs abrem no navegador. */
function wireLinks(root: HTMLElement, view: EditorView) {
  root.addEventListener("click", (e) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    e.stopPropagation();
    if (a.dataset.target) host(view.state).openLink(a.dataset.target, e.ctrlKey || e.metaKey);
    else if (a.getAttribute("href")) host(view.state).openExternal(a.getAttribute("href")!);
  });
}

/** Clique fora de links coloca o cursor no início do bloco para editar o Markdown. */
function clickToEdit(dom: HTMLElement, view: EditorView) {
  dom.addEventListener("mousedown", (e) => {
    if ((e.target as HTMLElement).closest("a, button, input, textarea, select, video, audio")) return;
    e.preventDefault();
    const pos = view.posAtDOM(dom);
    view.dispatch({ selection: { anchor: pos } });
    view.focus();
  });
}

// ---------- Inline ----------

export class CheckboxWidget extends WidgetType {
  constructor(readonly status: string) {
    super();
  }
  eq(other: CheckboxWidget) {
    return other.status === this.status;
  }
  toDOM(view: EditorView) {
    const st = taskStatus(this.status);
    // O respiro até o texto é padding do invólucro, não margem da caixa: o cursor
    // numa tarefa vazia é desenhado na borda do widget e ficava colado no checkbox
    const slot = el("span", "sx-task-slot");
    const box = el("span", "sx-task", st.glyph);
    slot.append(box);
    box.dataset.status = st.name;
    box.title = `${st.label} — clique para alternar, botão direito para mudar o status`;
    const statusPos = () => {
      const line = view.state.doc.lineAt(view.posAtDOM(box));
      const m = TASK_LINE.exec(line.text);
      return m ? line.from + m[1].length : -1;
    };
    box.addEventListener("mousedown", (e) => {
      e.preventDefault();
      if (e.button !== 0) return;
      const pos = statusPos();
      if (pos < 0) return;
      const next = this.status === " " ? "x" : " ";
      view.dispatch({ changes: { from: pos, to: pos + 1, insert: next } });
    });
    box.addEventListener("contextmenu", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const pos = statusPos();
      if (pos >= 0) host(view.state).taskMenu(view, pos);
    });
    return slot;
  }
  /** Cliques no respiro ao lado da caixa posicionam o cursor normalmente. */
  ignoreEvent(e: Event) {
    return !!(e.target as HTMLElement).closest?.(".sx-task");
  }
}

export class BulletWidget extends WidgetType {
  eq() {
    return true;
  }
  toDOM() {
    return el("span", "sx-bullet", "•");
  }
}

export class CodeHeaderWidget extends WidgetType {
  constructor(readonly lang: string) {
    super();
  }
  eq(other: CodeHeaderWidget) {
    return other.lang === this.lang;
  }
  toDOM(view: EditorView) {
    const wrap = el("span", "sx-code-header");
    wrap.append(el("span", "sx-code-lang", this.lang || "texto"));
    const copy = el("button", "sx-code-copy");
    copy.title = "Copiar código";
    copy.append(svgIcon("copy", 13));
    copy.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
      const doc = view.state.doc;
      let n = doc.lineAt(view.posAtDOM(wrap)).number + 1;
      const lines: string[] = [];
      while (n <= doc.lines && !/^\s*(```|~~~)/.test(doc.line(n).text)) lines.push(doc.line(n++).text);
      navigator.clipboard.writeText(lines.join("\n"));
      copy.classList.add("is-done");
      setTimeout(() => copy.classList.remove("is-done"), 900);
    });
    wrap.append(copy);
    return wrap;
  }
  ignoreEvent() {
    return true;
  }
}

export class CalloutTitleWidget extends WidgetType {
  constructor(
    readonly type: string,
    readonly showDefaultTitle: boolean,
  ) {
    super();
  }
  eq(other: CalloutTitleWidget) {
    return other.type === this.type && other.showDefaultTitle === this.showDefaultTitle;
  }
  toDOM() {
    const kind = calloutKind(this.type);
    const wrap = el("span", "sx-callout-icon");
    wrap.append(svgIcon(kind.icon, 16));
    if (this.showDefaultTitle) wrap.append(el("span", "sx-callout-default-title", kind.title));
    return wrap;
  }
}

const IMAGE = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;
const VIDEO = /\.(mp4|webm|mov|m4v|ogv)$/i;
const AUDIO = /\.(mp3|wav|m4a|ogg|flac|aac)$/i;

export class EmbedWidget extends WidgetType {
  constructor(
    readonly target: string,
    readonly size: string,
  ) {
    super();
  }
  eq(other: EmbedWidget) {
    return other.target === this.target && other.size === this.size;
  }
  toDOM(view: EditorView) {
    const h = host(view.state);
    const path = h.resolveFile(this.target);
    const name = this.target.split(/[\\/]/).pop()!;
    if (!path) {
      const miss = el("span", "sx-embed-missing", `“${name}” não encontrado`);
      return miss;
    }
    const url = h.assetUrl(path);
    const [w, hgt] = this.size.split("x").map((n) => parseInt(n, 10));
    if (IMAGE.test(path)) {
      const img = el("img", "sx-embed-img");
      img.src = url;
      img.alt = name;
      img.loading = "lazy";
      img.draggable = false;
      if (w) img.style.width = `${w}px`;
      if (hgt) img.style.height = `${hgt}px`;
      img.addEventListener("dblclick", () => h.openLink(this.target, true));
      img.addEventListener("load", () => view.requestMeasure());
      return img;
    }
    if (VIDEO.test(path) || AUDIO.test(path)) {
      const media = el(VIDEO.test(path) ? "video" : "audio", "sx-embed-media");
      media.src = url;
      media.controls = true;
      media.preload = "metadata";
      if (w) media.style.width = `${w}px`;
      media.addEventListener("loadedmetadata", () => view.requestMeasure());
      return media;
    }
    const card = el("span", "sx-embed-card");
    card.append(svgIcon(/\.pdf$/i.test(path) ? "pdf" : /\.(xlsx?|xlsm|xlsb|ods|csv)$/i.test(path) ? "sheet" : "file", 16));
    card.append(el("span", "", name));
    card.addEventListener("mousedown", (e) => {
      e.preventDefault();
      h.openLink(this.target, e.ctrlKey || e.metaKey);
    });
    return card;
  }
  ignoreEvent() {
    return true;
  }
}

export class ImageWidget extends WidgetType {
  constructor(
    readonly src: string,
    readonly alt: string,
  ) {
    super();
  }
  eq(other: ImageWidget) {
    return other.src === this.src && other.alt === this.alt;
  }
  toDOM(view: EditorView) {
    const h = host(view.state);
    let src = this.src;
    if (!/^(https?:|data:)/i.test(src)) {
      const path = h.resolveFile(decodeURIComponent(src));
      if (!path) return el("span", "sx-embed-missing", `“${src}” não encontrado`);
      src = h.assetUrl(path);
    }
    const img = el("img", "sx-embed-img");
    img.src = src;
    img.alt = this.alt;
    img.loading = "lazy";
    const size = /\|(\d+)(?:x(\d+))?$/.exec(this.alt);
    if (size) img.style.width = `${size[1]}px`;
    img.addEventListener("load", () => view.requestMeasure());
    return img;
  }
  ignoreEvent() {
    return true;
  }
}

// ---------- Blocos ----------

// ---------- Tabela interativa ----------
//
// A tabela fica sempre renderizada na visualização ao vivo e cada célula é
// editável no lugar (o Markdown cru só aparece no modo fonte). A célula em
// edição mostra o texto cru; as demais, o Markdown renderizado. Ao sair de uma
// célula alterada, a tabela inteira é reescrita (alinhada) no documento.

type Align = "" | "left" | "center" | "right";
/** rows[0] é o cabeçalho. */
export type TableModel = { rows: string[][]; aligns: Align[] };
type TableState = { source: string; model: TableModel; editing: HTMLElement | null; painting: boolean };
type TableDom = HTMLElement & { _table?: TableState };

function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") {
      cur += "\\|";
      i++;
    } else if (s[i] === "|") {
      cells.push(cur.trim());
      cur = "";
    } else cur += s[i];
  }
  cells.push(cur.trim());
  return cells;
}

export function parseTable(src: string): TableModel {
  const lines = src.split("\n").filter((l) => l.trim());
  const head = splitRow(lines[0] ?? "");
  const aligns = splitRow(lines[1] ?? "").map((d): Align => {
    const l = d.startsWith(":");
    const r = d.endsWith(":");
    return l && r ? "center" : r ? "right" : l ? "left" : "";
  });
  const body = lines.slice(2).map(splitRow);
  const cols = Math.max(1, head.length, ...body.map((r) => r.length));
  const fit = (r: string[]) => Array.from({ length: cols }, (_, i) => r[i] ?? "");
  return { rows: [fit(head), ...body.map(fit)], aligns: Array.from({ length: cols }, (_, i) => aligns[i] ?? "") };
}

export function formatTable({ rows, aligns }: TableModel): string {
  const width = aligns.map((_, c) => Math.max(3, ...rows.map((r) => r[c].length)));
  const line = (r: string[]) => `| ${r.map((cell, c) => cell.padEnd(width[c])).join(" | ")} |`;
  const delim = aligns.map((a, c) => {
    const w = width[c];
    if (a === "center") return `:${"-".repeat(w - 2)}:`;
    if (a === "right") return `${"-".repeat(w - 1)}:`;
    if (a === "left") return `:${"-".repeat(w - 1)}`;
    return "-".repeat(w);
  });
  return [line(rows[0]), `| ${delim.join(" | ")} |`, ...rows.slice(1).map(line)].join("\n");
}

// Na edição a célula mostra "|" puro; no Markdown ele precisa de escape
const unescapeCell = (raw: string) => raw.replace(/\\\|/g, "|");
const escapeCell = (text: string) => text.replace(/\r?\n/g, " ").trim().replace(/\|/g, "\\|");

/** Célula a focar quando a tabela for recriada depois de uma alteração. */
let pendingTableFocus: { pos: number; r: number; c: number } | null = null;

/** Insere uma tabela 2×2 no cursor e já foca a primeira célula. */
export function insertTable(view: EditorView) {
  const { state } = view;
  const line = state.doc.lineAt(state.selection.main.head);
  const before = line.text.trim() ? "\n\n" : line.from > 0 && state.doc.lineAt(line.from - 1).text.trim() ? "\n" : "";
  const at = line.text.trim() ? line.to : line.from;
  const text = formatTable({ rows: [["Coluna", "Coluna"], ["", ""]], aligns: ["", ""] });
  pendingTableFocus = { pos: at + before.length, r: 0, c: 0 };
  view.dispatch({
    changes: { from: at, insert: before + text + "\n" },
    selection: { anchor: at + before.length + text.length + 1 },
  });
  view.focus();
}

function cellAt(dom: HTMLElement, r: number, c: number) {
  return dom.querySelector<HTMLElement>(`[data-r="${r}"][data-c="${c}"] .sx-cell`);
}

function cellPos(box: HTMLElement) {
  const cell = box.parentElement!;
  return { r: Number(cell.dataset.r), c: Number(cell.dataset.c) };
}

function paintTable(dom: TableDom) {
  const st = dom._table!;
  st.painting = true;
  const { rows, aligns } = st.model;
  const table = el("table");
  const thead = el("thead");
  const tbody = el("tbody");
  rows.forEach((row, r) => {
    const tr = el("tr");
    row.forEach((raw, c) => {
      const cell = el(r === 0 ? "th" : "td");
      cell.dataset.r = String(r);
      cell.dataset.c = String(c);
      if (aligns[c]) cell.style.textAlign = aligns[c];
      const box = el("div", "sx-cell");
      box.contentEditable = "plaintext-only";
      box.innerHTML = renderInline(unescapeCell(raw));
      cell.append(box);
      tr.append(cell);
    });
    (r === 0 ? thead : tbody).append(tr);
  });
  table.append(thead, tbody);
  const addRow = el("button", "sx-table-add is-row");
  addRow.title = "Adicionar linha";
  addRow.append(svgIcon("plus", 12, 2.2));
  const addCol = el("button", "sx-table-add is-col");
  addCol.title = "Adicionar coluna";
  addCol.append(svgIcon("plus", 12, 2.2));
  (dom.firstElementChild as HTMLElement).replaceChildren(table, addRow, addCol);
  st.editing = null;
  st.painting = false;
}

/** Reescreve a tabela no documento a partir do modelo. */
function commitTable(dom: TableDom, view: EditorView, next: { r: number; c: number } | null) {
  const st = dom._table!;
  if (!dom.isConnected) return;
  const from = view.posAtDOM(dom);
  const to = from + st.source.length;
  if (view.state.sliceDoc(from, to) !== st.source) return;
  const text = formatTable(st.model);
  if (text === st.source) return;
  st.source = text;
  pendingTableFocus = next ? { pos: from, ...next } : null;
  view.dispatch({ changes: { from, to, insert: text }, userEvent: "input.table" });
}

export class TableWidget extends WidgetType {
  constructor(readonly source: string) {
    super();
  }
  eq(other: TableWidget) {
    return other.source === this.source;
  }
  /** Depois de uma edição feita pela própria tabela o DOM já está certo: mantém (e o foco junto). */
  updateDOM(dom: HTMLElement, _view: EditorView) {
    const st = (dom as TableDom)._table;
    if (!st) return false;
    if (st.source !== this.source) {
      st.source = this.source;
      st.model = parseTable(this.source);
      paintTable(dom as TableDom);
    }
    return true;
  }
  toDOM(view: EditorView) {
    const dom = shell(el("div", "sx-table-wrap sx-table-edit")) as TableDom;
    const st: TableState = { source: this.source, model: parseTable(this.source), editing: null, painting: false };
    dom._table = st;
    paintTable(dom);
    wireLinks(dom, view);

    const readEditing = () => {
      const box = st.editing;
      if (!box) return false;
      const { r, c } = cellPos(box);
      const value = escapeCell(box.textContent ?? "");
      if (st.model.rows[r]?.[c] === undefined || st.model.rows[r][c] === value) return false;
      st.model.rows[r][c] = value;
      return true;
    };
    const focus = (r: number, c: number) => cellAt(dom, r, c)?.focus();
    const mutate = (fn: (m: TableModel) => void, next: { r: number; c: number }) => {
      readEditing();
      fn(st.model);
      paintTable(dom);
      commitTable(dom, view, next);
      focus(next.r, next.c);
    };
    const blankRow = () => st.model.aligns.map(() => "");
    const exit = (dir: -1 | 1) => {
      const from = view.posAtDOM(dom);
      (document.activeElement as HTMLElement | null)?.blur();
      const to = from + st.source.length;
      const doc = view.state.doc;
      if (dir < 0) {
        if (from === 0) view.dispatch({ changes: { from: 0, insert: "\n" }, selection: { anchor: 0 } });
        else view.dispatch({ selection: { anchor: from - 1 }, scrollIntoView: true });
      } else if (to >= doc.length) {
        view.dispatch({ changes: { from: doc.length, insert: "\n" }, selection: { anchor: doc.length + 1 }, scrollIntoView: true });
      } else {
        view.dispatch({ selection: { anchor: to + 1 }, scrollIntoView: true });
      }
      view.focus();
    };

    dom.addEventListener("mousedown", (e) => {
      const t = e.target as HTMLElement;
      // Link numa célula renderizada abre (o clique é tratado por wireLinks) em vez de entrar em edição
      if (t.closest("a") && !t.closest(".sx-cell.is-editing")) e.preventDefault();
      if (t.closest(".sx-table-add")) e.preventDefault();
      e.stopPropagation();
    });
    dom.addEventListener("click", (e) => {
      const btn = (e.target as HTMLElement).closest(".sx-table-add");
      if (!btn) return;
      const rows = st.model.rows.length;
      const cols = st.model.aligns.length;
      if (btn.classList.contains("is-row")) mutate((m) => m.rows.push(blankRow()), { r: rows, c: 0 });
      else
        mutate((m) => {
          m.rows.forEach((row) => row.push(""));
          m.aligns.push("");
        }, { r: 0, c: cols });
    });
    dom.addEventListener("focusin", (e) => {
      const box = (e.target as HTMLElement).closest<HTMLElement>(".sx-cell");
      if (!box) return;
      pendingTableFocus = null;
      st.editing = box;
      box.classList.add("is-editing");
      const { r, c } = cellPos(box);
      box.textContent = unescapeCell(st.model.rows[r][c]);
      const sel = window.getSelection();
      sel?.selectAllChildren(box);
      sel?.collapseToEnd();
    });
    dom.addEventListener("focusout", (e) => {
      const box = (e.target as HTMLElement).closest<HTMLElement>(".sx-cell");
      if (!box || st.painting || st.editing !== box) return;
      const changed = readEditing();
      st.editing = null;
      box.classList.remove("is-editing");
      const { r, c } = cellPos(box);
      box.innerHTML = renderInline(unescapeCell(st.model.rows[r][c]));
      if (!changed) return;
      const next = (e.relatedTarget as HTMLElement | null)?.closest<HTMLElement>(".sx-cell");
      commitTable(dom, view, next && dom.contains(next) ? cellPos(next) : null);
    });
    // Enquanto se digita, o documento acompanha (o DOM é mantido por updateDOM,
    // então foco e cursor não mudam); assim nada se perde ao trocar de nota
    let typing: ReturnType<typeof setTimeout> | undefined;
    dom.addEventListener("input", () => {
      clearTimeout(typing);
      typing = setTimeout(() => {
        if (st.editing && readEditing()) commitTable(dom, view, null);
      }, 350);
    });
    dom.addEventListener("keydown", (e) => {
      const box = (e.target as HTMLElement).closest<HTMLElement>(".sx-cell");
      if (!box) return;
      const { r, c } = cellPos(box);
      const rows = st.model.rows.length;
      const cols = st.model.aligns.length;
      const handled = () => {
        e.preventDefault();
        e.stopPropagation();
      };
      if (e.key === "Tab") {
        handled();
        if (e.shiftKey) {
          if (c > 0) focus(r, c - 1);
          else if (r > 0) focus(r - 1, cols - 1);
        } else if (c < cols - 1) focus(r, c + 1);
        else if (r < rows - 1) focus(r + 1, 0);
        else mutate((m) => m.rows.push(blankRow()), { r: rows, c: 0 });
      } else if (e.key === "Enter") {
        handled();
        if (e.ctrlKey || e.metaKey) exit(1);
        else if (r < rows - 1) focus(r + 1, c);
        else mutate((m) => m.rows.push(blankRow()), { r: rows, c });
      } else if (e.key === "ArrowUp" && !e.shiftKey) {
        handled();
        if (r > 0) focus(r - 1, c);
        else exit(-1);
      } else if (e.key === "ArrowDown" && !e.shiftKey) {
        handled();
        if (r < rows - 1) focus(r + 1, c);
        else exit(1);
      } else if (e.key === "Escape") {
        handled();
        exit(1);
      }
    });
    dom.addEventListener("contextmenu", (e) => {
      const cell = (e.target as HTMLElement).closest<HTMLElement>("[data-r]");
      if (!cell) return;
      e.preventDefault();
      e.stopPropagation();
      const r = Number(cell.dataset.r);
      const c = Number(cell.dataset.c);
      const rows = st.model.rows.length;
      const cols = st.model.aligns.length;
      const align = (a: Align) => ({
        text: a === "left" ? "Alinhar à esquerda" : a === "center" ? "Centralizar" : a === "right" ? "Alinhar à direita" : "Alinhamento padrão",
        checked: st.model.aligns[c] === a,
        action: () => mutate((m) => (m.aligns[c] = a), { r, c }),
      });
      const swap = (a: number, b: number) => mutate((m) => ([m.rows[a], m.rows[b]] = [m.rows[b], m.rows[a]]), { r: b, c });
      popupMenu([
        { text: "Inserir linha acima", enabled: r > 0, action: () => mutate((m) => m.rows.splice(r, 0, blankRow()), { r, c }) },
        { text: "Inserir linha abaixo", action: () => mutate((m) => m.rows.splice(r + 1, 0, blankRow()), { r: r + 1, c }) },
        {
          text: "Inserir coluna à esquerda",
          action: () =>
            mutate((m) => {
              m.rows.forEach((row) => row.splice(c, 0, ""));
              m.aligns.splice(c, 0, "");
            }, { r, c }),
        },
        {
          text: "Inserir coluna à direita",
          action: () =>
            mutate((m) => {
              m.rows.forEach((row) => row.splice(c + 1, 0, ""));
              m.aligns.splice(c + 1, 0, "");
            }, { r, c: c + 1 }),
        },
        "separator",
        { text: "Mover linha para cima", enabled: r > 1, action: () => swap(r, r - 1) },
        { text: "Mover linha para baixo", enabled: r > 0 && r < rows - 1, action: () => swap(r, r + 1) },
        "separator",
        { text: "Alinhamento", items: (["", "left", "center", "right"] as Align[]).map(align) },
        "separator",
        { text: "Excluir linha", enabled: r > 0, action: () => mutate((m) => m.rows.splice(r, 1), { r: Math.min(r, rows - 2), c }) },
        {
          text: "Excluir coluna",
          enabled: cols > 1,
          action: () =>
            mutate((m) => {
              m.rows.forEach((row) => row.splice(c, 1));
              m.aligns.splice(c, 1);
            }, { r, c: Math.min(c, cols - 2) }),
        },
        {
          text: "Excluir tabela",
          action: () => {
            const from = view.posAtDOM(dom);
            const to = Math.min(from + st.source.length + 1, view.state.doc.length);
            view.dispatch({ changes: { from, to }, selection: { anchor: from } });
            view.focus();
          },
        },
      ]);
    });

    if (pendingTableFocus) {
      requestAnimationFrame(() => {
        const p = pendingTableFocus;
        if (!p || !dom.isConnected || view.posAtDOM(dom) !== p.pos) return;
        pendingTableFocus = null;
        focus(p.r, p.c);
      });
    }
    return dom;
  }
  ignoreEvent() {
    return true;
  }
}

function renderInlineNodes(nodes: InlineNode[]): DocumentFragment {
  const frag = document.createDocumentFragment();
  for (const n of nodes ?? []) {
    switch (n.kind) {
      case "md": {
        const span = el("span");
        span.innerHTML = renderInline(n.text);
        frag.append(span);
        break;
      }
      case "span": {
        const span = el("span", "", n.text);
        const color = safeColor(n.color);
        if (color) span.style.color = color;
        frag.append(span);
        break;
      }
      case "bold":
        frag.append(el("strong", "", n.text));
        break;
      case "italic":
        frag.append(el("em", "", n.text));
        break;
      case "link": {
        const a = el("a", "sx-wikilink", n.text);
        a.dataset.target = n.target ?? n.text;
        frag.append(a);
        break;
      }
      default:
        frag.append(document.createTextNode(n.text ?? ""));
    }
  }
  return frag;
}

export function renderNodes(nodes: ViewNode[]): HTMLElement {
  const root = el("div", "sx-view-nodes");
  for (const node of nodes) {
    switch (node.type) {
      case "header": {
        const h = el(`h${Math.min(6, Math.max(1, node.level))}` as "h3");
        h.append(renderInlineNodes(node.content));
        root.append(h);
        break;
      }
      case "paragraph": {
        const p = el("p");
        p.append(renderInlineNodes(node.content));
        root.append(p);
        break;
      }
      case "list": {
        const ul = el("ul");
        for (const item of node.items) {
          const li = el("li");
          li.append(renderInlineNodes(item));
          ul.append(li);
        }
        root.append(ul);
        break;
      }
      case "table": {
        const wrap = el("div", "sx-table-wrap");
        const table = el("table");
        const thead = el("thead");
        const hr = el("tr");
        for (const h of node.headers) {
          const th = el("th");
          th.append(renderInlineNodes(h));
          hr.append(th);
        }
        thead.append(hr);
        const tbody = el("tbody");
        for (const row of node.rows) {
          const tr = el("tr");
          for (const cell of row) {
            const td = el("td");
            td.append(renderInlineNodes(cell));
            tr.append(td);
          }
          tbody.append(tr);
        }
        table.append(thead, tbody);
        wrap.append(table);
        root.append(wrap);
        break;
      }
      case "error":
        root.append(el("pre", "sx-block-error", node.message));
        break;
      case "calc": {
        // Numerals: expressão à esquerda, resultado à direita
        const box = el("div", "sx-calc");
        for (const r of node.rows) {
          const row = el("div", `sx-calc-row is-${r.kind}`);
          if (r.kind === "heading") row.append(el("span", "sx-calc-heading", r.input));
          else if (r.kind !== "blank") {
            row.append(el("span", "sx-calc-input", r.input));
            if (r.result !== undefined) row.append(el("span", "sx-calc-result", r.result));
            if (r.error) row.append(el("span", "sx-calc-error", r.error));
          }
          box.append(row);
        }
        root.append(box);
        break;
      }
    }
  }
  return root;
}

// Último resultado e altura de cada bloco: o widget já nasce com o conteúdo e o
// tamanho certos, então o editor não calcula posições erradas enquanto ele carrega
// (isso fazia o clique cair na linha de baixo e as setas pularem).
const blockCache = new Map<string, { result: BlockResult; height: number }>();

/** Saída de um bloco ```rustview (ou de plugin). Reexecuta quando a nota muda. */
export class BlockOutputWidget extends WidgetType {
  constructor(
    readonly lang: string,
    readonly source: string,
    readonly doc: string,
  ) {
    super();
  }
  private get key() {
    return `${this.lang} ${this.source}`;
  }
  get estimatedHeight() {
    return blockCache.get(this.key)?.height ?? 80;
  }
  eq(other: BlockOutputWidget) {
    return other.lang === this.lang && other.source === this.source && other.doc === this.doc;
  }
  private paint(dom: HTMLElement, result: BlockResult) {
    dom.querySelector(".sx-block-body")!.replaceChildren(renderNodes(result?.nodes ?? []));
    dom.querySelector(".sx-block-meta")!.textContent =
      result?.ms !== undefined ? `${this.lang} · ${result.ms.toFixed(1)} ms` : this.lang;
  }
  private run(dom: HTMLElement, view: EditorView) {
    const state = dom as HTMLElement & { _timer?: ReturnType<typeof setTimeout>; _seq?: number };
    clearTimeout(state._timer);
    const seq = (state._seq = (state._seq ?? 0) + 1);
    const key = this.key;
    state._timer = setTimeout(async () => {
      const result = await host(view.state)
        .renderBlock(this.lang, this.source, this.doc)
        .catch((e): BlockResult => ({ nodes: [{ type: "error", message: String(e) }] }));
      if (seq !== state._seq || !dom.isConnected) return;
      this.paint(dom, result);
      blockCache.set(key, { result, height: dom.offsetHeight });
      view.requestMeasure();
    }, 60);
  }
  toDOM(view: EditorView) {
    const box = el("div", "sx-block-out");
    box.dataset.lang = this.lang;
    box.append(el("div", "sx-block-meta", this.lang), el("div", "sx-block-body"));
    const dom = shell(box);
    const cached = blockCache.get(this.key);
    if (cached) this.paint(dom, cached.result);
    else box.style.minHeight = "60px";
    wireLinks(dom, view);
    clickToEdit(dom, view);
    this.run(dom, view);
    return dom;
  }
  updateDOM(dom: HTMLElement, view: EditorView) {
    this.run(dom, view);
    return true;
  }
  ignoreEvent() {
    return true;
  }
}

export class NoticeWidget extends WidgetType {
  constructor(
    readonly title: string,
    readonly message: string,
  ) {
    super();
  }
  eq(other: NoticeWidget) {
    return other.title === this.title && other.message === this.message;
  }
  toDOM(view: EditorView) {
    const box = el("div", "sx-block-notice");
    box.append(svgIcon("info", 16));
    const text = el("div");
    text.append(el("strong", "", this.title), el("div", "", this.message));
    box.append(text);
    const dom = shell(box);
    clickToEdit(dom, view);
    return dom;
  }
  ignoreEvent() {
    return true;
  }
}

// ---------- Propriedades (frontmatter) ----------

export function frontmatterRange(state: EditorState) {
  return frontmatterRangeIn(state.doc);
}

/** Intervalo do YAML entre os "---" do início da nota (from/to do conteúdo, end = fim do "---" final). */
export function frontmatterRangeIn(doc: Text): { from: number; to: number; end: number } | null {
  if (doc.lines < 2 || doc.line(1).text.trimEnd() !== "---") return null;
  for (let n = 2; n <= doc.lines; n++) {
    if (doc.line(n).text.trimEnd() === "---") {
      const from = doc.line(2).from;
      return { from: Math.min(from, doc.line(n).from), to: doc.line(n).from, end: doc.line(n).to };
    }
  }
  return null;
}

let focusNewKey: string | null = null;

/** Adiciona uma propriedade (criando o frontmatter se preciso) e foca o nome dela. */
export function addProperty(view: EditorView, key = "propriedade") {
  const range = frontmatterRange(view.state);
  if (!range) {
    view.dispatch({ changes: { from: 0, insert: `---\n${key}: \n---\n` } });
  } else {
    const doc = parseDocument(view.state.sliceDoc(range.from, range.to));
    let name = key;
    for (let i = 1; doc.has(name); i++) name = `${key}${i}`;
    key = name;
    doc.set(name, null);
    view.dispatch({ changes: { from: range.from, to: range.to, insert: String(doc) } });
  }
  focusNewKey = key;
}

type PropType = "list" | "checkbox" | "number" | "date" | "text";

function propType(key: string, value: unknown): PropType {
  if (Array.isArray(value) || key === "tags" || key === "aliases") return "list";
  if (typeof value === "boolean") return "checkbox";
  if (typeof value === "number") return "number";
  if (typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value)) return "date";
  return "text";
}

const TYPE_ICON: Record<PropType, string> = {
  list: "list",
  checkbox: "check",
  number: "target",
  date: "clipboard",
  text: "pencil",
};

export class PropertiesWidget extends WidgetType {
  constructor(readonly yaml: string) {
    super();
  }
  eq(other: PropertiesWidget) {
    return other.yaml === this.yaml;
  }

  private commit(view: EditorView, edit: (doc: Document) => void) {
    const range = frontmatterRange(view.state);
    if (!range) return;
    const doc = parseDocument(view.state.sliceDoc(range.from, range.to));
    edit(doc);
    let text = doc.contents && (doc.contents as { items?: unknown[] }).items?.length ? String(doc) : "";
    if (text && !text.endsWith("\n")) text += "\n";
    view.dispatch({ changes: { from: range.from, to: range.to, insert: text } });
  }

  toDOM(view: EditorView) {
    const root = el("div", "sx-props");
    let doc: Document;
    let data: Record<string, unknown>;
    try {
      doc = parseDocument(this.yaml);
      data = (doc.toJS() ?? {}) as Record<string, unknown>;
      if (typeof data !== "object" || Array.isArray(data)) throw new Error("frontmatter não é um mapa");
    } catch (e) {
      root.append(el("div", "sx-props-error", `YAML inválido: ${e instanceof Error ? e.message : e}`));
      const dom = shell(root, "is-props");
      clickToEdit(dom, view);
      return dom;
    }

    const head = el("div", "sx-props-head", "Propriedades");
    root.append(head);

    for (const [key, value] of Object.entries(data)) {
      const type = propType(key, value);
      const row = el("div", "sx-prop");
      row.dataset.type = type;

      const keyCell = el("label", "sx-prop-key");
      keyCell.append(svgIcon(TYPE_ICON[type], 14));
      const keyInput = el("input");
      keyInput.value = key;
      keyInput.spellcheck = false;
      const renameKey = () => {
        const next = keyInput.value.trim();
        if (!next || next === key) return void (keyInput.value = key);
        this.commit(view, (d) => {
          const node = d.get(key, true);
          d.delete(key);
          d.set(next, node ?? null);
        });
      };
      keyInput.addEventListener("keydown", (e) => {
        if (e.key === "Enter") keyInput.blur();
        if (e.key === "Escape") {
          keyInput.value = key;
          keyInput.blur();
        }
      });
      keyInput.addEventListener("blur", renameKey);
      keyCell.append(keyInput);
      if (focusNewKey === key) {
        focusNewKey = null;
        requestAnimationFrame(() => {
          keyInput.focus();
          keyInput.select();
        });
      }

      const valueCell = el("div", "sx-prop-value");
      if (type === "list") {
        const items = Array.isArray(value) ? value : value == null || value === "" ? [] : [value];
        for (const [i, item] of items.entries()) {
          const chip = el("span", key === "tags" ? "sx-chip is-tag" : "sx-chip");
          const text = String(item ?? "");
          const link = /^\[\[([^\]|]+)(?:\|([^\]]+))?\]\]$/.exec(text);
          if (link) {
            const a = el("a", "sx-wikilink", link[2] ?? link[1]);
            a.dataset.target = link[1];
            chip.append(a);
          } else {
            chip.append(el("span", "", key === "tags" ? `#${text.replace(/^#/, "")}` : text));
          }
          const x = el("button", "sx-chip-x");
          x.append(svgIcon("x", 11, 2.5));
          x.title = "Remover";
          x.addEventListener("click", () =>
            this.commit(view, (d) => {
              const next = items.filter((_, j) => j !== i);
              d.set(key, next.length ? d.createNode(next) : null);
            }),
          );
          chip.append(x);
          valueCell.append(chip);
        }
        const add = el("input", "sx-chip-input");
        add.placeholder = items.length ? "" : "Vazio";
        add.addEventListener("keydown", (e) => {
          if (e.key === "Enter" && add.value.trim()) {
            const v = add.value.trim().replace(key === "tags" ? /^#/ : /$^/, "");
            this.commit(view, (d) => d.set(key, d.createNode([...items, v])));
          } else if (e.key === "Backspace" && !add.value && items.length) {
            this.commit(view, (d) => d.set(key, items.length > 1 ? d.createNode(items.slice(0, -1)) : null));
          }
        });
        valueCell.append(add);
      } else if (type === "checkbox") {
        const box = el("input");
        box.type = "checkbox";
        box.className = "sx-prop-check";
        box.checked = value === true;
        box.addEventListener("change", () => this.commit(view, (d) => d.set(key, box.checked)));
        valueCell.append(box);
      } else {
        const input = el("input", "sx-prop-input");
        input.type = type === "number" ? "number" : type === "date" ? "date" : "text";
        input.value = value == null ? "" : String(value);
        input.placeholder = "Vazio";
        input.spellcheck = false;
        const save = () => {
          const raw = input.value;
          if (raw === (value == null ? "" : String(value))) return;
          this.commit(view, (d) => {
            const node = d.get(key, true);
            const next = raw === "" ? null : type === "number" ? Number(raw) : raw;
            // Mantém aspas originais (ex.: "202609" continua string)
            if (isScalar(node) && typeof next === "string" && typeof node.value === "string") node.value = next;
            else d.set(key, next);
          });
        };
        input.addEventListener("keydown", (e) => e.key === "Enter" && input.blur());
        input.addEventListener("blur", save);
        if (type === "date") input.addEventListener("change", save);
        valueCell.append(input);
      }

      const del = el("button", "sx-prop-del");
      del.title = "Remover propriedade";
      del.append(svgIcon("x", 12, 2));
      del.addEventListener("click", () => this.commit(view, (d) => d.delete(key)));

      row.append(keyCell, valueCell, del);
      root.append(row);
    }

    const add = el("button", "sx-props-add");
    add.append(svgIcon("plus", 14, 2), document.createTextNode("Adicionar propriedade"));
    add.addEventListener("click", () => addProperty(view));
    root.append(add);

    const dom = shell(root, "is-props");
    wireLinks(dom, view);
    // Mantém o editor sem foco enquanto se edita propriedades
    dom.addEventListener("mousedown", (e) => e.stopPropagation());
    return dom;
  }
  ignoreEvent() {
    return true;
  }
}

