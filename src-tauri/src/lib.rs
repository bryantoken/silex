mod chat;
mod config;
mod dialogs;
mod export;
mod jev_home;
mod mesa;
mod native;
mod net;
mod numerals;
mod organizer;
mod pages;
mod plugins;
mod rustview;
mod search;
mod secrets;
mod templater;

use config::{SavedVault, VaultConfig};
use pages::Page;
use plugins::{PluginHost, PluginInfo};
use search::{SearchHit, SearchIndex};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};

pub struct VaultState {
    /// Raiz do cofre aberto. Todas as operações de arquivo são restritas a ela.
    root: Mutex<Option<PathBuf>>,
    search: SearchIndex,
    /// Metadados de cada nota (caminho absoluto → página), base do rustview
    pages: RwLock<HashMap<String, Page>>,
    plugins: RwLock<PluginHost>,
    /// Índice do Jev Home (montado sob demanda)
    jev: RwLock<Option<jev_home::Index>>,
    jev_version: AtomicU64,
    organizer_cancel: AtomicBool,
    organizer_running: AtomicBool,
}

#[derive(Serialize, Debug, Clone)]
pub struct Note {
    title: String,
    relative_path: String,
    absolute_path: String,
    links: Vec<String>,
    tags: Vec<String>,
}

impl From<(&Page, &Path)> for Note {
    fn from((p, root): (&Page, &Path)) -> Self {
        Note {
            title: p.name.clone(),
            relative_path: Path::new(&p.path)
                .strip_prefix(root)
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or_else(|_| p.path.clone()),
            absolute_path: p.path.clone(),
            links: p.links.clone(),
            tags: p.tags.clone(),
        }
    }
}

/// Nó da árvore do explorador de arquivos.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum TreeNode {
    Folder { name: String, path: String, children: Vec<TreeNode> },
    File { name: String, path: String, extension: String },
}

#[derive(Serialize)]
pub struct OpenedVault {
    tree: TreeNode,
    config: VaultConfig,
}

pub(crate) type Res<T> = Result<T, String>;

pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub(crate) fn vault_root(state: &State<VaultState>) -> Res<PathBuf> {
    state.root.lock().map_err(err)?.clone().ok_or_else(|| "Nenhum cofre aberto.".to_string())
}

/// Garante que `target` está dentro do cofre (bloqueia `..` e caminhos externos).
pub(crate) fn ensure_inside(root: &Path, target: &Path) -> Res<PathBuf> {
    let parent = target.parent().ok_or("Caminho inválido.")?;
    let canon_parent = parent.canonicalize().map_err(err)?;
    if !canon_parent.starts_with(root) {
        return Err("Caminho fora do cofre.".to_string());
    }
    Ok(canon_parent.join(target.file_name().ok_or("Caminho inválido.")?))
}

pub(crate) fn validate_name(name: &str) -> Res<&str> {
    let n = name.trim();
    if n.is_empty() || n.starts_with('.') || n.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) {
        return Err(format!("Nome inválido: {name}"));
    }
    Ok(n)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name().and_then(|s| s.to_str()).is_some_and(|n| n.starts_with('.'))
}

pub(crate) fn is_md(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

fn build_tree(dir: &Path) -> Vec<TreeNode> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut folders = Vec::new();
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if is_hidden(&path) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let path_str = path.to_string_lossy().to_string();
        if path.is_dir() {
            folders.push(TreeNode::Folder { name, path: path_str, children: build_tree(&path) });
        } else {
            let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            files.push(TreeNode::File { name, path: path_str, extension });
        }
    }
    // Pastas primeiro, depois arquivos, alfabético sem diferenciar caixa
    let key = |n: &TreeNode| match n {
        TreeNode::Folder { name, .. } | TreeNode::File { name, .. } => name.to_lowercase(),
    };
    folders.sort_by_key(key);
    files.sort_by_key(key);
    folders.extend(files);
    folders
}

fn read_tree_at(root: &Path) -> TreeNode {
    TreeNode::Folder {
        name: root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| root.to_string_lossy().to_string()),
        path: root.to_string_lossy().to_string(),
        children: build_tree(root),
    }
}

pub(crate) fn app_dir(app: &AppHandle) -> Res<PathBuf> {
    app.path().app_config_dir().map_err(err)
}

// ---------- Cofres ----------

