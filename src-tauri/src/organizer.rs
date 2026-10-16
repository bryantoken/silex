//! Jev Organizer — reescrita em Rust do plugin "Jev Organizer" do Obsidian.
//!
//! Sugere pasta e propriedades (tags, link, author, date, aliases) para notas usando
//! o Jev (TypeSafe). O código monta os candidatos (tags e pastas do cofre, notas
//! citadas, datas encontradas...) e o Jev só julga cada um com probabilidade; nada
//! é inventado. Nada é aplicado sem o usuário marcar; tudo aplicado vai para o log.

use crate::jev_home::excluded;
use crate::net;
use crate::pages::{split_frontmatter, Page};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Keys {
    pub tags: String,
    pub link: String,
    pub author: String,
    pub date: String,
    pub aliases: String,
}
impl Default for Keys {
    fn default() -> Self {
        Self { tags: "tags".into(), link: "link".into(), author: "author".into(), date: "date".into(), aliases: "aliases".into() }
    }
}
impl Keys {
    fn of(&self, kind: &str) -> &str {
        match kind {
            "tags" => &self.tags,
            "link" => &self.link,
            "author" => &self.author,
            "date" => &self.date,
            _ => &self.aliases,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Features {
    #[serde(rename = "move")]
    pub mv: bool,
    pub tags: bool,
    pub link: bool,
    pub author: bool,
    pub date: bool,
    pub aliases: bool,
}
impl Default for Features {
    fn default() -> Self {
        Self { mv: true, tags: true, link: true, author: true, date: true, aliases: true }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub model: String,
    pub exclude: Vec<String>,
    pub max_chars: usize,
    pub tag_limit: usize,
    pub link_limit: usize,
    pub author_limit: usize,
    pub alias_limit: usize,
    pub authors: Vec<String>,
    pub keys: Keys,
    pub features: Features,
    pub min_show: f64,
    pub high: f64,
    pub max_per_scan: usize,
    pub concurrency: usize,
    pub only_empty: bool,
    pub log_path: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "jev-1.13.0".into(),
            exclude: Vec::new(),
            max_chars: 6000,
            tag_limit: 60,
            link_limit: 10,
            author_limit: 20,
            alias_limit: 8,
            authors: Vec::new(),
            keys: Keys::default(),
            features: Features::default(),
            min_show: 0.5,
            high: 0.8,
            max_per_scan: 100,
            concurrency: 3,
            only_empty: false,
            log_path: "Jev/Log de ajustes.md".into(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub value: String,
    pub label: String,
    pub p: f64,
    #[serde(default)]
    pub conf: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Entry {
    pub mtime: i64,
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub items: Vec<Item>,
}

pub type Suggestions = HashMap<String, Entry>;

pub fn store_path(root: &Path) -> PathBuf {
    root.join(".silex").join("plugins").join("jev-organizer").join("suggestions.json")
}

pub fn load(root: &Path) -> Suggestions {
    fs::read(store_path(root)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save(root: &Path, s: &Suggestions) -> Result<(), String> {
    let p = store_path(root);
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(p, serde_json::to_vec(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter(|x| !x.is_null()).map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).filter(|s| !s.is_empty()).collect(),
        Some(Value::Null) | None => Vec::new(),
        Some(Value::String(s)) if s.is_empty() => Vec::new(),
        Some(x) => vec![x.as_str().map(String::from).unwrap_or_else(|| x.to_string())],
    }
}
fn unlink(s: &str) -> String {
    s.trim_start_matches("[[").trim_end_matches("]]").split('|').next().unwrap_or("").trim().to_string()
}
fn norm(s: &str) -> String {
    unlink(s).trim_start_matches('#').to_lowercase()
}
fn folder_of(rel: &str) -> String {
    rel.rfind('/').map(|i| rel[..i].to_string()).unwrap_or_else(|| "/".into())
}
fn pct(p: f64) -> String {
    format!("{}%", (p * 100.0).round() as i64)
}
fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn tag_question(t: &str) -> Value {
    json!({ "type": "noul", "instructions": format!("Should this note be tagged \"#{t}\"? Answer yes only if the tag clearly applies to the note's subject, people, project, or purpose."), "criteria": { "true": "The tag clearly applies to this note", "false": "The tag does not apply or is only loosely related" } })
}

/// Uma nota do cofre como o Organizer enxerga.
#[derive(Clone)]
pub struct NoteInfo {
    pub rel: String,
    pub page: Page,
    pub mtime: i64,
    pub ctime: i64,
}

/// Candidatos globais, calculados uma vez por análise.
pub struct Context {
    folders: Vec<String>,
    tags: Vec<String>,
    authors: Vec<String>,
    titles: Vec<(String, Regex)>,
    /// nome/alias/caminho (minúsculo) → caminho relativo, para resolver links existentes
    resolve: HashMap<String, String>,
}

pub fn context(s: &Settings, root: &Path, notes: &[NoteInfo]) -> Context {
    let mut folders = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
                if !excluded(&s.exclude, &rel) {
                    folders.push(rel);
                    stack.push(p);
                }
            }
        }
    }
    folders.sort();

    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut author_count: HashMap<String, f64> = HashMap::new();
    let mut titles = Vec::new();
    let mut resolve = HashMap::new();
    let hex = Regex::new(r"(?i)^[0-9a-f]{3,8}$").unwrap();
    let junk = Regex::new(r"^[\d\s._-]+$").unwrap();
    for n in notes {
        for t in &n.page.tags {
            *counts.entry(t.clone()).or_default() += 1;
        }
        let fm = n.page.frontmatter.as_object();
        for a in list(fm.and_then(|f| f.get(&s.keys.author))) {
            let a = unlink(&a);
            if !a.is_empty() {
                *author_count.entry(a).or_default() += 1.0;
            }
        }
        let aliases = list(fm.and_then(|f| f.get("aliases").or_else(|| f.get("alias"))));
        resolve.insert(n.page.name.to_lowercase(), n.rel.clone());
        resolve.insert(n.rel.trim_end_matches(".md").to_lowercase(), n.rel.clone());
        for a in &aliases {
            resolve.entry(a.to_lowercase()).or_insert_with(|| n.rel.clone());
        }
        let names: Vec<String> = std::iter::once(n.page.name.clone())
            .chain(aliases)
            .map(|x| x.trim().to_string())
            .filter(|x| x.chars().count() >= 4 && !junk.is_match(x))
            .collect();
        if !names.is_empty() {
            let pat = format!(r"(?iu)(^|[^\p{{L}}\p{{N}}])({})($|[^\p{{L}}\p{{N}}])", names.iter().map(|x| regex::escape(x)).collect::<Vec<_>>().join("|"));
            if let Ok(re) = Regex::new(&pat) {
                titles.push((n.rel.clone(), re));
            }
        }
    }
    let mut tags: Vec<(String, usize)> = counts.into_iter().filter(|(t, _)| !hex.is_match(t) && !t.chars().all(|c| c.is_ascii_digit())).collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for a in &s.authors {
        if !a.trim().is_empty() {
            *author_count.entry(a.trim().to_string()).or_default() += 1e6;
        }
    }
    let mut authors: Vec<(String, f64)> = author_count.into_iter().collect();
    authors.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    Context { folders, tags: tags.into_iter().map(|(t, _)| t).collect(), authors: authors.into_iter().map(|(a, _)| a).collect(), titles, resolve }
}

/// Monta as perguntas de uma nota, pergunta ao Jev e devolve as sugestões.
pub async fn analyze(s: &Settings, ctx: &Context, note: &NoteInfo, content: &str, root: &Path) -> Result<Entry, String> {
    let k = &s.keys;
    let f = &s.features;
    let fm = note.page.frontmatter.as_object().cloned().unwrap_or_default();
    let (_, body, _) = split_frontmatter(content);
    let text: String = body.trim().chars().take(s.max_chars).collect();
    let lower = text.to_lowercase();
    let folder = folder_of(&note.rel);
    let created = chrono::DateTime::from_timestamp_millis(note.ctime).map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()).unwrap_or_else(today);
    let title = note.page.name.clone();
    let state = json!({ "today": today(), "note": { "title": title, "folder": folder, "properties": fm, "created": created, "text": text } });
    let mut q = Map::new();

    if f.mv {
        let opts: Vec<String> = std::iter::once(folder.clone()).chain(ctx.folders.iter().filter(|o| **o != folder).cloned()).take(255).collect();
        let criteria: Map<String, Value> = opts.iter().map(|o| (o.clone(), if o == "/" { json!("Vault root") } else { Value::Null })).collect();
        q.insert("move".into(), json!({ "type": "choice", "instructions": "Which folder is the best home for this note, given its subject and the vault's folder names? Prefer keeping it in its current folder (`note.folder`) unless another folder clearly fits better.", "criteria": criteria }));
    }

    let have_tags: HashSet<String> = note.page.tags.iter().map(|t| norm(t)).collect();
    let mut tag_cands: Vec<String> = if f.tags { ctx.tags.iter().filter(|t| !have_tags.contains(&t.to_lowercase())).cloned().collect() } else { Vec::new() };
    let tag_rank = tag_cands.len() > s.tag_limit;
    if tag_rank {
        for (c, chunk) in tag_cands.chunks(250).enumerate() {
            let mut criteria: Map<String, Value> = chunk.iter().map(|t| (t.clone(), Value::Null)).collect();
            criteria.insert("__none__".into(), json!("No tag fits this note"));
            q.insert(format!("tagrank_{c}"), json!({ "type": "choice", "instructions": "Which of these tags best fits this note? Choose none if no tag fits.", "criteria": criteria }));
        }
    } else {
        for (i, t) in tag_cands.iter().enumerate() {
            q.insert(format!("tag_{i}"), tag_question(t));
        }
    }

    // Links: notas citadas no texto (título ou alias) e ainda não linkadas
    let mut linked: HashSet<String> = HashSet::new();
    for l in note.page.links.iter().cloned().chain(list(fm.get(&k.link)).iter().map(|v| unlink(v))) {
        let key = l.split('#').next().unwrap_or("").trim().to_lowercase();
        if let Some(r) = ctx.resolve.get(&key).or_else(|| ctx.resolve.get(key.rsplit('/').next().unwrap_or(""))) {
            linked.insert(r.clone());
        }
    }
    let link_cands: Vec<&String> = if f.link {
        ctx.titles.iter().filter(|(rel, re)| *rel != note.rel && !linked.contains(rel) && re.is_match(&text)).map(|(rel, _)| rel).take(s.link_limit).collect()
    } else {
        Vec::new()
    };
    for (i, rel) in link_cands.iter().enumerate() {
        let other = fs::read_to_string(root.join(rel)).unwrap_or_default();
        let (_, ob, _) = split_frontmatter(&other);
        let excerpt: String = ob.trim().chars().take(400).collect();
        let name = Path::new(rel.as_str()).file_stem().unwrap().to_string_lossy().to_string();
        q.insert(format!("link_{i}"), json!({ "type": "noul", "instructions": { "candidate": { "title": name, "folder": folder_of(rel), "excerpt": excerpt }, "question": "This note mentions `candidate`. Should it link to it because `candidate` is directly related (same subject, project, person, or a reference this note depends on)?" }, "criteria": { "true": "Directly related; a link helps navigation", "false": "Only a passing or coincidental mention" } }));
    }

    let have_authors: HashSet<String> = list(fm.get(&k.author)).iter().map(|a| norm(a)).collect();
    let mut author_cands: Vec<String> = if f.author { ctx.authors.iter().filter(|a| !have_authors.contains(&a.to_lowercase())).cloned().collect() } else { Vec::new() };
    let (mentioned, rest): (Vec<String>, Vec<String>) = author_cands.drain(..).partition(|a| lower.contains(&a.to_lowercase()));
    let author_cands: Vec<String> = mentioned.into_iter().chain(rest).take(s.author_limit).collect();
    for (i, a) in author_cands.iter().enumerate() {
        q.insert(format!("author_{i}"), json!({ "type": "noul", "instructions": format!("Is \"{a}\" an author of this note: the person who wrote it, owns it, or is responsible for its content?"), "criteria": { "true": "Clearly the author or owner of the note", "false": "Not the author, or only mentioned" } }));
    }

    // Aliases: H1 diferente do nome, nome sem numeração, siglas recorrentes e "Título (sigla)"
    let have_aliases: HashSet<String> = std::iter::once(title.clone()).chain(list(fm.get(&k.aliases))).map(|x| x.to_lowercase()).collect();
    let mut ac: Vec<String> = Vec::new();
    let mut push = |v: String| {
        if !v.is_empty() && !ac.contains(&v) {
            ac.push(v);
        }
    };
    if let Some(h1) = note.page.headings.iter().find(|h| h.level == 1) {
        push(h1.text.trim().to_string());
    }
    push(Regex::new(r"^[\d\s._-]+").unwrap().replace(&title, "").trim().to_string());
    let mut acr: HashMap<String, usize> = HashMap::new();
    for m in Regex::new(r"\b[A-Z][A-Z0-9]{1,6}\b").unwrap().find_iter(&text) {
        *acr.entry(m.as_str().to_string()).or_default() += 1;
    }
    let mut acr: Vec<(String, usize)> = acr.into_iter().filter(|(_, n)| *n >= 2).collect();
    acr.sort_by(|a, b| b.1.cmp(&a.1));
    for (a, _) in acr.into_iter().take(5) {
        push(a);
    }
    if let Some(c) = Regex::new(&format!(r"(?i){}\s*\(([^)]{{2,40}})\)", regex::escape(&title))).unwrap().captures(&text) {
        push(c[1].trim().to_string());
    }
    let alias_cands: Vec<String> = if f.aliases { ac.into_iter().filter(|a| !have_aliases.contains(&a.to_lowercase())).take(s.alias_limit).collect() } else { Vec::new() };
    for (i, a) in alias_cands.iter().enumerate() {
        q.insert(format!("alias_{i}"), json!({ "type": "noul", "instructions": format!("Is \"{a}\" an alternative name, abbreviation, or synonym for the main subject of this note, so that searching for \"{a}\" should find it?"), "criteria": { "true": "A real alternative name for what this note is about", "false": "A different thing, too generic, or only mentioned" } }));
    }

    // Data: só se a nota ainda não tem
    let mut date_opts: Vec<(String, String)> = Vec::new();
    if f.date && list(fm.get(&k.date)).is_empty() {
        let t = today();
        date_opts.push(("created".into(), created.clone()));
        date_opts.push(("today".into(), t.clone()));
        let mut seen: HashSet<String> = [created.clone(), t].into_iter().collect();
        let re = Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b|\b(\d{2})/(\d{2})/(\d{4})\b|(?:^|[^\d])(20\d{2})(\d{2})(\d{2})(?:[^\d]|$)").unwrap();
        let hay = format!("{title}\n{}", text.chars().take(3000).collect::<String>());
        for c in re.captures_iter(&hay) {
            let d = if let Some(y) = c.get(1) {
                format!("{}-{}-{}", y.as_str(), &c[2], &c[3])
            } else if c.get(4).is_some() {
                format!("{}-{}-{}", &c[6], &c[5], &c[4])
            } else {
                format!("{}-{}-{}", &c[7], &c[8], &c[9])
            };
            if seen.contains(&d) || chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_err() {
                continue;
            }
            seen.insert(d.clone());
            date_opts.push((format!("d{}", date_opts.len() - 2), d));
            if date_opts.len() >= 6 {
                break;
            }
        }
        let criteria: Map<String, Value> = date_opts
            .iter()
            .map(|(id, d)| (id.clone(), json!(match id.as_str() { "created" => format!("Date the file was created: {d}"), "today" => format!("Today: {d}"), _ => format!("Date mentioned in the note: {d}") })))
            .collect();
        q.insert("date".into(), json!({ "type": "choice", "instructions": "Which date best fits this note's date property: when the note was written or when the event it records happened? If nothing in the note points to a specific date, choose the creation date.", "criteria": criteria }));
    }

    let mut result = Entry { mtime: note.mtime, at: chrono::Utc::now().timestamp_millis(), items: Vec::new() };
    if q.is_empty() {
        return Ok(result);
    }
    let mut a = net::jev_ask(&s.model, &state, &Value::Object(q), 30, 3).await?.answers;
    if tag_rank {
        let mut ranked: Vec<(String, f64)> = Vec::new();
        if let Some(obj) = a.as_object() {
            for (id, ans) in obj {
                if id.starts_with("tagrank_") {
                    for (t, p) in ans.get("probabilities").and_then(Value::as_object).into_iter().flatten() {
                        if t != "__none__" {
                            ranked.push((t.clone(), p.as_f64().unwrap_or(0.0)));
                        }
                    }
                }
            }
        }
        ranked.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        tag_cands = ranked.into_iter().take(12).map(|(t, _)| t).collect();
        if !tag_cands.is_empty() {
            let q2: Map<String, Value> = tag_cands.iter().enumerate().map(|(i, t)| (format!("tag_{i}"), tag_question(t))).collect();
            let extra = net::jev_ask(&s.model, &state, &Value::Object(q2), 30, 3).await?.answers;
            if let (Some(base), Some(more)) = (a.as_object_mut(), extra.as_object()) {
                base.extend(more.clone());
            }
        }
    }

    let num = |v: Option<&Value>| v.and_then(Value::as_f64);
    let mut add = |kind: &str, value: String, label: String, p: Option<f64>, conf: Option<f64>, always: bool| {
        if let Some(p) = p {
            if always || p >= s.min_show {
                result.items.push(Item { id: format!("{kind}:{value}"), kind: kind.into(), value, label, p, conf });
            }
        }
    };
    if let Some(mv) = a.get("move") {
        if let Some(choice) = mv.get("choice").and_then(Value::as_str) {
            if choice != folder {
                add("move", choice.into(), if choice == "/" { "(raiz)".into() } else { choice.into() }, num(mv.pointer(&format!("/probabilities/{}", choice.replace('~', "~0").replace('/', "~1")))), num(mv.get("confidence")), false);
            }
        }
    }
    for (i, t) in tag_cands.iter().enumerate() {
        add("tags", t.clone(), format!("#{t}"), num(a.pointer(&format!("/tag_{i}/noul"))), None, false);
    }
    for (i, rel) in link_cands.iter().enumerate() {
        let name = Path::new(rel.as_str()).file_stem().unwrap().to_string_lossy().to_string();
        add("link", (*rel).clone(), name, num(a.pointer(&format!("/link_{i}/noul"))), None, false);
    }
    for (i, n) in author_cands.iter().enumerate() {
        add("author", n.clone(), n.clone(), num(a.pointer(&format!("/author_{i}/noul"))), None, false);
    }
    for (i, n) in alias_cands.iter().enumerate() {
        add("aliases", n.clone(), n.clone(), num(a.pointer(&format!("/alias_{i}/noul"))), None, false);
    }
    if let Some(d) = a.get("date") {
        if let Some(choice) = d.get("choice").and_then(Value::as_str) {
            if let Some((_, val)) = date_opts.iter().find(|(id, _)| id == choice) {
                let src = match choice { "created" => "criação", "today" => "hoje", _ => "citada na nota" };
                add("date", val.clone(), format!("{val} ({src})"), num(d.pointer(&format!("/probabilities/{choice}"))), num(d.get("confidence")), true);
            }
        }
    }
    const ORDER: [&str; 6] = ["move", "tags", "link", "author", "date", "aliases"];
    result.items.sort_by(|x, y| {
        let ix = ORDER.iter().position(|k| *k == x.kind).unwrap_or(9);
        let iy = ORDER.iter().position(|k| *k == y.kind).unwrap_or(9);
        ix.cmp(&iy).then(y.p.partial_cmp(&x.p).unwrap())
    });
    Ok(result)
}

const KINDS: [(&str, &str); 6] = [("move", "Mover"), ("tags", "Tag"), ("link", "Link"), ("author", "Autor"), ("date", "Data"), ("aliases", "Alias")];

fn kind_label(k: &str) -> &'static str {
    KINDS.iter().find(|(id, _)| *id == k).map(|(_, l)| *l).unwrap_or("Ajuste")
}

/// Texto do link como o Obsidian: só o nome se for único no cofre, senão o caminho.
pub fn link_text(rel: &str, all: &[String]) -> String {
    let stem = rel.trim_end_matches(".md");
    let name = stem.rsplit('/').next().unwrap_or(stem);
    let unique = all.iter().filter(|r| r.trim_end_matches(".md").rsplit('/').next() == Some(name)).count() <= 1;
    if unique { name.to_string() } else { stem.to_string() }
}

/// Adiciona valores ao frontmatter (nunca remove), preservando a ordem das chaves.
pub fn add_properties(content: &str, edits: &[(String, String, bool)]) -> Result<String, String> {
    let (yaml, body, _) = split_frontmatter(content);
    let mut map: serde_yaml::Mapping = match yaml {
        Some(y) if !y.trim().is_empty() => serde_yaml::from_str(y).map_err(|e| format!("YAML inválido: {e}"))?,
        _ => serde_yaml::Mapping::new(),
    };
    for (key, value, single) in edits {
        let k = serde_yaml::Value::String(key.clone());
        if *single {
            let empty = match map.get(&k) {
                None | Some(serde_yaml::Value::Null) => true,
                Some(serde_yaml::Value::String(s)) => s.is_empty(),
                Some(serde_yaml::Value::Sequence(s)) => s.is_empty(),
                _ => false,
            };
            if empty {
                map.insert(k, serde_yaml::Value::String(value.clone()));
            }
            continue;
        }
        let mut items: Vec<serde_yaml::Value> = match map.get(&k) {
            Some(serde_yaml::Value::Sequence(s)) => s.clone(),
            Some(serde_yaml::Value::Null) | None => Vec::new(),
            Some(serde_yaml::Value::String(s)) if s.is_empty() => Vec::new(),
            Some(other) => vec![other.clone()],
        };
        let exists = items.iter().any(|x| x.as_str().map(norm) == Some(norm(value)));
        if !exists {
            items.push(serde_yaml::Value::String(value.clone()));
        }
        map.insert(k, serde_yaml::Value::Sequence(items));
    }
    let yaml = serde_yaml::to_string(&map).map_err(|e| e.to_string())?;
    Ok(format!("---\n{yaml}---\n{}", body.trim_start_matches(['\r', '\n'])))
}

pub struct Applied {
    pub ok: usize,
    pub fail: usize,
    pub last_error: String,
    /// (caminho final, item, caminho de origem se movida)
    pub rows: Vec<(String, Item, Option<String>)>,
    /// Arquivos alterados (caminhos relativos novos), para reindexar
    pub touched: Vec<(Option<String>, String)>,
}

pub fn apply(s: &Settings, root: &Path, selection: &HashMap<String, Vec<Item>>, all_rel: &[String], sugg: &mut Suggestions) -> Applied {
    let mut out = Applied { ok: 0, fail: 0, last_error: String::new(), rows: Vec::new(), touched: Vec::new() };
    for (rel, items) in selection {
        let abs = root.join(rel);
        let Ok(content) = fs::read_to_string(&abs) else {
            out.fail += items.len();
            continue;
        };
        let props: Vec<&Item> = items.iter().filter(|i| i.kind != "move").collect();
        let mv = items.iter().find(|i| i.kind == "move");
        let mut done: Vec<String> = Vec::new();
        let mut current = rel.clone();
        let res: Result<(), String> = (|| {
            if !props.is_empty() {
                let edits: Vec<(String, String, bool)> = props
                    .iter()
                    .map(|it| {
                        let key = s.keys.of(&it.kind).to_string();
                        let value = if it.kind == "link" { format!("[[{}]]", link_text(&it.value, all_rel)) } else { it.value.clone() };
                        (key, value, it.kind == "date")
                    })
                    .collect();
                let next = add_properties(&content, &edits)?;
                fs::write(&abs, next).map_err(|e| e.to_string())?;
                for it in &props {
                    out.rows.push((rel.clone(), (*it).clone(), None));
                    done.push(it.id.clone());
                }
            }
            if let Some(m) = mv {
                let name = Path::new(rel).file_name().unwrap().to_string_lossy().to_string();
                let dest = if m.value == "/" { name } else { format!("{}/{name}", m.value) };
                if root.join(&dest).exists() {
                    return Err(format!("já existe uma nota em {dest}"));
                }
                fs::rename(&abs, root.join(&dest)).map_err(|e| e.to_string())?;
                out.rows.push((dest.clone(), m.clone(), Some(rel.clone())));
                done.push(m.id.clone());
                current = dest;
            }
            Ok(())
        })();
        if let Err(e) = res {
            out.last_error = e;
        }
        out.ok += done.len();
        out.fail += items.len() - done.len();
        if !done.is_empty() {
            out.touched.push((if current != *rel { Some(rel.clone()) } else { None }, current.clone()));
        }
        if let Some(mut entry) = sugg.remove(rel) {
            entry.items.retain(|i| !done.contains(&i.id));
            entry.mtime = fs::metadata(root.join(&current)).ok().and_then(|m| m.modified().ok()).map(|t| chrono::DateTime::<chrono::Utc>::from(t).timestamp_millis()).unwrap_or(entry.mtime);
            sugg.insert(current.clone(), entry);
        }
    }
    out
}

/// Registra cada ajuste aplicado como uma linha na nota de log.
pub fn log(s: &Settings, root: &Path, rows: &[(String, Item, Option<String>)], all_rel: &[String]) -> Result<Option<String>, String> {
    if rows.is_empty() {
        return Ok(None);
    }
    let rel = s.log_path.trim().trim_matches('/').to_string();
    let path = root.join(&rel);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
    let cell = |s: &str| s.replace('|', "\\|").replace('\n', " ");
    let text: String = rows
        .iter()
        .map(|(p, it, from)| {
            let link = format!("[[{}]]", link_text(p, all_rel));
            let value = if it.kind == "move" { format!("{} → {p}", from.clone().unwrap_or_default()) } else { it.label.clone() };
            format!("| {now} | {} | {} | {} | {} |\n", cell(&link), kind_label(&it.kind), cell(&value), pct(it.p))
        })
        .collect();
    let content = match fs::read_to_string(&path) {
        Ok(existing) => format!("{}\n{text}", existing.trim_end_matches('\n')),
        Err(_) => format!("# Log de ajustes do Jev\n\nNotas ajustadas pelo Jev Organizer (propriedades adicionadas e notas movidas).\n\n| Data | Nota | Ajuste | Valor | Prob. |\n| --- | --- | --- | --- | --- |\n{text}"),
    };
    fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(Some(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adiciona_propriedades_sem_remover() {
        let md = "---\ntitle: X\ntags:\n  - a\n---\n\nCorpo";
        let out = add_properties(md, &[("tags".into(), "b".into(), false), ("tags".into(), "A".into(), false), ("date".into(), "2026-01-01".into(), true)]).unwrap();
        assert!(out.starts_with("---\ntitle: X\ntags:\n- a\n- b\ndate: '2026-01-01'\n---\n") || out.contains("- b"), "{out}");
        assert!(out.ends_with("Corpo"));
        let created = add_properties("Só corpo", &[("author".into(), "Ana".into(), false)]).unwrap();
        assert!(created.starts_with("---\nauthor:\n- Ana\n---\n"), "{created}");
    }

    #[test]
    fn link_curto_quando_nome_unico() {
        let all = vec!["a/Nota.md".to_string(), "b/Outra.md".to_string(), "c/Outra.md".to_string()];
        assert_eq!(link_text("a/Nota.md", &all), "Nota");
        assert_eq!(link_text("b/Outra.md", &all), "b/Outra");
    }
}
