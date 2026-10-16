/*
 * Plugins nativos do Silex: reescritas em Rust dos plugins do Obsidian.
 * Cada um declara suas configurações por esquema; a tela de Configurações
 * desenha os campos e grava em config.plugin_settings[id].
 */
import type { VaultConfig } from "../components/Settings.svelte";

export type Field =
  | { key: string; type: "text" | "textarea"; label: string; desc?: string; placeholder?: string }
  | { key: string; type: "number"; label: string; desc?: string; min: number; max: number; step?: number }
  | { key: string; type: "toggle"; label: string; desc?: string }
  | { key: string; type: "select"; label: string; desc?: string; options: [string, string][] }
  | { key: string; type: "list"; label: string; desc?: string; placeholder?: string }
  | { key: string; type: "pairs"; label: string; desc?: string; columns: [string, string]; fields: [string, string] }
  | { key: string; type: "secret"; label: string; desc?: string; secret: "typesafe" | "anthropic" | "openai" | "gemini" }
  | { key: string; type: "action"; label: string; desc?: string; button: string; action: string }
  | { key: string; type: "heading"; label: string; desc?: string };

export type CorePlugin = {
  id: string;
  name: string;
  /** Plugin do Obsidian que este substitui */
  origin: string;
  description: string;
  icon: string;
  defaults: Record<string, unknown>;
  fields: Field[];
};

const jevNote =
  "Ao perguntar, a busca e trechos das notas são enviados à TypeSafe (api.typesafe.ai). Pastas excluídas nunca são lidas nem enviadas. A chave fica no Cofre de Credenciais do Windows.";

