<script lang="ts">
  import { convertFileSrc } from "@tauri-apps/api/core";
  import { fileKind } from "./kinds";

  let { path }: { path: string } = $props();
  let kind = $derived(fileKind(path));
  let src = $derived(convertFileSrc(path));
  let fit = $state(true);
  let size = $state("");
</script>

<div class="sx-media" class:is-actual={!fit}>
  {#if kind === "image"}
    <button class="sx-media-stage" title={fit ? "Clique para ver em 100%" : "Clique para ajustar à tela"} onclick={() => (fit = !fit)}>
      <img
        {src}
        alt=""
        draggable="false"
        onload={(e) => {
          const img = e.currentTarget as HTMLImageElement;
          size = `${img.naturalWidth} × ${img.naturalHeight}`;
        }}
      />
    </button>
    {#if size}<div class="sx-media-meta">{size} px · {fit ? "ajustada" : "100%"}</div>{/if}
  {:else if kind === "video"}
    <div class="sx-media-stage">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video {src} controls preload="metadata"></video>
    </div>
  {:else}
    <div class="sx-media-stage is-audio">
      <audio {src} controls preload="metadata"></audio>
    </div>
  {/if}
</div>
