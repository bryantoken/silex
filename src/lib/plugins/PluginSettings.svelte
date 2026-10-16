<script lang="ts">
  // Desenha as configurações de um plugin nativo a partir do esquema em registry.ts.
  import { invoke } from "@tauri-apps/api/core";
  import { getPath, setPath, type CorePlugin } from "./registry";

  type Props = {
    plugin: CorePlugin;
    value: Record<string, unknown>;
    onchange: (v: Record<string, unknown>) => void;
    onaction: (action: string) => Promise<string | void>;
  };
  let { plugin, value, onchange, onaction }: Props = $props();

  let secrets = $state<Record<string, number>>({});
  let drafts = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);
  let messages = $state<Record<string, string>>({});

  $effect(() => {
    for (const f of plugin.fields) {
      if (f.type === "secret") invoke<number>("secret_len", { name: f.secret }).then((n) => (secrets[f.secret] = n));
    }
  });

  const set = (key: string, v: unknown) => onchange(setPath(value, key, v));

  async function saveSecret(name: string) {
    secrets[name] = await invoke<number>("set_secret", { name, value: drafts[name] ?? "" });
    drafts[name] = "";
  }

  async function run(key: string, action: string) {
    busy = key;
    try {
      messages[key] = (await onaction(action)) ?? "Feito.";
    } catch (e) {
      messages[key] = String(e);
    } finally {
      busy = null;
    }
  }
</script>

<div class="sx-plugin-settings">
  {#each plugin.fields as f (f.key)}
    {#if f.type === "heading"}
      <div class="sx-ps-heading">{f.label}</div>
      {#if f.desc}<div class="sx-ps-note">{f.desc}</div>{/if}
    {:else}
      <div class="sx-setting" class:is-block={f.type === "list" || f.type === "pairs" || f.type === "textarea"}>
        <div class="sx-ps-text">
          <div class="name">{f.label}</div>
          {#if f.type === "secret"}
            <div class="desc">
              {secrets[f.secret] ? `Chave salva no Cofre de Credenciais (${secrets[f.secret]} caracteres).` : "Nenhuma chave salva."}
            </div>
          {:else if f.desc}
            <div class="desc">{f.desc}</div>
          {/if}
          {#if messages[f.key]}<div class="desc sx-ps-msg">{messages[f.key]}</div>{/if}
        </div>
        {#if f.type === "text"}
          <input class="sx-input" value={String(getPath(value, f.key) ?? "")} placeholder={f.placeholder} onchange={(e) => set(f.key, e.currentTarget.value.trim())} />
        {:else if f.type === "textarea"}
          <textarea class="sx-input" rows="4" value={String(getPath(value, f.key) ?? "")} onchange={(e) => set(f.key, e.currentTarget.value)}></textarea>
        {:else if f.type === "number"}
          <input
            class="sx-input sm"
            type="number"
            min={f.min}
            max={f.max}
            step={f.step ?? 1}
            value={Number(getPath(value, f.key) ?? 0)}
            onchange={(e) => {
              const n = Number(e.currentTarget.value.replace(",", "."));
              if (Number.isFinite(n) && n >= f.min && n <= f.max) set(f.key, n);
              else e.currentTarget.value = String(getPath(value, f.key));
            }}
          />
        {:else if f.type === "toggle"}
          <input type="checkbox" class="sx-switch" checked={!!getPath(value, f.key)} onchange={(e) => set(f.key, e.currentTarget.checked)} />
        {:else if f.type === "select"}
          <select value={String(getPath(value, f.key) ?? "")} onchange={(e) => set(f.key, e.currentTarget.value)}>
            {#each f.options as [v, label]}<option value={v}>{label}</option>{/each}
          </select>
        {:else if f.type === "list"}
          <textarea
            class="sx-input"
            rows="4"
            placeholder={f.placeholder}
            value={((getPath(value, f.key) as string[]) ?? []).join("\n")}
            onchange={(e) => set(f.key, e.currentTarget.value.split("\n").map((x) => x.trim()).filter(Boolean))}
          ></textarea>
        {:else if f.type === "pairs"}
          {@const rows = ((getPath(value, f.key) as Record<string, string>[]) ?? []).filter((r) => r)}
          <div class="sx-pairs">
            <div class="sx-pairs-head"><span>{f.columns[0]}</span><span>{f.columns[1]}</span><span></span></div>
            {#each rows as row, i}
              <div class="sx-pairs-row">
                <input class="sx-input" value={row[f.fields[0]] ?? ""} onchange={(e) => set(f.key, rows.map((r, j) => (j === i ? { ...r, [f.fields[0]]: e.currentTarget.value.trim() } : r)))} />
                <input class="sx-input" value={row[f.fields[1]] ?? ""} onchange={(e) => set(f.key, rows.map((r, j) => (j === i ? { ...r, [f.fields[1]]: e.currentTarget.value.trim() } : r)))} />
                <button class="icon-btn sm" title="Remover" onclick={() => set(f.key, rows.filter((_, j) => j !== i))}>×</button>
              </div>
            {/each}
            <button class="sx-link-btn" onclick={() => set(f.key, [...rows, { [f.fields[0]]: "", [f.fields[1]]: "" }])}>+ Adicionar</button>
          </div>
        {:else if f.type === "secret"}
          <div class="sx-secret">
            <input class="sx-input" type="password" placeholder={secrets[f.secret] ? "••••••••" : "Cole a chave"} bind:value={drafts[f.secret]} autocomplete="off" />
            <button class="btn-ghost" disabled={drafts[f.secret] === undefined} onclick={() => saveSecret(f.secret)}>
              {drafts[f.secret] === "" && secrets[f.secret] ? "Remover" : "Salvar"}
            </button>
          </div>
        {:else if f.type === "action"}
          <button class="btn-ghost" disabled={busy === f.key} onclick={() => run(f.key, f.action)}>{busy === f.key ? "…" : f.button}</button>
        {/if}
      </div>
    {/if}
  {/each}
</div>
