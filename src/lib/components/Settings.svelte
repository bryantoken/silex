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
  export type ImportReport = {
    themes: string[];
    snippets: string[];
    active_theme: string | null;
    enabled_snippets: string[];
    plugins: { id: string; name: string; enabled_in_obsidian: boolean; status: string; note: string }[];
  };
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
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
    onimport: () => Promise<ImportReport | null>;
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
    onimport,
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

  type Section = "appearance" | "editor" | "plugins" | "import" | "hotkeys" | "about";
  let section = $state<Section>("appearance");
  let report = $state<ImportReport | null>(null);
  let importing = $state(false);

  const SECTIONS: { id: Section; label: string; icon: string }[] = [
    { id: "appearance", label: "Aparência", icon: "palette" },
    { id: "editor", label: "Editor", icon: "edit" },
    { id: "plugins", label: "Plugins", icon: "zap" },
    { id: "import", label: "Importar do Obsidian", icon: "repeat" },
    { id: "hotkeys", label: "Atalhos", icon: "command" },
    { id: "about", label: "Sobre", icon: "info" },
  ];

  const HUES = [256, 212, 172, 146, 28, 340];

  const STATUS: Record<string, { label: string; cls: string }> = {
    native: { label: "Nativo em Rust", cls: "ok" },
    plugin: { label: "Plugin WASM", cls: "ok" },
    replaced: { label: "Substituído", cls: "info" },
    unneeded: { label: "Desnecessário", cls: "muted" },
    pending: { label: "Não portado", cls: "warn" },
  };

  const HOTKEYS: [string, string][] = [
    ["Paleta de comandos", "Ctrl P"],
    ["Abrir arquivo rapidamente", "Ctrl O"],
    ["Buscar no cofre", "Ctrl Shift F"],
    ["Nova nota", "Ctrl N"],
    ["Visualização em grafo", "Ctrl G"],
    ["Configurações", "Ctrl ,"],
    ["Alternar tarefa", "Ctrl Enter"],
    ["Renomear nota", "F2"],
    ["Nova aba / fechar aba", "Ctrl T / Ctrl W"],
    ["Próxima / anterior aba", "Ctrl Tab / Ctrl Shift Tab"],
    ["Voltar / avançar", "Alt ← / Alt →"],
    ["Alternar barra lateral", "Ctrl B"],
    ["Buscar na nota", "Ctrl F"],
    ["Salvar agora", "Ctrl S"],
  ];

  function set<K extends keyof VaultConfig>(key: K, value: VaultConfig[K]) {
    onconfig({ ...config, [key]: value });
  }

  function toggleIn(list: string[], item: string, on: boolean) {
    return on ? [...new Set([...list, item])] : list.filter((x) => x !== item);
  }

  async function runImport() {
    importing = true;
    report = await onimport();
    importing = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

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
          <div><div class="name">Tema</div><div class="desc">Temas ficam em <code>.silex/themes</code>. Temas importados do Obsidian aplicam as cores e as fontes deles.</div></div>
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
      {:else if section === "import"}
        <h2>Importar do Obsidian</h2>
        <p class="lead">
          Copia temas e snippets de <code>.obsidian</code> para <code>.silex</code>, traz as configurações de cada plugin
          (Jev, Mesa, Templater, Excalidraw, Exportar) para o equivalente nativo e ativa os que estavam ativos. Chaves de API
          não são copiadas: o Obsidian as guarda à parte, então informe-as em Plugins.
        </p>
        <button class="btn-primary" disabled={importing} onclick={runImport}>
          {importing ? "Importando…" : "Importar temas e analisar plugins"}
        </button>
        {#if report}
          <div class="sx-import-summary">
            {report.themes.length} tema{report.themes.length === 1 ? "" : "s"} e {report.snippets.length} snippet{report.snippets.length === 1 ? "" : "s"} importados.
            {#if report.active_theme}O tema ativo no Obsidian era <strong>{report.active_theme}</strong>.{/if}
          </div>
          {#if report.active_theme && appearance.themes.includes(report.active_theme)}
            <button class="btn-ghost" onclick={() => onconfig({ ...config, theme: report!.active_theme!, snippets: [...new Set([...config.snippets, ...report!.enabled_snippets.filter((s) => appearance.snippets.includes(s))])] })}>
              Usar o tema {report.active_theme}
            </button>
          {/if}
          <div class="sx-import-list">
            {#each report.plugins as p (p.id)}
              <div class="sx-import-row">
                <div>
                  <div class="name">{p.name}{#if p.enabled_in_obsidian}<span class="ver">ativo no Obsidian</span>{/if}</div>
                  <div class="desc">{p.note}</div>
                </div>
                <span class="sx-badge {STATUS[p.status]?.cls}">{STATUS[p.status]?.label ?? p.status}</span>
              </div>
            {/each}
          </div>
        {/if}
      {:else if section === "hotkeys"}
        <h2>Atalhos</h2>
        <div class="sx-hotkeys">
          {#each HOTKEYS as [name, key]}
            <div class="sx-hotkey"><span>{name}</span><kbd>{key}</kbd></div>
          {/each}
        </div>
      {:else}
        <div class="sx-about">
          <Logo size={72} />
          <h2>Silex</h2>
          <p>Notas Markdown locais, com núcleo em Rust: índice e busca (tantivy), rustview e plugins WebAssembly.</p>
          <p class="desc">Versão 0.1.0</p>
        </div>
      {/if}
    </section>
  </div>
</div>
