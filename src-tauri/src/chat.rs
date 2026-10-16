//! Chat de IA nativo: conversas salvas em disco e cinco provedores.
//!
//! - APIs com streaming (SSE): Anthropic, OpenAI, Gemini. Chaves no cofre de
//!   credenciais do sistema (secrets.rs), nunca no frontend.
//! - CLIs locais: Claude Code (`claude -p`) e Codex (`codex exec`), rodando com o
//!   cofre como pasta de trabalho; a conversa continua pela sessão do próprio CLI.
//!   O nível de permissão (ler / editar / total) vira flag do CLI; pedidos negados
//!   voltam na resposta para o usuário liberar e continuar.
//!
//! Anexos (imagens, PDFs, planilhas, documentos, textos) são copiados para a pasta
//! da IA no cofre (`ai/anexos/AAAA-MM/`, configurável) e enviados no formato que
//! cada provedor entende. `[[Notas]]` citadas na mensagem entram como contexto.
//!
//! Cada conversa é um JSON em `<config do app>/chats/<id>.json`, com um índice
//! leve (`index.json`) para a lista. Nada fica em memória além das execuções em
//! andamento. Texto, raciocínio e passos chegam ao frontend por `chat-delta`.

use crate::{app_dir, config, ensure_inside, err, export::base64, secrets, vault_root, Res, VaultState};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, State};

const PROVIDERS: &[&str] = &["anthropic", "openai", "gemini", "claude-code", "codex"];
/// Histórico enviado às APIs: últimas mensagens até este limite de caracteres
const HISTORY_CHARS: usize = 120_000;
const NOTE_CHARS: usize = 40_000;
/// Total de texto de [[notas]] citadas numa mensagem
const REFS_CHARS: usize = 80_000;
/// Anexos binários só são reenviados nas mensagens mais recentes
const ATTACH_HISTORY: usize = 6;
const MAX_IMAGE: u64 = 8 * 1024 * 1024;
const MAX_PDF: u64 = 24 * 1024 * 1024;
const MAX_TEXT: usize = 100_000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Msg {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub ts: u64,
    /// Anexos (caminhos relativos ao cofre)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    /// Raciocínio exibido pelo modelo, quando o provedor envia
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thinking: String,
    /// Ferramentas usadas pelos CLIs ("Read · nota.md")
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    /// Ações que o CLI quis fazer e a permissão atual não deixou
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub denied: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Chat {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub model: String,
    /// Sessão do Claude Code / Codex para continuar a conversa
    #[serde(default)]
    pub session: Option<String>,
    pub created: u64,
    pub updated: u64,
    pub messages: Vec<Msg>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub provider: String,
    pub model: String,
    pub updated: u64,
}

impl From<&Chat> for Summary {
    fn from(c: &Chat) -> Self {
        Summary { id: c.id.clone(), title: c.title.clone(), provider: c.provider.clone(), model: c.model.clone(), updated: c.updated }
    }
}

/// Configurações do plugin (Configurações › Plugins › Chat).
#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Settings {
    system_prompt: String,
    claude_path: String,
    codex_path: String,
    ai_folder: String,
    show_thinking: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { system_prompt: String::new(), claude_path: String::new(), codex_path: String::new(), ai_folder: "ai".into(), show_thinking: true }
    }
}

static INDEX_LOCK: Mutex<()> = Mutex::new(());
/// Execuções em andamento: id da conversa → sinal de parada (+ processo do CLI)
static RUNS: LazyLock<Mutex<HashMap<String, Run>>> = LazyLock::new(Default::default);
static WIKILINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\[\]|#\n]+)(?:#[^\[\]|\n]*)?(?:\|[^\[\]\n]*)?\]\]").unwrap());

#[derive(Clone)]
struct Run {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn dir(app: &AppHandle) -> Res<PathBuf> {
    let d = app_dir(app)?.join("chats");
    fs::create_dir_all(&d).map_err(err)?;
    Ok(d)
}

fn valid_id(id: &str) -> Res<&str> {
    if !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_alphanumeric()) { Ok(id) } else { Err("Conversa inválida.".into()) }
}

/// Modelo/sessão vão para a URL ou para a linha de comando: só caracteres seguros.
fn safe_token(s: &str) -> bool {
    !s.is_empty() && s.len() <= 120 && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.:/@".contains(c))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Res<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(err)?;
    fs::rename(&tmp, path).map_err(err)
}

fn load(app: &AppHandle, id: &str) -> Res<Chat> {
    let bytes = fs::read(dir(app)?.join(format!("{}.json", valid_id(id)?))).map_err(|_| "Conversa não encontrada.".to_string())?;
    serde_json::from_slice(&bytes).map_err(err)
}

