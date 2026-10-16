// Status de tarefas: - [ ] padrão do Markdown e extensões populares do Obsidian.
export type TaskStatus = { char: string; name: string; label: string; glyph: string };

export const TASK_STATUSES: TaskStatus[] = [
  { char: " ", name: "todo", label: "A fazer", glyph: "" },
  { char: "x", name: "done", label: "Concluída", glyph: "✓" },
  { char: "/", name: "progress", label: "Em andamento", glyph: "" },
  { char: "-", name: "cancelled", label: "Cancelada / não se aplica", glyph: "–" },
  { char: ">", name: "forwarded", label: "Adiada", glyph: "›" },
  { char: "<", name: "scheduled", label: "Agendada", glyph: "‹" },
  { char: "?", name: "question", label: "Pergunta", glyph: "?" },
  { char: "!", name: "important", label: "Importante", glyph: "!" },
  { char: "*", name: "star", label: "Destaque", glyph: "★" },
  { char: "~", name: "partial", label: "Parcial", glyph: "~" },
  { char: "i", name: "info", label: "Informação", glyph: "i" },
  { char: "b", name: "bookmark", label: "Marcador", glyph: "▾" },
];

const BY_CHAR = new Map(TASK_STATUSES.map((s) => [s.char, s]));

export function taskStatus(char: string): TaskStatus {
  if (char === "X") return BY_CHAR.get("x")!;
  return BY_CHAR.get(char) ?? { char, name: "custom", label: char, glyph: char };
}

/** "- [c] " no início da linha (após indentação). Grupo 1 = prefixo até "[", 2 = status. */
export const TASK_LINE = /^(\s*(?:[-*+]|\d+[.)])\s+\[)(.)\](?=\s|$)/;
