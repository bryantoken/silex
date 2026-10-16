<script lang="ts">
  /*
   * Painel de exportação no próprio app: nome do arquivo + pasta de destino
   * (lembrada entre exportações). O diálogo de pastas do sistema só abre em
   * "Alterar…"; o fluxo normal não passa por janela modal nativa.
   */
  import { invoke } from "@tauri-apps/api/core";
  import { documentDir } from "@tauri-apps/api/path";
  import { tick, untrack } from "svelte";
  import { openFile as openDialog } from "../dialogs";

  type Props = {
    note: string;
    format: string;
    label: string;
    defaultName: string;
    openAfter: boolean;
    ondone: (out: string, open: boolean) => void;
    onerror: (e: unknown) => void;
    onclose: () => void;
  };
  let { note, format, label, defaultName, openAfter, ondone, onerror, onclose }: Props = $props();

  const KEY = "silex.exportDir";
  let name = $state(untrack(() => defaultName));
  let folder = $state("");
  let open = $state(untrack(() => openAfter));
  let busy = $state(false);
  let exists = $state(false);
  let input = $state<HTMLInputElement>();

  $effect(() => {
    let saved = "";
    try {
      saved = localStorage.getItem(KEY) ?? "";
    } catch {}
    if (saved) folder = saved;
    else documentDir().then((d) => (folder ||= d)).catch(() => {});
    tick().then(() => {
      input?.focus();
      input?.select();
    });
  });

  async function pick() {
    const dir = await openDialog({ title: "Pasta de destino", directory: true });
    if (typeof dir === "string") {
      folder = dir;
      exists = false;
    }
  }

  async function run(overwrite = false) {
    if (!name.trim() || !folder || busy) return;
    busy = true;
    try {
      const out = await invoke<string>("export_write", { path: note, format, folder, name: name.trim(), overwrite });
      try {
        localStorage.setItem(KEY, folder);
      } catch {}
      onclose();
      ondone(out, open);
    } catch (e) {
      if (String(e) === "EXISTE") exists = true;
      else onerror(e);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="sx-overlay"
  role="presentation"
  onmousedown={(e) => e.target === e.currentTarget && onclose()}
  onkeydown={(e) => e.key === "Escape" && onclose()}
>
  <form
    class="sx-prompt sx-export"
    onsubmit={(e) => {
      e.preventDefault();
      run(exists);
    }}
  >
    <div class="sx-prompt-title">Exportar como {label}</div>
    <label class="mesa-field is-wide">
      <span>Nome do arquivo</span>
      <div class="sx-export-name">
        <input class="sx-input" bind:this={input} bind:value={name} oninput={() => (exists = false)} spellcheck="false" />
        <span class="sx-export-ext">.{format}</span>
      </div>
    </label>
    <div class="mesa-field is-wide">
      <span>Pasta</span>
      <div class="sx-export-folder">
        <span class="sx-export-path" title={folder}>{folder || "…"}</span>
        <button type="button" class="btn-ghost" onclick={pick}>Alterar…</button>
      </div>
    </div>
    <label class="sx-export-check">
      <input type="checkbox" bind:checked={open} />
      Abrir o arquivo depois de exportar
    </label>
    {#if exists}
      <p class="sx-export-warn">Já existe “{name.trim()}.{format}” nesta pasta.</p>
    {/if}
    <div class="mesa-form-actions">
      <button type="button" class="btn-ghost" onclick={onclose}>Cancelar</button>
      <button type="submit" class="btn-primary" class:is-danger={exists} disabled={busy || !name.trim() || !folder}>
        {busy ? "Exportando…" : exists ? "Substituir" : "Exportar"}
      </button>
    </div>
  </form>
</div>
