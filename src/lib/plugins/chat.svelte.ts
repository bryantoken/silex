/*
 * Estado do chat, fora do componente: fechar o painel ou trocar entre janela e
 * painel lateral não interrompe uma resposta em andamento; ela continua em
 * segundo plano (no Rust) e aparece quando o painel volta.
 *
 * Só a conversa aberta e o texto das respostas em andamento ficam em memória.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Usage = { input: number; output: number; cost?: number | null };
export type Msg = {
  role: "user" | "assistant";
  content: string;
  ts: number;
  attachments?: string[];
  thinking?: string;
  steps?: string[];
  usage?: Usage;
  denied?: string[];
};
export type Summary = { id: string; title: string; provider: string; model: string; updated: number };
export type Chat = Summary & { messages: Msg[] };
export type Attachment = { path: string; name: string; kind: "image" | "pdf" | "sheet" | "doc" | "text"; size: number };
export type Permission = "read" | "edit" | "full";
type Live = { text: string; thinking: string; steps: string[] };

const LAST = "silex.chat.last";
const PERM = "silex.chat.perm";

function load<T>(key: string, fallback: T): T {
  try {
    return (localStorage.getItem(key) as T) ?? fallback;
  } catch {
    return fallback;
  }
}
function save(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {}
}

class ChatStore {
  list = $state<Summary[]>([]);
  chat = $state<Chat | null>(null);
  /** Respostas sendo geradas, por conversa */
  live = $state<Record<string, Live>>({});
  errors = $state<Record<string, string>>({});
  /** Anexos ainda não enviados */
  pending = $state<Attachment[]>([]);
  permission = $state<Permission>(load<Permission>(PERM, "read"));
  running = $derived(Object.keys(this.live).length);

  private listening: Promise<unknown> | null = null;

  private listen() {
    this.listening ??= listen<{ id: string; kind: "text" | "thinking" | "step"; text: string }>("chat-delta", ({ payload: p }) => {
      const l = this.live[p.id];
      if (!l) return;
      if (p.kind === "text") l.text += p.text;
      else if (p.kind === "thinking") l.thinking += p.text;
      else l.steps.push(p.text);
    });
    return this.listening;
  }

  async init() {
    await this.listen();
    await this.refresh();
    if (this.chat) return;
    const last = load(LAST, "");
    const pick = this.list.find((s) => s.id === last) ?? this.list[0];
    if (pick) await this.open(pick.id);
  }

  async refresh() {
    this.list = await invoke<Summary[]>("chat_list").catch(() => []);
  }

  async open(id: string) {
    this.chat = await invoke<Chat>("chat_get", { id });
    save(LAST, id);
  }

  async create(provider: string, model: string) {
    const c = await invoke<Chat>("chat_create", { provider, model });
    await this.refresh();
    this.chat = c;
    save(LAST, c.id);
    return c;
  }

  async rename(id: string, title: string) {
    if (!title.trim()) return;
    await invoke("chat_update", { id, title });
    if (this.chat?.id === id) this.chat.title = title.trim();
    await this.refresh();
  }

  async remove(id: string) {
    await invoke("chat_delete", { id });
    delete this.live[id];
    await this.refresh();
    if (this.chat?.id === id) {
      this.chat = null;
      if (this.list[0]) await this.open(this.list[0].id);
    }
  }

  async setProvider(provider: string, model: string) {
    if (!this.chat) return;
    const s = await invoke<Summary>("chat_update", { id: this.chat.id, provider, model });
    this.chat.provider = s.provider;
    this.chat.model = s.model;
    await this.refresh();
  }

  setPermission(p: Permission) {
    this.permission = p;
    save(PERM, p);
  }

  /** Arquivo colado/solto: bytes crus para o Rust, que grava na pasta da IA. */
  async attachBytes(file: File) {
    const generic = /^(image|imagem|blob)\.\w+$/i.test(file.name) || !file.name;
    const stamp = new Date().toISOString().slice(0, 19).replace(/[-:]/g, "").replace("T", "-");
    const ext = file.name.split(".").pop() || file.type.split("/")[1] || "bin";
    const name = generic ? `Colado ${stamp}.${ext}` : file.name;
    const a = await invoke<Attachment>("chat_attach", new Uint8Array(await file.arrayBuffer()), {
      headers: { "x-name": encodeURIComponent(name) },
    });
    this.pending.push(a);
  }

  async attachPaths(paths: string[]) {
    if (!paths.length) return;
    const list = await invoke<Attachment[]>("chat_attach_files", { paths });
    this.pending.push(...list);
  }

  async send(text: string, note: string | null, permission: Permission = this.permission, provider = "anthropic", model = "") {
    const attachments = this.pending.map((a) => a.path);
    if (!text.trim() && !attachments.length) return;
    const chat = this.chat ?? (await this.create(provider, model));
    const id = chat.id;
    if (this.live[id]) return;
    this.pending = [];
    delete this.errors[id];
    chat.messages.push({ role: "user", content: text.trim(), ts: Date.now(), attachments });
    this.live[id] = { text: "", thinking: "", steps: [] };
    await this.listen();
    try {
      const reply = await invoke<Msg>("chat_send", { id, text, note, attachments, permission });
      if (this.chat?.id === id) this.chat.messages.push(reply);
    } catch (e) {
      this.errors[id] = String(e);
    } finally {
      delete this.live[id];
      await this.refresh();
      const s = this.list.find((x) => x.id === id);
      if (s && this.chat?.id === id) this.chat.title = s.title;
    }
  }

  stop(id: string) {
    invoke("chat_stop", { id });
  }
}

export const chatStore = new ChatStore();
