//! Jev Home — reescrita em Rust do plugin "Jev Home" do Obsidian.
//!
//! Página inicial com uma barra de busca: você descreve a nota com suas palavras e
//! o Jev (TypeSafe) diz qual nota você procura, com a probabilidade de acerto.
//! O código escolhe as candidatas (índice local por palavras com peso por campo +
//! opcionalmente uma passada do Jev pelos títulos de todas as notas); o Jev só
//! escolhe entre notas que existem. Nenhuma nota é alterada.

use crate::net;
use crate::pages::Page;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;
use std::time::Instant;
use unicode_normalization::UnicodeNormalization;

const FIND: &str = "The user typed `query` into the search box on the home page of their Obsidian vault to find one specific note. The query is free text: a partial or misspelled title, a paraphrase, a topic, a person, a project, or a detail remembered from the content, often in Portuguese. Which note are they looking for? Judge by meaning, not only by shared words. `recently_opened` lists the notes the user opened most recently; use it only if the query points to something recent. Choose __none__ if no note plausibly matches.";
const BROAD: &str = "The user typed `query` into the search box on the home page of their Obsidian vault to find one specific note. Here you only see note paths (folder/title). Which of these notes is most likely the one they want? Judge by meaning, not only by shared words. Choose __none__ if none of these could be it.";

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub model: String,
    pub exclude: Vec<String>,
    pub new_tab: bool,
    pub on_startup: bool,
    pub open_in: String,
    pub auto_ask: bool,
    pub auto_delay: u64,
    pub broad: bool,
    pub local_limit: usize,
    pub final_limit: usize,
    pub per_chunk: usize,
    pub excerpt_chars: usize,
    pub index_chars: usize,
    pub max_similar: usize,
    pub min_similar: f64,
    pub high: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: "jev-1.13.0".into(),
            exclude: Vec::new(),
            new_tab: true,
            on_startup: false,
            open_in: "same".into(),
            auto_ask: false,
            auto_delay: 900,
            broad: true,
            local_limit: 40,
            final_limit: 40,
            per_chunk: 4,
            excerpt_chars: 280,
            index_chars: 4000,
            max_similar: 8,
            min_similar: 0.01,
            high: 0.8,
        }
    }
}

pub fn fold(s: &str) -> String {
    s.nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect::<String>().to_lowercase()
}

fn words(s: &str) -> Vec<String> {
    fold(s)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.chars().count() > 1 || t.chars().any(|c| c.is_ascii_digit()))
        .filter(|t| !t.is_empty())
        .map(String::from)
        .collect()
}

static STOP: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    "a o as os um uma uns umas de da do das dos em no na nos nas num numa por pelo pela para pra pro com sem sobre \
     que qual quais quem onde quando como e ou se ao aos eu meu minha meus minhas seu sua isso isto esse essa este esta aquele aquela aquilo \
     nota notas arquivo the an of and or to in on at for with about from my is it this that which note notes"
        .split(' ')
        .collect()
});

pub fn tokens(q: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let all: Vec<String> = words(q).into_iter().filter(|w| seen.insert(w.clone())).collect();
    let kept: Vec<String> = all.iter().filter(|t| !STOP.contains(t.as_str())).cloned().collect();
    if kept.is_empty() { all } else { kept }
}


macro_rules! re {
    ($name:ident, $pat:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pat).unwrap());
    };
}
re!(RE_CODE, r"(?s)```.*?(```|$)");
re!(RE_COMMENT, r"(?s)%%.*?(%%|$)");
re!(RE_HTML, r"<[^>]+>");
re!(RE_EMBED, r"!\[\[[^\]]*\]\]");
re!(RE_IMG, r"!\[[^\]]*\]\([^)]*\)");
re!(RE_WIKI, r"\[\[([^\]|#]*)(?:#[^\]|]*)?(?:\|([^\]]*))?\]\]");
re!(RE_LINK, r"\[([^\]]*)\]\([^)]*\)");
re!(RE_HEAD, r"(?m)^[ \t]{0,3}#{1,6}[ \t]+");
re!(RE_LIST, r"(?m)^[ \t]*(?:[-*+]|\d+[.)])[ \t]+(?:\[.\][ \t]+)?");
re!(RE_QUOTE, r"(?m)^[ \t]*>[ \t]?(?:\[![^\]]*\][-+]?)?");
re!(RE_MARKS, r"[*_`~=|]+");
re!(RE_WS, r"\s+");

