/*
 * Visualização ao vivo: decorações dentro das linhas.
 * Esconde a marcação (#, **, ==, `, >, [](), ```) onde o cursor não está e
 * renderiza tarefas, callouts, cabeçalhos de código, imagens e wikilinks.
 * Blocos inteiros (tabelas, propriedades, rustview) ficam em blocks.ts.
 */
import { syntaxTree } from "@codemirror/language";
import type { EditorState, Range } from "@codemirror/state";
import { Decoration, type DecorationSet, EditorView, ViewPlugin, type ViewUpdate } from "@codemirror/view";
import { host, isRevealed, revealField, touches } from "./context";
import { CALLOUT_HEADER, calloutKind } from "./callouts";
import { TASK_LINE, taskStatus } from "./tasks";
import { BulletWidget, CalloutTitleWidget, CheckboxWidget, CodeHeaderWidget, EmbedWidget, ImageWidget } from "./widgets";

const WIKI = /(!?)\[\[([^\]|#\n]+)(#[^\]|\n]*)?(?:\|([^\]\n]+))?\]\]/g;
const HIDDEN_MARKS = new Set(["EmphasisMark", "StrikethroughMark", "HighlightMark", "CodeMark"]);
const HEADING_LINE: Record<string, string> = {
  ATXHeading1: "sx-h1",
  ATXHeading2: "sx-h2",
  ATXHeading3: "sx-h3",
  ATXHeading4: "sx-h4",
  ATXHeading5: "sx-h5",
  ATXHeading6: "sx-h6",
  SetextHeading1: "sx-h1",
  SetextHeading2: "sx-h2",
};

const hide = Decoration.replace({});
const bullet = Decoration.replace({ widget: new BulletWidget() });

/** Linguagens de bloco renderizadas por blocks.ts (não recebem cabeçalho de código). */
export function isRenderedLang(state: EditorState, lang: string) {
  return lang === "rustview" || lang === "dataview" || lang === "dataviewjs" || host(state).blockLangs().has(lang);
}

export function codeLang(state: EditorState, node: { node: { getChild(n: string): { from: number; to: number } | null } }) {
  const info = node.node.getChild("CodeInfo");
  return info ? state.doc.sliceString(info.from, info.to).trim().toLowerCase() : "";
}

function build(view: EditorView): DecorationSet {
  const { state } = view;
  const doc = state.doc;
  const decos: Range<Decoration>[] = [];
  const revealed = (from: number, to: number) => isRevealed(state, from, to);
  const lineDeco = (pos: number, cls: string, style?: string) =>
    decos.push(
      Decoration.line(style ? { class: cls, attributes: { style } } : { class: cls }).range(doc.lineAt(pos).from),
    );
  const afterSpace = (pos: number) => (doc.sliceString(pos, pos + 1) === " " ? 1 : 0);
  // Trechos que outro mecanismo já substitui (tabelas e blocos renderizados)
  const covered: [number, number][] = [];
  const isCovered = (a: number, b: number) => covered.some(([f, t]) => a < t && b > f);

  for (const { from, to } of view.visibleRanges) {
    syntaxTree(state).iterate({
      from,
      to,
      enter: (node) => {
        const name = node.name;

        if (name === "Frontmatter") return false;

        if (name === "FencedCode") {
          const lang = codeLang(state, node);
          const open = !revealed(node.from, node.to);
          // Blocos renderizados (rustview, plugins) viram widget em blocks.ts;
          // em edição aparecem como código normal, com as crases visíveis
          if (isRenderedLang(state, lang) && open) {
            covered.push([node.from, node.to]);
            return false;
          }
          const first = doc.lineAt(node.from);
          const last = doc.lineAt(node.to);
          for (let n = first.number; n <= last.number; n++) {
            const line = doc.line(n);
            let cls = "sx-codeblock";
            if (n === first.number) cls += " sx-codeblock-start";
            if (n === last.number) cls += " sx-codeblock-end";
            decos.push(Decoration.line({ class: cls }).range(line.from));
          }
          if (open) {
            decos.push(Decoration.replace({ widget: new CodeHeaderWidget(lang) }).range(first.from, first.to));
            if (last.number > first.number && /^\s*(```|~~~)/.test(last.text)) {
              decos.push(hide.range(last.from, last.to));
            }
          }
          return false;
        }

        if (name === "Table") {
          covered.push([node.from, node.to]);
          return false;
        }

        if (HEADING_LINE[name]) {
          lineDeco(node.from, `sx-heading ${HEADING_LINE[name]}`);
          return;
        }

        if (name === "HeaderMark") {
          if (node.node.parent?.name.startsWith("ATXHeading") && !revealed(node.from, node.to)) {
            decos.push(hide.range(node.from, node.to + afterSpace(node.to)));
          }
          return;
        }

        if (name === "Blockquote") {
          const firstLine = doc.lineAt(node.from);
          const m = CALLOUT_HEADER.exec(firstLine.text);
          const lastNo = doc.lineAt(node.to).number;
          if (m) {
            const kind = calloutKind(m[2]);
            const style = `--sx-callout: ${kind.color}`;
            for (let n = firstLine.number; n <= lastNo; n++) {
              let cls = "sx-callout";
              if (n === firstLine.number) cls += " sx-callout-first";
              if (n === lastNo) cls += " sx-callout-last";
              decos.push(Decoration.line({ class: cls, attributes: { style } }).range(doc.line(n).from));
            }
            if (!revealed(firstLine.from, firstLine.to)) {
              const end = firstLine.from + m[1].length + m[2].length + 3 + m[3].length;
              const title = m[4].trim();
              decos.push(
                Decoration.replace({ widget: new CalloutTitleWidget(m[2], !title) }).range(
                  firstLine.from,
                  Math.min(end + afterSpace(end), firstLine.to),
                ),
              );
            }
          } else {
            for (let n = firstLine.number; n <= lastNo; n++) lineDeco(doc.line(n).from, "sx-quote");
          }
          return;
        }

        if (name === "QuoteMark") {
          if (!revealed(node.from, node.to)) {
            const line = doc.lineAt(node.from);
            // Na linha de título do callout o widget já cobre o ">"
            if (!(CALLOUT_HEADER.test(line.text) && node.from === line.from + line.text.indexOf(">"))) {
              decos.push(hide.range(node.from, node.to + afterSpace(node.to)));
            }
          }
          return;
        }

        if (name === "HorizontalRule") {
          lineDeco(node.from, "sx-hr");
          if (!revealed(node.from, node.to)) decos.push(hide.range(node.from, node.to));
          return;
        }

        if (name === "ListMark") {
          const line = doc.lineAt(node.from);
          const m = TASK_LINE.exec(line.text);
          // Só a marca de lista que abre a linha (não a de um item aninhado na mesma linha)
          if (m && /^\s*$/.test(line.text.slice(0, node.from - line.from))) {
            const status = taskStatus(m[2]);
            const boxEnd = line.from + m[0].length;
            const textFrom = boxEnd + afterSpace(boxEnd);
            // O checkbox só vira texto cru quando o cursor está sobre "- [ ]";
            // editar o texto da tarefa não mexe no checkbox
            if (!touches(state, node.from, boxEnd)) {
              decos.push(Decoration.replace({ widget: new CheckboxWidget(m[2]) }).range(node.from, textFrom));
            }
            if (textFrom < line.to) {
              decos.push(Decoration.mark({ class: `sx-task-text sx-task-${status.name}` }).range(textFrom, line.to));
            }
            return;
          }
          const mark = doc.sliceString(node.from, node.to);
          if (/^[-*+]$/.test(mark) && !touches(state, node.from, node.to)) decos.push(bullet.range(node.from, node.to));
          return;
        }

        // O marcador [x] já virou checkbox; o texto da tarefa continua sendo decorado
        if (name === "TaskMarker") return false;

        if (name === "Image") {
          if (revealed(node.from, node.to)) return false;
          const url = node.node.getChild("URL");
          const marks = node.node.getChildren("LinkMark");
          if (url && marks.length >= 2) {
            const alt = doc.sliceString(marks[0].to, marks[1].from);
            decos.push(
              Decoration.replace({ widget: new ImageWidget(doc.sliceString(url.from, url.to), alt) }).range(
                node.from,
                node.to,
              ),
            );
          }
          return false;
        }

        if (name === "Link") {
          const url = node.node.getChild("URL");
          const marks = node.node.getChildren("LinkMark");
          if (!url || marks.length < 2) return false; // "[texto]" literal, sem URL
          if (doc.sliceString(node.from, node.from + 2) === "[[") return false; // wikilink
          decos.push(
            Decoration.mark({ class: "sx-link", attributes: { "data-href": doc.sliceString(url.from, url.to) } }).range(
              marks[0].to,
              marks[1].from,
            ),
          );
          if (!revealed(node.from, node.to)) {
            decos.push(hide.range(marks[0].from, marks[0].to));
            decos.push(hide.range(marks[1].from, node.to));
          }
          return false;
        }

        if (name === "InlineCode") {
          decos.push(Decoration.mark({ class: "sx-inline-code" }).range(node.from, node.to));
          return;
        }

        if (name === "Highlight") {
          decos.push(Decoration.mark({ class: "sx-highlight" }).range(node.from, node.to));
          return;
        }

        if (HIDDEN_MARKS.has(name) && !revealed(node.from, node.to)) {
          decos.push(hide.range(node.from, node.to));
        }
      },
    });

    // [[wikilinks]] e ![[embeds]] (sintaxe do Obsidian, fora do CommonMark)
    const text = doc.sliceString(from, to);
    for (const m of text.matchAll(WIKI)) {
      const start = from + m.index!;
      const end = start + m[0].length;
      if (isCovered(start, end)) continue;
      const ctx = syntaxTree(state).resolveInner(start, 1);
      let inCode = false;
      for (let n: typeof ctx | null = ctx; n; n = n.parent) {
        if (n.name === "InlineCode" || n.name === "FencedCode" || n.name === "CodeBlock" || n.name === "Frontmatter") inCode = true;
      }
      if (inCode) continue;

      const embed = m[1] === "!";
      const target = m[2].trim();
      const alias = m[4];
      const open = !revealed(start, end);

      if (embed) {
        if (open) {
          const size = alias && /^\d+(x\d+)?$/.test(alias.trim()) ? alias.trim() : "";
          decos.push(Decoration.replace({ widget: new EmbedWidget(target, size) }).range(start, end));
        } else {
          decos.push(Decoration.mark({ class: "sx-embed-raw" }).range(start, end));
        }
        continue;
      }

      const labelStart = alias ? end - 2 - alias.length : start + 2;
      const labelEnd = end - 2;
      if (open) {
        decos.push(hide.range(start, labelStart));
        decos.push(Decoration.mark({ class: "sx-wikilink", attributes: { "data-target": target } }).range(labelStart, labelEnd));
        decos.push(hide.range(labelEnd, end));
      } else {
        decos.push(
          Decoration.mark({ class: "sx-wikilink", attributes: { "data-target": target, "data-raw": "1" } }).range(start, end),
        );
      }
    }
  }

  return Decoration.set(decos, true);
}

export const inlinePreview = [
  ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = build(view);
      }
      update(u: ViewUpdate) {
        if (
          u.docChanged ||
          u.viewportChanged ||
          u.startState.field(revealField) !== u.state.field(revealField) ||
          syntaxTree(u.startState) !== syntaxTree(u.state)
        ) {
          this.decorations = build(u.view);
        }
      }
    },
    { decorations: (v) => v.decorations },
  ),
  EditorView.domEventHandlers({
    mousedown(e, view) {
      const target = (e.target as HTMLElement).closest<HTMLElement>(".sx-wikilink, .sx-link");
      if (!target || e.button !== 0) return false;
      const mod = e.ctrlKey || e.metaKey;
      const h = host(view.state);
      // Link renderizado abre com clique simples; em edição (marcação visível), só com Ctrl
      if (target.dataset.target && (mod || !target.dataset.raw)) {
        e.preventDefault();
        h.openLink(target.dataset.target, mod);
        return true;
      }
      if (target.dataset.href && mod) {
        e.preventDefault();
        h.openExternal(target.dataset.href);
        return true;
      }
      return false;
    },
  }),
];