#[tauri::command]
fn list_vaults(app: AppHandle) -> Res<Vec<SavedVault>> {
    let mut cfg = config::load_app(&app_dir(&app)?);
    cfg.vaults.retain(|v| Path::new(&v.path).is_dir());
    Ok(cfg.vaults)
}

#[tauri::command]
fn forget_vault(app: AppHandle, path: String) -> Res<Vec<SavedVault>> {
    let dir = app_dir(&app)?;
    let mut cfg = config::load_app(&dir);
    cfg.vaults.retain(|v| v.path != path);
    config::save_app(&dir, &cfg)?;
    Ok(cfg.vaults)
}

/// Define o cofre ativo, salva-o na lista de cofres e devolve árvore + configuração.
#[tauri::command]
fn open_vault(app: AppHandle, vault_path: String, state: State<VaultState>) -> Res<OpenedVault> {
    let root = Path::new(&vault_path)
        .canonicalize()
        .map_err(|_| "O caminho fornecido para o cofre não é válido.".to_string())?;
    if !root.is_dir() {
        return Err("O caminho fornecido para o cofre não é uma pasta.".to_string());
    }
    // Libera imagens/PDFs/vídeos do cofre para o protocolo asset:// do webview
    app.asset_protocol_scope().allow_directory(&root, true).map_err(err)?;
    config::touch_vault(&app_dir(&app)?, &root)?;

    let cfg = config::load_vault(&root);
    state.plugins.write().map_err(err)?.reload(&root, &cfg.plugins);
    state.pages.write().map_err(err)?.clear();
    *state.jev.write().map_err(err)? = None;
    *state.root.lock().map_err(err)? = Some(root.clone());
    Ok(OpenedVault { tree: read_tree_at(&root), config: cfg })
}

#[tauri::command]
fn read_tree(state: State<VaultState>) -> Res<TreeNode> {
    Ok(read_tree_at(&vault_root(&state)?))
}

pub(crate) fn collect_markdown(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_hidden(&path) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if is_md(&path) {
                files.push(path);
            }
        }
    }
    files
}

/// Lê todas as notas em paralelo, reconstrói o índice de busca e o de páginas.
#[tauri::command]
fn index_vault(state: State<VaultState>) -> Res<Vec<Note>> {
    let start_time = Instant::now();
    let root = vault_root(&state)?;
    let files = collect_markdown(&root);

    let workers = thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = files.len().div_ceil(workers).max(1);
    let loaded: Vec<(PathBuf, String, Page)> = thread::scope(|scope| {
        let root = &root;
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .filter_map(|p| {
                            let content = fs::read_to_string(p).ok()?;
                            let page = pages::parse_page(p, root, &content);
                            Some((p.clone(), content, page))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });

    state.search.rebuild(loaded.iter().map(|(p, c, _)| (p.as_path(), c.as_str())))?;

    let notes = loaded.iter().map(|(_, _, page)| Note::from((page, root.as_path()))).collect();
    *state.pages.write().map_err(err)? = loaded.into_iter().map(|(_, _, page)| (page.path.clone(), page)).collect();

    println!("Indexação de {} notas concluída em: {:?}", files.len(), start_time.elapsed());
    Ok(notes)
}

/// Busca de texto completo. Sintaxe: termos (E implícito), `OR`, `-excluir`,
/// `"frase exata"`, `"termo1 termo2"~N` (proximidade), `title:palavra`.
#[tauri::command]
fn search_vault(query: String, limit: Option<usize>, state: State<VaultState>) -> Res<Vec<SearchHit>> {
    vault_root(&state)?;
    state.search.search(&query, limit.unwrap_or(50).min(500))
}

// ---------- Arquivos ----------

#[tauri::command]
fn read_note(file_path: String, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let path = ensure_inside(&root, Path::new(&file_path))?;
    fs::read_to_string(path).map_err(err)
}

pub(crate) fn reindex_note(state: &State<VaultState>, root: &Path, path: &Path, content: &str) -> Res<Note> {
    state.search.upsert(path, content)?;
    let page = pages::parse_page(path, root, content);
    native::jev_touch(state, root, &page, content);
    let note = Note::from((&page, root));
    state.pages.write().map_err(err)?.insert(page.path.clone(), page);
    Ok(note)
}

pub(crate) fn forget_note(state: &State<VaultState>, path: &Path) -> Res<()> {
    state.search.remove(path)?;
    native::jev_drop(state, path);
    state.pages.write().map_err(err)?.remove(&*path.to_string_lossy());
    Ok(())
}

/// Salva a nota e devolve seus metadados atualizados (links/tags para backlinks e grafo).
#[tauri::command]
fn save_note(file_path: String, content: String, state: State<VaultState>) -> Res<Note> {
    let root = vault_root(&state)?;
    let path = ensure_inside(&root, Path::new(&file_path))?;
    if !is_md(&path) {
        return Err("Apenas arquivos Markdown (.md) podem ser salvos.".to_string());
    }
    fs::write(&path, &content).map_err(err)?;
    reindex_note(&state, &root, &path, &content)
}

fn folder_inside(root: &Path, folder: &str) -> Res<PathBuf> {
    let dir = Path::new(folder).canonicalize().map_err(err)?;
    if !dir.starts_with(root) || !dir.is_dir() {
        return Err("Pasta fora do cofre.".to_string());
    }
    Ok(dir)
}

/// Cria uma nota dentro de `folder` e devolve o caminho.
/// Sem `name`, usa "Sem título.md" (ou "Sem título 1.md", ...).
/// Com `name` (ex.: alvo de um wikilink), reaproveita a nota se ela já existir.
#[tauri::command]
fn create_note(folder: String, name: Option<String>, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let dir = folder_inside(&root, &folder)?;
    let candidate = match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => {
            let path = dir.join(format!("{}.md", validate_name(n)?));
            if path.exists() {
                return Ok(path.to_string_lossy().to_string());
            }
            path
        }
        None => {
            let mut path = dir.join("Sem título.md");
            let mut n = 1;
            while path.exists() {
                path = dir.join(format!("Sem título {n}.md"));
                n += 1;
            }
            path
        }
    };
    fs::write(&candidate, "").map_err(err)?;
    reindex_note(&state, &root, &candidate, "")?;
    Ok(candidate.to_string_lossy().to_string())
}

#[tauri::command]
fn create_folder(parent: String, name: Option<String>, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let dir = folder_inside(&root, &parent)?;
    let path = match name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
        Some(n) => dir.join(validate_name(n)?),
        None => {
            let mut path = dir.join("Nova pasta");
            let mut n = 1;
            while path.exists() {
                path = dir.join(format!("Nova pasta {n}"));
                n += 1;
            }
            path
        }
    };
    if path.exists() {
        return Err("Já existe um item com esse nome.".into());
    }
    fs::create_dir(&path).map_err(err)?;
    Ok(path.to_string_lossy().to_string())
}

