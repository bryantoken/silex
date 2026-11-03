/*
 * Diagramas ```mermaid. A biblioteca é grande: só carrega no primeiro diagrama.
 * securityLevel "strict" desliga cliques/HTML dentro dos diagramas.
 */
type Mermaid = typeof import("mermaid").default;

let lib: Promise<Mermaid> | null = null;
let theme = "";
let queue: Promise<unknown> = Promise.resolve();
let seq = 0;

const isDark = () => document.documentElement.dataset.theme !== "light";

function load(): Promise<Mermaid> {
  lib ??= import("mermaid").then((m) => m.default);
  return lib;
}

/** Renderiza para SVG. As chamadas vão em fila (o mermaid não aceita renderizações em paralelo). */
export function renderMermaid(source: string): Promise<string> {
  const job = queue.then(async () => {
    const mermaid = await load();
    const want = isDark() ? "dark" : "default";
    if (theme !== want) {
      theme = want;
      mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: want, fontFamily: "inherit" });
    }
    const id = `sx-mermaid-${++seq}`;
    try {
      const { svg } = await mermaid.render(id, source);
      return svg;
    } finally {
      document.getElementById(id)?.remove();
      document.getElementById(`d${id}`)?.remove();
    }
  });
  queue = job.catch(() => {});
  return job;
}

export const mermaidThemeKey = () => (isDark() ? "dark" : "light");
