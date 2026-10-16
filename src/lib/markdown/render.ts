/*
 * Renderização de Markdown para HTML seguro, usada nos widgets do editor
 * (tabelas, saída do rustview, propriedades). Suporta [[wikilinks]] e ==realce==.
 * Todo HTML passa pelo DOMPurify antes de ir para o DOM.
 */
import { Marked, type TokenizerAndRendererExtension } from "marked";
import DOMPurify from "dompurify";

const escape = (s: string) =>
  s.replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]!);

const wikilink: TokenizerAndRendererExtension = {
  name: "wikilink",
  level: "inline",
  start: (src) => src.indexOf("[["),
  tokenizer(src) {
    const m = /^!?\[\[([^\]|#\n]+)(#[^\]|\n]*)?(?:\|([^\]\n]+))?\]\]/.exec(src);
    if (!m) return;
    return { type: "wikilink", raw: m[0], target: m[1].trim(), label: (m[3] ?? m[1]).trim() };
  },
  renderer: (tok) => `<a class="sx-wikilink" data-target="${escape(tok.target)}">${escape(tok.label)}</a>`,
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

const marked = new Marked({ gfm: true, breaks: false, extensions: [wikilink, highlight] });

const PURIFY = { ADD_ATTR: ["data-target", "target"], FORBID_TAGS: ["style", "form", "input"] };

export function renderInline(md: string): string {
  return DOMPurify.sanitize(marked.parseInline(md, { async: false }) as string, PURIFY);
}

export function renderBlock(md: string): string {
  return DOMPurify.sanitize(marked.parse(md, { async: false }) as string, PURIFY);
}

/** Aceita só cores simples (nome, #hex, rgb/hsl) para spans do rustview. */
export function safeColor(c: unknown): string | null {
  if (typeof c !== "string") return null;
  return /^(#[0-9a-f]{3,8}|[a-z]{3,20}|(rgb|hsl)a?\([\d\s.,%/]+\))$/i.test(c.trim()) ? c.trim() : null;
}
