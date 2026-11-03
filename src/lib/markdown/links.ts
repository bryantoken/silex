/** [[Nota#Cabeçalho|apelido]]: separa a nota do caminho de cabeçalhos (o Obsidian aceita #A#B). */
export function splitLink(target: string): { file: string; heading: string; path: string[] } {
  const i = target.indexOf("#");
  if (i < 0) return { file: target.trim(), heading: "", path: [] };
  const path = target
    .slice(i + 1)
    .split("#")
    .map((s) => s.trim())
    .filter(Boolean);
  return { file: target.slice(0, i).trim(), heading: path[path.length - 1] ?? "", path };
}

/** Texto exibido de um wikilink sem apelido. */
export function linkLabel(target: string): string {
  const { file, heading } = splitLink(target);
  if (!heading) return file;
  return file ? `${file} › ${heading}` : heading;
}

/** Normaliza um título para comparar (o Obsidian ignora maiúsculas e espaços repetidos). */
export const headingKey = (s: string) => s.replace(/\s+/g, " ").trim().toLowerCase();
