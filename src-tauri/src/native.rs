//! Comandos dos plugins nativos (reescritas em Rust dos plugins do Obsidian):
//! Jev Home, Jev Organizer, Templater, Numerals, Exportar e Mesa, além das chaves
//! de API e de arquivos genéricos usados por Canvas e Excalidraw.

use crate::config::{self, VaultConfig};
use crate::pages::{self, Page};
use crate::{app_dir, collect_markdown, ensure_inside, err, is_md, reindex_note, validate_name, vault_root, Res, VaultState};
use crate::{export, jev_home, mesa, net, organizer, secrets, templater};
use futures::StreamExt;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

/// Plugins nativos ativos por padrão (os que dependem de chave ficam desligados).
const DEFAULT_CORE: &[&str] = &["rustview", "excel-paste", "templater", "numerals", "export", "canvas", "excalidraw", "chat"];

pub fn enabled_core(cfg: &VaultConfig) -> Vec<String> {
    cfg.core_plugins.clone().unwrap_or_else(|| DEFAULT_CORE.iter().map(|s| s.to_string()).collect())
}

pub fn core_enabled(cfg: &VaultConfig, id: &str) -> bool {
    enabled_core(cfg).iter().any(|x| x == id)
}

fn rel_of(root: &Path, p: &Path) -> String {
    p.strip_prefix(root).unwrap_or(p).to_string_lossy().replace('\\', "/")
}

fn mtime_ms(p: &Path) -> i64 {
    fs::metadata(p).ok().and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(0)
}