fn read_index(d: &Path) -> Vec<Summary> {
    fs::read(d.join("index.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// Grava a conversa e atualiza (ou remove, com `None`) a entrada dela no índice.
fn store(app: &AppHandle, id: &str, chat: Option<&Chat>) -> Res<()> {
    let d = dir(app)?;
    let _guard = INDEX_LOCK.lock().map_err(err)?;
    let file = d.join(format!("{}.json", valid_id(id)?));
    let mut index = read_index(&d);
    index.retain(|s| s.id != id);
    match chat {
        Some(c) => {
            write_atomic(&file, &serde_json::to_vec(c).map_err(err)?)?;
            index.push(c.into());
        }
        None => {
            let _ = fs::remove_file(&file);
        }
    }
    index.sort_by(|a, b| b.updated.cmp(&a.updated));
    write_atomic(&d.join("index.json"), &serde_json::to_vec(&index).map_err(err)?)
}

#[tauri::command]
pub fn chat_list(app: AppHandle) -> Res<Vec<Summary>> {
    Ok(read_index(&dir(&app)?))
}

#[tauri::command]
pub fn chat_get(app: AppHandle, id: String) -> Res<Chat> {
    load(&app, &id)
}

#[tauri::command]
pub fn chat_create(app: AppHandle, provider: String, model: String) -> Res<Chat> {
    if !PROVIDERS.contains(&provider.as_str()) {
        return Err("Provedor desconhecido.".into());
    }
    let t = now();
    let chat = Chat {
        id: format!("{t:x}{:04x}", (SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0) >> 8) & 0xffff),
        title: "Nova conversa".into(),
        provider,
        model: model.trim().to_string(),
        created: t,
        updated: t,
        ..Default::default()
    };
    store(&app, &chat.id, Some(&chat))?;
    Ok(chat)
}

/// Renomeia e/ou troca provedor e modelo (trocar de provedor reinicia a sessão do CLI).
#[tauri::command]
pub fn chat_update(app: AppHandle, id: String, title: Option<String>, provider: Option<String>, model: Option<String>) -> Res<Summary> {
    let mut chat = load(&app, &id)?;
    if let Some(t) = title.map(|t| t.trim().chars().take(120).collect::<String>()).filter(|t| !t.is_empty()) {
        chat.title = t;
    }
    if let Some(p) = provider {
        if !PROVIDERS.contains(&p.as_str()) {
            return Err("Provedor desconhecido.".into());
        }
        if p != chat.provider {
            chat.session = None;
        }
        chat.provider = p;
    }
    if let Some(m) = model {
        chat.model = m.trim().to_string();
    }
    store(&app, &id, Some(&chat))?;
    Ok((&chat).into())
}

#[tauri::command]
pub fn chat_delete(app: AppHandle, id: String) -> Res<()> {
    chat_stop(id.clone());
    store(&app, &id, None)
}

#[tauri::command]
pub fn chat_stop(id: String) {
    let run = RUNS.lock().ok().and_then(|r| r.get(&id).cloned());
    if let Some(run) = run {
        run.stop.store(true, Ordering::Relaxed);
        if let Some(child) = run.child.lock().ok().as_mut().and_then(|c| c.as_mut()) {
            let _ = child.kill();
        }
    }
}

/// Conversas com resposta sendo gerada (o painel pode ter sido fechado e reaberto).
#[tauri::command]
pub fn chat_running() -> Vec<String> {
    RUNS.lock().map(|r| r.keys().cloned().collect()).unwrap_or_default()
}

// ---------- Anexos ----------

#[derive(Serialize)]
pub struct Attachment {
    /// Caminho relativo ao cofre (com "/")
    path: String,
    name: String,
    kind: &'static str,
    size: u64,
}

fn kind_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("png" | "jpg" | "jpeg" | "gif" | "webp") => "image",
        Some("pdf") => "pdf",
        Some("xlsx" | "xlsm" | "xls" | "xlsb" | "ods") => "sheet",
        Some("docx") => "doc",
        _ => "text",
    }
}

fn mime_of(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn clean_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let s: String = base.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let s = s.trim().trim_start_matches('.').to_string();
    if s.is_empty() { "anexo".into() } else { s.chars().take(120).collect() }
}

/// `<cofre>/<pasta da IA>/anexos/AAAA-MM/`: tudo que entra pelo chat fica organizado por mês.
fn attach_dir(root: &Path, settings: &Settings) -> Res<PathBuf> {
    let folder = settings.ai_folder.trim().trim_matches(['/', '\\']);
    let folder = if folder.is_empty() || folder.contains("..") { "ai" } else { folder };
    let d = root.join(folder).join("anexos").join(chrono::Local::now().format("%Y-%m").to_string());
    fs::create_dir_all(&d).map_err(err)?;
    Ok(d)
}

fn save_attachment(root: &Path, settings: &Settings, name: &str, bytes: &[u8]) -> Res<Attachment> {
    let dir = attach_dir(root, settings)?;
    let name = clean_name(name);
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.clone(), String::new()),
    };
    let mut path = dir.join(&name);
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem} {n}{ext}"));
        n += 1;
    }
    fs::write(&path, bytes).map_err(err)?;
    Ok(Attachment {
        path: path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/"),
        name: path.file_name().unwrap_or_default().to_string_lossy().to_string(),
        kind: kind_of(&path),
        size: bytes.len() as u64,
    })
}

fn settings_of(state: &State<VaultState>) -> Res<(PathBuf, Settings)> {
    let root = vault_root(state)?;
    let settings = config::load_vault(&root).plugin("chat");
    Ok((root, settings))
}

/// Arquivo colado/solto no chat: bytes crus no corpo, nome no cabeçalho `x-name`.
#[tauri::command]
pub fn chat_attach(request: tauri::ipc::Request, state: State<VaultState>) -> Res<Attachment> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Conteúdo do anexo ausente.".into());
    };
    let name = request.headers().get("x-name").and_then(|v| v.to_str().ok()).map(crate::percent_decode).unwrap_or_else(|| "anexo".into());
    let (root, settings) = settings_of(&state)?;
    save_attachment(&root, &settings, &name, bytes)
}

