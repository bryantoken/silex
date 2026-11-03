/*
 * Notas de rodapé inline (texto^[nota]): numeradas pela ordem em que aparecem.
 * A lista é calculada uma vez por árvore de sintaxe e reaproveitada pelas decorações.
 */
import { syntaxTree } from "@codemirror/language";
import type { EditorState } from "@codemirror/state";
import type { Footnote } from "./widgets";

type Tree = ReturnType<typeof syntaxTree>;

const cache = new WeakMap<Tree, Footnote[]>();
const SKIP = new Set(["FencedCode", "CodeBlock", "Frontmatter", "InlineCode", "BlockMath", "InlineMath"]);

export function collectFootnotes(state: EditorState, tree: Tree = syntaxTree(state)): Footnote[] {
  let notes = cache.get(tree);
  if (notes) return notes;
  const found: Footnote[] = [];
  tree.iterate({
    enter: (n) => {
      if (SKIP.has(n.name)) return false;
      if (n.name === "InlineFootnote") {
        found.push({ from: n.from, to: n.to, text: state.doc.sliceString(n.from + 2, n.to - 1).trim() });
        return false;
      }
    },
  });
  cache.set(tree, found);
  return found;
}

/** Número (1, 2, …) da nota que começa em `from`. */
export function footnoteNumber(state: EditorState, from: number): number {
  const i = collectFootnotes(state).findIndex((n) => n.from === from);
  return i < 0 ? 0 : i + 1;
}
