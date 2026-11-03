/*
 * Renderização de Markdown para HTML seguro, usada nos widgets do editor
 * (tabelas, saída do rustview, propriedades), nos rótulos de [[links]] e na
 * exportação HTML/PDF. Suporta [[wikilinks]], ==realce==, $LaTeX$ e ^[notas].
 * Todo HTML passa pelo DOMPurify antes de ir para o DOM.
 */
import { Marked, type MarkedExtension, type TokenizerAndRendererExtension } from "marked";
import DOMPurify from "dompurify";
import { headingKey, linkLabel, splitLink } from "./links";
import { renderMath } from "./math";
import { findHeadingIndex, parseHeadings } from "./headings";

export const escapeHtml = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);
const escape = escapeHtml;

/** Âncora de um cabeçalho (mesma regra na exportação e nos links [[#Cabeçalho]]). */
export const headingSlug = (text: string) =>
  "h-" +
  headingKey(text)
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^\w]+/g, "-")
    .replace(/^-+|-+$/g, "");

type Options = {
  /** Exportação: [[#Cabeçalho]] vira âncora e ![[embed]] vira marcador a resolver depois */
  exporting?: boolean;
  /** Exportação: recebe o texto das notas ^[...] (numeradas pela ordem) */
  notes?: string[];
  /** Exportação: âncora do cabeçalho apontado por um caminho #A#B */
  anchor?: (path: string[]) => string;
};

/** Âncoras únicas na ordem do texto: títulos repetidos ganham -2, -3... */
function uniqueSlugs(texts: string[]): string[] {
  const seen = new Map<string, number>();
  return texts.map((t) => {
    const base = headingSlug(t);
    const n = (seen.get(base) ?? 0) + 1;
    seen.set(base, n);
    return n === 1 ? base : `${base}-${n}`;
  });
}