fn ctime_ms(p: &Path) -> i64 {
    fs::metadata(p).ok().and_then(|m| m.created().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or_else(|| mtime_ms(p))
}

// ---------- Chaves ----------

#[tauri::command]
pub fn secret_len(name: String) -> usize {
    secrets::len(&name)
}

#[tauri::command]
pub fn set_secret(name: String, value: String) -> Res<usize> {
    secrets::set(&name, &value)?;
    Ok(secrets::len(&name))
}

#[tauri::command]
pub async fn jev_test(model: String) -> Res<f64> {
    let a = net::jev_ask(&model, &json!("ok"), &json!({ "ok": { "type": "noul", "instructions": "Is this text the word ok?" } }), 20, 1).await?;
    a.answers.pointer("/ok/noul").and_then(Value::as_f64).ok_or_else(|| "Jev respondeu sem o valor esperado.".into())
}

#[tauri::command]
pub async fn llm_test(provider: String, model: String) -> Res<bool> {
    let r = net::llm_call(&provider, &model, "Responda JSON {\"ok\":true}.", &json!({})).await?;
    Ok(r.value.get("ok") == Some(&json!(true)))
}

// ---------- Jev Home ----------

fn jev_settings(cfg: &VaultConfig) -> jev_home::Settings {
    cfg.plugin("jev-home")
}

fn build_jev(root: &Path, s: &jev_home::Settings, version: u64) -> jev_home::Index {
    let docs = collect_markdown(root)
        .into_iter()
        .filter_map(|p| {
            let rel = rel_of(root, &p);
            if jev_home::excluded(&s.exclude, &rel) {
                return None;
            }
            let content = fs::read_to_string(&p).ok()?;
            let page = pages::parse_page(&p, root, &content);
            Some(jev_home::make_doc(&page, &rel, &content, mtime_ms(&p), s.index_chars))
        })
        .collect();
    jev_home::Index::new(docs, version)
}

fn with_jev<T>(state: &State<VaultState>, root: &Path, f: impl FnOnce(&mut jev_home::Index) -> T) -> Res<T> {
    let mut guard = state.jev.write().map_err(err)?;
    if guard.is_none() {
        let v = state.jev_version.fetch_add(1, Ordering::SeqCst) + 1;
        *guard = Some(build_jev(root, &jev_settings(&config::load_vault(root)), v));
    }
    Ok(f(guard.as_mut().unwrap()))
}

/// Mantém o índice do Jev Home atualizado quando uma nota é salva.
pub fn jev_touch(state: &State<VaultState>, root: &Path, page: &Page, content: &str) {
    let Ok(mut guard) = state.jev.write() else { return };
    let Some(index) = guard.as_mut() else { return };
    let s = jev_settings(&config::load_vault(root));
    let rel = rel_of(root, Path::new(&page.path));
    let mut docs: Vec<jev_home::Doc> = index.docs.iter().filter(|d| d.path != rel).cloned().collect();
    if !jev_home::excluded(&s.exclude, &rel) {
        docs.push(jev_home::make_doc(page, &rel, content, chrono::Utc::now().timestamp_millis(), s.index_chars));
    }
    let v = state.jev_version.fetch_add(1, Ordering::SeqCst) + 1;
    *index = jev_home::Index::new(docs, v);
}

pub fn jev_drop(state: &State<VaultState>, path: &Path) {
    if let Ok(mut guard) = state.jev.write() {
        if let Some(index) = guard.as_mut() {
            let abs = path.to_string_lossy();
            if index.docs.iter().any(|d| d.abs == abs) {
                let docs = index.docs.iter().filter(|d| d.abs != abs).cloned().collect();
                let v = state.jev_version.fetch_add(1, Ordering::SeqCst) + 1;
                *index = jev_home::Index::new(docs, v);
            }
        }
    }
}

#[tauri::command]
pub fn jev_home_local(query: String, limit: Option<usize>, state: State<VaultState>) -> Res<Vec<Value>> {
    let root = vault_root(&state)?;
    with_jev(&state, &root, |idx| idx.local_search(&query, limit.unwrap_or(8)).into_iter().map(|i| jev_home::doc_json(&idx.docs[i])).collect())
}

#[tauri::command]
pub fn jev_home_reindex(state: State<'_, VaultState>) -> Res<usize> {
    let root = vault_root(&state)?;
    *state.jev.write().map_err(err)? = None;
    with_jev(&state, &root, |idx| idx.docs.len())
}

#[tauri::command]
pub async fn jev_home_find(query: String, force_broad: bool, recent: Vec<String>, state: State<'_, VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let s = jev_settings(&config::load_vault(&root));
    let broad = s.broad || force_broad;
    let (input, key) = with_jev(&state, &root, |idx| {
        let key = jev_home::cache_key(idx.version, force_broad, &query);
        if let Some(hit) = jev_home::cache_get(idx, &key) {
            return (None, Some((key, hit)));
        }
        let local = idx.local_search(&query, s.local_limit);
        let recent_idx = recent.iter().filter_map(|abs| idx.docs.iter().position(|d| d.abs == *abs)).collect();
        (Some(jev_home::FindInput { docs: idx.docs.clone(), local, recent: recent_idx, version: idx.version }), Some((key, Value::Null)))
    })?;
    let (key, hit) = key.unwrap();
    let Some(input) = input else {
        let mut hit = hit;
        hit["cached"] = json!(true);
        return Ok(hit);
    };
    let mut settings = s.clone();
    settings.broad = broad;
    let version = input.version;
    let res = jev_home::find(&settings, input, &query, force_broad).await?;
    if let Ok(mut guard) = state.jev.write() {
        if let Some(idx) = guard.as_mut().filter(|i| i.version == version) {
            jev_home::cache_put(idx, key, res.clone());
        }
    }
    Ok(res)
}

// ---------- Jev Organizer ----------

fn organizer_settings(cfg: &VaultConfig) -> organizer::Settings {
    cfg.plugin("jev-organizer")
}

fn notes_info(root: &Path, s: &organizer::Settings) -> Vec<(organizer::NoteInfo, String)> {
    let log = s.log_path.trim().trim_matches('/').to_string();
    collect_markdown(root)
        .into_iter()
        .filter_map(|p| {
            let rel = rel_of(root, &p);
            if rel == log || jev_home::excluded(&s.exclude, &rel) {
                return None;
            }
            let content = fs::read_to_string(&p).ok()?;
            let page = pages::parse_page(&p, root, &content);
            Some((organizer::NoteInfo { rel, page, mtime: mtime_ms(&p), ctime: ctime_ms(&p) }, content))
        })
        .collect()
}

/// Analisa a nota (arquivo), uma pasta ou o cofre inteiro (scope = None).
#[tauri::command]
pub async fn organizer_scan(app: AppHandle, scope: Option<String>, force: bool, state: State<'_, VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    if secrets::len("typesafe") == 0 {
        return Err("Informe a API key da TypeSafe nas configurações do Jev Organizer.".into());
    }
    if state.organizer_running.swap(true, Ordering::SeqCst) {
        return Err("Já existe uma análise em andamento.".into());
    }
    state.organizer_cancel.store(false, Ordering::SeqCst);
    let result = async {
        let s = organizer_settings(&config::load_vault(&root));
        let all = notes_info(&root, &s);
        let infos: Vec<organizer::NoteInfo> = all.iter().map(|(n, _)| n.clone()).collect();
        let mut suggestions = organizer::load(&root);
        let scope_rel = scope.as_ref().map(|p| rel_of(&root, Path::new(p)));
        let is_file = scope_rel.as_ref().is_some_and(|r| r.ends_with(".md"));
        let mut files: Vec<&(organizer::NoteInfo, String)> = all
            .iter()
            .filter(|(n, _)| match &scope_rel {
                Some(r) if is_file => n.rel == *r,
                Some(r) if !r.is_empty() => n.rel.starts_with(&format!("{r}/")),
                _ => true,
            })
            .collect();
        if is_file && files.is_empty() {
            return Err("Esta nota está em uma pasta excluída.".to_string());
        }
        if !force && !is_file {
            files.retain(|(n, _)| suggestions.get(&n.rel).map(|e| e.mtime) != Some(n.mtime));
        }
        if s.only_empty && !is_file {
            files.retain(|(n, _)| n.page.frontmatter.as_object().map_or(true, |m| m.is_empty()));
        }
        if files.is_empty() {
            return Ok(json!({ "done": 0, "errors": 0, "message": "Nada novo para analisar." }));
        }
        files.truncate(s.max_per_scan.max(1));
        let total = files.len();
        let ctx = organizer::context(&s, &root, &infos);
        let (mut done, mut errors, mut last_error) = (0usize, 0usize, String::new());
        let emit = |done: usize, errors: usize, last: &str, running: bool| {
            let _ = app.emit("organizer-progress", json!({ "running": running, "done": done, "total": total, "errors": errors, "lastError": last }));
        };
        emit(0, 0, "", true);
        let concurrency = s.concurrency.clamp(1, 8);
        let jobs: Vec<(organizer::NoteInfo, String)> = files.into_iter().cloned().collect();
        let (s, ctx, root_arc) = (Arc::new(s), Arc::new(ctx), Arc::new(root.clone()));
        let mut stream = futures::stream::iter(jobs.into_iter().map(|(n, content)| {
            let (s, ctx, root) = (s.clone(), ctx.clone(), root_arc.clone());
            async move {
                let r = organizer::analyze(&s, &ctx, &n, &content, &root).await;
                (n.rel, r)
            }
        }))
        .buffer_unordered(concurrency);
        while let Some((rel, res)) = stream.next().await {
            match res {
                Ok(entry) => {
                    suggestions.insert(rel, entry);
                }
                Err(e) => {
                    errors += 1;
                    if e.contains("401") || e.contains("API key") {
                        state.organizer_cancel.store(true, Ordering::SeqCst);
                    }
                    last_error = e;
                }
            }
            done += 1;
            emit(done, errors, &last_error, true);
            if state.organizer_cancel.load(Ordering::SeqCst) {
                break;
            }
        }
        drop(stream);
        organizer::save(&root, &suggestions)?;
        emit(done, errors, &last_error, false);
        Ok(json!({ "done": done, "errors": errors, "lastError": last_error }))
    }
    .await;
    state.organizer_running.store(false, Ordering::SeqCst);
    result
}

