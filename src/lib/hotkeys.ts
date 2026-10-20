/*
 * Atalhos de teclado. Formato canônico: "Ctrl+Alt+Shift+Tecla" (Ctrl também cobre
 * Cmd no macOS). Letras e números usam a tecla física, então Alt+letra e
 * Shift+número funcionam em qualquer layout.
 */

const ARROWS: Record<string, string> = { ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→" };

export function comboOf(e: KeyboardEvent): string | null {
  if (["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock", "Dead", "Unidentified"].includes(e.key)) return null;
  let key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit\d$/.test(e.code)) key = e.code.slice(5);
  else if (e.code === "Comma") key = ",";
  else if (e.code === "Period") key = ".";
  if (key === " ") key = "Space";
  const mods = [(e.ctrlKey || e.metaKey) && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift"].filter(Boolean);
  return [...mods, key].join("+");
}

/** Atalho válido: com Ctrl ou Alt, ou uma tecla de função sozinha (F1–F12). */
export function validCombo(c: string) {
  return /(^|\+)(Ctrl|Alt)\+/.test(c) || /^(Shift\+)?F([1-9]|1[0-2])$/.test(c);
}

/** Para exibir: "Ctrl Shift ↑" */
export function showCombo(c: string | undefined) {
  return c ? c.split("+").map((k) => ARROWS[k] ?? k).join(" ") : "";
}