function extensions(opts: Options): TokenizerAndRendererExtension[] {
  const wikilink: TokenizerAndRendererExtension = {
    name: "wikilink",
    level: "inline",
    start: (src) => src.indexOf("[["),
    tokenizer(src) {
      const m = /^!?\[\[([^\]|#\n]*)(#[^\]|\n]*)?(?:\|([^\]\n]+))?\]\]/.exec(src);
      if (!m || (!m[1].trim() && !m[2])) return;
      const target = m[1].trim() + (m[2] ?? "");
      const label = (m[3] ?? linkLabel(target)).trim();
      // O rótulo é Markdown também: [[#X|O vetor $\vec{u}$ **unitário**]]
      return { type: "wikilink", raw: m[0], target, label, embed: m[0].startsWith("!"), alias: m[3] ?? "", tokens: this.lexer.inlineTokens(label) };
    },
    renderer(tok) {
      const inner = this.parser.parseInline(tok.tokens ?? []);
      if (opts.exporting && tok.embed) {
        return `<span class="x-embed" data-target="${escape(tok.target)}" data-size="${escape(tok.alias)}">${inner}</span>`;
      }
      if (opts.exporting) {
        const { file, heading, path } = splitLink(tok.target);
        if (!file && heading) return `<a class="sx-wikilink" href="#${opts.anchor?.(path) ?? headingSlug(heading)}">${inner}</a>`;
        return `<span class="sx-wikilink">${inner}</span>`;
      }
      return `<a class="sx-wikilink" data-target="${escape(tok.target)}">${inner}</a>`;
    },
  };

  const highlight: TokenizerAndRendererExtension = {
    name: "highlight",
    level: "inline",
    start: (src) => src.indexOf("=="),
    tokenizer(src) {
      const m = /^==(?=\S)([\s\S]*?\S)==/.exec(src);
      if (!m) return;
      return { type: "highlight", raw: m[0], tokens: this.lexer.inlineTokens(m[1]) };
    },
    renderer(tok) {
      return `<mark class="sx-highlight">${this.parser.parseInline(tok.tokens ?? [])}</mark>`;
    },
  };

  const math: TokenizerAndRendererExtension = {
    name: "math",
    level: "inline",
    start: (src) => src.indexOf("$"),
    tokenizer(src) {
      const m = /^\$\$([^$]+?)\$\$/.exec(src) ?? /^\$(?![\s$])((?:\\.|[^$\\\n])*?[^\s\\])\$(?!\d)/.exec(src);
      if (!m) return;
      return { type: "math", raw: m[0], text: m[1], display: m[0].startsWith("$$") };
    },
    renderer: (tok) => renderMath(tok.text, tok.display),
  };

  /** Bloco $$ ... $$ (também logo abaixo de uma linha de texto, como no editor). */
  const mathBlock: TokenizerAndRendererExtension = {
    name: "mathBlock",
    level: "block",
    start: (src) => /^ {0,3}\$\$/m.exec(src)?.index,
    tokenizer(src) {
      const m = /^ {0,3}\$\$([\s\S]+?)\$\$[ \t]*(?:\n|$)/.exec(src);
      if (!m) return;
      return { type: "mathBlock", raw: m[0], text: m[1].trim() };
    },
    renderer: (tok) => `<div class="sx-math-block">${renderMath(tok.text, true)}</div>\n`,
  };

  const footnote: TokenizerAndRendererExtension = {
    name: "inlineFootnote",
    level: "inline",
    start: (src) => src.indexOf("^["),
    tokenizer(src) {
      const m = /^\^\[((?:[^[\]]|\[[^\]]*\])+)\]/.exec(src);
      if (!m) return;
      return { type: "inlineFootnote", raw: m[0], text: m[1].trim() };
    },
    renderer(tok) {
      if (!opts.notes) return `<sup class="sx-fn-ref" title="${escape(tok.text)}">*</sup>`;
      opts.notes.push(tok.text);
      const n = opts.notes.length;
      return `<sup class="sx-fn-ref" id="fnref-${n}"><a href="#fn-${n}">${n}</a></sup>`;
    },
  };

  return [wikilink, highlight, mathBlock, math, footnote];
}

const marked = new Marked({ gfm: true, breaks: false, extensions: extensions({}) });

const PURIFY = { ADD_ATTR: ["data-target", "target"], FORBID_TAGS: ["style", "form", "input"] };

export function renderInline(md: string): string {
  return DOMPurify.sanitize(marked.parseInline(md, { async: false }) as string, PURIFY);
}

export function renderBlock(md: string): string {
  return DOMPurify.sanitize(marked.parse(md, { async: false }) as string, PURIFY);
}

/** Markdown da nota inteira para a exportação (HTML/PDF), igual à leitura. */
export function renderForExport(
  md: string,
  notes: string[],
  blocks: { lang: string; source: string }[],
  langs: ReadonlySet<string>,
): string {
  const all = parseHeadings(md);
  const ids = uniqueSlugs(all.map((h) => h.text));
  const anchor = (path: string[]) => ids[findHeadingIndex(all, path)] ?? headingSlug(path[path.length - 1] ?? "");
  const seen = new Map<string, number>();
  const custom: MarkedExtension = {
    gfm: true,
    // Como no editor: cada quebra de linha do texto é uma quebra na página
    breaks: true,
    extensions: extensions({ exporting: true, notes, anchor }),
    renderer: {
      heading({ tokens, depth, text }) {
        const base = headingSlug(text);
        const n = (seen.get(base) ?? 0) + 1;
        seen.set(base, n);
        return `<h${depth} id="${n === 1 ? base : `${base}-${n}`}">${this.parser.parseInline(tokens)}</h${depth}>\n`;
      },
      checkbox({ checked }) {
        return `<span class="x-task${checked ? " is-done" : ""}"></span>`;
      },
      code({ text, lang }) {
        const l = (lang ?? "").trim().split(/\s+/)[0].toLowerCase();
        if (l === "mermaid" || l === "rustview" || langs.has(l)) {
          blocks.push({ lang: l, source: text });
          return `<div class="x-block" data-i="${blocks.length - 1}"></div>\n`;
        }
        return `<pre><code${l ? ` class="language-${escape(l)}"` : ""}>${escape(text)}</code></pre>\n`;
      },
    },
  };
  const m = new Marked(custom);
  return DOMPurify.sanitize(m.parse(md, { async: false }) as string, { ADD_ATTR: ["data-target", "data-size", "data-i"], FORBID_TAGS: ["style", "form", "input", "script"] });
}

/** Aceita só cores simples (nome, #hex, rgb/hsl) para spans do rustview. */
export function safeColor(c: unknown): string | null {
  if (typeof c !== "string") return null;
  return /^(#[0-9a-f]{3,8}|[a-z]{3,20}|(rgb|hsl)a?\([\d\s.,%/]+\))$/i.test(c.trim()) ? c.trim() : null;
}