#[tauri::command]
pub fn organizer_cancel(state: State<VaultState>) {
    state.organizer_cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
pub fn organizer_list(state: State<VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let s = organizer::load(&root);
    let live: Map<String, Value> = s
        .into_iter()
        .filter(|(rel, e)| !e.items.is_empty() && root.join(rel).is_file())
        .map(|(rel, e)| (rel, serde_json::to_value(e).unwrap()))
        .collect();
    Ok(json!({ "suggestions": live, "running": state.organizer_running.load(Ordering::SeqCst) }))
}

#[tauri::command]
pub fn organizer_apply(selection: HashMap<String, Vec<organizer::Item>>, state: State<VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let s = organizer_settings(&config::load_vault(&root));
    let all_rel: Vec<String> = collect_markdown(&root).iter().map(|p| rel_of(&root, p)).collect();
    let mut sugg = organizer::load(&root);
    let applied = organizer::apply(&s, &root, &selection, &all_rel, &mut sugg);
    let log = organizer::log(&s, &root, &applied.rows, &all_rel)?;
    organizer::save(&root, &sugg)?;
    for (old, new) in &applied.touched {
        if let Some(old) = old {
            crate::forget_note(&state, &root.join(old))?;
        }
        let p = root.join(new);
        if let Ok(c) = fs::read_to_string(&p) {
            reindex_note(&state, &root, &p, &c)?;
        }
    }
    if let Some(l) = &log {
        let p = root.join(l);
        if let Ok(c) = fs::read_to_string(&p) {
            reindex_note(&state, &root, &p, &c)?;
        }
    }
    Ok(json!({ "ok": applied.ok, "fail": applied.fail, "lastError": applied.last_error, "log": log }))
}

#[tauri::command]
pub fn organizer_dismiss(items: HashMap<String, Vec<String>>, state: State<VaultState>) -> Res<()> {
    let root = vault_root(&state)?;
    let mut s = organizer::load(&root);
    for (rel, ids) in items {
        if let Some(e) = s.get_mut(&rel) {
            e.items.retain(|i| !ids.contains(&i.id));
        }
    }
    organizer::save(&root, &s)
}

#[tauri::command]
pub fn organizer_clear(state: State<VaultState>) -> Res<()> {
    organizer::save(&vault_root(&state)?, &HashMap::new())
}

// ---------- Templater ----------

#[tauri::command]
pub fn templater_list(state: State<VaultState>) -> Res<Vec<Value>> {
    let root = vault_root(&state)?;
    let s: templater::Settings = config::load_vault(&root).plugin("templater");
    let Some(dir) = templater::templates_dir(&root, &s) else { return Ok(Vec::new()) };
    let mut out: Vec<Value> = collect_markdown(&dir)
        .into_iter()
        .map(|p| json!({ "path": p.to_string_lossy(), "name": p.file_stem().unwrap().to_string_lossy(), "folder": rel_of(&root, p.parent().unwrap()) }))
        .collect();
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(out)
}

#[tauri::command]
pub fn templater_render(template: String, target: String, answers: Vec<String>, clipboard: String, state: State<VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let tpl = ensure_inside(&root, Path::new(&template))?;
    let target = ensure_inside(&root, Path::new(&target))?;
    let src = fs::read_to_string(&tpl).map_err(err)?;
    let existing = fs::read_to_string(&target).unwrap_or_default();
    let page = pages::parse_page(&target, &root, &existing);
    let to_local = |ms: i64| chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&chrono::Local).naive_local()).unwrap_or_else(|| chrono::Local::now().naive_local());
    let now = chrono::Utc::now().timestamp_millis();
    let ctx = templater::FileCtx {
        root: &root,
        target: &target,
        frontmatter: page.frontmatter.clone(),
        tags: page.tags.clone(),
        created: to_local(if target.exists() { ctime_ms(&target) } else { now }),
        modified: to_local(if target.exists() { mtime_ms(&target) } else { now }),
        clipboard,
        answers,
    };
    let (text, prompts) = templater::render(&src, &ctx)?;
    // Posição do cursor em unidades UTF-16 (o editor conta assim)
    let cursor = text.find(templater::CURSOR).map(|b| text[..b].encode_utf16().count());
    Ok(json!({ "text": text.replace(templater::CURSOR, ""), "prompts": prompts, "cursor": cursor }))
}

