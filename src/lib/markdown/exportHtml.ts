/*
 * Exportação HTML/PDF com o mesmo renderizador da leitura: LaTeX (KaTeX),
 * Mermaid, callouts, ==grifos==, tarefas, notas ^[...], links para cabeçalhos,
 * imagens e seções embutidas. O mesmo documento vira o arquivo .html e o PDF
 * (impresso pelo WebView), então os dois saem iguais.
 */
import katexCss from "katex/dist/katex.min.css?inline";
import { escapeHtml, renderBlock, renderForExport, renderInline } from "./render";
import { sectionText } from "./headings";
import { splitLink } from "./links";
import { renderMermaid } from "./mermaid";
import { calloutKind } from "../editor/callouts";
import { ICONS } from "../icons";
import { renderNodes } from "../editor/widgets";
import type { BlockResult } from "../editor/context";

export type ExportEnv = {
  title: string;
  resolveFile(name: string): string | null;
  assetUrl(path: string): string;
  readNote(path: string): Promise<string | null>;
  renderBlock(lang: string, source: string, content: string): Promise<BlockResult>;
  blockLangs: ReadonlySet<string>;
};

const IMAGE = /\.(png|jpe?g|gif|webp|svg|bmp|avif)$/i;

function toDataUrl(url: string): Promise<string | null> {
  return fetch(url)
    .then((r) => (r.ok ? r.blob() : null))
    .then(
      (b) =>
        b &&
        new Promise<string | null>((resolve) => {
          const fr = new FileReader();
          fr.onload = () => resolve(String(fr.result));
          fr.onerror = () => resolve(null);
          fr.readAsDataURL(b);
        }),
    )
    .catch(() => null);
}

/** CSS do KaTeX com as fontes embutidas: o .html exportado funciona offline. */
let katexStandalone: Promise<string> | null = null;
function katexCssEmbedded(): Promise<string> {
  katexStandalone ??= (async () => {
    let css = katexCss;
    const urls = [...new Set([...css.matchAll(/url\(["']?([^)"']+?\.woff2)["']?\)/g)].map((m) => m[1]))];
    const data = await Promise.all(urls.map((u) => toDataUrl(new URL(u, location.href).href)));
    urls.forEach((u, i) => data[i] && (css = css.split(u).join(data[i]!)));
    return css;
  })();
  return katexStandalone;
}

function iconSvg(name: string) {
  const paths = (ICONS[name] ?? ICONS.dot).map((d) => `<path d="${d}"/>`).join("");
  return `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${paths}</svg>`;
}

/** > [!tipo] Título  →  caixa colorida com ícone (como no editor). */
function renderCallouts(root: ParentNode) {
  for (const bq of [...root.querySelectorAll("blockquote")].reverse()) {
    const p = bq.querySelector(":scope > p");
    if (!p) continue;
    const m = /^\s*\[!([\w-]+)\]([+-]?)[ \t]*/.exec(p.innerHTML);
    if (!m) continue;
    const kind = calloutKind(m[1]);
    let rest = p.innerHTML.slice(m[0].length);
    const cut = rest.search(/<br\s*\/?>|\n/);
    const title = (cut < 0 ? rest : rest.slice(0, cut)).trim() || escapeHtml(kind.title);
    rest = cut < 0 ? "" : rest.slice(cut).replace(/^(<br\s*\/?>)?\s*/, "");
    const box = document.createElement("div");
    box.className = "callout";
    box.style.setProperty("--c", kind.color.startsWith("var(") ? "#6b5bd6" : kind.color);
    box.innerHTML = `<div class="callout-title">${iconSvg(kind.icon)}<span>${title}</span></div>`;
    const body = document.createElement("div");
    body.className = "callout-body";
    if (rest.trim()) {
      const first = document.createElement("p");
      first.innerHTML = rest;
      body.append(first);
    }
    p.remove();
    body.append(...bq.childNodes);
    if (body.childNodes.length) box.append(body);
    bq.replaceWith(box);
  }
}