/// Renomeia arquivo ou pasta (mesmo diretório). Devolve o novo caminho.
#[tauri::command]
fn rename_path(path: String, new_name: String, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let from = ensure_inside(&root, Path::new(&path))?;
    if from == root {
        return Err("Não é possível renomear a raiz do cofre.".into());
    }
    let to = from.with_file_name(validate_name(&new_name)?);
    if to == from {
        return Ok(to.to_string_lossy().to_string());
    }
    // Permite trocar só a caixa ("nota" → "Nota") em sistemas que ignoram caixa
    let only_case = to.to_string_lossy().to_lowercase() == from.to_string_lossy().to_lowercase();
    if to.exists() && !only_case {
        return Err("Já existe um item com esse nome.".into());
    }
    fs::rename(&from, &to).map_err(err)?;
    if is_md(&from) {
        forget_note(&state, &from)?;
        if is_md(&to) {
            let content = fs::read_to_string(&to).unwrap_or_default();
            reindex_note(&state, &root, &to, &content)?;
        }
    }
    Ok(to.to_string_lossy().to_string())
}

/// Move para a Lixeira do sistema (recuperável), nunca apaga definitivamente.
#[tauri::command]
fn delete_path(path: String, state: State<VaultState>) -> Res<()> {
    let root = vault_root(&state)?;
    let target = ensure_inside(&root, Path::new(&path))?;
    if target == root {
        return Err("Não é possível excluir a raiz do cofre.".into());
    }
    trash::delete(&target).map_err(err)?;
    if is_md(&target) {
        forget_note(&state, &target)?;
    }
    Ok(())
}