/// Arquivos escolhidos no seletor: copiados para a pasta da IA (os que já estão nela ficam onde estão).
#[tauri::command]
pub fn chat_attach_files(paths: Vec<String>, state: State<VaultState>) -> Res<Vec<Attachment>> {
    let (root, settings) = settings_of(&state)?;
    let ai = root.join(settings.ai_folder.trim().trim_matches(['/', '\\']));
    paths
        .iter()
        .map(|p| {
            let src = Path::new(p);
            if src.starts_with(&ai) && src.is_file() {
                let size = fs::metadata(src).map(|m| m.len()).unwrap_or(0);
                return Ok(Attachment {
                    path: src.strip_prefix(&root).unwrap_or(src).to_string_lossy().replace('\\', "/"),
                    name: src.file_name().unwrap_or_default().to_string_lossy().to_string(),
                    kind: kind_of(src),
                    size,
                });
            }
            let bytes = fs::read(src).map_err(|e| format!("{p}: {e}"))?;
            save_attachment(&root, &settings, &src.file_name().unwrap_or_default().to_string_lossy(), &bytes)
        })
        .collect()
}

/// Conteúdo de um anexo, no formato mais útil para o modelo.
enum Part {
    Text(String),
    Image { mime: &'static str, data: String },
    Pdf { name: String, data: String },
}

fn sheet_text(path: &Path) -> Res<String> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(path).map_err(err)?;
    let mut out = String::new();
    for name in wb.sheet_names().to_vec().into_iter().take(5) {
        let Ok(range) = wb.worksheet_range(&name) else { continue };
        out.push_str(&format!("\n### Aba “{name}”\n\n"));
        for (i, row) in range.rows().take(300).enumerate() {
            let cells: Vec<String> = row.iter().take(40).map(|c| c.to_string().replace('|', "\\|").replace('\n', " ")).collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
            if i == 0 {
                out.push_str(&format!("|{}\n", " --- |".repeat(cells.len().max(1))));
            }
            if out.len() > MAX_TEXT {
                out.push_str("\n_(planilha truncada)_\n");
                return Ok(out);
            }
        }
    }
    Ok(out)
}

/// Texto de um .docx: lê o XML do documento e mantém só o texto e as quebras de parágrafo.
fn docx_text(path: &Path) -> Res<String> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(err)?).map_err(err)?;
    let mut xml = String::new();
    zip.by_name("word/document.xml").map_err(err)?.read_to_string(&mut xml).map_err(err)?;
    let xml = xml.replace("</w:p>", "\n").replace("<w:tab/>", "\t").replace("<w:br/>", "\n");
    let mut out = String::with_capacity(xml.len() / 4);
    let mut tag = false;
    for c in xml.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    Ok(out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").chars().take(MAX_TEXT).collect())
}

fn load_part(root: &Path, rel: &str) -> Res<(PathBuf, Part)> {
    let path = ensure_inside(root, &root.join(rel))?;
    let size = fs::metadata(&path).map_err(|e| format!("{rel}: {e}"))?.len();
    let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
    let part = match kind_of(&path) {
        "image" if size <= MAX_IMAGE => Part::Image { mime: mime_of(&path), data: base64(&fs::read(&path).map_err(err)?) },
        "pdf" if size <= MAX_PDF => Part::Pdf { name, data: base64(&fs::read(&path).map_err(err)?) },
        "image" | "pdf" => Part::Text(format!("[{name}: arquivo grande demais para enviar ({} MB)]", size / 1_048_576)),
        "sheet" => Part::Text(format!("Planilha anexada “{name}”:\n{}", sheet_text(&path)?)),
        "doc" => Part::Text(format!("Documento anexado “{name}”:\n\n{}", docx_text(&path)?)),
        _ => match fs::read(&path).map(String::from_utf8) {
            Ok(Ok(t)) => Part::Text(format!("Arquivo anexado “{name}”:\n```\n{}\n```", t.chars().take(MAX_TEXT).collect::<String>())),
            _ => Part::Text(format!("[{name}: formato binário não suportado]")),
        },
    };
    Ok((path, part))
}

// ---------- Contexto: [[notas]] citadas ----------

/// Resolve as [[notas]] citadas no texto como o editor: nome da nota ou caminho relativo.
fn referenced_notes(text: &str, root: &Path, state: &State<VaultState>) -> Vec<PathBuf> {
    let Ok(pages) = state.pages.read() else { return Vec::new() };
    let mut out: Vec<PathBuf> = Vec::new();
    for cap in WIKILINK.captures_iter(text) {
        let want = cap[1].trim().replace('\\', "/").trim_end_matches(".md").to_lowercase();
        let found = pages.values().find(|p| {
            let rel = Path::new(&p.path).strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/").to_lowercase()).unwrap_or_default();
            p.name.to_lowercase() == want || rel.trim_end_matches(".md") == want
        });
        if let Some(p) = found.map(|p| PathBuf::from(&p.path)).filter(|p| !out.contains(p)) {
            out.push(p);
        }
    }
    out
}

// ---------- Envio ----------

#[derive(Serialize, Clone)]
struct Delta<'a> {
    id: &'a str,
    /// "text", "thinking" ou "step"
    kind: &'a str,
    text: &'a str,
}

#[derive(Default)]
struct Reply {
    text: String,
    thinking: String,
    steps: Vec<String>,
    usage: Option<Usage>,
    denied: Vec<String>,
    session: Option<String>,
}