/// Tira a marcação do Markdown para indexar e mandar trechos legíveis ao Jev.
pub fn clean_text(s: &str) -> String {
    let s = RE_CODE.replace_all(s, " ");
    let s = RE_COMMENT.replace_all(&s, " ");
    let s = RE_HTML.replace_all(&s, " ");
    let s = RE_EMBED.replace_all(&s, " ");
    let s = RE_IMG.replace_all(&s, " ");
    let s = RE_WIKI.replace_all(&s, |c: &regex::Captures| {
        c.get(2).filter(|m| !m.as_str().is_empty()).or(c.get(1)).map(|m| m.as_str().to_string()).unwrap_or_default()
    });
    let s = RE_LINK.replace_all(&s, "$1");
    let s = RE_HEAD.replace_all(&s, "");
    let s = RE_LIST.replace_all(&s, "");
    let s = RE_QUOTE.replace_all(&s, "");
    let s = RE_MARKS.replace_all(&s, " ");
    RE_WS.replace_all(&s, " ").trim().to_string()
}

fn as_list(v: Option<&Value>) -> Vec<String> {
    match v {
        Some(Value::Array(a)) => a.iter().filter_map(|x| match x {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }).collect(),
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Number(n)) => vec![n.to_string()],
        Some(Value::Bool(b)) => vec![b.to_string()],
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone)]
pub struct Doc {
    pub abs: String,
    pub path: String,
    pub title: String,
    pub title_fold: String,
    pub alias_fold: Vec<String>,
    pub folder: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    pub headings: Vec<String>,
    pub props: String,
    pub mtime: i64,
    pub body: String,
    pub terms: HashMap<String, f64>,
}

pub fn make_doc(page: &Page, rel: &str, content: &str, mtime: i64, index_chars: usize) -> Doc {
    let fm = page.frontmatter.as_object().cloned().unwrap_or_default();
    let aliases = as_list(fm.get("aliases").or(fm.get("alias")));
    let title_fold = fold(&page.name);
    let headings: Vec<String> = page.headings.iter().map(|h| h.text.clone()).filter(|h| fold(h) != title_fold).collect();
    let skip = Regex::new(r"(?i)^(position|aliases|alias|tags|tag|cssclass(es)?)$").unwrap();
    let props: String = fm
        .iter()
        .filter(|(k, _)| !skip.is_match(k))
        .map(|(k, v)| (k, as_list(Some(v)).join(", ")))
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join("; ")
        .chars()
        .take(200)
        .collect();
    let (_, raw_body, _) = crate::pages::split_frontmatter(content);
    let head: String = raw_body.chars().take(index_chars * 2).collect();
    let cleaned = clean_text(&head);
    let title_re = Regex::new(&format!(r"(?i)^{}\s*", regex::escape(&page.name))).unwrap();
    let body: String = title_re.replace(&cleaned, "").chars().take(index_chars).collect();
    let folder = Path::new(rel).parent().map(|p| p.to_string_lossy().replace('\\', "/")).filter(|s| !s.is_empty()).unwrap_or_else(|| "/".into());

    let mut terms: HashMap<String, f64> = HashMap::new();
    let mut add = |w: f64, text: &str| {
        for t in words(text).into_iter().collect::<HashSet<_>>() {
            *terms.entry(t).or_default() += w;
        }
    };
    add(6.0, &page.name);
    add(5.0, &aliases.join(" "));
    add(3.0, &headings.join(" "));
    add(3.0, &page.tags.join(" "));
    add(2.0, &props);
    add(2.0, if folder == "/" { "" } else { &folder });
    add(1.0, &body);

    Doc {
        abs: page.path.clone(),
        path: rel.to_string(),
        title: page.name.clone(),
        title_fold,
        alias_fold: aliases.iter().map(|a| fold(a)).collect(),
        folder,
        aliases,
        tags: page.tags.clone(),
        headings,
        props,
        mtime,
        body,
        terms,
    }
}