/// Template de pasta configurado para uma nova nota (se o gatilho estiver ligado).
#[tauri::command]
pub fn templater_for_folder(target: String, state: State<VaultState>) -> Res<Option<String>> {
    let root = vault_root(&state)?;
    let cfg = config::load_vault(&root);
    if !core_enabled(&cfg, "templater") {
        return Ok(None);
    }
    let s: templater::Settings = cfg.plugin("templater");
    if !s.trigger_on_file_creation {
        return Ok(None);
    }
    let rel = rel_of(&root, Path::new(&target));
    let folder = rel.rsplit_once('/').map(|(f, _)| f).unwrap_or("");
    Ok(templater::folder_template(&s, folder).map(|t| {
        let t = if t.ends_with(".md") { t } else { format!("{t}.md") };
        root.join(t).to_string_lossy().to_string()
    }).filter(|p| Path::new(p).is_file()))
}

// ---------- Exportar ----------

// Comandos assíncronos: a conversão roda fora da thread principal. Comandos
// síncronos do Tauri rodam nela e congelavam a janela (e o cursor do mouse)
// enquanto o HTML/DOCX era montado.

#[tauri::command]
pub async fn export_html(path: String, state: State<'_, VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let note = ensure_inside(&root, Path::new(&path))?;
    tauri::async_runtime::spawn_blocking(move || export::to_html(&root, &note)).await.map_err(err)?
}

