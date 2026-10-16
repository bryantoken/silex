//! Mesa — núcleo em Rust da reescrita do plugin "Mesa" do Obsidian.
//!
//! Mesa de projetos com tarefas, notas e sugestões de IA, com dados em Markdown no
//! cofre (mesmo formato do plugin original, então os dois convivem):
//!   <raiz>/Projetos, Tarefas, Dias, Sugestoes/<uuid>.md   registros com frontmatter YAML
//!   <raiz>/Historico/<uuid>.json                         antes/depois de cada alteração (desfazer)
//!   <raiz>/Uso/AAAA-MM-<dispositivo>.jsonl              consumo de IA por chamada
//!   <raiz>/Core/*.md                                     contexto para a IA
//! Aqui ficam arquivos, YAML, histórico, consumo, contexto e chamadas de IA (com as
//! chaves no cofre do sistema). A lógica de tela fica no frontend.

use crate::net;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Settings {
    pub root: String,
    pub provider: String,
    pub model: String,
    pub tone: String,
    pub auto: bool,
    pub daily: bool,
    pub quick: bool,
    pub capacity: u32,
    pub sweep: bool,
    #[serde(rename = "sweepMax")]
    pub sweep_max: u32,
    pub limit: u32,
    pub jev: bool,
    #[serde(rename = "jevModel")]
    pub jev_model: String,
    #[serde(rename = "jevMin")]
    pub jev_min: f64,
    pub memory: String,
    pub interests: String,
    pub preferences: String,
    #[serde(rename = "priceIn")]
    pub price_in: f64,
    #[serde(rename = "priceOut")]
    pub price_out: f64,
    #[serde(rename = "jevPrice")]
    pub jev_price: f64,
    pub fx: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            root: "Mesa".into(),
            provider: "openai".into(),
            model: String::new(),
            tone: "Direto, conciso e concreto. Preserve minha intenção.".into(),
            auto: false,
            daily: false,
            quick: true,
            capacity: 5,
            sweep: false,
            sweep_max: 3,
            limit: 20,
            jev: true,
            jev_model: "jev-1.13.0".into(),
            jev_min: 0.5,
            memory: "Mesa/Core/Memoria.md".into(),
            interests: "Mesa/Core/Interesses.md".into(),
            preferences: "Mesa/Core/Preferencias.md".into(),
            price_in: 0.0,
            price_out: 0.0,
            jev_price: 0.0,
            fx: 0.0,
        }
    }
}

static FM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)").unwrap());

/// Caminho visível e relativo dentro do cofre (sem "..", "." ou pastas ocultas).
pub fn safe_path(p: &str) -> Result<String, String> {
    let p = p.trim().replace('\\', "/").trim_end_matches('/').to_string();
    if p.is_empty() || p.split('/').any(|s| s.is_empty() || s == ".." || s == "." || s.starts_with('.')) {
        return Err("Use um caminho visível dentro do cofre.".into());
    }
    Ok(p)
}

fn under(path: &str, root: &str) -> bool {
    path == root || path.starts_with(&format!("{root}/"))
}

#[derive(Serialize, Debug, Clone)]
pub struct Record {
    pub path: String,
    pub raw: String,
    pub data: Value,
    pub body: String,
}

pub fn parse_record(path: &str, raw: &str) -> Result<Record, String> {
    let m = FM.captures(raw).ok_or_else(|| format!("Nota sem metadados Mesa: {path}"))?;
    let yaml: serde_yaml::Value = serde_yaml::from_str(&m[1]).map_err(|_| format!("YAML inválido: {path}"))?;
    let data: Value = serde_json::to_value(&yaml).map_err(|e| e.to_string())?;
    let kind = data.get("kind").and_then(Value::as_str).unwrap_or("");
    if data.get("id").is_none() || !["task", "project", "day", "suggestion"].contains(&kind) {
        return Err(format!("Registro inválido: {path}"));
    }
    Ok(Record { path: path.into(), raw: raw.into(), data, body: raw[m.get(0).unwrap().end()..].to_string() })
}

fn to_yaml(v: &Value) -> Result<String, String> {
    serde_yaml::to_string(v).map_err(|e| e.to_string())
}

