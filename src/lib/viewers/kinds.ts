export type FileKind = "markdown" | "image" | "video" | "audio" | "pdf" | "sheet" | "canvas" | "excalidraw" | "other";

const KINDS: Record<string, FileKind> = {
  md: "markdown",
  png: "image",
  jpg: "image",
  jpeg: "image",
  gif: "image",
  webp: "image",
  svg: "image",
  bmp: "image",
  avif: "image",
  ico: "image",
  mp4: "video",
  webm: "video",
  mov: "video",
  m4v: "video",
  ogv: "video",
  mp3: "audio",
  wav: "audio",
  m4a: "audio",
  ogg: "audio",
  flac: "audio",
  aac: "audio",
  pdf: "pdf",
  xlsx: "sheet",
  xlsm: "sheet",
  xlsb: "sheet",
  xls: "sheet",
  ods: "sheet",
  csv: "sheet",
  tsv: "sheet",
  canvas: "canvas",
  excalidraw: "excalidraw",
};

export function fileKind(path: string): FileKind {
  if (/\.excalidraw(\.md)?$/i.test(path)) return "excalidraw";
  const ext = /\.([^.\\/]+)$/.exec(path)?.[1]?.toLowerCase() ?? "";
  return KINDS[ext] ?? "other";
}

export const KIND_ICON: Record<FileKind, string> = {
  markdown: "dot",
  image: "image",
  video: "video",
  audio: "video",
  pdf: "pdf",
  sheet: "sheet",
  canvas: "graph",
  excalidraw: "pencil",
  other: "file",
};