/// Grava a exportação em `folder/name.format`. Sem `overwrite`, um arquivo já
/// existente devolve o erro "EXISTE" (o painel pergunta antes de substituir).
#[tauri::command]
pub async fn export_write(path: String, format: String, folder: String, name: String, overwrite: bool, state: State<'_, VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let note = ensure_inside(&root, Path::new(&path))?;
    let dir = Path::new(&folder);
    if !dir.is_dir() {
        return Err("A pasta de destino não existe.".into());
    }
    let stem = validate_name(name.trim().trim_end_matches(&format!(".{format}")))?;
    let out = dir.join(format!("{stem}.{format}"));
    if out.exists() && !overwrite {
        return Err("EXISTE".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        match format.as_str() {
            "html" => fs::write(&out, export::to_html(&root, &note)?).map_err(err),
            "docx" => export::to_docx(&root, &note, &out),
            "epub" | "odt" | "rtf" | "tex" | "pptx" => export::with_pandoc(&root, &note, &out),
            _ => Err("Formato inválido.".into()),
        }?;
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub fn export_pandoc() -> Option<String> {
    export::pandoc_version()
}

// ---------- Mesa ----------

fn mesa_settings(root: &Path) -> mesa::Settings {
    config::load_vault(root).plugin("mesa")
}

fn store_for<'a>(root: &'a Path, s: &mesa::Settings) -> Res<mesa::Store<'a>> {
    Ok(mesa::Store { vault: root, root: mesa::safe_path(&s.root)? })
}

fn reindex_rel(state: &State<VaultState>, root: &Path, rel: &str) {
    let p = root.join(rel);
    if is_md(&p) {
        if let Ok(c) = fs::read_to_string(&p) {
            let _ = reindex_note(state, root, &p, &c);
        }
    }
}

#[tauri::command]
pub fn mesa_init(state: State<VaultState>) -> Res<mesa::Settings> {
    let root = vault_root(&state)?;
    let s = mesa_settings(&root);
    let store = store_for(&root, &s)?;
    store.init()?;
    if mesa::config_note_read(&root, &s.root).is_none() {
        mesa::config_note_write(&root, &s)?;
    }
    Ok(s)
}

#[tauri::command]
pub fn mesa_save_settings(settings: mesa::Settings, state: State<VaultState>) -> Res<()> {
    let root = vault_root(&state)?;
    let mut cfg = config::load_vault(&root);
    cfg.plugin_settings.insert("mesa".into(), serde_json::to_value(&settings).map_err(err)?);
    config::save_vault(&root, &cfg)?;
    mesa::config_note_write(&root, &settings)
}

#[tauri::command]
pub fn mesa_all(state: State<VaultState>) -> Res<Vec<mesa::Record>> {
    let root = vault_root(&state)?;
    Ok(store_for(&root, &mesa_settings(&root))?.all(None))
}

#[tauri::command]
pub fn mesa_create(kind: String, title: String, extra: Map<String, Value>, body: String, state: State<VaultState>) -> Res<mesa::Record> {
    let root = vault_root(&state)?;
    let r = store_for(&root, &mesa_settings(&root))?.create(&kind, &title, &extra, &body)?;
    reindex_rel(&state, &root, &r.path);
    Ok(r)
}

#[tauri::command]
pub fn mesa_update(path: String, base: String, patch: Map<String, Value>, body: Option<String>, state: State<VaultState>) -> Res<mesa::Record> {
    let root = vault_root(&state)?;
    let r = store_for(&root, &mesa_settings(&root))?.update(&path, &base, &patch, body.as_deref())?;
    reindex_rel(&state, &root, &r.path);
    Ok(r)
}

#[tauri::command]
pub fn mesa_undo(file: String, state: State<VaultState>) -> Res<()> {
    let root = vault_root(&state)?;
    store_for(&root, &mesa_settings(&root))?.undo(&file)
}

#[tauri::command]
pub fn mesa_history(limit: usize, state: State<VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let (rows, total) = store_for(&root, &mesa_settings(&root))?.history(limit);
    Ok(json!({ "rows": rows, "total": total }))
}

#[tauri::command]
pub fn mesa_usage(state: State<VaultState>) -> Res<Vec<Value>> {
    let root = vault_root(&state)?;
    Ok(mesa::usage_load(&store_for(&root, &mesa_settings(&root))?))
}

#[tauri::command]
pub fn mesa_context(projects: Vec<Value>, focus: Option<Vec<String>>, budget: usize, state: State<VaultState>) -> Res<Map<String, Value>> {
    let root = vault_root(&state)?;
    let s = mesa_settings(&root);
    mesa::context(&store_for(&root, &s)?, &s, &projects, focus.as_deref(), budget)
}

#[tauri::command]
pub async fn mesa_ai(app: AppHandle, system: String, input: Value, purpose: String, task: Option<String>, tasks: Vec<Value>, state: State<'_, VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let s = mesa_settings(&root);
    let store = store_for(&root, &s)?;
    mesa::ai_call(&store, &app_dir(&app)?, &s, &system, &input, &purpose, task.as_deref(), &tasks).await
}

#[tauri::command]
pub async fn mesa_jev(app: AppHandle, state_json: Value, questions: Value, purpose: String, tasks: Vec<Value>, state: State<'_, VaultState>) -> Res<Value> {
    let root = vault_root(&state)?;
    let s = mesa_settings(&root);
    let store = store_for(&root, &s)?;
    mesa::jev_call(&store, &app_dir(&app)?, &s, &state_json, &questions, &purpose, &tasks).await
}

#[tauri::command]
pub fn mesa_local_get(app: AppHandle, state: State<VaultState>) -> Res<Map<String, Value>> {
    Ok(mesa::local_get(&app_dir(&app)?, &vault_root(&state)?))
}

#[tauri::command]
pub fn mesa_local_set(app: AppHandle, key: String, value: Value, state: State<VaultState>) -> Res<()> {
    mesa::local_set(&app_dir(&app)?, &vault_root(&state)?, &key, value)
}

#[tauri::command]
pub fn mesa_read(path: String, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    store_for(&root, &mesa_settings(&root))?.read(&path)
}

// ---------- Arquivos genéricos (Canvas, Excalidraw) ----------

const TEXT_EXTS: &[&str] = &["canvas", "excalidraw", "md", "json"];

/// Grava um arquivo de texto do cofre (canvas, desenhos). Notas usam save_note.
#[tauri::command]
pub fn write_text_file(path: String, content: String, state: State<VaultState>) -> Res<()> {
    let root = vault_root(&state)?;
    let p = ensure_inside(&root, Path::new(&path))?;
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !TEXT_EXTS.contains(&ext.as_str()) {
        return Err("Tipo de arquivo não permitido.".into());
    }
    fs::write(&p, &content).map_err(err)?;
    if is_md(&p) {
        reindex_note(&state, &root, &p, &content)?;
    }
    Ok(())
}

/// Cria um arquivo novo (nome único) numa pasta do cofre e devolve o caminho.
#[tauri::command]
pub fn create_file(folder: String, name: String, ext: String, content: String, state: State<VaultState>) -> Res<String> {
    let root = vault_root(&state)?;
    let dir = Path::new(&folder).canonicalize().map_err(err)?;
    if !dir.starts_with(&root) {
        return Err("Pasta fora do cofre.".into());
    }
    if !TEXT_EXTS.contains(&ext.rsplit('.').next().unwrap_or("")) {
        return Err("Tipo de arquivo não permitido.".into());
    }
    fs::create_dir_all(&dir).map_err(err)?;
    let base = validate_name(&name)?;
    let mut p = dir.join(format!("{base}.{ext}"));
    let mut n = 1;
    while p.exists() {
        p = dir.join(format!("{base} {n}.{ext}"));
        n += 1;
    }
    fs::write(&p, &content).map_err(err)?;
    if is_md(&p) {
        reindex_note(&state, &root, &p, &content)?;
    }
    Ok(p.to_string_lossy().to_string())
}
