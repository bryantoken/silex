/*
 * Formato de arquivo do plugin Excalidraw do Obsidian (.excalidraw.md):
 *   frontmatter (excalidraw-plugin: parsed) + aviso
 *   # Excalidraw Data
 *   ## Text Elements      textos do desenho (para busca e links), "texto ^id"
 *   ## Element Links / ## Embedded Files   "fileId: [[imagem.png]]"
 *   %%
 *   ## Drawing
 *   ```compressed-json    JSON da cena comprimido com LZString (base64, linhas de 256)
 *   ```
 *   %%
 * Também lê .excalidraw (JSON puro).
 */
import LZString from "lz-string";

export type Scene = { elements: any[]; appState: Record<string, any>; files: Record<string, any>; [k: string]: unknown };
export type Parsed = { scene: Scene; head: string; extraSections: string; embedded: Record<string, string>; compressed: boolean; plainJson: boolean };

const DRAWING = /```(compressed-json|json)\r?\n([\s\S]*?)```/;

export function parseExcalidraw(raw: string, isPlainJson: boolean): Parsed {
  if (isPlainJson) {
    const scene = JSON.parse(raw || "{}");
    return { scene: normalize(scene), head: "", extraSections: "", embedded: {}, compressed: false, plainJson: true };
  }
  const m = DRAWING.exec(raw);
  let scene: Scene = { elements: [], appState: {}, files: {} };
  let compressed = true;
  if (m) {
    compressed = m[1] === "compressed-json";
    const text = compressed ? LZString.decompressFromBase64(m[2].replace(/\s+/g, "")) : m[2];
    if (!text) throw Error("Não foi possível descomprimir o desenho.");
    scene = normalize(JSON.parse(text));
  }
  const dataAt = raw.search(/^# Excalidraw Data\s*$/m);
  const head = dataAt >= 0 ? raw.slice(0, dataAt) : raw.split(/^%%\s*$/m)[0];
  // Seções preservadas (Element Links, Embedded Files), sem a de textos (regerada)
  const body = dataAt >= 0 ? raw.slice(dataAt) : "";
  const drawingAt = body.search(/^%%\s*$\r?\n## Drawing/m);
  const sections = (drawingAt >= 0 ? body.slice(0, drawingAt) : body).split(/^(?=## )/m).slice(1);
  const extraSections = sections.filter((s) => !s.startsWith("## Text Elements")).join("").trim();
  const embedded: Record<string, string> = {};
  for (const line of extraSections.split("\n")) {
    const e = /^([0-9a-f]{20,64}):\s*\[\[([^\]|]+)(?:\|[^\]]*)?\]\]/i.exec(line.trim());
    if (e) embedded[e[1]] = e[2];
  }
  return { scene, head: head.trimEnd(), extraSections, embedded, compressed, plainJson: false };
}

function normalize(s: any): Scene {
  return {
    ...s,
    type: s.type ?? "excalidraw",
    version: s.version ?? 2,
    elements: Array.isArray(s.elements) ? s.elements : [],
    appState: s.appState ?? {},
    files: s.files ?? {},
  };
}

/** Só as partes do estado da tela que o plugin do Obsidian também guarda. */
const KEEP_APPSTATE = [
  "theme",
  "viewBackgroundColor",
  "currentItemStrokeColor",
  "currentItemBackgroundColor",
  "currentItemFillStyle",
  "currentItemStrokeWidth",
  "currentItemStrokeStyle",
  "currentItemRoughness",
  "currentItemOpacity",
  "currentItemFontFamily",
  "currentItemFontSize",
  "currentItemTextAlign",
  "currentItemStartArrowhead",
  "currentItemEndArrowhead",
  "currentItemRoundness",
  "gridSize",
  "scrollX",
  "scrollY",
  "zoom",
];

export function serializeExcalidraw(p: Parsed, elements: readonly any[], appState: Record<string, any>, compress: boolean): string {
  const live = elements.filter((e) => !e.isDeleted);
  const keep = Object.fromEntries(KEEP_APPSTATE.filter((k) => k in appState).map((k) => [k, appState[k]]));
  const scene = { ...p.scene, elements: live, appState: keep, files: {} };
  if (p.plainJson) return JSON.stringify({ ...scene, files: p.scene.files ?? {} }, null, 2);

  const json = JSON.stringify(scene, null, "\t");
  let drawing: string;
  if (compress) {
    const b64 = LZString.compressToBase64(json);
    const lines: string[] = [];
    for (let i = 0; i < b64.length; i += 256) lines.push(b64.slice(i, i + 256));
    drawing = "```compressed-json\n" + lines.join("\n\n") + "\n```";
  } else {
    drawing = "```json\n" + json + "\n```";
  }
  const texts = live
    .filter((e) => e.type === "text" && typeof e.text === "string" && e.text.trim())
    .map((e) => `${e.text} ^${e.id}\n`)
    .join("\n");
  const head = p.head || "---\n\nexcalidraw-plugin: parsed\ntags: [excalidraw]\n\n---\n==⚠  Switch to EXCALIDRAW VIEW in the MORE OPTIONS menu of this document. ⚠==";
  return `${head}\n\n\n# Excalidraw Data\n## Text Elements\n${texts}\n${p.extraSections ? p.extraSections + "\n\n" : ""}%%\n## Drawing\n${drawing}\n%%`;
}

export function emptyDrawing(): string {
  return serializeExcalidraw(
    { scene: { type: "excalidraw", version: 2, elements: [], appState: {}, files: {} }, head: "", extraSections: "", embedded: {}, compressed: true, plainJson: false },
    [],
    { viewBackgroundColor: "#ffffff" },
    true,
  );
}