export async function noteToHtml(md: string, env: ExportEnv, standalone: boolean): Promise<string> {
  const source = md
    .replace(/^---\r?\n[\s\S]*?\r?\n---[ \t]*(\r?\n|$)/, "")
    .replace(/%%[\s\S]*?%%/g, "");
  const notes: string[] = [];
  const blocks: { lang: string; source: string }[] = [];
  const html = renderForExport(source, notes, blocks, env.blockLangs);

  const root = document.createElement("div");
  root.innerHTML = html;
  renderCallouts(root);

  // ![[imagem]] e ![[#Seção]] / ![[Nota#Seção]]
  await Promise.all(
    [...root.querySelectorAll<HTMLElement>(".x-embed")].map(async (span) => {
      const target = span.dataset.target ?? "";
      const { file, heading, path: hpath } = splitLink(target);
      if (heading) {
        let text: string | null = source;
        if (file) {
          const path = env.resolveFile(file);
          text = path ? await env.readNote(path) : null;
        }
        const section = text === null ? null : sectionText(text, hpath);
        const box = document.createElement("div");
        box.className = "embed-section";
        box.innerHTML = section === null ? `<em>Cabeçalho “${escapeHtml(heading)}” não encontrado</em>` : renderBlock(section);
        span.replaceWith(box);
        return;
      }
      const path = env.resolveFile(file);
      if (path && IMAGE.test(path)) {
        const data = await toDataUrl(env.assetUrl(path));
        if (data) {
          const img = document.createElement("img");
          img.src = data;
          img.alt = file;
          const w = parseInt(span.dataset.size ?? "", 10);
          if (w) img.width = w;
          span.replaceWith(img);
          return;
        }
      }
      span.replaceWith(Object.assign(document.createElement("em"), { textContent: file }));
    }),
  );

  // ```mermaid e blocos renderizados (rustview, plugins)
  await Promise.all(
    [...root.querySelectorAll<HTMLElement>(".x-block")].map(async (div) => {
      const b = blocks[Number(div.dataset.i)];
      if (!b) return div.remove();
      try {
        if (b.lang === "mermaid") {
          div.className = "mermaid";
          div.innerHTML = await renderMermaid(b.source, "default");
        } else {
          const result = await env.renderBlock(b.lang, b.source, md);
          div.className = "block-out";
          div.replaceChildren(renderNodes(result?.nodes ?? []));
        }
      } catch (e) {
        div.outerHTML = `<pre><code>${escapeHtml(b.source)}</code></pre>`;
      }
    }),
  );

  if (notes.length) {
    const items = notes
      .map((t, i) => `<li id="fn-${i + 1}">${renderInline(t)} <a class="fn-back" href="#fnref-${i + 1}">↩</a></li>`)
      .join("");
    root.insertAdjacentHTML("beforeend", `<section class="footnotes"><div class="footnotes-title">Notas</div><ol>${items}</ol></section>`);
  }

  const katex = standalone ? await katexCssEmbedded() : katexCss;
  const title = escapeHtml(env.title);
  return `<!doctype html>
<html lang="pt-BR"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>${title}</title>
<style>${katex}</style>
<style>${EXPORT_CSS}</style></head>
<body><article><h1 class="title">${title}</h1>
${root.innerHTML}
</article></body></html>`;
}

