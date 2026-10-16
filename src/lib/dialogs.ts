// Diálogos de arquivo do sistema, abertos pelo Rust na thread principal (src-tauri/src/dialogs.rs).
import { invoke } from "@tauri-apps/api/core";

type Filter = { name: string; extensions: string[] };

// O diálogo modal só abre depois que o clique/tecla que o pediu terminou de ser
// processado pelo WebView2 (abrir no meio do evento deixava a entrada presa)
const afterInput = () => new Promise((r) => setTimeout(r, 30));

export async function openFile(opts: { title: string; directory?: boolean; filters?: Filter[] }) {
  await afterInput();
  const f = opts.filters?.[0];
  return invoke<string | null>("dialog_open", {
    title: opts.title,
    directory: !!opts.directory,
    filterName: f?.name ?? null,
    extensions: f?.extensions ?? null,
  });
}

export async function openFiles(title: string) {
  await afterInput();
  return invoke<string[]>("dialog_open_many", { title });
}