/// Envia a mensagem e devolve a resposta completa; texto, raciocínio e passos
/// chegam antes por `chat-delta`. `note`: nota aberta, anexada como contexto.
/// `permission` (CLIs): "read", "edit" ou "full".
#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    id: String,
    text: String,
    note: Option<String>,
    attachments: Option<Vec<String>>,
    permission: Option<String>,
    state: State<'_, VaultState>,
) -> Res<Msg> {
    let text = text.trim().to_string();
    let attachments = attachments.unwrap_or_default();
    if text.is_empty() && attachments.is_empty() {
        return Err("Mensagem vazia.".into());
    }
    let mut chat = load(&app, &id)?;
    let root = vault_root(&state).ok();
    let settings: Settings = root.as_deref().map(|r| config::load_vault(r).plugin("chat")).unwrap_or_default();
    let is_cli = matches!(chat.provider.as_str(), "claude-code" | "codex");

    let mut context = String::new();
    if let (Some(r), Some(n)) = (&root, note.as_deref().filter(|n| !n.is_empty())) {
        let path = ensure_inside(r, Path::new(n))?;
        let body: String = fs::read_to_string(&path).unwrap_or_default().chars().take(NOTE_CHARS).collect();
        let rel = path.strip_prefix(r).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        context.push_str(&format!("Nota aberta no Silex: \"{rel}\"\n\n<nota caminho=\"{rel}\">\n{body}\n</nota>\n"));
    }
    if let Some(r) = &root {
        let mut budget = REFS_CHARS;
        for path in referenced_notes(&text, r, &state) {
            let rel = path.strip_prefix(r).unwrap_or(&path).to_string_lossy().replace('\\', "/");
            if is_cli {
                // Os CLIs leem o arquivo sozinhos: basta apontar
                context.push_str(&format!("\nNota citada: \"{rel}\" (leia o arquivo para o conteúdo)."));
                continue;
            }
            let body: String = fs::read_to_string(&path).unwrap_or_default().chars().take(budget).collect();
            budget = budget.saturating_sub(body.len());
            context.push_str(&format!("\n<nota caminho=\"{rel}\">\n{body}\n</nota>\n"));
            if budget == 0 {
                break;
            }
        }
    }
    let mut system = if settings.system_prompt.trim().is_empty() {
        format!(
            "Você é o assistente do Silex, um app de notas em Markdown. Responda em português do Brasil, em Markdown, de forma direta. \
             Ao citar notas do cofre use [[Nome da nota]]. Arquivos que você criar para o usuário (exceto notas pedidas em outro lugar) \
             vão na pasta \"{}\" do cofre.",
            settings.ai_folder.trim().trim_matches(['/', '\\'])
        )
    } else {
        settings.system_prompt.trim().to_string()
    };
    if !context.is_empty() {
        system.push_str("\n\n");
        system.push_str(&context);
    }

    // A conversa ganha título com o início da primeira pergunta
    if chat.messages.is_empty() && chat.title == "Nova conversa" {
        let base = if text.is_empty() { attachments.first().map(|a| a.rsplit('/').next().unwrap_or(a).to_string()).unwrap_or_default() } else { text.clone() };
        let t: String = base.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(48).collect();
        chat.title = if base.chars().count() > 48 { format!("{t}…") } else { t };
    }
    chat.messages.push(Msg { role: "user".into(), content: text.clone(), ts: now(), attachments: attachments.clone(), ..Default::default() });
    chat.updated = now();
    store(&app, &id, Some(&chat))?;

    let run = Run { stop: Arc::new(AtomicBool::new(false)), child: Arc::new(Mutex::new(None)) };
    RUNS.lock().map_err(err)?.insert(id.clone(), run.clone());

    let emit = {
        let app = app.clone();
        let id = id.clone();
        move |kind: &str, delta: &str| {
            let _ = app.emit("chat-delta", Delta { id: &id, kind, text: delta });
        }
    };

    let result = if is_cli {
        let chat2 = chat.clone();
        let run2 = run.clone();
        let program = if chat.provider == "codex" { settings.codex_path.clone() } else { settings.claude_path.clone() };
        let perm = permission.unwrap_or_else(|| "read".into());
        let req = CliRequest { system, text, attachments, has_context: !context.is_empty(), permission: perm, thinking: settings.show_thinking };
        let cwd = root.clone();
        tauri::async_runtime::spawn_blocking(move || cli_run(&chat2, &req, cwd.as_deref(), &program, &run2, &emit)).await.map_err(err)?
    } else {
        api(&chat, &system, root.as_deref(), settings.show_thinking, &run, &emit).await
    };
    RUNS.lock().map_err(err)?.remove(&id);
    let stopped = run.stop.load(Ordering::Relaxed);

    // Recarrega (pode ter sido renomeada enquanto respondia) e grava a resposta
    let mut saved = load(&app, &id).unwrap_or(chat);
    let reply = match result {
        Ok(r) => r,
        Err(_) if stopped => Reply::default(),
        Err(e) => return Err(e),
    };
    let content = if stopped && reply.text.trim().is_empty() { "_(interrompido)_".to_string() } else { reply.text };
    if reply.session.is_some() {
        saved.session = reply.session;
    }
    let msg = Msg {
        role: "assistant".into(),
        content,
        ts: now(),
        thinking: reply.thinking,
        steps: reply.steps,
        usage: reply.usage,
        denied: reply.denied,
        ..Default::default()
    };
    saved.messages.push(msg.clone());
    saved.updated = now();
    store(&app, &id, Some(&saved))?;
    Ok(msg)
}