// Papel branco, mesma hierarquia visual da leitura. Cores de fundo são impressas
// (print-color-adjust) para grifos, tabelas e callouts saírem no PDF.
const EXPORT_CSS = `
* { box-sizing: border-box; -webkit-print-color-adjust: exact; print-color-adjust: exact; }
@page { margin: 0; }
html { background: #fff; }
body { margin: 0; color: #1c1c1f; font-family: Inter, "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif; font-size: 15.5px; line-height: 1.7; -webkit-font-smoothing: antialiased; }
article { max-width: 780px; margin: 0 auto; padding: 40px 32px 64px; }
@media print { article { max-width: none; padding: 16mm 15mm; box-decoration-break: clone; -webkit-box-decoration-break: clone; } }
h1, h2, h3, h4, h5, h6 { color: #0b0b0d; line-height: 1.3; margin: 1.5em 0 0.5em; break-after: avoid; page-break-after: avoid; }
h1 { font-size: 1.9em; letter-spacing: -0.02em; } h2 { font-size: 1.55em; letter-spacing: -0.015em; }
h3 { font-size: 1.3em; } h4 { font-size: 1.12em; } h5, h6 { font-size: 1em; color: #5f5f66; }
h1.title { font-size: 2.2em; margin: 0 0 0.6em; }
p { margin: 0.55em 0; }
strong { font-weight: 700; color: #0b0b0d; }
a { color: #3b5bdb; text-decoration: none; border-bottom: 1px solid rgb(59 91 219 / 0.3); }
.sx-wikilink { color: #3b5bdb; }
mark, .sx-highlight { background: #fff0a0; color: inherit; padding: 0 2px; border-radius: 3px; }
del { color: #77777f; }
code { font-family: "JetBrains Mono", "Cascadia Code", Consolas, monospace; font-size: 0.86em; background: #f2f2f5; padding: 1px 5px; border-radius: 4px; }
pre { background: #f6f6f9; border: 1px solid #e6e6ec; border-radius: 10px; padding: 12px 14px; white-space: pre-wrap; overflow-wrap: anywhere; break-inside: avoid; }
pre code { background: none; padding: 0; font-size: 0.85em; }
blockquote { margin: 0.8em 0; padding: 2px 16px; border-left: 3px solid #d0d0d8; color: #4a4a52; }
hr { border: none; border-top: 1px solid #e2e2e8; margin: 2em 0; }
img { max-width: 100%; height: auto; border-radius: 6px; }
ul, ol { padding-left: 1.6em; margin: 0.4em 0; }
li { margin: 0.15em 0; }
li:has(> .x-task) { list-style: none; margin-left: -1.4em; }
.x-task { display: inline-block; width: 14px; height: 14px; margin-right: 8px; vertical-align: -2px; border: 1.5px solid #9a9aa3; border-radius: 4px; }
.x-task.is-done { background: #2f6fdf; border-color: #2f6fdf; box-shadow: inset 0 0 0 2px #fff; }
li:has(> .x-task.is-done) { color: #8a8a92; text-decoration: line-through; }

/* Tabelas: sem rolagem, o texto quebra dentro das células */
table { border-collapse: collapse; width: 100%; margin: 1em 0; font-size: 0.92em; line-height: 1.5; }
th, td { border: 1px solid #dedee5; padding: 7px 10px; text-align: left; vertical-align: top; overflow-wrap: break-word; }
th { background: #f3f3f7; font-weight: 600; color: #0b0b0d; }
thead { display: table-header-group; }
tr { break-inside: avoid; page-break-inside: avoid; }

/* LaTeX */
.katex { font-size: 1.08em; }
.sx-math-block { margin: 0.8em 0; text-align: center; overflow-x: auto; overflow-y: hidden; }
.katex-display { margin: 0.4em 0; }

/* Callouts */
.callout { --c: #3b82f6; margin: 1em 0; border-radius: 10px; border: 1px solid color-mix(in srgb, var(--c) 35%, transparent); background: color-mix(in srgb, var(--c) 9%, #fff); padding: 10px 16px; break-inside: avoid; }
.callout-title { display: flex; align-items: center; gap: 8px; font-weight: 650; color: color-mix(in srgb, var(--c) 75%, #000); }
.callout-title svg { flex: none; }
.callout-body > :first-child { margin-top: 0.3em; } .callout-body > :last-child { margin-bottom: 0.1em; }

/* Mermaid, blocos e seções embutidas */
.mermaid { display: flex; justify-content: center; margin: 1em 0; break-inside: avoid; }
.mermaid svg { max-width: 100%; height: auto; }
.block-out, .embed-section { margin: 1em 0; padding: 10px 16px; border: 1px solid #e6e6ec; border-radius: 10px; background: #fafafc; }
.embed-section { border-left: 3px solid #c8c8d2; border-radius: 0 10px 10px 0; }

/* Notas de rodapé */
.sx-fn-ref { font-size: 0.72em; font-weight: 600; line-height: 0; }
.sx-fn-ref a { border: none; }
.footnotes { margin-top: 2.5em; padding-top: 0.8em; border-top: 1px solid #dedee5; font-size: 0.9em; color: #4a4a52; }
.footnotes-title { font-size: 0.78em; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; color: #8a8a92; margin-bottom: 4px; }
.footnotes ol { margin: 0; padding-left: 1.6em; }
.fn-back { border: none; font-size: 0.85em; }
`;
