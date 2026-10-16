// Comandos de edição usados pela paleta (Ctrl+P), menus, barra de formatação e atalhos.
import { EditorSelection, type ChangeSpec, type EditorState, type Line } from "@codemirror/state";
import type { EditorView } from "@codemirror/view";
import { syntaxTree } from "@codemirror/language";
import { startCompletion } from "@codemirror/autocomplete";
import { TASK_LINE } from "./tasks";
import { frontmatterRange } from "./widgets";

export { addProperty, insertTable } from "./widgets";

export function selectionText(view: EditorView) {
  const { from, to } = view.state.selection.main;
  return view.state.sliceDoc(from, to);
}

export function replaceSelection(view: EditorView, text: string) {
  view.dispatch(view.state.replaceSelection(text));
  view.focus();
}

export function setContent(view: EditorView, text: string) {
  view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
}

/** Insere um bloco em linha própria no cursor. */
export function insertBlock(view: EditorView, text: string, cursorOffset?: number) {
  const { state } = view;
  const pos = state.selection.main.head;
  const line = state.doc.lineAt(pos);
  const before = line.text.trim() ? "\n\n" : line.from > 0 && state.doc.lineAt(line.from - 1).text.trim() ? "\n" : "";
  const insert = before + text + "\n";
  const at = line.text.trim() ? line.to : line.from;
  view.dispatch({
    changes: { from: at, insert },
    selection: { anchor: at + before.length + (cursorOffset ?? text.length) },
  });
  view.focus();
}

/** Define o status das tarefas nas linhas selecionadas; linhas sem tarefa viram tarefa. */
export function setTaskStatus(view: EditorView, status: string | null) {
  const { state } = view;
  const changes: { from: number; to: number; insert: string }[] = [];
  for (const line of selectedLines(state)) {
    const m = TASK_LINE.exec(line.text);
    if (m) {
      const pos = line.from + m[1].length;
      const current = m[2];
      const next = status ?? (current === " " ? "x" : " ");
      changes.push({ from: pos, to: pos + 1, insert: next });
    } else {
      const list = /^(\s*)([-*+]|\d+[.)])\s+/.exec(line.text);
      const indent = /^\s*/.exec(line.text)![0];
      const at = list ? line.from + list[0].length : line.from + indent.length;
      changes.push({ from: at, to: at, insert: `${list ? "" : "- "}[${status ?? " "}] ` });
    }
  }
  // assoc 1: num cursor encostado no ponto de inserção, ele fica depois do "- [ ] "
  const set = state.changes(changes);
  view.dispatch({ changes: set, selection: state.selection.map(set, 1) });
}

export function wrapSelection(view: EditorView, mark: string) {
  view.dispatch(
    view.state.changeByRange((r) => ({
      changes: [
        { from: r.from, insert: mark },
        { from: r.to, insert: mark },
      ],
      range: EditorSelection.range(r.from + mark.length, r.to + mark.length),
    })),
  );
  view.focus();
}

/**
 * Alterna uma marca de formatação (**, *, ==, ~~, `) em cada seleção:
 * remove se o trecho já está marcado; sem seleção, insere o par com o cursor no meio.
 */
export function toggleWrap(view: EditorView, mark: string) {
  const { state } = view;
  const n = mark.length;
  view.dispatch(
    state.changeByRange((r) => {
      const before = state.sliceDoc(r.from - n, r.from);
      const after = state.sliceDoc(r.to, r.to + n);
      // "*" colado em "**" é negrito, não itálico
      const lone = mark !== "*" || (state.sliceDoc(r.from - 2, r.from) !== "**" && state.sliceDoc(r.to, r.to + 2) !== "**");
      if (before === mark && after === mark && lone) {
        return {
          changes: [
            { from: r.from - n, to: r.from },
            { from: r.to, to: r.to + n },
          ],
          range: EditorSelection.range(r.from - n, r.to - n),
        };
      }
      const inner = state.sliceDoc(r.from, r.to);
      if (inner.length >= 2 * n && inner.startsWith(mark) && inner.endsWith(mark)) {
        return {
          changes: { from: r.from, to: r.to, insert: inner.slice(n, -n) },
          range: EditorSelection.range(r.from, r.to - 2 * n),
        };
      }
      return {
        changes: [
          { from: r.from, insert: mark },
          { from: r.to, insert: mark },
        ],
        range: EditorSelection.range(r.from + n, r.to + n),
      };
    }),
  );
  view.focus();
}

function selectedLines(state: EditorState): Line[] {
  const seen = new Set<number>();
  const out: Line[] = [];
  for (const r of state.selection.ranges) {
    for (let n = state.doc.lineAt(r.from).number; n <= state.doc.lineAt(r.to).number; n++) {
      if (seen.has(n)) continue;
      seen.add(n);
      out.push(state.doc.line(n));
    }
  }
  return out;
}

const LIST_PREFIX = /^(\s*)(?:([-*+])\s+(?:\[.\]\s+)?|(\d+)[.)]\s+)?/;

