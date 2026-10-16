<script lang="ts">
  /*
   * Visualizador de PDF com pdf.js. Os bytes vêm do Rust (read_file_bytes) e as
   * páginas só são desenhadas quando entram na tela.
   */
  import { invoke } from "@tauri-apps/api/core";
  import * as pdfjs from "pdfjs-dist";
  import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";
  import type { PDFDocumentProxy } from "pdfjs-dist";
  import Icon from "../components/Icon.svelte";

  pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

  let { path }: { path: string } = $props();

  let pdf = $state.raw<PDFDocumentProxy | null>(null);
  let pages = $state<number[]>([]);
  let zoom = $state(1.2);
  let error = $state<string | null>(null);
  let container = $state<HTMLDivElement>();

  $effect(() => {
    const p = path;
    let cancelled = false;
    let task: ReturnType<typeof pdfjs.getDocument> | null = null;
    pdf = null;
    error = null;
    (async () => {
      try {
        const buf = await invoke<ArrayBuffer>("read_file_bytes", { path: p });
        if (cancelled) return;
        task = pdfjs.getDocument({ data: new Uint8Array(buf) });
        const loaded = await task.promise;
        if (cancelled) return;
        pdf = loaded;
        pages = Array.from({ length: loaded.numPages }, (_, i) => i + 1);
      } catch (e) {
        if (!cancelled) error = String(e);
      }
    })();
    return () => {
      cancelled = true;
      task?.destroy();
    };
  });

  function page(node: HTMLCanvasElement, params: { n: number; zoom: number }) {
    let { n, zoom: scale } = params;
    let rendered = -1;
    let visible = false;
    const render = async () => {
      if (!pdf || !visible || rendered === scale) return;
      rendered = scale;
      const pg = await pdf.getPage(n);
      const dpr = window.devicePixelRatio || 1;
      const viewport = pg.getViewport({ scale: scale * dpr });
      node.width = viewport.width;
      node.height = viewport.height;
      node.style.width = `${viewport.width / dpr}px`;
      node.style.height = `${viewport.height / dpr}px`;
      await pg.render({ canvasContext: node.getContext("2d")!, viewport, canvas: node }).promise;
    };
    const size = () =>
      pdf?.getPage(n).then((pg) => {
        // Tamanho provisório para a rolagem não pular antes de desenhar
        const v = pg.getViewport({ scale });
        node.style.width = `${v.width}px`;
        node.style.height = `${v.height}px`;
      });
    size();
    const io = new IntersectionObserver(
      (entries) => {
        visible = entries[0].isIntersecting;
        render();
      },
      { root: container, rootMargin: "600px 0px" },
    );
    io.observe(node);
    return {
      update(next: { n: number; zoom: number }) {
        ({ n, zoom: scale } = next);
        size();
        render();
      },
      destroy: () => io.disconnect(),
    };
  }
</script>

<div class="sx-pdf">
  <div class="sx-pdf-bar">
    <button class="icon-btn sm" title="Diminuir zoom" onclick={() => (zoom = Math.max(0.4, zoom - 0.2))}>−</button>
    <span>{Math.round(zoom * 100)}%</span>
    <button class="icon-btn sm" title="Aumentar zoom" onclick={() => (zoom = Math.min(4, zoom + 0.2))}>
      <Icon name="plus" size={14} />
    </button>
    {#if pdf}<span class="muted">{pdf.numPages} página{pdf.numPages === 1 ? "" : "s"}</span>{/if}
  </div>
  <div class="sx-pdf-pages" bind:this={container}>
    {#if error}
      <div class="sx-viewer-error">Não foi possível abrir o PDF: {error}</div>
    {:else if !pdf}
      <div class="sx-viewer-loading">Carregando…</div>
    {:else}
      {#each pages as n (n)}
        <canvas class="sx-pdf-page" use:page={{ n, zoom }}></canvas>
      {/each}
    {/if}
  </div>
</div>
