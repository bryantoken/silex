/**
 * Busca aproximada por subsequência (como a paleta do VS Code/Obsidian).
 * Pontua sequências contíguas e início de palavras; ignora acentos e caixa.
 */
const fold = (s: string) => s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();

export type FuzzyMatch = { score: number; indices: number[] };

export function fuzzy(query: string, text: string): FuzzyMatch | null {
  const q = fold(query).replace(/\s+/g, "");
  if (!q) return { score: 0, indices: [] };
  const t = fold(text);
  const indices: number[] = [];
  let score = 0;
  let ti = 0;
  let prev = -2;
  for (const ch of q) {
    const found = t.indexOf(ch, ti);
    if (found < 0) return null;
    const wordStart = found === 0 || /[\s\-_/.:()]/.test(t[found - 1]);
    score += found === prev + 1 ? 6 : wordStart ? 4 : 1;
    score -= Math.min(3, (found - ti) * 0.1);
    indices.push(found);
    prev = found;
    ti = found + 1;
  }
  // Prefere textos mais curtos e com match no começo
  score -= t.length * 0.02 + indices[0] * 0.05;
  return { score, indices };
}

/** Divide o texto em partes destacadas/não destacadas a partir dos índices. */
export function highlightParts(text: string, indices: number[]) {
  const set = new Set(indices);
  const parts: { text: string; hl: boolean }[] = [];
  for (let i = 0; i < text.length; i++) {
    const hl = set.has(i);
    const last = parts[parts.length - 1];
    if (last && last.hl === hl) last.text += text[i];
    else parts.push({ text: text[i], hl });
  }
  return parts;
}