// ---------- APIs (streaming SSE) ----------

static CLIENT: LazyLock<reqwest::Client> =
    LazyLock::new(|| reqwest::Client::builder().user_agent("Silex/0.1").connect_timeout(Duration::from_secs(15)).build().expect("cliente HTTP"));

/// Histórico mais recente que cabe no limite, em ordem, começando pelo usuário.
fn history(chat: &Chat) -> Vec<&Msg> {
    let mut total = 0;
    let mut out: Vec<&Msg> = chat
        .messages
        .iter()
        .rev()
        .take_while(|m| {
            total += m.content.len();
            total <= HISTORY_CHARS
        })
        .collect();
    out.reverse();
    while out.first().is_some_and(|m| m.role != "user") {
        out.remove(0);
    }
    out
}

/// Conteúdo de uma mensagem do usuário com anexos, no formato de cada API.
fn user_content(provider: &str, m: &Msg, root: Option<&Path>, with_files: bool) -> Value {
    let mut parts: Vec<Value> = Vec::new();
    let mut extra = String::new();
    for rel in &m.attachments {
        let loaded = match root {
            Some(r) if with_files => load_part(r, rel).map(|(_, p)| p),
            _ => Ok(Part::Text(format!("[anexo anterior: {rel}]"))),
        };
        match loaded {
            Ok(Part::Text(t)) => {
                extra.push_str(&t);
                extra.push_str("\n\n");
            }
            Ok(Part::Image { mime, data }) => parts.push(match provider {
                "anthropic" => json!({ "type": "image", "source": { "type": "base64", "media_type": mime, "data": data } }),
                "openai" => json!({ "type": "image_url", "image_url": { "url": format!("data:{mime};base64,{data}") } }),
                _ => json!({ "inline_data": { "mime_type": mime, "data": data } }),
            }),
            Ok(Part::Pdf { name, data }) => parts.push(match provider {
                "anthropic" => json!({ "type": "document", "source": { "type": "base64", "media_type": "application/pdf", "data": data } }),
                "openai" => json!({ "type": "file", "file": { "filename": name, "file_data": format!("data:application/pdf;base64,{data}") } }),
                _ => json!({ "inline_data": { "mime_type": "application/pdf", "data": data } }),
            }),
            Err(e) => extra.push_str(&format!("[anexo indisponível: {e}]\n\n")),
        }
    }
    let text = format!("{extra}{}", m.content);
    let text = if text.trim().is_empty() { "(veja os anexos)".to_string() } else { text };
    if provider == "gemini" {
        parts.push(json!({ "text": text }));
        return Value::Array(parts);
    }
    if parts.is_empty() {
        return Value::String(text);
    }
    parts.push(json!({ "type": "text", "text": text }));
    Value::Array(parts)
}

