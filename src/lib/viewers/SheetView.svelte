<script lang="ts">
  // Planilhas (xlsx, xls, ods, csv...) lidas em Rust com calamine/csv.
  import { invoke } from "@tauri-apps/api/core";

  type Sheet = { name: string; rows: string[][]; truncated: boolean };
  let { path }: { path: string } = $props();

  let sheets = $state<Sheet[]>([]);
  let active = $state(0);
  let error = $state<string | null>(null);
  let loading = $state(true);

  $effect(() => {
    const p = path;
    loading = true;
    error = null;
    invoke<Sheet[]>("read_spreadsheet", { path: p })
      .then((s) => {
        sheets = s;
        active = 0;
      })
      .catch((e) => (error = String(e)))
      .finally(() => (loading = false));
  });

  let sheet = $derived(sheets[active]);
  let cols = $derived(sheet ? Math.max(0, ...sheet.rows.map((r) => r.length)) : 0);
  const colName = (i: number) => {
    let s = "";
    for (i++; i > 0; i = Math.floor((i - 1) / 26)) s = String.fromCharCode(65 + ((i - 1) % 26)) + s;
    return s;
  };
</script>

<div class="sx-sheet">
  {#if loading}
    <div class="sx-viewer-loading">Carregando…</div>
  {:else if error}
    <div class="sx-viewer-error">Não foi possível abrir a planilha: {error}</div>
  {:else if sheet}
    <div class="sx-sheet-grid">
      <table>
        <thead>
          <tr>
            <th class="corner"></th>
            {#each { length: cols } as _, c}<th>{colName(c)}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each sheet.rows as row, r}
            <tr>
              <th>{r + 1}</th>
              {#each { length: cols } as _, c}
                {@const v = row[c] ?? ""}
                <td class:is-num={v !== "" && !isNaN(Number(v))}>{v}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
      {#if sheet.truncated}<div class="sx-sheet-note">Mostrando as primeiras 5.000 linhas.</div>{/if}
    </div>
    {#if sheets.length > 1}
      <div class="sx-sheet-tabs">
        {#each sheets as s, i}
          <button class:is-on={i === active} onclick={() => (active = i)}>{s.name}</button>
        {/each}
      </div>
    {/if}
  {:else}
    <div class="sx-viewer-error">Planilha vazia.</div>
  {/if}
</div>
