/*
 * Retrair cabeçalhos (a seção inteira), itens de lista com subitens, callouts e blocos.
 * Uma setinha aparece à esquerda da linha ao passar o mouse (e fica fixa quando retraído).
 * Atalhos: Ctrl+Shift+[ retrai, Ctrl+Shift+] expande, Ctrl+Alt+[ / ] tudo.
 */
import { codeFolding, foldable, foldedRanges, foldEffect, foldKeymap, syntaxTree, unfoldEffect } from "@codemirror/language";
import type { EditorState, Range } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate, WidgetType, keymap } from "@codemirror/view";
import { svgIcon } from "../icons";

function foldedAt(state: EditorState, lineFrom: number, lineTo: number) {
  let found: { from: number; to: number } | null = null;
  foldedRanges(state).between(lineFrom, lineTo, (from, to) => {
    if (from >= lineFrom && from <= lineTo) found = { from, to };
  });
  return found as { from: number; to: number } | null;
}

/** Retrai/expande o bloco que começa na linha de `pos`. */
export function toggleFoldAt(view: EditorView, pos: number) {
  const line = view.state.doc.lineAt(pos);
  const folded = foldedAt(view.state, line.from, line.to);
  if (folded) return view.dispatch({ effects: unfoldEffect.of(folded) });
  const range = foldable(view.state, line.from, line.to);
  if (range) view.dispatch({ effects: foldEffect.of(range) });
}

/** Leva o cursor (e a rolagem) até `pos`, expandindo o que estiver retraído por cima. */
export function revealPos(view: EditorView, pos: number) {
  const at = Math.min(pos, view.state.doc.length);
  const unfold: ReturnType<typeof unfoldEffect.of>[] = [];
  foldedRanges(view.state).between(at, at, (from, to) => {
    if (from <= at && to >= at) unfold.push(unfoldEffect.of({ from, to }));
  });
  view.dispatch({
    selection: { anchor: at },
    effects: [...unfold, EditorView.scrollIntoView(at, { y: "start", yMargin: 32 })],
  });
  view.focus();
}

class FoldToggle extends WidgetType {
  constructor(readonly folded: boolean) {
    super();
  }
  eq(other: FoldToggle) {
    return other.folded === this.folded;
  }
  toDOM(view: EditorView) {
    const btn = document.createElement("span");
    btn.className = this.folded ? "sx-fold is-folded" : "sx-fold";
    btn.title = this.folded ? "Expandir" : "Retrair";
    btn.setAttribute("aria-hidden", "true");
    btn.append(svgIcon("chevron", 14, 2.2));
    btn.addEventListener("mousedown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      toggleFoldAt(view, view.posAtDOM(btn));
    });
    return btn;
  }
  ignoreEvent() {
    return true;
  }
}

const open = Decoration.widget({ widget: new FoldToggle(false), side: -1 });
const closed = Decoration.widget({ widget: new FoldToggle(true), side: -1 });

function build(view: EditorView): DecorationSet {
  const { state } = view;
  const decos: Range<Decoration>[] = [];
  const seen = new Set<number>();
  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name;
        if (name === "Frontmatter" || name === "FencedCode" || name === "Table" || name === "BlockMath") return false;
        const heading = /^(ATX|Setext)Heading/.test(name);
        if (!heading && name !== "ListItem" && name !== "Blockquote") return;
        const line = state.doc.lineAt(node.from);
        if (seen.has(line.from)) return;
        const at = heading ? line.from : node.from;
        if (foldedAt(state, line.from, line.to)) {
          seen.add(line.from);
          decos.push(closed.range(at));
        } else if (foldable(state, line.from, line.to)) {
          seen.add(line.from);
          decos.push(open.range(at));
        }
      },
    });
  }
  return Decoration.set(decos, true);
}

const foldToggles = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;
    constructor(view: EditorView) {
      this.decorations = build(view);
    }
    update(u: ViewUpdate) {
      if (
        u.docChanged ||
        u.viewportChanged ||
        foldedRanges(u.startState) !== foldedRanges(u.state) ||
        syntaxTree(u.startState) !== syntaxTree(u.state)
      ) {
        this.decorations = build(u.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);

export const folding = [
  codeFolding({
    placeholderDOM: (_view, onclick) => {
      const el = document.createElement("span");
      el.className = "sx-fold-placeholder";
      el.textContent = "…";
      el.title = "Expandir";
      el.onclick = onclick;
      return el;
    },
  }),
  foldToggles,
  keymap.of(foldKeymap),
];
