/*
 * Estado compartilhado pelas extensões do editor:
 * - EditorHost: ponte para o app (abrir links, resolver anexos, rodar rustview...)
 * - revealField: onde a marcação Markdown fica visível para edição
 */
import { Facet, StateEffect, StateField, type EditorState } from "@codemirror/state";
import { EditorView } from "@codemirror/view";

export type InlineNode = {
  kind: "md" | "text" | "span" | "bold" | "italic" | "link";
  text: string;
  color?: string;
  target?: string;
};
export type ViewNode =
  | { type: "header"; level: number; content: InlineNode[] }
  | { type: "paragraph"; content: InlineNode[] }
  | { type: "list"; items: InlineNode[][] }
  | { type: "table"; headers: InlineNode[][]; rows: InlineNode[][][] }
  | { type: "error"; message: string }
  | { type: "calc"; rows: { input: string; result?: string; error?: string; kind: string }[] };
export type BlockResult = { nodes: ViewNode[]; ms?: number } | null;

export interface EditorHost {
  path(): string;
  openLink(target: string, newTab: boolean): void;
  openExternal(url: string): void;
  /** Resolve o nome de um anexo/nota (como no Obsidian) para caminho absoluto */
  resolveFile(name: string): string | null;
  assetUrl(path: string): string;
  renderBlock(lang: string, source: string, content: string): Promise<BlockResult>;
  /** Linguagens de bloco renderizadas (rustview + plugins) */
  blockLangs(): Set<string>;
  /** Colar células do Excel como tabela Markdown */
  excelPaste(): boolean;
  taskMenu(view: EditorView, statusPos: number): void;
  /** Sugestões ao digitar [[ (só notas) ou ![[ (qualquer arquivo) */
  linkTargets(embed: boolean): readonly LinkTarget[];
  /** Salva uma imagem colada como anexo e devolve o nome para o ![[embed]] */
  saveImage(data: Uint8Array, ext: string): Promise<string | null>;
}

export type LinkTarget = { label: string; link: string; detail: string };

export const hostFacet = Facet.define<EditorHost, EditorHost | null>({
  combine: (values) => values[values.length - 1] ?? null,
});

export const host = (state: EditorState) => state.facet(hostFacet)!;

// ---------- Revelação da marcação ----------
//
// A marcação aparece nas linhas tocadas pela seleção (cursor ou trecho
// selecionado) quando o editor tem foco. Durante um arraste do mouse o estado
// fica congelado: o texto não muda de lugar embaixo do ponteiro enquanto você
// seleciona, e só se ajusta quando o botão é solto.

export const setFocus = StateEffect.define<boolean>();
export const setPointer = StateEffect.define<boolean>();

type Range = readonly [number, number];
type Reveal = { focus: boolean; pointer: boolean; ranges: readonly Range[]; lines: ReadonlySet<number> };

const sameRanges = (a: readonly Range[], b: readonly Range[]) =>
  a.length === b.length && a.every((r, i) => r[0] === b[i][0] && r[1] === b[i][1]);

export const revealField = StateField.define<Reveal>({
  create: () => ({ focus: false, pointer: false, ranges: [], lines: new Set() }),
  update(value, tr) {
    let { focus, pointer } = value;
    for (const e of tr.effects) {
      if (e.is(setFocus)) focus = e.value;
      if (e.is(setPointer)) pointer = e.value;
    }
    if (pointer) {
      if (focus === value.focus && pointer === value.pointer && !tr.docChanged) return value;
      // Congelado: só acompanha as posições se o texto mudar
      const ranges = value.ranges.map(([a, b]) => [tr.changes.mapPos(a), tr.changes.mapPos(b)] as const);
      return { focus, pointer, ranges, lines: linesOf(tr.newDoc, ranges) };
    }
    const ranges: Range[] = focus ? tr.newSelection.ranges.map((r) => [r.from, r.to] as const) : [];
    if (focus === value.focus && pointer === value.pointer && sameRanges(ranges, value.ranges) && !tr.docChanged) {
      return value;
    }
    return { focus, pointer, ranges, lines: linesOf(tr.newDoc, ranges) };
  },
});

function linesOf(doc: EditorState["doc"], ranges: readonly Range[]) {
  const lines = new Set<number>();
  for (const [a, b] of ranges) {
    const first = doc.lineAt(Math.min(a, doc.length)).number;
    const last = doc.lineAt(Math.min(b, doc.length)).number;
    for (let n = first; n <= last; n++) lines.add(n);
  }
  return lines;
}

/** A seleção toca alguma linha entre `from` e `to`? */
export function isRevealed(state: EditorState, from: number, to: number): boolean {
  const { lines } = state.field(revealField);
  if (lines.size === 0) return false;
  const a = state.doc.lineAt(from).number;
  const b = state.doc.lineAt(Math.min(to, state.doc.length)).number;
  for (const n of lines) if (n >= a && n <= b) return true;
  return false;
}

/** A seleção toca exatamente o trecho [from, to] (não a linha inteira)? */
export function touches(state: EditorState, from: number, to: number): boolean {
  for (const [a, b] of state.field(revealField).ranges) if (a <= to && b >= from) return true;
  return false;
}

export const revealExtensions = [
  revealField,
  EditorView.focusChangeEffect.of((_state, focusing) => setFocus.of(focusing)),
  EditorView.domEventHandlers({
    mousedown(e, view) {
      if (e.button !== 0) return false;
      view.dispatch({ effects: setPointer.of(true) });
      const up = () => {
        window.removeEventListener("mouseup", up, true);
        // Espera o CodeMirror aplicar a seleção final do clique
        requestAnimationFrame(() => view.dom.isConnected && view.dispatch({ effects: setPointer.of(false) }));
      };
      window.addEventListener("mouseup", up, true);
      return false;
    },
  }),
];