/// Índice invertido: termo → (documento → peso).
#[derive(Default)]
pub struct Index {
    pub docs: Vec<Doc>,
    post: HashMap<String, Vec<(usize, f64)>>,
    vocab: Vec<String>,
    pub version: u64,
    cache: Vec<(String, Value)>,
}

impl Index {
    pub fn new(docs: Vec<Doc>, version: u64) -> Self {
        let mut post: HashMap<String, Vec<(usize, f64)>> = HashMap::new();
        for (i, d) in docs.iter().enumerate() {
            for (t, w) in &d.terms {
                post.entry(t.clone()).or_default().push((i, *w));
            }
        }
        let vocab = post.keys().cloned().collect();
        Self { docs, post, vocab, version, cache: Vec::new() }
    }


    /// Busca por palavras (sem acento, com prefixo, peso por campo e raridade).
    pub fn local_search(&self, query: &str, limit: usize) -> Vec<usize> {
        let qt = tokens(query);
        let n = self.docs.len();
        if qt.is_empty() || n == 0 {
            return Vec::new();
        }
        let mut score = vec![0.0f64; n];
        let mut hits = vec![0u32; n];
        for t in &qt {
            let mut best: HashMap<usize, f64> = HashMap::new();
            let mut consider = |term: &str, factor: f64| {
                if let Some(m) = self.post.get(term) {
                    let idf = (1.0 + n as f64 / m.len() as f64).ln();
                    for (i, w) in m {
                        let v = w * idf * factor;
                        let e = best.entry(*i).or_default();
                        if v > *e {
                            *e = v;
                        }
                    }
                }
            };
            consider(t, 1.0);
            if t.chars().count() >= 2 {
                for term in &self.vocab {
                    if term != t && term.starts_with(t.as_str()) {
                        consider(term, 0.6);
                    }
                }
            }
            for (i, v) in best {
                score[i] += v;
                hits[i] += 1;
            }
        }
        let qf = fold(query).split_whitespace().collect::<Vec<_>>().join(" ");
        let qf_len = qf.chars().count();
        let mut out: Vec<(usize, f64)> = Vec::new();
        for (i, d) in self.docs.iter().enumerate() {
            let mut s = score[i];
            if s > 0.0 {
                s *= 0.4 + 0.6 * (hits[i] as f64 / qt.len() as f64);
            }
            if qf_len >= 3 && d.title_fold.contains(&qf) {
                s += if d.title_fold == qf { 16.0 } else { 8.0 };
            } else if qf_len >= 3 && d.alias_fold.iter().any(|a| a.contains(&qf)) {
                s += 6.0;
            } else if qt.len() == 1 && qf_len >= 3 && subsequence(&qf, &d.title_fold) {
                s += 1.5;
            }
            if s > 0.0 {
                out.push((i, s));
            }
        }
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(self.docs[b.0].mtime.cmp(&self.docs[a.0].mtime)));
        out.into_iter().take(limit).map(|(i, _)| i).collect()
    }

}

