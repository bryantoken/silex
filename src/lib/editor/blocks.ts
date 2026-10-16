/*
 * Decorações de bloco (substituem linhas inteiras): propriedades do frontmatter,
 * tabelas interativas e blocos renderizados (```rustview, blocos de plugins, avisos de dataview).
 * Decorações de bloco precisam vir direto do estado (não de um ViewPlugin).
 */
import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
import { EditorState, type Range } from "@codemirror/state";
import { Decoration, EditorView } from "@codemirror/view";
import { isRevealed, revealField } from "./context";
import { codeLang, isRenderedLang } from "./inline";
import { BlockOutputWidget, frontmatterRange, frontmatterRangeIn, NoticeWidget, PropertiesWidget, TableWidget } from "./widgets";

function build(state: EditorState) {
  const doc = state.doc;
  const decos: Range<Decoration>[] = [];
  const tree = ensureSyntaxTree(state, doc.length, 200) ?? syntaxTree(state);
  let docText: string | null = null;

  // Propriedades ficam sempre renderizadas (edição pelo painel; YAML cru no modo fonte)
  const fm = frontmatterRange(state);
  if (fm) {
    decos.push(
      Decoration.replace({ widget: new PropertiesWidget(state.sliceDoc(fm.from, fm.to)), block: true }).range(0, fm.end),
    );
  }

  tree.iterate({
    enter: (node) => {
      if (node.name === "Frontmatter") return false;

      // Tabelas são sempre interativas na visualização ao vivo (Markdown cru só no modo fonte)
      if (node.name === "Table") {
        const first = doc.lineAt(node.from);
        const last = doc.lineAt(node.to);
        decos.push(
          Decoration.replace({ widget: new TableWidget(doc.sliceString(first.from, last.to)), block: true }).range(
            first.from,
            last.to,
          ),
        );
        return false;
      }

      if (node.name === "FencedCode") {
        const lang = codeLang(state, node);
        if (!isRenderedLang(state, lang)) return false;
        const first = doc.lineAt(node.from);
        const last = doc.lineAt(node.to);
        const closed = last.number > first.number && /^\s*(```|~~~)/.test(last.text);
        const source = doc.sliceString(Math.min(first.to + 1, doc.length), closed ? Math.max(first.to, last.from - 1) : node.to);
        const revealed = isRevealed(state, node.from, node.to);

        if (lang === "dataview" || lang === "dataviewjs") {
          if (!revealed) {
            const widget = new NoticeWidget(
              `Bloco ${lang} não executado`,
              lang === "dataviewjs"
                ? "O Silex não executa JavaScript. Reescreva como ```rustview (veja docs/rustview.md)."
                : "Consultas DQL ainda não são suportadas. Use ```rustview.",
            );
            decos.push(Decoration.replace({ widget, block: true }).range(first.from, last.to));
          }
          return false;
        }

        docText ??= doc.toString();
        const widget = new BlockOutputWidget(lang, source, docText);
        decos.push(
          revealed
            ? Decoration.widget({ widget, block: true, side: 1 }).range(last.to)
            : Decoration.replace({ widget, block: true }).range(first.from, last.to),
        );
        return false;
      }
    },
  });

  return Decoration.set(decos, true);
}

const blockDecorations = EditorView.decorations.compute(["doc", revealField], build);

/**
 * Mantém o cursor fora do frontmatter (renderizado como painel de propriedades):
 * setas e cliques que cairiam dentro dele param na primeira linha do texto.
 */
const frontmatterGuard = EditorState.transactionFilter.of((tr) => {
  if (!tr.selection) return tr;
  const fm = frontmatterRangeIn(tr.newDoc);
  if (!fm || fm.end >= tr.newDoc.length) return tr;
  const sel = tr.newSelection.main;
  const body = fm.end + 1;
  if (sel.from > fm.end) return tr;
  if (!sel.empty && sel.to > fm.end) return tr; // Ctrl+A e seleções que atravessam o painel
  return [tr, { selection: { anchor: body }, sequential: true }];
});

export const blockPreview = [blockDecorations, frontmatterGuard];
