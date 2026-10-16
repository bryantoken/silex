// Aplica a configuração visual do cofre: tokens, tema e snippets CSS.
import { invoke } from "@tauri-apps/api/core";
import type { VaultConfig } from "./components/Settings.svelte";

function styleTag(id: string) {
  let el = document.getElementById(id) as HTMLStyleElement | null;
  if (!el) {
    el = document.createElement("style");
    el.id = id;
    document.head.appendChild(el);
  }
  return el;
}

const systemDark = typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;
let scheme: VaultConfig["color_scheme"] = "dark";

function applyScheme() {
  const dark = scheme === "dark" || (scheme === "system" && systemDark?.matches !== false);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  try {
    localStorage.setItem("silex.scheme", scheme);
  } catch {}
}
systemDark?.addEventListener("change", () => scheme === "system" && applyScheme());

export async function applyAppearance(cfg: VaultConfig) {
  const root = document.documentElement;
  scheme = cfg.color_scheme ?? "dark";
  applyScheme();
  root.style.setProperty("--sx-accent-h", String(cfg.accent_hue));
  root.style.setProperty("--sx-editor-size", `${cfg.font_size}px`);
  root.style.setProperty("--sx-line-width", cfg.readable_line_width ? `${cfg.line_width}px` : "100%");

  let themeCss = "";
  if (cfg.theme) {
    themeCss = await invoke<string>("read_css", { kind: "theme", name: cfg.theme }).catch(() => "");
  }
  styleTag("sx-theme").textContent = themeCss;
  // Temas do Obsidian definem variáveis como --background-primary em .theme-dark;
  // a classe sx-obsidian-theme liga o mapeamento delas para os tokens do Silex.
  const obsidian = /--background-primary|\.theme-dark|--text-normal/.test(themeCss);
  document.body.classList.toggle("theme-dark", obsidian);
  document.body.classList.toggle("sx-obsidian-theme", obsidian);

  const snippets = await Promise.all(
    cfg.snippets.map((name) => invoke<string>("read_css", { kind: "snippet", name }).catch(() => "")),
  );
  styleTag("sx-snippets").textContent = snippets.join("\n");
}