/// Move arquivo ou pasta para outra pasta do cofre (arrastar e soltar / "Mover para…").
/// Reindexa só as notas afetadas e devolve o novo caminho.
#[tauri::command]
fn move_path(path: String, target_folder: String, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let from = ensure_inside(&root, Path::new(&path))?;
    if from == root {
        return Err("Não é possível mover a raiz do cofre.".into());
    }
    let dir = folder_inside(&root, &target_folder)?;
    if dir.starts_with(&from) {
        return Err("Não é possível mover uma pasta para dentro dela mesma.".into());
    }
    let to = dir.join(from.file_name().ok_or("Caminho inválido.")?);
    if to == from {
        return Ok(to.to_string_lossy().to_string());
    }
    if to.exists() {
        return Err(format!("Já existe “{}” na pasta de destino.", to.file_name().unwrap_or_default().to_string_lossy()));
    }
    fs::rename(&from, &to).map_err(err)?;
    let moved = if to.is_dir() { collect_markdown(&to) } else if is_md(&to) { vec![to.clone()] } else { Vec::new() };
    for new in moved {
        let old = match new.strip_prefix(&to) {
            Ok(rel) if !rel.as_os_str().is_empty() => from.join(rel),
            _ => from.clone(),
        };
        forget_note(&state, &old)?;
        let content = fs::read_to_string(&new).unwrap_or_default();
        reindex_note(&state, &root, &new, &content)?;
    }
    Ok(to.to_string_lossy().to_string())
}

pub(crate) fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Pasta de anexos como no Obsidian (`attachmentFolderPath` do .obsidian/app.json):
/// "/" ou vazio = raiz do cofre, "./" = pasta da nota, "./sub" = subpasta da nota.
fn attachment_dir(root: &Path, note: &Path) -> PathBuf {
    let setting = fs::read_to_string(root.join(".obsidian").join("app.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.get("attachmentFolderPath").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default();
    let setting = setting.trim().replace('\\', "/");
    let note_dir = note.parent().unwrap_or(root);
    let dir = if setting.is_empty() || setting == "/" {
        root.to_path_buf()
    } else if setting == "." || setting == "./" {
        note_dir.to_path_buf()
    } else if let Some(sub) = setting.strip_prefix("./") {
        note_dir.join(sub)
    } else {
        root.join(setting.trim_start_matches('/'))
    };
    if dir.starts_with(root) && !dir.components().any(|c| matches!(c, std::path::Component::ParentDir)) { dir } else { root.to_path_buf() }
}

/// Salva uma imagem colada no editor. Os bytes vêm crus no corpo da requisição
/// (sem JSON); `x-note` e `x-ext` vêm nos cabeçalhos. Devolve o nome do arquivo.
#[tauri::command]
fn save_attachment(request: tauri::ipc::Request, state: State<VaultState>) -> Res<String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("Conteúdo da imagem ausente.".into());
    };
    let header = |k: &str| request.headers().get(k).and_then(|v| v.to_str().ok()).map(percent_decode).unwrap_or_default();
    let root = vault_root(&state)?;
    let note = ensure_inside(&root, Path::new(&header("x-note")))?;
    let ext = header("x-ext").to_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "avif") {
        return Err("Formato de imagem não suportado.".into());
    }
    let dir = attachment_dir(&root, &note);
    fs::create_dir_all(&dir).map_err(err)?;
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S");
    let mut path = dir.join(format!("Pasted image {stamp}.{ext}"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("Pasted image {stamp} {n}.{ext}"));
        n += 1;
    }
    fs::write(&path, bytes).map_err(err)?;
    Ok(path.file_name().unwrap_or_default().to_string_lossy().to_string())
}

/// Bytes brutos de um arquivo do cofre (ex.: PDF para o pdf.js), sem passar por JSON.
#[tauri::command]
fn read_file_bytes(path: String, state: State<VaultState>) -> Res<tauri::ipc::Response> {
    let root = vault_root(&state)?;
    let path = ensure_inside(&root, Path::new(&path))?;
    Ok(tauri::ipc::Response::new(fs::read(path).map_err(err)?))
}

#[derive(Serialize)]
struct Sheet {
    name: String,
    rows: Vec<Vec<String>>,
    truncated: bool,
}

const MAX_SHEET_ROWS: usize = 5000;