export const CORE_PLUGINS: CorePlugin[] = [
  {
    id: "rustview",
    name: "rustview",
    origin: "Dataview",
    description: "Blocos ```rustview com scripts de sintaxe Rust sobre o índice de páginas (tarefas, propriedades, tags).",
    icon: "code",
    defaults: {},
    fields: [],
  },
  {
    id: "templater",
    name: "Templater",
    origin: "Templater",
    description: "Templates com <% tp.* %>: datas, arquivo, frontmatter, perguntas e inclusão de notas.",
    icon: "file",
    defaults: { templates_folder: "", trigger_on_file_creation: false, enable_folder_templates: true, folder_templates: [], auto_jump_to_cursor: true },
    fields: [
      { key: "templates_folder", type: "text", label: "Pasta de templates", desc: "Vazio usa “templates” se existir.", placeholder: "templates" },
      { key: "trigger_on_file_creation", type: "toggle", label: "Aplicar template ao criar nota", desc: "Usa o template da pasta (abaixo) em toda nota nova." },
      { key: "enable_folder_templates", type: "toggle", label: "Templates por pasta" },
      { key: "folder_templates", type: "pairs", label: "Template de cada pasta", desc: "A pasta mais específica vence. Pasta vazia = cofre inteiro.", columns: ["Pasta", "Template (.md)"], fields: ["folder", "template"] },
      { key: "auto_jump_to_cursor", type: "toggle", label: "Ir para tp.file.cursor() depois de inserir" },
    ],
  },
  {
    id: "numerals",
    name: "Numerals",
    origin: "Numerals",
    description: "Blocos ```math viram calculadora: variáveis, porcentagem, @prev, @total e moeda.",
    icon: "target",
    defaults: {},
    fields: [],
  },
  {
    id: "export",
    name: "Exportar",
    origin: "Enhancing Export",
    description: "Exporta a nota para PDF, HTML ou Word (.docx), sem Pandoc. Outros formatos se o Pandoc estiver instalado.",
    icon: "pdf",
    defaults: { openAfter: true },
    fields: [{ key: "openAfter", type: "toggle", label: "Abrir o arquivo depois de exportar" }],
  },
  {
    id: "canvas",
    name: "Canvas",
    origin: "Canvas (núcleo)",
    description: "Quadros .canvas (formato JSON Canvas): cartões, notas, imagens, links, grupos e conexões.",
    icon: "graph",
    defaults: {},
    fields: [],
  },
  {
    id: "excalidraw",
    name: "Excalidraw",
    origin: "Excalidraw",
    description: "Desenhos .excalidraw.md no mesmo formato do Obsidian, com o editor oficial do Excalidraw.",
    icon: "pencil",
    defaults: { folder: "Excalidraw", compress: true, filenamePrefix: "Drawing ", filenameDateTime: "YYYY-MM-DD HH.mm.ss" },
    fields: [
      { key: "folder", type: "text", label: "Pasta dos novos desenhos" },
      { key: "filenamePrefix", type: "text", label: "Prefixo do nome" },
      { key: "filenameDateTime", type: "text", label: "Data no nome (formato moment)" },
      { key: "compress", type: "toggle", label: "Salvar comprimido", desc: "Compatível com o plugin do Obsidian (compressed-json)." },
    ],
  },
  {
    id: "excel-paste",
    name: "Excel para tabela",
    origin: "Excel to Markdown Table",
    description: "Colar células copiadas do Excel gera uma tabela Markdown.",
    icon: "sheet",
    defaults: {},
    fields: [],
  },
  {
    id: "jev-home",
    name: "Jev Home",
    origin: "Jev Home",
    description: "Página inicial: descreva a nota com suas palavras e o Jev diz qual é, com a probabilidade de acerto.",
    icon: "search",
    defaults: {
      model: "jev-1.13.0",
      exclude: [],
      newTab: true,
      onStartup: false,
      openIn: "same",
      autoAsk: false,
      autoDelay: 900,
      broad: true,
      localLimit: 40,
      finalLimit: 40,
      perChunk: 4,
      excerptChars: 280,
      indexChars: 4000,
      maxSimilar: 8,
      minSimilar: 0.01,
      high: 0.8,
    },
    fields: [
      { key: "_conn", type: "heading", label: "Conexão", desc: jevNote },
      { key: "_key", type: "secret", label: "API key da TypeSafe", secret: "typesafe" },
      { key: "_test", type: "action", label: "Testar Jev", button: "Testar", action: "jev-test" },
      { key: "model", type: "text", label: "Modelo", desc: "Fixe a versão (ex.: jev-1.13.0) para as probabilidades não mudarem de escala; jev-latest acompanha a mais nova." },
      { key: "_page", type: "heading", label: "Página inicial" },
      { key: "newTab", type: "toggle", label: "Usar nas abas novas", desc: "Toda aba nova vazia abre a página inicial, como num navegador." },
      { key: "onStartup", type: "toggle", label: "Abrir ao iniciar o Silex" },
      { key: "openIn", type: "select", label: "Onde abrir a nota escolhida", desc: "Ctrl+Enter ou botão do meio abrem sempre em nova aba.", options: [["same", "Na mesma aba"], ["tab", "Em uma nova aba"]] },
      { key: "_search", type: "heading", label: "Busca" },
      { key: "autoAsk", type: "toggle", label: "Perguntar enquanto digito", desc: "Desligado, o Jev só responde com Enter (gasta menos chamadas)." },
      { key: "autoDelay", type: "number", label: "Pausa antes de perguntar (ms)", min: 300, max: 5000 },
      { key: "broad", type: "toggle", label: "Ler os títulos de todas as notas", desc: "Uma chamada a mais; acha a nota mesmo quando a busca não usa as palavras dela." },
      { key: "finalLimit", type: "number", label: "Notas comparadas pelo Jev", min: 5, max: 250 },
      { key: "localLimit", type: "number", label: "Candidatas da busca por palavras", min: 5, max: 250 },
      { key: "perChunk", type: "number", label: "Candidatas por bloco de títulos", min: 1, max: 20 },
      { key: "excerptChars", type: "number", label: "Trecho enviado de cada finalista (caracteres)", min: 0, max: 2000 },
      { key: "indexChars", type: "number", label: "Caracteres indexados por nota", desc: "Vale depois de reindexar.", min: 500, max: 50000 },
      { key: "maxSimilar", type: "number", label: "Máximo de parecidas", min: 0, max: 30 },
      { key: "minSimilar", type: "number", label: "Probabilidade mínima das parecidas", min: 0, max: 1, step: 0.01 },
      { key: "high", type: "number", label: "Alta confiança", desc: "A partir daqui a probabilidade fica verde.", min: 0, max: 1, step: 0.01 },
      { key: "_privacy", type: "heading", label: "Privacidade" },
      { key: "exclude", type: "list", label: "Pastas excluídas", desc: "Uma por linha, a partir da raiz do cofre. Use para credenciais, acessos, templates e anexos." },
      { key: "_reindex", type: "action", label: "Reindexar notas", desc: "O índice se atualiza sozinho ao editar.", button: "Reindexar", action: "jev-home-reindex" },
    ],
  },
  {
    id: "jev-organizer",
    name: "Jev Organizer",
    origin: "Jev Organizer",
    description: "Sugere pasta e propriedades (tags, link, autor, data, aliases) com o Jev. Você revisa e aplica; tudo vai para um log.",
    icon: "zap",
    defaults: {
      model: "jev-1.13.0",
      exclude: [],
      maxChars: 6000,
      tagLimit: 60,
      linkLimit: 10,
      authorLimit: 20,
      aliasLimit: 8,
      authors: [],
      keys: { tags: "tags", link: "link", author: "author", date: "date", aliases: "aliases" },
      features: { move: true, tags: true, link: true, author: true, date: true, aliases: true },
      minShow: 0.5,
      high: 0.8,
      maxPerScan: 100,
      concurrency: 3,
      onlyEmpty: false,
      logPath: "Jev/Log de ajustes.md",
    },
    fields: [
      { key: "_conn", type: "heading", label: "Conexão", desc: jevNote },
      { key: "_key", type: "secret", label: "API key da TypeSafe", secret: "typesafe" },
      { key: "_test", type: "action", label: "Testar Jev", button: "Testar", action: "jev-test" },
      { key: "model", type: "text", label: "Modelo" },
      { key: "_what", type: "heading", label: "O que analisar" },
      { key: "exclude", type: "list", label: "Pastas excluídas", desc: "Uma por linha." },
      { key: "onlyEmpty", type: "toggle", label: "Só notas sem propriedades" },
      { key: "maxPerScan", type: "number", label: "Máximo de notas por análise", min: 1, max: 5000 },
      { key: "concurrency", type: "number", label: "Chamadas em paralelo", min: 1, max: 8 },
      { key: "maxChars", type: "number", label: "Caracteres do texto enviados", min: 500, max: 60000 },
      { key: "_sugg", type: "heading", label: "Sugestões" },
      { key: "features.move", type: "toggle", label: "Mover para outra pasta" },
      { key: "features.tags", type: "toggle", label: "Tags" },
      { key: "features.link", type: "toggle", label: "Links" },
      { key: "features.author", type: "toggle", label: "Autores" },
      { key: "features.date", type: "toggle", label: "Data" },
      { key: "features.aliases", type: "toggle", label: "Aliases" },
      { key: "minShow", type: "number", label: "Probabilidade mínima para mostrar", min: 0, max: 1, step: 0.01 },
      { key: "high", type: "number", label: "Alta confiança", min: 0, max: 1, step: 0.01 },
      { key: "tagLimit", type: "number", label: "Tags avaliadas uma a uma", min: 5, max: 250 },
      { key: "linkLimit", type: "number", label: "Máximo de links por nota", min: 0, max: 40 },
      { key: "authorLimit", type: "number", label: "Máximo de autores avaliados", min: 0, max: 60 },
      { key: "aliasLimit", type: "number", label: "Máximo de aliases avaliados", min: 0, max: 20 },
      { key: "authors", type: "list", label: "Autores conhecidos", desc: "Um por linha." },
      { key: "_keys", type: "heading", label: "Nomes das propriedades" },
      { key: "keys.tags", type: "text", label: "Tags" },
      { key: "keys.link", type: "text", label: "Links" },
      { key: "keys.author", type: "text", label: "Autores" },
      { key: "keys.date", type: "text", label: "Data" },
      { key: "keys.aliases", type: "text", label: "Aliases" },
      { key: "_log", type: "heading", label: "Log" },
      { key: "logPath", type: "text", label: "Nota de log" },
      { key: "_clear", type: "action", label: "Limpar sugestões pendentes", button: "Limpar", action: "organizer-clear" },
    ],
  },
  {
    id: "mesa",
    name: "Mesa",
    origin: "Mesa",
    description: "Mesa de projetos com tarefas, calendário, sugestões de IA e painel de consumo. Dados em Markdown na pasta Mesa/.",
    icon: "clipboard",
    defaults: {
      root: "Mesa",
      provider: "openai",
      model: "",
      tone: "Direto, conciso e concreto. Preserve minha intenção.",
      daily: false,
      quick: true,
      capacity: 5,
      limit: 20,
      jev: true,
      jevModel: "jev-1.13.0",
      jevMin: 0.5,
      memory: "Mesa/Core/Memoria.md",
      interests: "Mesa/Core/Interesses.md",
      preferences: "Mesa/Core/Preferencias.md",
      priceIn: 0,
      priceOut: 0,
      jevPrice: 0,
      fx: 0,
    },
    fields: [
      { key: "_ai", type: "heading", label: "IA", desc: "Funciona sem IA. Ao usá-la, o texto da tarefa, o contexto dos projetos e as notas Core configuradas são enviados ao provedor." },
      { key: "root", type: "text", label: "Pasta da Mesa", desc: "Mantenha o mesmo caminho do Obsidian para preservar os vínculos." },
      { key: "provider", type: "select", label: "Provedor", options: [["anthropic", "Anthropic (Claude)"], ["openai", "OpenAI"]] },
      { key: "model", type: "text", label: "Modelo da API", desc: "Um ID disponível na sua conta (ex.: claude-sonnet-5-5)." },
      { key: "_k1", type: "secret", label: "API key da Anthropic", secret: "anthropic" },
      { key: "_k2", type: "secret", label: "API key da OpenAI", secret: "openai" },
      { key: "_t1", type: "action", label: "Testar conexão do provedor", button: "Testar", action: "llm-test" },
      { key: "_jev", type: "heading", label: "Jev (TypeSafe)" },
      { key: "jev", type: "toggle", label: "Usar Jev nas vitórias rápidas e prioridades", desc: "Se falhar, usa o provedor acima (só quando você pede)." },
      { key: "_k3", type: "secret", label: "API key da TypeSafe", secret: "typesafe" },
      { key: "_t2", type: "action", label: "Testar Jev", button: "Testar", action: "jev-test-mesa" },
      { key: "jevModel", type: "text", label: "Modelo do Jev" },
      { key: "jevMin", type: "number", label: "Chance mínima para vitória rápida", min: 0, max: 1, step: 0.05 },
      { key: "_ctx", type: "heading", label: "Contexto" },
      { key: "memory", type: "text", label: "Memória" },
      { key: "interests", type: "text", label: "Interesses" },
      { key: "preferences", type: "text", label: "Preferências" },
      { key: "tone", type: "textarea", label: "Tom do enriquecimento" },
      { key: "_use", type: "heading", label: "Consumo e rotina" },
      { key: "quick", type: "toggle", label: "Sugestões rápidas do dia (“Dá pra fazer hoje”)" },
      { key: "capacity", type: "number", label: "Capacidade diária (tarefas)", min: 1, max: 50 },
      { key: "daily", type: "toggle", label: "Sugerir hoje ao abrir (só Jev)" },
      { key: "limit", type: "number", label: "Limite diário de chamadas neste dispositivo", min: 1, max: 500 },
      { key: "priceIn", type: "number", label: "Preço de entrada (US$/milhão de tokens)", desc: "0 = referência automática pelo modelo.", min: 0, max: 1000, step: 0.01 },
      { key: "priceOut", type: "number", label: "Preço de saída (US$/milhão de tokens)", min: 0, max: 1000, step: 0.01 },
      { key: "jevPrice", type: "number", label: "Preço do Jev por pergunta (US$)", min: 0, max: 10, step: 0.0001 },
      { key: "fx", type: "number", label: "Cotação do dólar (R$)", desc: "0 = mostrar em dólar.", min: 0, max: 100, step: 0.01 },
    ],
  },
  {
    id: "chat",
    name: "Chat",
    origin: "Copilot / Smart Connections",
    description:
      "Chat de IA em janela flutuante ou painel lateral, com várias conversas. Provedores: Claude Code e Codex (CLIs instalados, rodando no cofre) e as APIs da Anthropic, OpenAI e Gemini.",
    icon: "chat",
    defaults: {
      provider: "anthropic",
      anthropicModel: "claude-sonnet-5-5",
      openaiModel: "gpt-5",
      geminiModel: "gemini-2.5-flash",
      claudeModel: "",
      codexModel: "",
      includeNote: true,
      showThinking: true,
      aiFolder: "ai",
      systemPrompt: "",
      claudePath: "",
      codexPath: "",
    },
    fields: [
      {
        key: "provider",
        type: "select",
        label: "Provedor padrão das novas conversas",
        options: [
          ["anthropic", "Anthropic (API)"],
          ["openai", "OpenAI (API)"],
          ["gemini", "Gemini (API)"],
          ["claude-code", "Claude Code (CLI)"],
          ["codex", "Codex (CLI)"],
        ],
      },
      { key: "includeNote", type: "toggle", label: "Enviar a nota aberta como contexto (padrão)" },
      { key: "showThinking", type: "toggle", label: "Mostrar o raciocínio do modelo", desc: "Quando o provedor envia (Claude, Gemini, Codex). Na API da Anthropic liga o pensamento estendido." },
      {
        key: "aiFolder",
        type: "text",
        label: "Pasta da IA no cofre",
        desc: "Anexos do chat (PDFs, imagens, planilhas…) ficam em <pasta>/anexos/AAAA-MM/. É também onde a IA guarda arquivos que criar.",
        placeholder: "ai",
      },
      { key: "systemPrompt", type: "textarea", label: "Instruções do assistente", desc: "Vazio = assistente de notas em português." },
      { key: "_api", type: "heading", label: "APIs", desc: "As chaves ficam no Cofre de Credenciais do Windows, nunca nas notas." },
      { key: "_ka", type: "secret", label: "API key da Anthropic", secret: "anthropic" },
      { key: "anthropicModel", type: "text", label: "Modelo da Anthropic", placeholder: "claude-sonnet-5-5" },
      { key: "_ko", type: "secret", label: "API key da OpenAI", secret: "openai" },
      { key: "openaiModel", type: "text", label: "Modelo da OpenAI", placeholder: "gpt-5" },
      { key: "_kg", type: "secret", label: "API key do Gemini", secret: "gemini" },
      { key: "geminiModel", type: "text", label: "Modelo do Gemini", placeholder: "gemini-2.5-flash" },
      {
        key: "_cli",
        type: "heading",
        label: "CLIs",
        desc: "Usam o login do próprio CLI e rodam com o cofre como pasta de trabalho (podem ler suas notas).",
      },
      { key: "claudeModel", type: "text", label: "Modelo do Claude Code", placeholder: "padrão do CLI" },
      { key: "claudePath", type: "text", label: "Caminho do claude", placeholder: "claude (no PATH)" },
      { key: "codexModel", type: "text", label: "Modelo do Codex", placeholder: "padrão do CLI" },
      { key: "codexPath", type: "text", label: "Caminho do codex", placeholder: "codex (no PATH)" },
    ],
  },
];

