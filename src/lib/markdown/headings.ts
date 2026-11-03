/*
 * Cabeçalhos de uma nota (sumário, [[#autocompletar]], salto e ![[#embed]] de seção).
 * Leitura direta do texto: ignora frontmatter e blocos de código/matemática.
 */
import { headingKey } from "./links";

export type Heading = { level: number; text: string; from: number; line: number };

export function parseHeadings(md: string): Heading[] {
  const out: Heading[] = [];
  const lines = md.split("\n");
  let pos = 0;
  let fence: string | null = null;
  let math = false;
  let i = 0;
  if (lines[0]?.trimEnd() === "---") {
    for (let j = 1; j < lines.length; j++) {
      if (lines[j].trimEnd() === "---") {
        for (let k = 0; k <= j; k++) pos += lines[k].length + 1;
        i = j + 1;
        break;
      }
    }
  }
  for (; i < lines.length; i++) {
    const line = lines[i];
    const from = pos;
    pos += line.length + 1;
    const f = /^\s{0,3}(```+|~~~+)/.exec(line);
    if (fence) {
      if (f && f[1][0] === fence[0] && f[1].length >= fence.length) fence = null;
      continue;
    }
    if (f) {
      fence = f[1];
      continue;
    }
    if (/^\s*\$\$\s*$/.test(line)) {
      math = !math;
      continue;
    }
    if (math) continue;
    const h = /^(#{1,6})[ \t]+(.+?)[ \t#]*$/.exec(line);
    if (h) out.push({ level: h[1].length, text: h[2].trim(), from, line: i + 1 });
  }
  return out;
}

/** Cabeçalho pelo texto (último segmento de #A#B). */
export function findHeading(md: string, heading: string): Heading | null {
  const key = headingKey(heading);
  return parseHeadings(md).find((h) => headingKey(h.text) === key) ?? null;
}

/** Texto da seção (sem a linha do cabeçalho) até o próximo cabeçalho de nível igual ou maior. */
export function sectionText(md: string, heading: string): string | null {
  const all = parseHeadings(md);
  const key = headingKey(heading);
  const i = all.findIndex((h) => headingKey(h.text) === key);
  if (i < 0) return null;
  const h = all[i];
  const next = all.slice(i + 1).find((n) => n.level <= h.level);
  const start = md.indexOf("\n", h.from);
  if (start < 0) return "";
  return md.slice(start + 1, next ? next.from : md.length).trim();
}