/// Lê planilhas (xlsx, xlsm, xls, xlsb, ods, csv) para visualização.
#[tauri::command]
fn read_spreadsheet(path: String, state: State<VaultState>) -> Res<Vec<Sheet>> {
    use calamine::{open_workbook_auto, Reader};
    let root = vault_root(&state)?;
    let path = ensure_inside(&root, Path::new(&path))?;
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();

    if ext == "csv" || ext == "tsv" {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(false)
            .flexible(true)
            .delimiter(if ext == "tsv" { b'\t' } else { b',' })
            .from_path(&path)
            .map_err(err)?;
        let mut rows = Vec::new();
        let mut truncated = false;
        for record in reader.records() {
            if rows.len() >= MAX_SHEET_ROWS {
                truncated = true;
                break;
            }
            rows.push(record.map_err(err)?.iter().map(String::from).collect());
        }
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        return Ok(vec![Sheet { name, rows, truncated }]);
    }

    let mut wb = open_workbook_auto(&path).map_err(err)?;
    let mut sheets = Vec::new();
    for name in wb.sheet_names().to_owned() {
        let Ok(range) = wb.worksheet_range(&name) else { continue };
        let truncated = range.height() > MAX_SHEET_ROWS;
        let rows = range
            .rows()
            .take(MAX_SHEET_ROWS)
            .map(|r| r.iter().map(|c| c.to_string()).collect())
            .collect();
        sheets.push(Sheet { name, rows, truncated });
    }
    Ok(sheets)
}

/// Converte células copiadas do Excel (separadas por tabulação) em tabela Markdown.
#[tauri::command]
fn tsv_to_markdown(text: String) -> Option<String> {
    let rows: Vec<Vec<String>> = text
        .trim_end_matches(['\r', '\n'])
        .lines()
        .map(|l| l.trim_end_matches('\r').split('\t').map(|c| c.trim().replace('|', "\\|")).collect())
        .collect();
    if rows.len() < 2 || rows.iter().any(|r| r.len() < 2) {
        return None;
    }
    let cols = rows.iter().map(Vec::len).max()?;
    let line = |cells: &[String]| {
        (0..cols).fold(String::from("|"), |mut s, i| {
            s.push(' ');
            s.push_str(cells.get(i).map(String::as_str).unwrap_or(""));
            s.push_str(" |");
            s
        })
    };
    let mut md = line(&rows[0]);
    md.push('\n');
    md.push_str(&line(&vec!["---".to_string(); cols]));
    for row in &rows[1..] {
        md.push('\n');
        md.push_str(&line(row));
    }
    md.push('\n');
    Some(md)
}

// ---------- rustview e blocos de plugins ----------

/// Renderiza um bloco de código especial (```rustview ou de um plugin).
/// `content` é o texto atual do editor, então o resultado reflete edições não salvas.
#[tauri::command]
fn render_block(lang: String, source: String, path: String, content: String, state: State<VaultState>) -> Res<Option<Value>> {
    let root = vault_root(&state)?;
    let started = Instant::now();
    if lang == "rustview" {
        let current = pages::parse_page(Path::new(&path), &root, &content);
        let all: Vec<Page> = state.pages.read().map_err(err)?.values().cloned().collect();
        let nodes = rustview::run(&source, &current, &all);
        return Ok(Some(serde_json::json!({ "nodes": nodes, "ms": started.elapsed().as_secs_f64() * 1000.0 })));
    }
    if (lang == "math" || lang == "numerals") && native::core_enabled(&config::load_vault(&root), "numerals") {
        let nodes = numerals::run(&source);
        return Ok(Some(serde_json::json!({ "nodes": nodes, "ms": started.elapsed().as_secs_f64() * 1000.0 })));
    }
    let note = serde_json::json!({ "path": path, "content": content });
    let host = state.plugins.read().map_err(err)?;
    match host.render(&lang, &source, note) {
        Some(Ok(mut out)) => {
            out["ms"] = (started.elapsed().as_secs_f64() * 1000.0).into();
            Ok(Some(out))
        }
        Some(Err(e)) => Ok(Some(serde_json::json!({ "nodes": [{ "type": "error", "message": e }] }))),
        None => Ok(None),
    }
}

// ---------- Plugins ----------

#[tauri::command]
fn list_plugins(state: State<VaultState>) -> Res<Vec<PluginInfo>> {
    Ok(state.plugins.read().map_err(err)?.infos().to_vec())
}

#[tauri::command]
fn install_plugin(source: String, state: State<VaultState>) -> Res<Vec<PluginInfo>> {
    let root = vault_root(&state)?;
    let src = Path::new(&source);
    if src.extension().and_then(|s| s.to_str()) != Some("wasm") {
        return Err("Selecione um arquivo .wasm".into());
    }
    let dir = plugins::plugins_dir(&root);
    fs::create_dir_all(&dir).map_err(err)?;
    fs::copy(src, dir.join(src.file_name().ok_or("Arquivo inválido")?)).map_err(err)?;
    let cfg = config::load_vault(&root);
    let mut host = state.plugins.write().map_err(err)?;
    host.reload(&root, &cfg.plugins);
    Ok(host.infos().to_vec())
}