/** Lista com marcador, numerada ou citação nas linhas selecionadas (alterna). */
export function toggleLinePrefix(view: EditorView, kind: "bullet" | "number" | "quote") {
  const { state } = view;
  const lines = selectedLines(state);
  const changes: ChangeSpec[] = [];
  if (kind === "quote") {
    const all = lines.every((l) => /^\s*>/.test(l.text));
    for (const l of lines) {
      const m = /^(\s*)>\s?/.exec(l.text);
      if (all && m) changes.push({ from: l.from + m[1].length, to: l.from + m[0].length });
      else if (!all && !m) changes.push({ from: l.from, insert: "> " });
    }
  } else {
    const is = (l: Line) => {
      const m = LIST_PREFIX.exec(l.text)!;
      return kind === "bullet" ? !!m[2] && !TASK_LINE.test(l.text) : !!m[3];
    };
    const all = lines.every(is);
    let n = 1;
    for (const l of lines) {
      const m = LIST_PREFIX.exec(l.text)!;
      const insert = all ? "" : kind === "bullet" ? "- " : `${n++}. `;
      if (l.text.slice(m[1].length, m[0].length) !== insert) {
        changes.push({ from: l.from + m[1].length, to: l.from + m[0].length, insert });
      }
    }
  }
  const set = state.changes(changes);
  view.dispatch({ changes: set, selection: state.selection.map(set, 1) });
  view.focus();
}

/** Título do nível dado nas linhas selecionadas (o mesmo nível de novo remove). */
export function toggleHeading(view: EditorView, level: number) {
  const { state } = view;
  const changes: ChangeSpec[] = [];
  for (const l of selectedLines(state)) {
    const m = /^(#{1,6})\s+/.exec(l.text);
    const insert = m && m[1].length === level ? "" : "#".repeat(level) + " ";
    changes.push({ from: l.from, to: l.from + (m ? m[0].length : 0), insert });
  }
  const set = state.changes(changes);
  view.dispatch({ changes: set, selection: state.selection.map(set, 1) });
  view.focus();
}

/** [[wikilink]] em volta da seleção; sem seleção abre a lista de notas. */
export function insertWikilink(view: EditorView) {
  const r = view.state.selection.main;
  const text = view.state.sliceDoc(r.from, r.to);
  view.dispatch({
    changes: { from: r.from, to: r.to, insert: text ? `[[${text}]]` : "[[" },
    selection: { anchor: r.from + 2 + (text ? text.length + 2 : 0) },
  });
  view.focus();
  if (!text) startCompletion(view);
}

/** Bloco de código em volta das linhas selecionadas (ou vazio, com o cursor dentro). */
export function insertCodeBlock(view: EditorView) {
  const r = view.state.selection.main;
  if (r.empty) return insertBlock(view, "```\n\n```", 4);
  const a = view.state.doc.lineAt(r.from);
  const b = view.state.doc.lineAt(r.to);
  view.dispatch({
    changes: [
      { from: a.from, insert: "```\n" },
      { from: b.to, insert: "\n```" },
    ],
  });
  view.focus();
}

type TreeNode = ReturnType<ReturnType<typeof syntaxTree>["resolveInner"]>;

/** Tabela ou bloco de código que contém a linha (para mover linhas por cima dele inteiro). */
function blockAround(state: EditorState, line: Line): { from: Line; to: Line } {
  for (let n: TreeNode | null = syntaxTree(state).resolveInner(line.from, 1); n; n = n.parent) {
    if (n.name === "Table" || n.name === "FencedCode") return { from: state.doc.lineAt(n.from), to: state.doc.lineAt(n.to) };
  }
  return { from: line, to: line };
}

/**
 * Move as linhas selecionadas para cima/baixo (Ctrl+Shift+↑/↓). Na visualização
 * ao vivo, tabelas e blocos de código são pulados inteiros (não se parte um
 * bloco renderizado), e o texto nunca entra no frontmatter.
 */
export function moveLines(view: EditorView, dir: -1 | 1, skipBlocks: boolean) {
  const { state } = view;
  const { doc } = state;
  const sel = state.selection.main;
  const first = doc.lineAt(sel.from);
  let last = doc.lineAt(sel.to);
  if (!sel.empty && sel.to === last.from && last.number > first.number) last = doc.line(last.number - 1);
  if (dir < 0 ? first.number === 1 : last.number === doc.lines) return true;
  const near = doc.line(dir < 0 ? first.number - 1 : last.number + 1);
  let block = { from: near, to: near };
  if (skipBlocks) {
    const b = blockAround(state, near);
    // Dentro do próprio bloco (ex.: linhas de um código), move linha a linha
    if (!(b.from.from <= first.from && b.to.to >= last.to)) block = b;
  }
  const fm = frontmatterRange(state);
  if (fm && block.from.from <= fm.end) return true;
  const moving = doc.sliceString(first.from, last.to);
  const other = doc.sliceString(block.from.from, block.to.to);
  const shift = dir < 0 ? -(other.length + 1) : other.length + 1;
  view.dispatch({
    changes: {
      from: dir < 0 ? block.from.from : first.from,
      to: dir < 0 ? last.to : block.to.to,
      insert: dir < 0 ? `${moving}\n${other}` : `${other}\n${moving}`,
    },
    selection: EditorSelection.create(
      state.selection.ranges.map((r) => EditorSelection.range(r.anchor + shift, r.head + shift)),
      state.selection.mainIndex,
    ),
    scrollIntoView: true,
    userEvent: "move.line",
  });
  return true;
}

/** Ctrl+A dentro de um bloco de código seleciona só o código; de novo, a nota toda. */
export function selectCodeBlock(view: EditorView) {
  const { state } = view;
  const sel = state.selection.main;
  for (let n: TreeNode | null = syntaxTree(state).resolveInner(sel.head, -1); n; n = n.parent) {
    if (n.name !== "FencedCode") continue;
    const first = state.doc.lineAt(n.from);
    const last = state.doc.lineAt(n.to);
    const closed = last.number > first.number && /^\s*(```|~~~)/.test(last.text);
    const from = Math.min(first.to + 1, state.doc.length);
    const to = closed ? Math.max(from, last.from - 1) : n.to;
    if (to <= from || (sel.from === from && sel.to === to)) return false;
    view.dispatch({ selection: { anchor: from, head: to } });
    return true;
  }
  return false;
}
