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

/**
 * Cabeçalho por caminho (#A#B, como o Obsidian grava): cada trecho é procurado
 * depois do anterior, então "#1ª Questão#Resolução" acha a "Resolução" daquela
 * questão mesmo havendo outras iguais. Trechos não encontrados são ignorados.
 */
export function findHeadingIndex(all: Heading[], path: string | string[]): number {
  const segs = (Array.isArray(path) ? path : [path]).map(headingKey).filter(Boolean);
  let at = -1;
  for (let s = 0; s < segs.length; s++) {
    const i = all.findIndex((h, j) => j > at && headingKey(h.text) === segs[s]);
    if (i >= 0) at = i;
    else if (s === segs.length - 1) {
      // Último trecho fora da ordem esperada: aceita a primeira ocorrência
      const any = all.findIndex((h) => headingKey(h.text) === segs[s]);
      return any;
    }
  }
  return segs.length ? at : -1;
}

export function findHeading(md: string, path: string | string[]): Heading | null {
  const all = parseHeadings(md);
  return all[findHeadingIndex(all, path)] ?? null;
}

/** Texto da seção (sem a linha do cabeçalho) até o próximo cabeçalho de nível igual ou maior. */
export function sectionText(md: string, path: string | string[]): string | null {
  const all = parseHeadings(md);
  const i = findHeadingIndex(all, path);
  if (i < 0) return null;
  const h = all[i];
  const next = all.slice(i + 1).find((n) => n.level <= h.level);
  const start = md.indexOf("\n", h.from);
  if (start < 0) return "";
  return md.slice(start + 1, next ? next.from : md.length).trim();
}