fn encode(data: &Value, body: &str) -> Result<String, String> {
    Ok(format!("---\n{}---\n{body}", to_yaml(data)?))
}

/// Aplica `patch` ao frontmatter preservando a ordem das chaves existentes.
fn patch_note(rec: &Record, patch: &Map<String, Value>, body: &str) -> Result<String, String> {
    let m = FM.captures(&rec.raw).ok_or("Registro sem frontmatter")?;
    let mut map: serde_yaml::Mapping = serde_yaml::from_str(&m[1]).map_err(|e| e.to_string())?;
    for (k, v) in patch {
        let yv: serde_yaml::Value = serde_yaml::to_value(v).map_err(|e| e.to_string())?;
        map.insert(serde_yaml::Value::String(k.clone()), yv);
    }
    Ok(format!("---\n{}---\n{body}", serde_yaml::to_string(&map).map_err(|e| e.to_string())?))
}

pub fn uid() -> String {
    // UUID v4 a partir de bytes aleatórios do sistema
    let mut b = [0u8; 16];
    getrandom(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn getrandom(buf: &mut [u8]) {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    for chunk in buf.chunks_mut(8) {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let v = h.finish().to_le_bytes();
        chunk.copy_from_slice(&v[..chunk.len()]);
    }
}

fn now_iso() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

pub struct Store<'a> {
    pub vault: &'a Path,
    pub root: String,
}

impl Store<'_> {
    fn abs(&self, rel: &str) -> PathBuf {
        self.vault.join(rel)
    }

    pub fn init(&self) -> Result<(), String> {
        for f in ["Core", "Projetos", "Tarefas", "Dias", "Sugestoes", "Historico", "Uso"] {
            fs::create_dir_all(self.abs(&format!("{}/{f}", self.root))).map_err(|e| e.to_string())?;
        }
        for (name, body) in [
            ("Memoria", "Contexto pessoal e profissional relevante."),
            ("Interesses", "Interesses que ajudam a contextualizar seus projetos."),
            ("Preferencias", "Como você prefere receber sugestões."),
            ("Enriquecimento", "Preserve a intenção original. Sugira um próximo passo executável. Liste dúvidas sem inventar respostas."),
            ("Planejamento", "Sugira uma tarefa principal e até duas alternativas, considerando bloqueios e datas."),
        ] {
            let p = self.abs(&format!("{}/Core/{name}.md", self.root));
            if !p.exists() {
                fs::write(p, format!("# {name}\n\n{body}\n")).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }

    pub fn all(&self, kind: Option<&str>) -> Vec<Record> {
        let mut out = Vec::new();
        for dir in ["Projetos", "Tarefas", "Dias", "Sugestoes"] {
            let base = format!("{}/{dir}", self.root);
            let mut stack = vec![self.abs(&base)];
            while let Some(d) = stack.pop() {
                for e in fs::read_dir(&d).into_iter().flatten().flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                        continue;
                    }
                    if p.extension().and_then(|x| x.to_str()) != Some("md") {
                        continue;
                    }
                    let rel = p.strip_prefix(self.vault).unwrap().to_string_lossy().replace('\\', "/");
                    if let Ok(raw) = fs::read_to_string(&p) {
                        if let Ok(r) = parse_record(&rel, &raw) {
                            if kind.map_or(true, |k| r.data.get("kind").and_then(Value::as_str) == Some(k)) {
                                out.push(r);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    pub fn create(&self, kind: &str, title: &str, extra: &Map<String, Value>, body: &str) -> Result<Record, String> {
        let id = uid();
        let dir = match kind {
            "task" => "Tarefas",
            "project" => "Projetos",
            "day" => "Dias",
            "suggestion" => "Sugestoes",
            _ => return Err("Tipo inválido".into()),
        };
        let rel = format!("{}/{dir}/{id}.md", self.root);
        let mut data = Map::new();
        data.insert("id".into(), json!(id));
        data.insert("kind".into(), json!(kind));
        data.insert("title".into(), json!(title));
        for (k, v) in extra {
            data.insert(k.clone(), v.clone());
        }
        data.insert("created".into(), json!(now_iso()));
        data.insert("revision".into(), json!(0));
        let raw = encode(&Value::Object(data), body)?;
        fs::create_dir_all(self.abs(&format!("{}/{dir}", self.root))).map_err(|e| e.to_string())?;
        fs::write(self.abs(&rel), &raw).map_err(|e| e.to_string())?;
        parse_record(&rel, &raw)
    }

    /// Atualiza com controle de concorrência: só grava se o arquivo ainda for `base_raw`.
    /// Guarda antes/depois no Histórico para desfazer.
    pub fn update(&self, rel: &str, base_raw: &str, patch: &Map<String, Value>, body: Option<&str>) -> Result<Record, String> {
        if !under(rel, &self.root) {
            return Err("Escrita fora da Mesa recusada.".into());
        }
        let current = fs::read_to_string(self.abs(rel)).map_err(|_| "Arquivo não encontrado.".to_string())?;
        if current != base_raw {
            return Err("A nota mudou. Reabra para preservar a edição mais recente.".into());
        }
        let rec = parse_record(rel, &current)?;
        let mut p = patch.clone();
        p.insert("revision".into(), json!(rec.data.get("revision").and_then(Value::as_i64).unwrap_or(0) + 1));
        p.insert("updated".into(), json!(now_iso()));
        let after = patch_note(&rec, &p, body.unwrap_or(&rec.body))?;
        let journal = format!("{}/Historico/{}.json", self.root, uid());
        fs::write(self.abs(&journal), serde_json::to_vec(&json!({ "path": rel, "before": current, "after": after, "at": now_iso() })).unwrap()).map_err(|e| e.to_string())?;
        fs::write(self.abs(rel), &after).map_err(|e| e.to_string())?;
        parse_record(rel, &after)
    }

    pub fn undo(&self, journal_rel: &str) -> Result<(), String> {
        let j: Value = serde_json::from_slice(&fs::read(self.abs(journal_rel)).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let path = j["path"].as_str().ok_or("Destino inválido")?;
        if !under(path, &self.root) || under(path, &format!("{}/Historico", self.root)) {
            return Err("Destino inválido".into());
        }
        let current = fs::read_to_string(self.abs(path)).map_err(|e| e.to_string())?;
        if current != j["after"].as_str().unwrap_or("") {
            return Err("A nota mudou depois desta alteração; não dá para desfazer.".into());
        }
        fs::write(self.abs(path), j["before"].as_str().unwrap_or("")).map_err(|e| e.to_string())
    }

    pub fn history(&self, limit: usize) -> (Vec<Value>, usize) {
        let dir = self.abs(&format!("{}/Historico", self.root));
        let mut files: Vec<(std::time::SystemTime, PathBuf)> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
            .filter_map(|p| Some((fs::metadata(&p).ok()?.modified().ok()?, p)))
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0));
        let total = files.len();
        let rows = files
            .into_iter()
            .take(limit)
            .filter_map(|(mtime, p)| {
                let mut j: Value = serde_json::from_slice(&fs::read(&p).ok()?).ok()?;
                j["file"] = json!(p.strip_prefix(self.vault).unwrap().to_string_lossy().replace('\\', "/"));
                if j.get("at").is_none() {
                    j["at"] = json!(chrono::DateTime::<chrono::Utc>::from(mtime).to_rfc3339());
                }
                Some(j)
            })
            .collect();
        (rows, total)
    }

    pub fn read(&self, rel: &str) -> Result<String, String> {
        fs::read_to_string(self.abs(&safe_path(rel)?)).map_err(|e| e.to_string())
    }
}

// ---------- Contexto para a IA ----------

fn compact_markdown(md: &str) -> String {
    static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^(```|~~~)[^\n]*\n[\s\S]*?^(```|~~~)[^\n]*$").unwrap());
    let s = FM.replace(md, "").replace("\r\n", "\n");
    let s = CODE.replace_all(&s, |c: &regex::Captures| format!("[bloco de código omitido, {} linhas]", c[0].lines().count()));
    let s = Regex::new(r"(?s)<!--.*?-->").unwrap().replace_all(&s, "");
    let s = Regex::new(r"(?s)%%.*?%%").unwrap().replace_all(&s, "");
    let s = Regex::new(r"!\[\[[^\]]*\]\]").unwrap().replace_all(&s, "");
    let s = Regex::new(r"!\[[^\]]*\]\([^)]*\)").unwrap().replace_all(&s, "");
    let s = Regex::new(r"\[([^\]]+)\]\((?:https?:)?[^)]*\)").unwrap().replace_all(&s, "$1");
    let s = Regex::new(r"(?m)[ \t]+$").unwrap().replace_all(&s, "");
    Regex::new(r"\n{3,}").unwrap().replace_all(&s, "\n\n").trim().to_string()
}

fn truncate_markdown(text: &str, cap: usize) -> String {
    if text.chars().count() <= cap {
        return text.to_string();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let mut head = String::new();
    let mut i = 0;
    while i < lines.len() && head.chars().count() + lines[i].chars().count() + 1 <= cap * 8 / 10 {
        head.push_str(lines[i]);
        head.push('\n');
        i += 1;
    }
    let rest: Vec<String> = lines[i..].iter().filter(|l| Regex::new(r"^#{1,4} ").unwrap().is_match(l)).map(|l| l.trim_start_matches('#').trim().to_string()).collect();
    let mut tail = format!("\n[… {} caracteres omitidos", text.chars().count() - head.chars().count());
    if !rest.is_empty() {
        let mut list = String::new();
        for h in rest {
            if head.len() + tail.len() + list.len() + h.len() + 6 > cap {
                break;
            }
            if !list.is_empty() {
                list.push_str("; ");
            }
            list.push_str(&h);
        }
        if !list.is_empty() {
            tail.push_str(&format!(". Seções: {list}"));
        }
    }
    format!("{head}{tail}]").chars().take(cap.max(200)).collect()
}

/// Contexto com orçamento: notas Core sempre; notas de projeto só dos projetos em foco
/// (ou de todos), compactadas: as curtas entram inteiras, as longas dividem o resto.
pub fn context(store: &Store, s: &Settings, projects: &[Value], focus: Option<&[String]>, budget: usize) -> Result<Map<String, Value>, String> {
    let core = [s.memory.clone(), s.interests.clone(), s.preferences.clone(), format!("{}/Core/Enriquecimento.md", store.root), format!("{}/Core/Planejamento.md", store.root)];
    let mut linked: Vec<String> = Vec::new();
    for p in projects {
        let id = p["id"].as_str().unwrap_or("");
        if p["ai"] == json!(true) && focus.map_or(true, |f| f.iter().any(|x| x == id)) {
            for n in p["notes"].as_array().into_iter().flatten() {
                if let Some(n) = n.as_str() {
                    linked.push(n.to_string());
                }
            }
        }
    }
    let read = |raw: &str| -> Result<Option<(String, String)>, String> {
        if raw.trim().is_empty() {
            return Ok(None);
        }
        let p = safe_path(raw)?;
        if !p.ends_with(".md") {
            return Err("Contexto aceita apenas Markdown.".into());
        }
        Ok(fs::read_to_string(store.vault.join(&p)).ok().map(|t| (p, compact_markdown(&t))))
    };
    let mut notes = Map::new();
    let mut left = budget as i64;
    let mut seen = std::collections::HashSet::new();
    for raw in &core {
        if !seen.insert(raw.clone()) {
            continue;
        }
        if let Some((p, t)) = read(raw)? {
            let t = truncate_markdown(&t, 4000);
            left -= t.chars().count() as i64;
            notes.insert(p, json!(t));
        }
    }
    let mut docs = Vec::new();
    for raw in linked.iter().filter(|p| !core.contains(p)) {
        if seen.insert(raw.clone()) {
            if let Some(d) = read(raw)? {
                docs.push(d);
            }
        }
    }
    docs.sort_by_key(|(_, t)| t.chars().count());
    let n = docs.len();
    for (i, (p, t)) in docs.into_iter().enumerate() {
        let cap = 600.max((left.max(0) as usize) / (n - i));
        let t = truncate_markdown(&t, cap);
        left -= t.chars().count() as i64;
        notes.insert(p, json!(t));
    }
    Ok(notes)
}

// ---------- Consumo e limites ----------

/// Estado local deste dispositivo (fora das notas, como o localStorage do original).
pub fn local_path(app_dir: &Path, vault: &Path) -> PathBuf {
    let mut h: u64 = 1469598103934665603;
    for b in vault.to_string_lossy().bytes() {
        h = (h ^ b as u64).wrapping_mul(1099511628211);
    }
    app_dir.join("mesa").join(format!("{h:016x}.json"))
}

pub fn local_get(app_dir: &Path, vault: &Path) -> Map<String, Value> {
    fs::read(local_path(app_dir, vault)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn local_set(app_dir: &Path, vault: &Path, key: &str, value: Value) -> Result<(), String> {
    let mut m = local_get(app_dir, vault);
    if value.is_null() {
        m.remove(key);
    } else {
        m.insert(key.into(), value);
    }
    let p = local_path(app_dir, vault);
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(p, serde_json::to_vec(&m).unwrap()).map_err(|e| e.to_string())
}

fn device(app_dir: &Path, vault: &Path) -> String {
    if let Some(d) = local_get(app_dir, vault).get("device").and_then(Value::as_str) {
        return d.to_string();
    }
    let d = uid()[..8].to_string();
    let _ = local_set(app_dir, vault, "device", json!(d));
    d
}

pub fn usage_add(store: &Store, app_dir: &Path, mut row: Map<String, Value>) {
    row.insert("at".into(), json!(now_iso()));
    let dir = store.vault.join(format!("{}/Uso", store.root));
    let _ = fs::create_dir_all(&dir);
    let file = dir.join(format!("{}-{}.jsonl", chrono::Local::now().format("%Y-%m"), device(app_dir, store.vault)));
    use std::io::Write;
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(file) {
        let _ = writeln!(f, "{}", Value::Object(row));
    }
}

pub fn usage_load(store: &Store) -> Vec<Value> {
    let dir = store.vault.join(format!("{}/Uso", store.root));
    let mut rows: Vec<Value> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("jsonl"))
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .flat_map(|t| t.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect::<Vec<_>>())
        .filter(|r| r.get("at").is_some() && r.get("engine").is_some())
        .collect();
    rows.sort_by(|a, b| a["at"].as_str().unwrap_or("").cmp(b["at"].as_str().unwrap_or("")));
    rows
}

/// Conta a chamada no limite diário deste dispositivo (erro se já atingiu).
pub fn take_daily_call(app_dir: &Path, vault: &Path, limit: u32) -> Result<(), String> {
    let key = format!("usage-{}", chrono::Local::now().format("%Y-%m-%d"));
    let used = local_get(app_dir, vault).get(&key).and_then(Value::as_u64).unwrap_or(0);
    if used >= limit as u64 {
        return Err("Limite diário de chamadas atingido neste dispositivo.".into());
    }
    local_set(app_dir, vault, &key, json!(used + 1))
}

/// Divide o custo de uma chamada entre os projetos das tarefas envolvidas.
pub fn project_split(tasks: &[Value]) -> Value {
    if tasks.is_empty() {
        return json!({ "": 1 });
    }
    let mut out = Map::new();
    for t in tasks {
        let k = t["project"].as_str().unwrap_or("").to_string();
        let v = out.get(&k).and_then(Value::as_f64).unwrap_or(0.0) + 1.0 / tasks.len() as f64;
        out.insert(k, json!(v));
    }
    Value::Object(out)
}

pub async fn ai_call(store: &Store<'_>, app_dir: &Path, s: &Settings, system: &str, input: &Value, purpose: &str, task: Option<&str>, tasks: &[Value]) -> Result<Value, String> {
    take_daily_call(app_dir, store.vault, s.limit)?;
    let r = net::llm_call(&s.provider, &s.model, system, input).await?;
    let mut row = Map::new();
    row.insert("engine".into(), json!(if s.provider == "anthropic" { "claude" } else { "openai" }));
    row.insert("model".into(), json!(r.model));
    row.insert("purpose".into(), json!(purpose));
    row.insert("task".into(), json!(task));
    row.insert("proj".into(), project_split(tasks));
    row.insert("in".into(), json!(r.tokens_in));
    row.insert("out".into(), json!(r.tokens_out));
    row.insert("cw".into(), json!(r.cache_write));
    row.insert("cr".into(), json!(r.cache_read));
    usage_add(store, app_dir, row);
    Ok(r.value)
}

pub async fn jev_call(store: &Store<'_>, app_dir: &Path, s: &Settings, state: &Value, questions: &Value, purpose: &str, tasks: &[Value]) -> Result<Value, String> {
    let r = net::jev_ask(&s.jev_model, state, questions, 20, 2).await?;
    let mut row = Map::new();
    row.insert("engine".into(), json!("jev"));
    row.insert("model".into(), json!(r.model.unwrap_or_else(|| s.jev_model.clone())));
    row.insert("purpose".into(), json!(purpose));
    row.insert("task".into(), Value::Null);
    row.insert("proj".into(), project_split(tasks));
    row.insert("q".into(), json!(questions.as_object().map_or(0, |q| q.len())));
    row.insert("cost".into(), json!(r.cost));
    row.insert("in".into(), json!(r.tokens_in));
    row.insert("out".into(), json!(r.tokens_out));
    usage_add(store, app_dir, row);
    Ok(r.answers)
}

/// Lê/grava a configuração espelhada em <raiz>/Core/Configuracao.md (compatível com o original).
pub fn config_note_read(vault: &Path, root: &str) -> Option<Map<String, Value>> {
    let raw = fs::read_to_string(vault.join(format!("{root}/Core/Configuracao.md"))).ok()?;
    let m = Regex::new(r"```json\s*([\s\S]*?)```").unwrap().captures(&raw)?;
    serde_json::from_str(&m[1]).ok()
}

pub fn config_note_write(vault: &Path, s: &Settings) -> Result<(), String> {
    let raw = format!(
        "# Configuração da Mesa\n\nEdite o JSON e reinicie o plugin. Não coloque chaves aqui.\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(s).unwrap()
    );
    let p = vault.join(format!("{}/Core/Configuracao.md", s.root));
    fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::write(p, raw).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cria_atualiza_desfaz() {
        let dir = std::env::temp_dir().join(format!("silex-mesa-{}", std::process::id()));
        let store = Store { vault: &dir, root: "Mesa".into() };
        store.init().unwrap();
        let mut extra = Map::new();
        extra.insert("state".into(), json!("pending"));
        extra.insert("project".into(), Value::Null);
        let t = store.create("task", "Ligar para o cliente", &extra, "## Captura original\n\nLigar\n").unwrap();
        assert_eq!(t.data["kind"], "task");
        assert_eq!(store.all(Some("task")).len(), 1);

        let mut patch = Map::new();
        patch.insert("state".into(), json!("done"));
        let t2 = store.update(&t.path, &t.raw, &patch, None).unwrap();
        assert_eq!(t2.data["state"], "done");
        assert_eq!(t2.data["revision"], 1);
        assert!(t2.raw.find("id:").unwrap() < t2.raw.find("state:").unwrap(), "ordem das chaves preservada");
        assert!(store.update(&t.path, &t.raw, &patch, None).is_err(), "base antiga é recusada");

        let (hist, total) = store.history(10);
        assert_eq!(total, 1);
        store.undo(hist[0]["file"].as_str().unwrap()).unwrap();
        assert_eq!(store.all(Some("task"))[0].data["state"], "pending");
        assert!(safe_path("../fora").is_err() && safe_path(".obsidian/x").is_err());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn trunca_com_secoes() {
        let text = (0..200).map(|i| if i % 50 == 0 { format!("## Seção {i}") } else { "linha de texto comum".into() }).collect::<Vec<_>>().join("\n");
        let t = truncate_markdown(&text, 600);
        assert!(t.chars().count() <= 600 && t.contains("caracteres omitidos") && t.contains("Seção"));
    }
}