async fn api(chat: &Chat, system: &str, root: Option<&Path>, thinking: bool, run: &Run, emit: &(impl Fn(&str, &str) + Sync)) -> Res<Reply> {
    let model = chat.model.trim();
    let provider = chat.provider.as_str();
    let (key_name, default_model) = match provider {
        "anthropic" => ("anthropic", "claude-sonnet-5-5"),
        "openai" => ("openai", "gpt-5"),
        _ => ("gemini", "gemini-2.5-flash"),
    };
    let model = if model.is_empty() { default_model } else { model };
    if !safe_token(model) {
        return Err("Nome de modelo inválido.".into());
    }
    let key = secrets::get(key_name).ok_or_else(|| format!("Configure a API key ({key_name}) em Configurações › Plugins › Chat."))?;
    let msgs = history(chat);
    let recent = msgs.len().saturating_sub(ATTACH_HISTORY);
    let content = |i: usize, m: &Msg| if m.role == "user" { user_content(provider, m, root, i >= recent) } else { Value::String(m.content.clone()) };
    let req = match provider {
        "anthropic" => {
            let mut body = json!({
                "model": model, "max_tokens": 16000, "stream": true, "system": system,
                "messages": msgs.iter().enumerate().map(|(i, m)| json!({ "role": m.role, "content": content(i, m) })).collect::<Vec<_>>(),
            });
            if thinking {
                body["thinking"] = json!({ "type": "enabled", "budget_tokens": 6000 });
            }
            CLIENT.post("https://api.anthropic.com/v1/messages").header("x-api-key", &key).header("anthropic-version", "2023-06-01").json(&body)
        }
        "openai" => {
            let mut all = vec![json!({ "role": "system", "content": system })];
            all.extend(msgs.iter().enumerate().map(|(i, m)| json!({ "role": m.role, "content": content(i, m) })));
            CLIENT
                .post("https://api.openai.com/v1/chat/completions")
                .bearer_auth(&key)
                .json(&json!({ "model": model, "stream": true, "stream_options": { "include_usage": true }, "messages": all }))
        }
        _ => {
            let mut body = json!({
                "systemInstruction": { "parts": [{ "text": system }] },
                "contents": msgs.iter().enumerate().map(|(i, m)| json!({
                    "role": if m.role == "assistant" { "model" } else { "user" },
                    "parts": if m.role == "user" { content(i, m) } else { json!([{ "text": m.content }]) },
                })).collect::<Vec<_>>(),
            });
            if thinking {
                body["generationConfig"] = json!({ "thinkingConfig": { "includeThoughts": true } });
            }
            CLIENT
                .post(format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:streamGenerateContent?alt=sse"))
                .header("x-goog-api-key", &key)
                .json(&body)
        }
    };
    let mut res = req.send().await.map_err(|e| format!("Falha de conexão: {e}"))?;
    let status = res.status().as_u16();
    if status >= 400 {
        let body: Value = res.json().await.unwrap_or(Value::Null);
        let detail = body
            .pointer("/error/message")
            .or_else(|| body.pointer("/0/error/message"))
            .and_then(Value::as_str)
            .map(|d| format!(": {d}"))
            .unwrap_or_default();
        return Err(format!("{} retornou HTTP {status}{detail}", provider_name(provider)));
    }

    let mut buf = String::new();
    let mut out = Reply::default();
    let mut usage = Usage::default();
    while let Some(chunk) = res.chunk().await.map_err(|e| format!("Conexão interrompida: {e}"))? {
        if run.stop.load(Ordering::Relaxed) {
            break;
        }
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(nl) = buf.find('\n') {
            let line = buf[..nl].trim_end_matches('\r').to_string();
            buf.drain(..=nl);
            let Some(data) = line.strip_prefix("data:").map(str::trim) else { continue };
            let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
            if let Some(e) = v.pointer("/error/message").and_then(Value::as_str) {
                return Err(e.to_string());
            }
            let n = |p: &str| v.pointer(p).and_then(Value::as_u64);
            match provider {
                "anthropic" => {
                    match v.pointer("/delta/type").and_then(Value::as_str) {
                        Some("text_delta") => push(&mut out.text, "text", v.pointer("/delta/text"), emit),
                        Some("thinking_delta") => push(&mut out.thinking, "thinking", v.pointer("/delta/thinking"), emit),
                        _ => {}
                    }
                    usage.input = n("/message/usage/input_tokens").map(|i| i + n("/message/usage/cache_read_input_tokens").unwrap_or(0)).unwrap_or(usage.input);
                    usage.output = n("/usage/output_tokens").unwrap_or(usage.output);
                }
                "openai" => {
                    push(&mut out.text, "text", v.pointer("/choices/0/delta/content"), emit);
                    usage.input = n("/usage/prompt_tokens").unwrap_or(usage.input);
                    usage.output = n("/usage/completion_tokens").unwrap_or(usage.output);
                }
                _ => {
                    for p in v.pointer("/candidates/0/content/parts").and_then(Value::as_array).into_iter().flatten() {
                        let thought = p.get("thought").and_then(Value::as_bool) == Some(true);
                        if thought {
                            push(&mut out.thinking, "thinking", p.get("text"), emit);
                        } else {
                            push(&mut out.text, "text", p.get("text"), emit);
                        }
                    }
                    usage.input = n("/usageMetadata/promptTokenCount").unwrap_or(usage.input);
                    usage.output = n("/usageMetadata/candidatesTokenCount").map(|c| c + n("/usageMetadata/thoughtsTokenCount").unwrap_or(0)).unwrap_or(usage.output);
                }
            }
        }
    }
    if usage.input + usage.output > 0 {
        out.usage = Some(usage);
    }
    Ok(out)
}

fn push(acc: &mut String, kind: &str, v: Option<&Value>, emit: &impl Fn(&str, &str)) {
    if let Some(t) = v.and_then(Value::as_str).filter(|t| !t.is_empty()) {
        acc.push_str(t);
        emit(kind, t);
    }
}

fn provider_name(p: &str) -> &'static str {
    match p {
        "anthropic" => "Anthropic",
        "openai" => "OpenAI",
        "gemini" => "Gemini",
        "claude-code" => "Claude Code",
        _ => "Codex",
    }
}

// ---------- CLIs (Claude Code, Codex) ----------

struct CliRequest {
    system: String,
    text: String,
    attachments: Vec<String>,
    has_context: bool,
    permission: String,
    thinking: bool,
}

fn command(program: &str, default: &str) -> Command {
    let custom = program.trim();
    if !custom.is_empty() {
        return Command::new(custom);
    }
    // No Windows: o .exe do PATH direto (instaladores nativos); senão via cmd, que
    // também acha os .cmd do npm. Os argumentos são fixos: o texto do usuário vai
    // por stdin, nunca na linha de comando.
    #[cfg(windows)]
    {
        let exe = format!("{default}.exe");
        if let Some(path) = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(&exe)).find(|p| p.is_file())) {
            return Command::new(path);
        }
        let mut c = Command::new("cmd");
        c.args(["/D", "/C", default]);
        c
    }
    #[cfg(not(windows))]
    Command::new(default)
}

