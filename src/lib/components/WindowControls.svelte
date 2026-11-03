<script lang="ts">
  // Minimizar / maximizar / fechar desenhados pelo Silex (a janela não tem a barra
  // nativa: decorations=false). Seguem as cores do tema ativo.
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { onMount } from "svelte";

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const sync = () => win.isMaximized().then((m) => (maximized = m)).catch(() => {});
    sync();
    const off = win.onResized(sync);
    return () => void off.then((f) => f());
  });
</script>

<div class="sx-winctl" role="group" aria-label="Janela">
  <button class="sx-winctl-btn" title="Minimizar" aria-label="Minimizar" onclick={() => win.minimize()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" stroke="currentColor" /></svg>
  </button>
  <button
    class="sx-winctl-btn"
    title={maximized ? "Restaurar" : "Maximizar"}
    aria-label={maximized ? "Restaurar" : "Maximizar"}
    onclick={() => win.toggleMaximize()}
  >
    {#if maximized}
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor">
        <rect x="0.5" y="2.5" width="7" height="7" rx="1" /><path d="M2.5 2.5V1.5a1 1 0 0 1 1-1h5a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-1" />
      </svg>
    {:else}
      <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true" fill="none" stroke="currentColor"><rect x="0.5" y="0.5" width="9" height="9" rx="1.5" /></svg>
    {/if}
  </button>
  <button class="sx-winctl-btn is-close" title="Fechar" aria-label="Fechar" onclick={() => win.close()}>
    <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true"><path d="M0.5 0.5l9 9M9.5 0.5l-9 9" stroke="currentColor" /></svg>
  </button>
</div>