/// Descrição de uma finalista para o Jev: título, pasta, aliases, tags, seções e começo do texto.
fn describe(d: &Doc, excerpt_chars: usize) -> String {
    {
        let mut lines = vec![format!("Title: {}", d.title)];
        if d.folder != "/" {
            lines.push(format!("Folder: {}", d.folder));
        }
        if !d.aliases.is_empty() {
            lines.push(format!("Aliases: {}", d.aliases.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
        }
        if !d.tags.is_empty() {
            lines.push(format!("Tags: {}", d.tags.iter().take(8).map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ")));
        }
        if !d.props.is_empty() {
            lines.push(format!("Properties: {}", d.props));
        }
        if !d.headings.is_empty() {
            let s: String = d.headings.iter().take(8).cloned().collect::<Vec<_>>().join(" | ").chars().take(200).collect();
            lines.push(format!("Sections: {s}"));
        }
        let date = chrono::DateTime::from_timestamp_millis(d.mtime).map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()).unwrap_or_default();
        lines.push(format!("Last modified: {date}"));
        let ex: String = d.body.chars().take(excerpt_chars).collect();
        let more = d.body.chars().count() > ex.chars().count();
        lines.push(format!("Beginning: {}", if ex.is_empty() { "(empty note)".into() } else { format!("{ex}{}", if more { "…" } else { "" }) }));
        lines.join("\n")
    }
}

/// Correspondência aproximada simples (letras da busca em ordem no título).
fn subsequence(q: &str, text: &str) -> bool {
    let mut it = text.chars();
    q.chars().filter(|c| !c.is_whitespace()).all(|c| it.any(|t| t == c))
}

/// Trecho da nota em volta da primeira palavra da busca, com as ocorrências marcadas.
pub fn snippet(d: &Doc, query: &str) -> Option<Value> {
    if d.body.is_empty() {
        return None;
    }
    let chars: Vec<char> = d.body.chars().collect();
    let folded: Vec<char> = chars.iter().map(|c| fold(&c.to_string()).chars().next().unwrap_or(*c)).collect();
    let fb: String = folded.iter().collect();
    let toks: Vec<String> = tokens(query).into_iter().filter(|t| t.chars().count() >= 3).collect();
    let char_index = |byte: usize| fb[..byte].chars().count();
    let mut at: Option<usize> = None;
    for t in &toks {
        if let Some(b) = fb.find(t.as_str()) {
            let i = char_index(b);
            if at.map_or(true, |a| i < a) {
                at = Some(i);
            }
        }
    }
    let mut start = at.map_or(0, |a| a.saturating_sub(60));
    if start > 0 {
        if let Some(sp) = chars[start..].iter().position(|c| *c == ' ') {
            if at.map_or(false, |a| start + sp < a) {
                start += sp + 1;
            }
        }
    }
    let end = (start + 220).min(chars.len());
    let window: String = folded[start..end].iter().collect();
    let text: Vec<char> = chars[start..end].to_vec();
    let mut segs: Vec<Value> = Vec::new();
    let mut pos = 0;
    if !toks.is_empty() {
        let re = Regex::new(&toks.iter().map(|t| regex::escape(t)).collect::<Vec<_>>().join("|")).unwrap();
        for m in re.find_iter(&window) {
            let a = window[..m.start()].chars().count();
            let b = a + m.as_str().chars().count();
            if a < pos {
                continue;
            }
            segs.push(json!({ "text": text[pos..a].iter().collect::<String>(), "hl": false }));
            segs.push(json!({ "text": text[a..b].iter().collect::<String>(), "hl": true }));
            pos = b;
        }
    }
    segs.push(json!({ "text": text[pos..].iter().collect::<String>(), "hl": false }));
    Some(json!({ "segments": segs, "pre": start > 0, "post": end < chars.len() }))
}

pub fn doc_json(d: &Doc) -> Value {
    json!({ "abs": d.abs, "path": d.path, "title": d.title, "folder": d.folder })
}

/// Dados que a busca precisa, copiados do índice (para não segurar o lock durante a rede).
pub struct FindInput {
    pub docs: Vec<Doc>,
    pub local: Vec<usize>,
    pub recent: Vec<usize>,
    pub version: u64,
}

pub fn cache_get(index: &Index, key: &str) -> Option<Value> {
    index.cache.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}

pub fn cache_put(index: &mut Index, key: String, value: Value) {
    index.cache.retain(|(k, _)| *k != key);
    index.cache.push((key, value));
    if index.cache.len() > 30 {
        index.cache.remove(0);
    }
}

pub fn cache_key(version: u64, broad: bool, query: &str) -> String {
    format!("{version}|{}|{}", broad as u8, fold(query).split_whitespace().collect::<Vec<_>>().join(" "))
}

/// 1) candidatas por palavras; 2) se o cofre é grande, o Jev lê os títulos de todas
/// as notas (blocos de 250, uma chamada); 3) o Jev compara as finalistas e devolve a
/// probabilidade de cada uma.
pub async fn find(s: &Settings, input: FindInput, query: &str, force_broad: bool) -> Result<Value, String> {
    let t0 = Instant::now();
    let FindInput { docs, local, recent, .. } = input;
    if docs.is_empty() {
        return Err("Nenhuma nota para buscar. Confira as pastas excluídas nas configurações do Jev Home.".into());
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let state = json!({
        "query": query,
        "today": today,
        "recently_opened": recent.iter().take(5).map(|&i| docs[i].title.clone()).collect::<Vec<_>>(),
    });
    let mut pool: Vec<usize> = local.clone();
    let mut broad = false;
    let mut calls = 0;
    let by_path: HashMap<&str, usize> = docs.iter().enumerate().map(|(i, d)| (d.path.as_str(), i)).collect();

    if docs.len() > s.final_limit && (s.broad || force_broad) {
        broad = true;
        let mut sorted: Vec<usize> = (0..docs.len()).collect();
        sorted.sort_by(|a, b| docs[*a].path.cmp(&docs[*b].path));
        let mut q = Map::new();
        for (c, chunk) in sorted.chunks(250).enumerate() {
            let mut criteria = Map::new();
            for &i in chunk {
                let d = &docs[i];
                criteria.insert(
                    d.path.clone(),
                    if d.aliases.is_empty() { Value::Null } else { json!(format!("Also called: {}", d.aliases.iter().take(4).cloned().collect::<Vec<_>>().join(", "))) },
                );
            }
            criteria.insert("__none__".into(), json!("None of these notes could be it"));
            q.insert(format!("b{c}"), json!({ "type": "choice", "instructions": BROAD, "criteria": criteria }));
        }
        let a = net::jev_ask(&s.model, &state, &Value::Object(q), 45, 3).await?;
        calls += 1;
        let mut picks: Vec<(usize, f64)> = Vec::new();
        if let Some(obj) = a.answers.as_object() {
            for ans in obj.values() {
                let mut probs: Vec<(String, f64)> = ans
                    .get("probabilities")
                    .and_then(Value::as_object)
                    .map(|p| p.iter().filter(|(k, _)| *k != "__none__").filter_map(|(k, v)| v.as_f64().map(|v| (k.clone(), v))).collect())
                    .unwrap_or_default();
                probs.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
                for (path, p) in probs.into_iter().take(s.per_chunk) {
                    if p >= 0.02 {
                        if let Some(&i) = by_path.get(path.as_str()) {
                            picks.push((i, p));
                        }
                    }
                }
            }
        }
        picks.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        pool = local.iter().take(10).copied().chain(picks.into_iter().map(|(i, _)| i)).chain(local.iter().skip(10).copied()).collect();
    }

    // Completa com as abertas e as editadas por último
    let mut by_date: Vec<usize> = (0..docs.len()).collect();
    by_date.sort_by(|a, b| docs[*b].mtime.cmp(&docs[*a].mtime));
    let mut seen = HashSet::new();
    let mut cands = Vec::new();
    for i in pool.into_iter().chain(recent.iter().copied()).chain(by_date) {
        if cands.len() >= s.final_limit {
            break;
        }
        if seen.insert(i) {
            cands.push(i);
        }
    }
    let mut criteria = Map::new();
    for &i in &cands {
        criteria.insert(docs[i].path.clone(), json!(describe(&docs[i], s.excerpt_chars)));
    }
    criteria.insert("__none__".into(), json!("None of these notes is the one the user is looking for"));
    let a = net::jev_ask(&s.model, &state, &json!({ "find": { "type": "choice", "instructions": FIND, "criteria": criteria } }), 45, 3).await?;
    calls += 1;
    let ans = a.answers.get("find").cloned().unwrap_or(Value::Null);
    let mut probs: HashMap<String, f64> = ans
        .get("probabilities")
        .and_then(Value::as_object)
        .map(|p| p.iter().filter_map(|(k, v)| v.as_f64().map(|v| (k.nfc().collect::<String>(), v))).collect())
        .unwrap_or_default();
    if probs.is_empty() {
        if let Some(choice) = ans.get("choice").and_then(Value::as_str) {
            probs.insert(choice.to_string(), ans.get("confidence").and_then(Value::as_f64).unwrap_or(1.0));
        }
    }
    let mut items: Vec<(usize, f64)> = cands.iter().map(|&i| (i, probs.get(&docs[i].path.nfc().collect::<String>()).copied().unwrap_or(0.0))).collect();
    items.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
    let snip = items.first().and_then(|(i, _)| snippet(&docs[*i], query));
    Ok(json!({
        "query": query,
        "items": items.iter().map(|(i, p)| { let mut d = doc_json(&docs[*i]); d["p"] = json!(p); d }).collect::<Vec<_>>(),
        "none": probs.get("__none__").copied().unwrap_or(0.0),
        "confidence": ans.get("confidence"),
        "compared": cands.len(),
        "total": docs.len(),
        "broad": broad,
        "calls": calls,
        "ms": t0.elapsed().as_millis() as u64,
        "snippet": snip,
    }))
}

pub fn excluded(exclude: &[String], rel: &str) -> bool {
    if rel.split('/').any(|s| s.starts_with('.')) {
        return true;
    }
    exclude.iter().any(|e| {
        let e = e.trim().trim_matches('/');
        !e.is_empty() && (rel == e || rel.starts_with(&format!("{e}/")))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::parse_page;

    fn idx() -> Index {
        let root = Path::new("/v");
        let notes = [
            ("Projetos/Comissão Mesa.md", "---\naliases: [CM]\ntags: [comissao]\n---\n# Regras\nRepasse de 20% por indicação"),
            ("Pessoas/Antonio.md", "Contrato de 6 meses com metas"),
            ("Diário/2026-09-01.md", "Reunião sobre câmbio e comissão"),
        ];
        let docs = notes
            .iter()
            .enumerate()
            .map(|(i, (rel, c))| {
                let p = parse_page(&root.join(rel), root, c);
                make_doc(&p, rel, c, i as i64, 4000)
            })
            .collect();
        Index::new(docs, 1)
    }

    #[test]
    fn busca_local_pesa_titulo_e_ignora_acentos() {
        let i = idx();
        let r = i.local_search("comissao", 5);
        assert_eq!(i.docs[r[0]].title, "Comissão Mesa", "título pesa mais que o texto");
        assert_eq!(r.len(), 2);
        let r = i.local_search("contr", 5);
        assert_eq!(i.docs[r[0]].title, "Antonio", "prefixo encontra 'contrato'");
        let r = i.local_search("cm", 5);
        assert_eq!(i.docs[r[0]].title, "Comissão Mesa", "alias");
    }

    #[test]
    fn limpa_markdown() {
        assert_eq!(clean_text("# Título\n- [x] **feito** [[Nota|apelido]] `x`"), "Título feito apelido x");
    }

    #[test]
    fn trecho_marca_termos() {
        let i = idx();
        let s = snippet(&i.docs[0], "indicacao").unwrap();
        assert!(s["segments"].as_array().unwrap().iter().any(|x| x["hl"] == true && x["text"] == "indicação"));
    }
}
