import type { MarkdownConfig } from "@lezer/markdown";
import { tags as t } from "@lezer/highlight";

const Punctuation = /[!"#$%&'()*+,\-./:;<=>?@[\\\]^_`{|}~\p{P}\p{S}]/u;
const HighlightDelim = { resolve: "Highlight", mark: "HighlightMark" };

/** ==texto realçado== (sintaxe do Obsidian). Segue o mesmo algoritmo do ~~tachado~~. */
export const Highlight: MarkdownConfig = {
  defineNodes: [
    { name: "Highlight", style: { "Highlight/...": t.special(t.content) } },
    { name: "HighlightMark", style: t.processingInstruction },
  ],
  parseInline: [
    {
      name: "Highlight",
      parse(cx, next, pos) {
        if (next != 61 /* = */ || cx.char(pos + 1) != 61 || cx.char(pos + 2) == 61) return -1;
        const before = cx.slice(pos - 1, pos);
        const after = cx.slice(pos + 2, pos + 3);
        const sBefore = /\s|^$/.test(before);
        const sAfter = /\s|^$/.test(after);
        const pBefore = Punctuation.test(before);
        const pAfter = Punctuation.test(after);
        return cx.addDelimiter(
          HighlightDelim,
          pos,
          pos + 2,
          !sAfter && (!pAfter || sBefore || pBefore),
          !sBefore && (!pBefore || sAfter || pAfter),
        );
      },
      after: "Emphasis",
    },
  ],
};

const DOLLAR = 36;

/**
 * Fórmulas LaTeX: $inline$, $$exibida$$ na mesma linha e blocos
 *
 *   $$
 *   N(t)=N_0\cdot2^{t/24}
 *   $$
 *
 * Como no Obsidian, "$" só abre se o próximo caractere não for espaço e só fecha
 * se o anterior não for espaço nem o seguinte um dígito (assim "R$ 10 e R$ 20" é texto).
 */
export const MathSyntax: MarkdownConfig = {
  defineNodes: [
    { name: "InlineMath", style: t.special(t.content) },
    { name: "BlockMath", block: true, style: t.special(t.content) },
    { name: "MathMark", style: t.processingInstruction },
  ],
  parseBlock: [
    {
      name: "BlockMath",
      parse(cx, line) {
        if (line.next != DOLLAR || line.text.charCodeAt(line.pos + 1) != DOLLAR) return false;
        const start = cx.lineStart + line.pos;
        const rest = line.text.slice(line.pos + 2);
        // $$ ... $$ numa linha só
        if (/\S/.test(rest) && /\$\$\s*$/.test(rest) && rest.trim().length > 2) {
          const end = cx.lineStart + line.text.length;
          cx.nextLine();
          cx.addElement(cx.elt("BlockMath", start, end));
          return true;
        }
        if (/\S/.test(rest)) return false;
        let end = -1;
        while (cx.nextLine()) {
          if (/^\s*\$\$\s*$/.test(line.text.slice(line.pos)) || /\$\$\s*$/.test(line.text)) {
            end = cx.lineStart + line.text.length;
            cx.nextLine();
            break;
          }
        }
        cx.addElement(cx.elt("BlockMath", start, end < 0 ? cx.prevLineEnd() : end));
        return true;
      },
      // "$$" sozinho na linha encerra o parágrafo de cima ("ou" na linha anterior e o bloco logo abaixo)
      endLeaf: (_cx, line) =>
        line.next == DOLLAR && line.text.charCodeAt(line.pos + 1) == DOLLAR && !/\S/.test(line.text.slice(line.pos + 2)),
      before: "FencedCode",
    },
  ],
  parseInline: [
    {
      name: "InlineMath",
      parse(cx, next, pos) {
        if (next != DOLLAR) return -1;
        if (cx.char(pos + 1) == DOLLAR) {
          // $$exibida$$ dentro de um parágrafo
          const close = cx.slice(pos + 2, cx.end).indexOf("$$");
          if (close <= 0) return -1;
          const end = pos + 2 + close + 2;
          return cx.addElement(
            cx.elt("InlineMath", pos, end, [cx.elt("MathMark", pos, pos + 2), cx.elt("MathMark", end - 2, end)]),
          );
        }
        const first = cx.char(pos + 1);
        if (first < 0 || first == 32 || first == 9 || first == 10) return -1;
        for (let i = pos + 1; i < cx.end; i++) {
          const ch = cx.char(i);
          if (ch == 92 /* \ */) {
            i++;
            continue;
          }
          if (ch == 10 && cx.char(i + 1) == 10) return -1;
          if (ch != DOLLAR) continue;
          const prev = cx.char(i - 1);
          const after = cx.char(i + 1);
          if (prev == 32 || prev == 9 || (after >= 48 && after <= 57)) return -1;
          return cx.addElement(
            cx.elt("InlineMath", pos, i + 1, [cx.elt("MathMark", pos, pos + 1), cx.elt("MathMark", i, i + 1)]),
          );
        }
        return -1;
      },
      before: "Emphasis",
    },
  ],
};

/** Notas de rodapé inline do Obsidian: texto^[conteúdo da nota], numeradas na ordem do texto. */
export const InlineFootnoteSyntax: MarkdownConfig = {
  defineNodes: [
    { name: "InlineFootnote", style: t.special(t.content) },
    { name: "FootnoteMark", style: t.processingInstruction },
  ],
  parseInline: [
    {
      name: "InlineFootnote",
      parse(cx, next, pos) {
        if (next != 94 /* ^ */ || cx.char(pos + 1) != 91 /* [ */) return -1;
        let depth = 0;
        for (let i = pos + 1; i < cx.end; i++) {
          const ch = cx.char(i);
          if (ch == 92) {
            i++;
            continue;
          }
          if (ch == 91) depth++;
          else if (ch == 93 && --depth == 0) {
            if (i == pos + 2) return -1; // ^[] vazio
            return cx.addElement(
              cx.elt("InlineFootnote", pos, i + 1, [cx.elt("FootnoteMark", pos, pos + 2), cx.elt("FootnoteMark", i, i + 1)]),
            );
          }
        }
        return -1;
      },
      before: "Link",
    },
  ],
};
