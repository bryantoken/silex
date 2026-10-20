<script lang="ts" module>
  export type VaultConfig = {
    theme: string;
    snippets: string[];
    plugins: string[];
    accent_hue: number;
    color_scheme: "dark" | "light" | "system";
    font_size: number;
    line_width: number;
    readable_line_width: boolean;
    core_plugins: string[] | null;
    plugin_settings: Record<string, unknown>;
    /** Atalhos alterados (id do comando → combinação; "" = sem atalho) */
    hotkeys?: Record<string, string>;
  };
  export type PluginInfo = {
    id: string;
    name: string;
    version: string;
    description: string;
    author: string;
    commands: { id: string; name: string }[];
    code_blocks: string[];
    file: string;
    enabled: boolean;
    error: string | null;
  };
  export type Appearance = { themes: string[]; snippets: string[]; dir: string };
  /** Comando que aceita atalho: id, nome e atalho padrão (formato "Ctrl+Shift+F") */
  export type HotkeyCommand = { id: string; title: string; hotkey?: string };
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import { comboOf, showCombo, validCombo } from "../hotkeys";
  import Logo from "./Logo.svelte";
  import PluginSettings from "../plugins/PluginSettings.svelte";
  import { CORE_PLUGINS, enabledCore, pluginSettings } from "../plugins/registry";

  type Props = {
    config: VaultConfig;
    appearance: Appearance;
    plugins: PluginInfo[];
    vaultName: string;
    onconfig: (c: VaultConfig) => void;
    oninstall: () => void;
    onopenplugins: () => void;
    onopenthemes: () => void;
    /** Todos os comandos da paleta (para editar atalhos) */
    commands: HotkeyCommand[];
    version: string;
    onvaults: () => void;
    onclose: () => void;
    /** Ações dos botões nas configurações dos plugins (testar chave, reindexar...) */
    onaction: (plugin: string, action: string) => Promise<string | void>;
    /** Plugin cujas configurações abrem expandidas */
    focusPlugin?: string | null;
  };
  let {
    config,
    appearance,
    plugins,
    vaultName,
    onconfig,
    oninstall,
    onopenplugins,
    onopenthemes,
    commands,
    version,
    onvaults,
    onclose,
    onaction,
    focusPlugin = null,
  }: Props = $props();

  let openPlugin = $state<string | null>(null);
  $effect(() => {
    if (focusPlugin) {
      section = "plugins";
      openPlugin = focusPlugin;
    }
  });
  let core = $derived(enabledCore(config));

  function toggleCore(id: string, on: boolean) {
    const next = new Set(core);
    if (on) next.add(id);
    else next.delete(id);
    onconfig({ ...config, core_plugins: [...next] });
  }

  function setPluginSettings(id: string, v: Record<string, unknown>) {
    onconfig({ ...config, plugin_settings: { ...config.plugin_settings, [id]: v } });
  }

  type Section = "appearance" | "editor" | "plugins" | "hotkeys" | "about";
  let section = $state<Section>("appearance");

  const SECTIONS: { id: Section; label: string; icon: string }[] = [
    { id: "appearance", label: "Aparência", icon: "palette" },
    { id: "editor", label: "Editor", icon: "edit" },
    { id: "plugins", label: "Plugins", icon: "zap" },
    { id: "hotkeys", label: "Atalhos", icon: "command" },
    { id: "about", label: "Sobre", icon: "info" },
  ];

  const HUES = [256, 212, 172, 146, 28, 340];

  function set<K extends keyof VaultConfig>(key: K, value: VaultConfig[K]) {
    onconfig({ ...config, [key]: value });
  }

  // ----- Atalhos -----
  /** Teclas tratadas dentro do editor (fixas) */
  const EDITOR_KEYS: [string, string][] = [
    ["Alternar tarefa (alternativo)", "Ctrl Enter"],
    ["Buscar / substituir na nota", "Ctrl F / Ctrl H"],
    ["Desfazer / refazer", "Ctrl Z / Ctrl Y"],
    ["Selecionar bloco de código / tudo", "Ctrl A"],
  ];
  let overrides = $derived((config.hotkeys ?? {}) as Record<string, string>);
  const bound = (c: HotkeyCommand) => (c.id in overrides ? overrides[c.id] : (c.hotkey ?? ""));
  let hkQuery = $state("");
  let recording = $state<string | null>(null);
  let hkNote = $state("");
  let hkList = $derived(
    commands.filter((c) => (c.title + " " + showCombo(bound(c))).toLowerCase().includes(hkQuery.toLowerCase())).sort((a, b) => Number(!bound(b)) - Number(!bound(a))),
  );

  function setHotkey(id: string, combo: string | null) {
    const next = { ...overrides };
    const cmd = commands.find((c) => c.id === id);
    if (combo === null || combo === (cmd?.hotkey ?? "")) delete next[id];
    else next[id] = combo;
    hkNote = "";
    if (combo) {
      // Quem já usava a combinação fica sem atalho
      for (const c of commands) {
        if (c.id !== id && (c.id in next ? next[c.id] : (c.hotkey ?? "")) === combo) {
          next[c.id] = "";
          hkNote = `${showCombo(combo)} saiu de “${c.title}”.`;
        }
      }
    }
    onconfig({ ...config, hotkeys: next });
  }

  function onRecordKey(e: KeyboardEvent) {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === "Escape") return void (recording = null);
    if ((e.key === "Backspace" || e.key === "Delete") && !e.ctrlKey && !e.altKey) {
      setHotkey(recording, "");
      recording = null;
      return;
    }
    const combo = comboOf(e);
    if (!combo) return;
    if (!validCombo(combo)) {
      hkNote = "Use Ctrl ou Alt junto com a tecla (ou uma tecla F1–F12).";
      return;
    }
    setHotkey(recording, combo);
    recording = null;
  }

  function toggleIn(list: string[], item: string, on: boolean) {
    return on ? [...new Set([...list, item])] : list.filter((x) => x !== item);
  }

