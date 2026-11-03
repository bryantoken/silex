/*
 * Fórmulas LaTeX renderizadas com KaTeX (editor, tabelas, notas de rodapé).
 * KaTeX escapa o texto e, sem `trust`, não gera links nem HTML arbitrário.
 */
import katex from "katex";
import "katex/dist/katex.min.css";

const cache = new Map<string, string>();

export function renderMath(tex: string, display: boolean): string {
  const key = `${display ? "D" : "I"}${tex}`;
  let html = cache.get(key);
  if (html === undefined) {
    html = katex.renderToString(tex, {
      displayMode: display,
      throwOnError: false,
      strict: "ignore",
      output: "html",
      errorColor: "var(--sx-danger)",
    });
    if (cache.size > 500) cache.clear();
    cache.set(key, html);
  }
  return html;
}

/** Remove os delimitadores: "$x$" → "x", "$$x$$" → "x" (também em blocos de várias linhas). */
export function mathSource(raw: string): { tex: string; display: boolean } {
  const s = raw.trim();
  if (s.startsWith("$$")) return { tex: s.slice(2, s.endsWith("$$") && s.length >= 4 ? -2 : undefined).trim(), display: true };
  return { tex: s.slice(1, -1), display: false };
}