export const DEFAULT_CORE = ["rustview", "excel-paste", "templater", "numerals", "export", "canvas", "excalidraw", "chat"];

export function enabledCore(cfg: VaultConfig): Set<string> {
  return new Set(cfg.core_plugins ?? DEFAULT_CORE);
}

export function pluginSettings<T = Record<string, unknown>>(cfg: VaultConfig, id: string): T {
  const def = CORE_PLUGINS.find((p) => p.id === id)?.defaults ?? {};
  const saved = (cfg.plugin_settings?.[id] ?? {}) as Record<string, unknown>;
  const merged: Record<string, unknown> = { ...structuredClone(def), ...saved };
  // Objetos aninhados (keys, features) mesclam campo a campo
  for (const [k, v] of Object.entries(def)) {
    if (v && typeof v === "object" && !Array.isArray(v)) merged[k] = { ...v, ...((saved[k] as object) ?? {}) };
  }
  return merged as T;
}

export function getPath(obj: Record<string, unknown>, key: string): unknown {
  return key.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown> | undefined)?.[k], obj);
}

export function setPath(obj: Record<string, unknown>, key: string, value: unknown): Record<string, unknown> {
  const out = structuredClone(obj);
  const parts = key.split(".");
  let cur = out as Record<string, unknown>;
  for (const p of parts.slice(0, -1)) cur = (cur[p] ??= {}) as Record<string, unknown>;
  cur[parts[parts.length - 1]] = value;
  return out;
}