#[tauri::command]
fn run_plugin_command(plugin: String, command: String, note: Value, state: State<VaultState>) -> Res<Value> {
    state.plugins.read().map_err(err)?.command(&plugin, &command, note)
}

#[tauri::command]
fn plugins_dir(state: State<VaultState>) -> Res<String> {
    let dir = plugins::plugins_dir(&vault_root(&state)?);
    fs::create_dir_all(&dir).map_err(err)?;
    Ok(dir.to_string_lossy().to_string())
}

// ---------- Configuração e temas ----------

#[tauri::command]
fn save_vault_config(config: VaultConfig, state: State<VaultState>) -> Res<Vec<PluginInfo>> {
    let root = vault_root(&state)?;
    let previous = config::load_vault(&root);
    config::save_vault(&root, &config)?;
    let mut host = state.plugins.write().map_err(err)?;
    if previous.plugins != config.plugins {
        host.reload(&root, &config.plugins);
    }
    Ok(host.infos().to_vec())
}

#[derive(Serialize)]
struct Appearance {
    themes: Vec<String>,
    snippets: Vec<String>,
    dir: String,
}

fn css_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("css"))
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect();
    names.sort();
    names
}

#[tauri::command]
fn list_appearance(state: State<VaultState>) -> Res<Appearance> {
    let root = vault_root(&state)?;
    let silex = root.join(".silex");
    Ok(Appearance {
        themes: css_names(&silex.join("themes")),
        snippets: css_names(&silex.join("snippets")),
        dir: silex.to_string_lossy().to_string(),
    })
}

#[tauri::command]
fn read_css(kind: String, name: String, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let folder = match kind.as_str() {
        "theme" => "themes",
        "snippet" => "snippets",
        _ => return Err("Tipo inválido".into()),
    };
    let path = root.join(".silex").join(folder).join(format!("{}.css", validate_name(&name)?));
    fs::read_to_string(path).map_err(err)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    // Atualização automática pelas Releases do GitHub (só no desktop)
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build()).plugin(tauri_plugin_process::init());
    builder
        .manage(VaultState {
            root: Mutex::new(None),
            search: SearchIndex::new().expect("Falha ao criar o índice de busca"),
            pages: RwLock::new(HashMap::new()),
            plugins: RwLock::new(PluginHost::default()),
            jev: RwLock::new(None),
            jev_version: AtomicU64::new(0),
            organizer_cancel: AtomicBool::new(false),
            organizer_running: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            list_vaults,
            forget_vault,
            open_vault,
            read_tree,
            index_vault,
            search_vault,
            read_note,
            save_note,
            create_note,
            create_folder,
            rename_path,
            delete_path,
            move_path,
            chat::chat_list,
            chat::chat_get,
            chat::chat_create,
            chat::chat_update,
            chat::chat_delete,
            chat::chat_stop,
            chat::chat_send,
            chat::chat_running,
            chat::chat_attach,
            chat::chat_attach_files,
            dialogs::dialog_open,
            dialogs::dialog_open_many,
            save_attachment,
            read_spreadsheet,
            read_file_bytes,
            tsv_to_markdown,
            render_block,
            list_plugins,
            install_plugin,
            run_plugin_command,
            plugins_dir,
            save_vault_config,
            list_appearance,
            read_css,
            native::secret_len,
            native::set_secret,
            native::jev_test,
            native::llm_test,
            native::jev_home_local,
            native::jev_home_find,
            native::jev_home_reindex,
            native::organizer_scan,
            native::organizer_cancel,
            native::organizer_list,
            native::organizer_apply,
            native::organizer_dismiss,
            native::organizer_clear,
            native::templater_list,
            native::templater_render,
            native::templater_for_folder,
            native::export_html,
            native::export_write,
            native::export_pandoc,
            native::mesa_init,
            native::mesa_all,
            native::mesa_create,
            native::mesa_update,
            native::mesa_undo,
            native::mesa_history,
            native::mesa_usage,
            native::mesa_context,
            native::mesa_ai,
            native::mesa_jev,
            native::mesa_local_get,
            native::mesa_local_set,
            native::mesa_read,
            native::mesa_save_settings,
            native::write_text_file,
            native::create_file
        ])
        .run(tauri::generate_context!())
        .expect("Erro ao executar a aplicação Silex");
}