</script>

<svelte:window onkeydowncapture={onRecordKey} onkeydown={(e) => e.key === "Escape" && !recording && onclose()} />

<div class="sx-overlay" role="presentation" onmousedown={(e) => e.target === e.currentTarget && onclose()}>
  <div class="sx-settings" role="dialog" aria-label="Configurações">
    <nav class="sx-settings-nav">
      <div class="sx-settings-vault">{vaultName}</div>
      {#each SECTIONS as s}
        <button class:is-on={section === s.id} onclick={() => (section = s.id)}>
          <Icon name={s.icon} size={15} />
          {s.label}
        </button>
      {/each}
      <div class="sx-settings-spacer"></div>
      <button onclick={onvaults}><Icon name="vaults" size={15} /> Trocar de cofre</button>
    </nav>

    <section class="sx-settings-body">
      <button class="icon-btn sx-settings-close" title="Fechar (Esc)" onclick={onclose}><Icon name="x" size={16} /></button>

      {#if section === "appearance"}
        <h2>Aparência</h2>
        <div class="sx-setting">
          <div><div class="name">Modo</div><div class="desc">Escuro, claro ou o mesmo do sistema.</div></div>
          <div class="sx-segmented">
            {#each [["dark", "Escuro"], ["light", "Claro"], ["system", "Sistema"]] as [v, label]}
              <button class:is-on={(config.color_scheme ?? "dark") === v} onclick={() => set("color_scheme", v as VaultConfig["color_scheme"])}>{label}</button>
            {/each}
          </div>
        </div>
        <div class="sx-setting">
          <div><div class="name">Tema</div><div class="desc">Temas ficam em <code>.silex/themes</code>. Temas CSS do Obsidian colocados lá também aplicam as cores e as fontes deles.</div></div>
          <select value={config.theme} onchange={(e) => set("theme", e.currentTarget.value)}>
            <option value="">Silex (padrão)</option>
            {#each appearance.themes as t}<option value={t}>{t}</option>{/each}
          </select>
        </div>
        <div class="sx-setting">
          <div><div class="name">Cor de destaque</div><div class="desc">Tinge a seleção de texto (o resto da interface é monocromático).</div></div>
          <div class="sx-hues">
            {#each HUES as h}
              <button
                class="sx-hue"
                class:is-on={config.accent_hue === h}
                style:background="hsl({h} 88% 70%)"
                title="Matiz {h}"
                aria-label="Matiz {h}"
                onclick={() => set("accent_hue", h)}
              ></button>
            {/each}
            <input type="range" min="0" max="359" value={config.accent_hue} oninput={(e) => set("accent_hue", +e.currentTarget.value)} />
          </div>
        </div>
        <div class="sx-setting-group">
          <div class="name">Snippets CSS</div>
          {#if appearance.snippets.length}
            {#each appearance.snippets as s}
              <label class="sx-check">
                <input
                  type="checkbox"
                  checked={config.snippets.includes(s)}
                  onchange={(e) => set("snippets", toggleIn(config.snippets, s, e.currentTarget.checked))}
                />
                {s}
              </label>
            {/each}
          {:else}
            <div class="desc">Nenhum snippet em <code>.silex/snippets</code>.</div>
          {/if}
          <button class="sx-link-btn" onclick={onopenthemes}>Abrir pasta de temas e snippets</button>
        </div>
      {:else if section === "editor"}
        <h2>Editor</h2>
        <div class="sx-setting">
          <div><div class="name">Tamanho da fonte</div><div class="desc">{config.font_size}px</div></div>
          <input type="range" min="12" max="24" value={config.font_size} oninput={(e) => set("font_size", +e.currentTarget.value)} />
        </div>
        <div class="sx-setting">
          <div><div class="name">Largura de leitura</div><div class="desc">Limita a largura do texto para facilitar a leitura.</div></div>
          <input
            type="checkbox"
            class="sx-switch"
            checked={config.readable_line_width}
            onchange={(e) => set("readable_line_width", e.currentTarget.checked)}
          />
        </div>
        {#if config.readable_line_width}
          <div class="sx-setting">
            <div><div class="name">Largura máxima</div><div class="desc">{config.line_width}px</div></div>
            <input type="range" min="520" max="1200" step="20" value={config.line_width} oninput={(e) => set("line_width", +e.currentTarget.value)} />
          </div>
        {/if}
      {:else if section === "plugins"}
        <h2>Plugins</h2>
        <p class="lead">
          Os plugins do Obsidian foram reescritos como plugins nativos do Silex, com o núcleo em <strong>Rust</strong>. Você também
          pode criar os seus em Rust (WebAssembly) com o SDK em <code>plugins/silex-plugin-sdk</code>.
        </p>
        <div class="sx-ps-group">Nativos</div>
        {#each CORE_PLUGINS as p (p.id)}
          {@const on = core.has(p.id)}
          <div class="sx-plugin" class:is-open={openPlugin === p.id}>
            <div class="sx-plugin-row">
              <span class="sx-plugin-icon"><Icon name={p.icon} size={16} /></span>
              <div class="sx-plugin-text">
                <div class="name">{p.name} <span class="ver">substitui {p.origin}</span></div>
                <div class="desc">{p.description}</div>
              </div>
              {#if p.fields.length}
                <button class="btn-ghost sm" onclick={() => (openPlugin = openPlugin === p.id ? null : p.id)}>
                  <Icon name="settings" size={14} /> {openPlugin === p.id ? "Fechar" : "Configurar"}
                </button>
              {/if}
              <input type="checkbox" class="sx-switch" checked={on} title={on ? "Desativar" : "Ativar"} onchange={(e) => toggleCore(p.id, e.currentTarget.checked)} />
            </div>
            {#if openPlugin === p.id}
              <PluginSettings
                plugin={p}
                value={pluginSettings(config, p.id)}
                onchange={(v) => setPluginSettings(p.id, v)}
                onaction={(a) => onaction(p.id, a)}
              />
            {/if}
          </div>
        {/each}

        <div class="sx-ps-group">WebAssembly (Rust)</div>
        <div class="sx-actions">
          <button class="btn-primary" onclick={oninstall}><Icon name="plus" size={14} stroke={2} /> Instalar plugin (.wasm)</button>
          <button class="btn-ghost" onclick={onopenplugins}><Icon name="folder" size={14} /> Abrir pasta de plugins</button>
        </div>
        {#each plugins as p (p.file)}
          <div class="sx-plugin" class:has-error={!!p.error}>
            <div class="sx-plugin-row">
              <span class="sx-plugin-icon"><Icon name="zap" size={16} /></span>
              <div class="sx-plugin-text">
                <div class="name">{p.name} <span class="ver">{p.version}</span></div>
                {#if p.error}
                  <div class="desc err">{p.error}</div>
                {:else}
                  <div class="desc">{p.description}</div>
                  <div class="meta">
                    {#if p.author}por {p.author} · {/if}{p.commands.length} comando{p.commands.length === 1 ? "" : "s"}
                    {#if p.code_blocks.length} · blocos {p.code_blocks.map((b) => "```" + b).join(", ")}{/if}
                  </div>
                {/if}
              </div>
              {#if !p.error}
                <input
                  type="checkbox"
                  class="sx-switch"
                  checked={p.enabled}
                  title={p.enabled ? "Desativar" : "Ativar"}
                  onchange={(e) => set("plugins", toggleIn(config.plugins, p.id, e.currentTarget.checked))}
                />
              {/if}
            </div>
          </div>
        {:else}
          <div class="sx-empty-note">Nenhum plugin WebAssembly instalado neste cofre.</div>
        {/each}
      {:else if section === "hotkeys"}
        <h2>Atalhos</h2>
        <p class="lead">Clique num atalho e pressione a nova combinação. <kbd>Backspace</kbd> remove, <kbd>Esc</kbd> cancela. Todo comando da paleta pode ganhar um atalho.</p>
        <div class="sx-hk-tools">
          <input class="sx-input" type="search" placeholder="Filtrar comandos ou teclas" bind:value={hkQuery} />
          {#if Object.keys(overrides).length}
            <button class="btn-ghost sm" onclick={() => ((hkNote = ""), onconfig({ ...config, hotkeys: {} }))}>Restaurar todos</button>
          {/if}
        </div>
        {#if hkNote}<div class="sx-hk-note">{hkNote}</div>{/if}
        <div class="sx-hotkeys">
          {#each hkList as c (c.id)}
            {@const key = bound(c)}
            <div class="sx-hotkey" class:is-custom={c.id in overrides}>
              <span>{c.title}</span>
              <div class="sx-hotkey-keys">
                {#if c.id in overrides}
                  <button class="icon-btn sm" title="Restaurar padrão{c.hotkey ? ` (${showCombo(c.hotkey)})` : ''}" onclick={() => setHotkey(c.id, null)}><Icon name="undo" size={13} /></button>
                {/if}
                <button class="sx-hotkey-btn" class:is-recording={recording === c.id} class:is-empty={!key} onclick={() => ((hkNote = ""), (recording = recording === c.id ? null : c.id))}>
                  {recording === c.id ? "Pressione as teclas…" : key ? showCombo(key) : "Adicionar"}
                </button>
              </div>
            </div>
          {/each}
        </div>
        <div class="sx-ps-group">No editor (fixos)</div>
        <div class="sx-hotkeys">
          {#each EDITOR_KEYS as [name, key]}
            <div class="sx-hotkey"><span>{name}</span><kbd>{key}</kbd></div>
          {/each}
        </div>
      {:else}
        <div class="sx-about">
          <Logo size={72} />
          <h2>Silex</h2>
          <p>Notas Markdown locais, com núcleo em Rust: índice e busca (tantivy), rustview e plugins WebAssembly.</p>
          <p class="desc">Versão {version}</p>
        </div>
      {/if}
    </section>
  </div>
</div>
