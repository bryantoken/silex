<script lang="ts">
  /*
   * Desenhos do Excalidraw (.excalidraw.md e .excalidraw) com o editor oficial
   * (carregado só quando um desenho é aberto). Salva no mesmo formato do plugin
   * do Obsidian, então o mesmo arquivo abre nos dois.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { parseExcalidraw, serializeExcalidraw, type Parsed } from "./excalidrawFormat";

  type Props = {
    path: string;
    compress: boolean;
    resolveFile: (name: string) => string | null;
    onnotify: (text: string, error?: boolean) => void;
  };
  let { path, compress, resolveFile, onnotify }: Props = $props();

  let host: HTMLDivElement;
  let status = $state("Carregando o Excalidraw…");
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let lastSaved = "";
  let pending: string | null = null;
  // Só salva quando o desenho muda de verdade (elementos ou fundo). Rolagem, zoom e o
  // enquadramento inicial não regravam o arquivo.
  let signature = "";
  const signOf = (elements: readonly any[], appState: any) =>
    elements.map((e) => `${e.id}:${e.version}:${e.isDeleted ? 1 : 0}`).join(",") + "|" + (appState?.viewBackgroundColor ?? "");

  async function writeNow() {
    const content = pending;
    pending = null;
    if (content === null || content === lastSaved) return;
    try {
      await invoke("write_text_file", { path, content });
      lastSaved = content;
    } catch (e) {
      onnotify(`Não foi possível salvar o desenho: ${e}`, true);
    }
  }

  function mimeOf(name: string) {
    const ext = name.split(".").pop()?.toLowerCase();
    return ext === "png" ? "image/png" : ext === "jpg" || ext === "jpeg" ? "image/jpeg" : ext === "gif" ? "image/gif" : ext === "svg" ? "image/svg+xml" : ext === "webp" ? "image/webp" : "application/octet-stream";
  }

  async function filesFor(p: Parsed) {
    const files: Record<string, any> = { ...p.scene.files };
    for (const [id, name] of Object.entries(p.embedded)) {
      const abs = resolveFile(name);
      if (!abs) continue;
      try {
        const buf = new Uint8Array(await invoke<ArrayBuffer>("read_file_bytes", { path: abs }));
        let bin = "";
        for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
        files[id] = { id, mimeType: mimeOf(name), dataURL: `data:${mimeOf(name)};base64,${btoa(bin)}`, created: Date.now() };
      } catch {
        /* imagem ausente: o Excalidraw mostra um marcador */
      }
    }
    return files;
  }

  onMount(() => {
    let root: { unmount(): void } | null = null;
    let parsed: Parsed;
    let cancelled = false;
    (async () => {
      try {
        const raw = await invoke<string>("read_note", { filePath: path });
        parsed = parseExcalidraw(raw, !/\.md$/i.test(path));
        lastSaved = raw;
        (window as unknown as { EXCALIDRAW_ASSET_PATH: string }).EXCALIDRAW_ASSET_PATH = "/excalidraw/";
        const [{ createElement }, { createRoot }, ex] = await Promise.all([
          import("react"),
          import("react-dom/client"),
          import("@excalidraw/excalidraw"),
          import("@excalidraw/excalidraw/index.css"),
        ]);
        if (cancelled) return;
        const files = await filesFor(parsed);
        signature = signOf(parsed.scene.elements, parsed.scene.appState);
        // O Excalidraw normaliza a cena ao carregar (ex.: remede textos quando as fontes
        // chegam); isso não é edição. Só grava depois que você interagir com o desenho.
        let touched = false;
        for (const ev of ["pointerdown", "keydown", "paste", "drop"]) host.addEventListener(ev, () => (touched = true), { capture: true });
        const save = (elements: readonly any[], appState: any) => {
          const sig = signOf(elements, appState);
          if (!touched) {
            signature = sig;
            return;
          }
          if (sig === signature) return;
          signature = sig;
          pending = serializeExcalidraw(parsed, elements, appState, compress);
          clearTimeout(saveTimer);
          saveTimer = setTimeout(writeNow, 800);
        };
        root = createRoot(host);
        const theme = document.documentElement.dataset.theme === "light" ? "light" : "dark";
        (root as any).render(
          createElement(ex.Excalidraw, {
            initialData: {
              elements: parsed.scene.elements,
              appState: { ...parsed.scene.appState, theme, collaborators: new Map() },
              files,
              scrollToContent: true,
            },
            theme,
            langCode: "pt-BR",
            onChange: save,
            UIOptions: { canvasActions: { loadScene: false, saveToActiveFile: false, export: { saveFileToDisk: true } } },
          }),
        );
        status = "";
      } catch (e) {
        status = `Não foi possível abrir o desenho: ${e}`;
      }
    })();
    return () => {
      cancelled = true;
      clearTimeout(saveTimer);
      writeNow();
      root?.unmount();
    };
  });
</script>

<div class="exd">
  {#if status}<div class="exd-status">{status}</div>{/if}
  <div class="exd-host" bind:this={host}></div>
</div>