/// Resumo de uma ferramenta usada pelo CLI: "Read · notas/x.md".
fn step_label(name: &str, input: &Value) -> String {
    let arg = ["file_path", "path", "pattern", "command", "url", "query", "description"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .unwrap_or("");
    let arg: String = arg.lines().next().unwrap_or("").chars().take(80).collect();
    if arg.is_empty() { name.to_string() } else { format!("{name} · {arg}") }
}

fn cli_run(chat: &Chat, req: &CliRequest, cwd: Option<&Path>, program: &str, run: &Run, emit: &impl Fn(&str, &str)) -> Res<Reply> {
    let codex = chat.provider == "codex";
    let name = provider_name(&chat.provider);
    let mut cmd = command(program, if codex { "codex" } else { "claude" });
    let session = chat.session.as_deref().filter(|s| safe_token(s));
    let model = Some(chat.model.trim()).filter(|m| safe_token(m));
    let images: Vec<PathBuf> = cwd
        .map(|r| req.attachments.iter().map(|a| r.join(a)).filter(|p| kind_of(p) == "image" && p.is_file()).collect())
        .unwrap_or_default();
    if codex {
        cmd.args(["exec", "--json", "--skip-git-repo-check"]);
        match req.permission.as_str() {
            "full" => {
                cmd.arg("--dangerously-bypass-approvals-and-sandbox");
            }
            "edit" => {
                cmd.args(["-s", "workspace-write"]);
            }
            _ => {
                cmd.args(["-s", "read-only"]);
            }
        }
        if let Some(m) = model {
            cmd.args(["-m", m]);
        }
        for img in &images {
            cmd.arg("-i").arg(img);
        }
        if let Some(s) = session {
            cmd.args(["resume", s]);
        }
        cmd.arg("-");
    } else {
        cmd.args(["-p", "--output-format", "stream-json", "--verbose", "--include-partial-messages"]);
        cmd.args(["--permission-mode", match req.permission.as_str() {
            "full" => "bypassPermissions",
            "edit" => "acceptEdits",
            _ => "default",
        }]);
        if let Some(m) = model {
            cmd.args(["--model", m]);
        }
        if let Some(s) = session {
            cmd.args(["--resume", s]);
        }
    }
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: sem janela de console
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Não foi possível iniciar o {name} ({e}). Ele está instalado e no PATH?"))?;

    // Anexos: os CLIs leem os arquivos do cofre sozinhos (imagens do Codex vão por -i)
    let mut prompt = String::new();
    if session.is_none() || req.has_context {
        prompt.push_str(&req.system);
        prompt.push_str("\n\n---\n\n");
    }
    if !req.attachments.is_empty() {
        prompt.push_str("Arquivos anexados (caminhos relativos ao cofre, leia-os):\n");
        for a in &req.attachments {
            prompt.push_str(&format!("- {a}\n"));
        }
        prompt.push('\n');
    }
    prompt.push_str(&req.text);
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(prompt.as_bytes());
    }
    let stdout = child.stdout.take().ok_or("stdout")?;
    let mut stderr = child.stderr.take();
    *run.child.lock().map_err(err)? = Some(child);
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(e) = stderr.as_mut() {
            let _ = e.take(8_000).read_to_string(&mut s);
        }
        s
    });

    let mut out = Reply::default();
    let mut final_text: Option<String> = None;
    let mut error: Option<String> = None;
    let mut streamed = false;
    let mut usage = Usage::default();
    let step = |out: &mut Reply, s: String| {
        emit("step", &s);
        out.steps.push(s);
    };
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        if run.stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        let sid = v.get("session_id").or_else(|| v.get("thread_id")).or_else(|| v.pointer("/msg/session_id")).and_then(Value::as_str);
        if let Some(s) = sid {
            out.session = Some(s.to_string());
        }
        let main = v.get("parent_tool_use_id").is_none_or(Value::is_null);
        if codex {
            match kind {
                "item.completed" => {
                    let item = v.get("item").unwrap_or(&Value::Null);
                    match item.get("type").and_then(Value::as_str).unwrap_or("") {
                        "agent_message" | "assistant_message" => {
                            let sep = if out.text.is_empty() { "" } else { "\n\n" };
                            if let Some(t) = item.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                                let t = format!("{sep}{t}");
                                out.text.push_str(&t);
                                emit("text", &t);
                            }
                        }
                        "reasoning" if req.thinking => {
                            if let Some(t) = item.get("text").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                                let t = format!("{t}\n\n");
                                out.thinking.push_str(&t);
                                emit("thinking", &t);
                            }
                        }
                        "command_execution" => step(&mut out, step_label("Comando", &json!({ "command": item.get("command") .and_then(Value::as_str).unwrap_or("") }))),
                        "file_change" => {
                            for c in item.get("changes").and_then(Value::as_array).into_iter().flatten() {
                                step(&mut out, step_label("Arquivo", c));
                            }
                        }
                        "web_search" => step(&mut out, step_label("Busca", item)),
                        _ => {}
                    }
                }
                "turn.completed" => {
                    let n = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_u64).unwrap_or(0);
                    usage.input += n("input_tokens");
                    usage.output += n("output_tokens") + n("reasoning_output_tokens");
                }
                "error" | "turn.failed" => {
                    error = v.get("message").or_else(|| v.pointer("/error/message")).and_then(Value::as_str).map(String::from);
                }
                _ => match v.pointer("/msg/type").and_then(Value::as_str) {
                    // Formato antigo do codex exec --json
                    Some("agent_message_delta") => {
                        streamed = true;
                        push(&mut out.text, "text", v.pointer("/msg/delta"), emit);
                    }
                    Some("agent_message") if !streamed => push(&mut out.text, "text", v.pointer("/msg/message"), emit),
                    Some("error") => error = v.pointer("/msg/message").and_then(Value::as_str).map(String::from),
                    _ => {}
                },
            }
        } else {
            match kind {
                // Só a conversa principal (subagentes têm parent_tool_use_id)
                "stream_event" if main => match v.pointer("/event/delta/type").and_then(Value::as_str) {
                    Some("text_delta") => {
                        streamed = true;
                        push(&mut out.text, "text", v.pointer("/event/delta/text"), emit);
                    }
                    Some("thinking_delta") if req.thinking => push(&mut out.thinking, "thinking", v.pointer("/event/delta/thinking"), emit),
                    _ => {
                        // Novo bloco de texto depois de uma ferramenta: separa do anterior
                        if v.pointer("/event/type").and_then(Value::as_str) == Some("content_block_start")
                            && v.pointer("/event/content_block/type").and_then(Value::as_str) == Some("text")
                            && !out.text.is_empty()
                        {
                            out.text.push_str("\n\n");
                            emit("text", "\n\n");
                        }
                    }
                },
                "assistant" if main => {
                    for p in v.pointer("/message/content").and_then(Value::as_array).into_iter().flatten() {
                        match p.get("type").and_then(Value::as_str) {
                            Some("tool_use") => step(&mut out, step_label(p.get("name").and_then(Value::as_str).unwrap_or("Ferramenta"), p.get("input").unwrap_or(&Value::Null))),
                            Some("text") if !streamed => push(&mut out.text, "text", p.get("text"), emit),
                            _ => {}
                        }
                    }
                }
                "result" => {
                    let n = |k: &str| v.pointer(&format!("/usage/{k}")).and_then(Value::as_u64).unwrap_or(0);
                    usage.input = n("input_tokens") + n("cache_read_input_tokens") + n("cache_creation_input_tokens");
                    usage.output = n("output_tokens");
                    usage.cost = v.get("total_cost_usd").and_then(Value::as_f64);
                    for d in v.get("permission_denials").and_then(Value::as_array).into_iter().flatten() {
                        let label = step_label(d.get("tool_name").and_then(Value::as_str).unwrap_or("Ferramenta"), d.get("tool_input").unwrap_or(&Value::Null));
                        if !out.denied.contains(&label) {
                            out.denied.push(label);
                        }
                    }
                    if v.get("is_error").and_then(Value::as_bool) == Some(true) {
                        error = v.get("result").and_then(Value::as_str).map(String::from).or(Some("Claude Code terminou com erro.".into()));
                    } else {
                        final_text = v.get("result").and_then(Value::as_str).map(String::from);
                    }
                }
                _ => {}
            }
        }
    }
    let status = run.child.lock().map_err(err)?.take().and_then(|mut c| c.wait().ok());
    let stderr = err_reader.join().unwrap_or_default();
    if usage.input + usage.output > 0 || usage.cost.is_some() {
        out.usage = Some(usage);
    }
    if run.stop.load(Ordering::Relaxed) {
        return Ok(out);
    }
    if let Some(e) = error {
        return Err(format!("{name}: {e}"));
    }
    if out.text.trim().is_empty() {
        if let Some(t) = final_text.filter(|t| !t.trim().is_empty()) {
            emit("text", &t);
            out.text = t;
            return Ok(out);
        }
        let detail = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
        let code = status.and_then(|s| s.code()).map(|c| format!(" (código {c})")).unwrap_or_default();
        return Err(if detail.is_empty() { format!("{name} não respondeu{code}.") } else { format!("{name}: {detail}") });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_cli(provider: &str, permission: &str, text: &str, dir: &Path) -> (Reply, usize) {
        let chat = Chat { id: "t".into(), provider: provider.into(), ..Default::default() };
        let run = Run { stop: Arc::new(AtomicBool::new(false)), child: Arc::new(Mutex::new(None)) };
        let deltas = Mutex::new(0usize);
        let req = CliRequest {
            system: "Responda de forma curtíssima.".into(),
            text: text.into(),
            attachments: Vec::new(),
            has_context: false,
            permission: permission.into(),
            thinking: true,
        };
        let reply = cli_run(&chat, &req, Some(dir), "", &run, &|_, _| *deltas.lock().unwrap() += 1).expect("CLI respondeu");
        (reply, deltas.into_inner().unwrap())
    }

    /// Usa os CLIs instalados de verdade: `cargo test -- --ignored chat`
    #[test]
    #[ignore]
    fn chat_claude_code_e_codex() {
        let dir = std::env::temp_dir();
        for p in ["claude-code", "codex"] {
            let (r, deltas) = run_cli(p, "read", "Diga: silex", &dir);
            println!("{p}: {:?} sessão={:?} deltas={deltas} uso={:?}", r.text, r.session, r.usage);
            assert!(r.text.to_lowercase().contains("silex") && r.session.is_some() && deltas > 0, "{p}");
        }
    }

    /// Escrita negada em modo leitura vira pedido de permissão; em "edit" o arquivo é criado.
    #[test]
    #[ignore]
    fn claude_code_pede_e_usa_permissao() {
        let dir = std::env::temp_dir().join(format!("silex-chat-perm-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let ask = "Crie o arquivo AAA.md nesta pasta com o texto oi. Use a ferramenta Write.";
        let (r, _) = run_cli("claude-code", "read", ask, &dir);
        println!("negado: {:?}", r.denied);
        assert!(!r.denied.is_empty() && !dir.join("AAA.md").exists());
        let (r, _) = run_cli("claude-code", "edit", ask, &dir);
        println!("passos: {:?}", r.steps);
        assert!(dir.join("AAA.md").exists());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn historico_respeita_limite_e_comeca_no_usuario() {
        let m = |role: &str, n: usize| Msg { role: role.into(), content: "x".repeat(n), ..Default::default() };
        let chat = Chat { messages: vec![m("user", 10), m("assistant", HISTORY_CHARS), m("user", 5), m("assistant", 5)], ..Default::default() };
        let h = history(&chat);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].role, "user");
    }

    #[test]
    fn nomes_de_anexo_seguros() {
        assert_eq!(clean_name("..\\..\\x:y?.png"), "x_y_.png");
        assert_eq!(clean_name(""), "anexo");
    }
}
