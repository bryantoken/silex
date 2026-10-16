//! Templater — reescrita em Rust do plugin "Templater" do Obsidian.
//!
//! Comandos `<% … %>` com as funções mais usadas:
//!   tp.date.now(fmt?, offset?, ref?, ref_fmt?)  tp.date.today/tomorrow/yesterday(fmt?)
//!   tp.date.weekday(fmt, dia, ref?, ref_fmt?)
//!   tp.file.title  tp.file.folder(relativo?)  tp.file.path(relativo?)  tp.file.tags
//!   tp.file.creation_date(fmt?)  tp.file.last_modified_date(fmt?)  tp.file.cursor(n?)
//!   tp.file.include("[[Nota]]")  tp.frontmatter.chave  tp.frontmatter["chave"]
//!   tp.system.prompt("Pergunta", "padrão")  tp.system.suggester([...], [...])  tp.system.clipboard()
//! Controle de espaço: `<%-` / `-%>` removem uma quebra de linha; `<%_` / `_%>` todo espaço.
//! Blocos `<%* … %>` (JavaScript) não são executados.

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveDateTime, Timelike};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::sync::LazyLock;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct FolderTemplate {
    pub folder: String,
    pub template: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct Settings {
    pub templates_folder: String,
    pub trigger_on_file_creation: bool,
    pub enable_folder_templates: bool,
    pub folder_templates: Vec<FolderTemplate>,
    pub auto_jump_to_cursor: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self { templates_folder: String::new(), trigger_on_file_creation: false, enable_folder_templates: true, folder_templates: Vec::new(), auto_jump_to_cursor: true }
    }
}

// ---------- Datas no formato do moment.js (pt-BR) ----------

const MONTHS: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];
const MONTHS_SHORT: [&str; 12] = ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"];
const DAYS: [&str; 7] = ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"];
const DAYS_SHORT: [&str; 7] = ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"];
const DAYS_MIN: [&str; 7] = ["do", "2ª", "3ª", "4ª", "5ª", "6ª", "sá"];

static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[[^\]]*\]|YYYY|YY|MMMM|MMM|MM|Mo|M|DDDD|DDD|DD|Do|D|dddd|ddd|dd|do|d|E|e|HH|H|hh|h|mm|m|ss|s|SSS|A|a|WW|W|ww|w|GGGG|gggg|Q|X|x|ZZ|Z").unwrap()
});

/// Formata uma data com os tokens do moment.js.
pub fn format_moment(dt: &NaiveDateTime, fmt: &str) -> String {
    let wd = dt.weekday().num_days_from_sunday() as usize;
    TOKEN
        .replace_all(fmt, |c: &regex::Captures| {
            let t = &c[0];
            if t.starts_with('[') {
                return t[1..t.len() - 1].to_string();
            }
            match t {
                "YYYY" => format!("{:04}", dt.year()),
                "YY" => format!("{:02}", dt.year() % 100),
                "MMMM" => MONTHS[dt.month0() as usize].to_string(),
                "MMM" => MONTHS_SHORT[dt.month0() as usize].to_string(),
                "MM" => format!("{:02}", dt.month()),
                "Mo" => format!("{}º", dt.month()),
                "M" => dt.month().to_string(),
                "DDDD" => format!("{:03}", dt.ordinal()),
                "DDD" => dt.ordinal().to_string(),
                "DD" => format!("{:02}", dt.day()),
                "Do" => format!("{}º", dt.day()),
                "D" => dt.day().to_string(),
                "dddd" => DAYS[wd].to_string(),
                "ddd" => DAYS_SHORT[wd].to_string(),
                "dd" => DAYS_MIN[wd].to_string(),
                "do" => format!("{wd}º"),
                "d" => wd.to_string(),
                "E" => dt.weekday().number_from_monday().to_string(),
                "e" => wd.to_string(),
                "HH" => format!("{:02}", dt.hour()),
                "H" => dt.hour().to_string(),
                "hh" => format!("{:02}", (dt.hour() + 11) % 12 + 1),
                "h" => ((dt.hour() + 11) % 12 + 1).to_string(),
                "mm" => format!("{:02}", dt.minute()),
                "m" => dt.minute().to_string(),
                "ss" => format!("{:02}", dt.second()),
                "s" => dt.second().to_string(),
                "SSS" => format!("{:03}", dt.and_utc().timestamp_subsec_millis()),
                "A" => if dt.hour() < 12 { "AM".into() } else { "PM".into() },
                "a" => if dt.hour() < 12 { "am".into() } else { "pm".into() },
                "WW" | "ww" => format!("{:02}", dt.iso_week().week()),
                "W" | "w" => dt.iso_week().week().to_string(),
                "GGGG" | "gggg" => format!("{:04}", dt.iso_week().year()),
                "Q" => ((dt.month0() / 3) + 1).to_string(),
                "X" => dt.and_utc().timestamp().to_string(),
                "x" => dt.and_utc().timestamp_millis().to_string(),
                "ZZ" | "Z" => Local::now().format(if t == "Z" { "%:z" } else { "%z" }).to_string(),
                _ => t.to_string(),
            }
        })
        .into_owned()
}

/// Lê uma data no formato do moment (só os tokens numéricos comuns).
fn parse_moment(s: &str, fmt: &str) -> Option<NaiveDateTime> {
    let mut pat = String::from("^");
    let mut fields = Vec::new();
    let mut last = 0;
    for m in TOKEN.find_iter(fmt) {
        pat.push_str(&regex::escape(&fmt[last..m.start()]));
        let t = m.as_str();
        let g = match t {
            "YYYY" => r"(\d{4})",
            "MM" | "DD" | "HH" | "mm" | "ss" => r"(\d{2})",
            "M" | "D" | "H" | "m" | "s" => r"(\d{1,2})",
            _ if t.starts_with('[') => {
                pat.push_str(&regex::escape(&t[1..t.len() - 1]));
                last = m.end();
                continue;
            }
            _ => r"(\S+?)",
        };
        pat.push_str(g);
        fields.push(t.to_string());
        last = m.end();
    }
    pat.push_str(&regex::escape(&fmt[last..]));
    let caps = Regex::new(&pat).ok()?.captures(s.trim())?;
    let (mut y, mut mo, mut d, mut h, mut mi, mut se) = (1970, 1, 1, 0, 0, 0);
    for (i, f) in fields.iter().enumerate() {
        let v: i32 = caps.get(i + 1)?.as_str().parse().unwrap_or(0);
        match f.as_str() {
            "YYYY" => y = v,
            "MM" | "M" => mo = v as u32,
            "DD" | "D" => d = v as u32,
            "HH" | "H" => h = v as u32,
            "mm" | "m" => mi = v as u32,
            "ss" | "s" => se = v as u32,
            _ => {}
        }
    }
    NaiveDate::from_ymd_opt(y, mo, d)?.and_hms_opt(h, mi, se)
}

/// Desloca a data: número = dias; "P1D", "P2W", "P1M", "P1Y", "-P3D".
fn offset(dt: NaiveDateTime, off: &Value) -> NaiveDateTime {
    match off {
        Value::Number(n) => dt + Duration::days(n.as_f64().unwrap_or(0.0).round() as i64),
        Value::String(s) => {
            let (neg, body) = s.strip_prefix('-').map(|b| (true, b)).unwrap_or((false, s.as_str()));
            let Some(c) = Regex::new(r"^P(\d+)([DWMY])$").unwrap().captures(body) else { return dt };
            let n: i64 = c[1].parse().unwrap_or(0) * if neg { -1 } else { 1 };
            match &c[2] {
                "D" => dt + Duration::days(n),
                "W" => dt + Duration::weeks(n),
                "M" => dt.checked_add_months(chrono::Months::new(n.unsigned_abs() as u32)).filter(|_| n >= 0).or_else(|| dt.checked_sub_months(chrono::Months::new(n.unsigned_abs() as u32))).unwrap_or(dt),
                _ => dt.with_year(dt.year() + n as i32).unwrap_or(dt),
            }
        }
        _ => dt,
    }
}

// ---------- Expressões ----------

#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Str(String),
    Num(f64),
    Bool(bool),
    List(Vec<Arg>),
}

impl Arg {
    fn to_value(&self) -> Value {
        match self {
            Arg::Str(s) => Value::String(s.clone()),
            Arg::Num(n) => serde_json::json!(n),
            Arg::Bool(b) => Value::Bool(*b),
            Arg::List(l) => Value::Array(l.iter().map(Arg::to_value).collect()),
        }
    }
    fn text(&self) -> String {
        match self {
            Arg::Str(s) => s.clone(),
            Arg::Num(n) => if n.fract() == 0.0 { format!("{}", *n as i64) } else { n.to_string() },
            Arg::Bool(b) => b.to_string(),
            Arg::List(l) => l.iter().map(Arg::text).collect::<Vec<_>>().join(", "),
        }
    }
}

/// Lê "tp.a.b(args)" ou "tp.frontmatter["chave"]". Devolve (caminho, args, é chamada).
fn parse_expr(src: &str) -> Option<(Vec<String>, Vec<Arg>, bool)> {
    let s = src.trim().trim_end_matches(';').trim();
    let s = s.strip_prefix("await ").unwrap_or(s).trim();
    let mut chars = s.char_indices().peekable();
    let mut path = Vec::new();
    let mut cur = String::new();
    let mut args = Vec::new();
    let mut call = false;
    while let Some((i, c)) = chars.next() {
        match c {
            '.' => path.push(std::mem::take(&mut cur)),
            '[' => {
                path.push(std::mem::take(&mut cur));
                let rest = &s[i + 1..];
                let end = rest.find(']')?;
                if let Some(Arg::Str(k)) = parse_args(&rest[..end])?.into_iter().next() {
                    path.push(k);
                }
                for _ in 0..=end {
                    chars.next();
                }
                if chars.peek().map(|(_, c)| *c) == Some('.') {
                    chars.next();
                }
            }
            '(' => {
                path.push(std::mem::take(&mut cur));
                let inner = s[i + 1..].strip_suffix(')')?;
                args = parse_args(inner)?;
                call = true;
                break;
            }
            c if c.is_alphanumeric() || c == '_' => cur.push(c),
            c if c.is_whitespace() => {}
            _ => return None,
        }
    }
    if !cur.is_empty() {
        path.push(cur);
    }
    path.retain(|p| !p.is_empty());
    Some((path, args, call))
}

fn parse_args(src: &str) -> Option<Vec<Arg>> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' || c == '`' {
            let mut j = i + 1;
            let mut s = String::new();
            while j < b.len() && b[j] as char != c {
                if b[j] == b'\\' && j + 1 < b.len() {
                    j += 1;
                }
                let ch = src[j..].chars().next()?;
                s.push(ch);
                j += ch.len_utf8();
            }
            out.push(Arg::Str(s));
            i = j + 1;
        } else if c == '[' {
            let mut depth = 0;
            let mut j = i;
            while j < b.len() {
                match b[j] {
                    b'[' => depth += 1,
                    b']' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            out.push(Arg::List(parse_args(&src[i + 1..j.min(b.len())])?));
            i = j + 1;
        } else {
            let end = src[i..].find(',').map(|e| i + e).unwrap_or(src.len());
            let tok = src[i..end].trim();
            out.push(match tok {
                "true" => Arg::Bool(true),
                "false" => Arg::Bool(false),
                "null" | "undefined" => Arg::Str(String::new()),
                _ => Arg::Num(tok.parse().ok()?),
            });
            i = end;
        }
    }
    Some(out)
}

/// Contexto do arquivo de destino.
pub struct FileCtx<'a> {
    pub root: &'a Path,
    pub target: &'a Path,
    pub frontmatter: Value,
    pub tags: Vec<String>,
    pub created: NaiveDateTime,
    pub modified: NaiveDateTime,
    pub clipboard: String,
    /// Respostas de tp.system.prompt/suggester, na ordem em que aparecem
    pub answers: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct Prompt {
    pub kind: String,
    pub label: String,
    pub default: String,
    pub options: Vec<String>,
    pub values: Vec<String>,
}

pub const CURSOR: &str = "\u{1F}cursor\u{1F}";

fn eval(path: &[String], args: &[Arg], ctx: &FileCtx, prompt_idx: &mut usize, prompts: &mut Vec<Prompt>, depth: usize) -> Result<String, String> {
    let a = |i: usize| args.get(i);
    let fmt = |i: usize, def: &str| a(i).map(Arg::text).filter(|s| !s.is_empty()).unwrap_or_else(|| def.to_string());
    let now = Local::now().naive_local();
    let rel = ctx.target.strip_prefix(ctx.root).unwrap_or(ctx.target).to_string_lossy().replace('\\', "/");
    let p: Vec<&str> = path.iter().map(String::as_str).collect();
    Ok(match p.as_slice() {
        ["tp", "date", "now"] => {
            let mut base = now;
            if let (Some(r), Some(rf)) = (a(2), a(3)) {
                base = parse_moment(&r.text(), &rf.text()).unwrap_or(now);
            }
            if let Some(off) = a(1) {
                base = offset(base, &off.to_value());
            }
            format_moment(&base, &fmt(0, "YYYY-MM-DD"))
        }
        ["tp", "date", "today"] => format_moment(&now, &fmt(0, "YYYY-MM-DD")),
        ["tp", "date", "tomorrow"] => format_moment(&(now + Duration::days(1)), &fmt(0, "YYYY-MM-DD")),
        ["tp", "date", "yesterday"] => format_moment(&(now - Duration::days(1)), &fmt(0, "YYYY-MM-DD")),
        ["tp", "date", "weekday"] => {
            let mut base = now;
            if let (Some(r), Some(rf)) = (a(2), a(3)) {
                base = parse_moment(&r.text(), &rf.text()).unwrap_or(now);
            }
            // weekday(0) = segunda da semana atual (locale com semana começando na segunda)
            let target: i64 = a(1).map(|x| x.text().parse().unwrap_or(0)).unwrap_or(0);
            let monday = base - Duration::days(base.weekday().num_days_from_monday() as i64);
            format_moment(&(monday + Duration::days(target)), &fmt(0, "YYYY-MM-DD"))
        }
        ["tp", "file", "title"] => ctx.target.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        ["tp", "file", "folder"] => {
            let relative = matches!(a(0), Some(Arg::Bool(true)));
            let folder = rel.rsplit_once('/').map(|(f, _)| f.to_string()).unwrap_or_default();
            if relative { folder } else { folder.rsplit('/').next().unwrap_or("").to_string() }
        }
        ["tp", "file", "path"] => {
            if matches!(a(0), Some(Arg::Bool(true))) { rel.clone() } else { ctx.target.to_string_lossy().to_string() }
        }
        ["tp", "file", "tags"] => ctx.tags.iter().map(|t| format!("#{t}")).collect::<Vec<_>>().join(","),
        ["tp", "file", "creation_date"] => format_moment(&ctx.created, &fmt(0, "YYYY-MM-DD HH:mm")),
        ["tp", "file", "last_modified_date"] => format_moment(&ctx.modified, &fmt(0, "YYYY-MM-DD HH:mm")),
        ["tp", "file", "cursor"] => CURSOR.to_string(),
        ["tp", "file", "include"] => {
            if depth > 5 {
                return Err("tp.file.include: inclusões demais (laço?)".into());
            }
            let target = a(0).map(Arg::text).unwrap_or_default();
            let name = target.trim_start_matches("[[").trim_end_matches("]]").split('|').next().unwrap_or("").split('#').next().unwrap_or("").trim().to_string();
            let file = find_note(ctx.root, &name).ok_or_else(|| format!("tp.file.include: nota não encontrada: {name}"))?;
            let text = std::fs::read_to_string(&file).map_err(|e| e.to_string())?;
            render_inner(&text, ctx, prompt_idx, prompts, depth + 1)?
        }
        ["tp", "frontmatter", key] => ctx.frontmatter.get(*key).map(|v| match v {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            Value::Array(arr) => arr.iter().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect::<Vec<_>>().join(", "),
            other => other.to_string(),
        }).unwrap_or_default(),
        ["tp", "system", "clipboard"] => ctx.clipboard.clone(),
        ["tp", "system", "prompt"] => {
            let i = *prompt_idx;
            *prompt_idx += 1;
            prompts.push(Prompt { kind: "prompt".into(), label: fmt(0, "Valor"), default: fmt(1, ""), options: Vec::new(), values: Vec::new() });
            ctx.answers.get(i).cloned().unwrap_or_else(|| fmt(1, ""))
        }
        ["tp", "system", "suggester"] => {
            let i = *prompt_idx;
            *prompt_idx += 1;
            let list = |x: Option<&Arg>| match x {
                Some(Arg::List(l)) => l.iter().map(Arg::text).collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            let options = list(a(0));
            let values = { let v = list(a(1)); if v.is_empty() { options.clone() } else { v } };
            prompts.push(Prompt { kind: "suggester".into(), label: fmt(3, "Escolha uma opção"), default: values.first().cloned().unwrap_or_default(), options, values: values.clone() });
            ctx.answers.get(i).cloned().unwrap_or_default()
        }
        _ => return Err(format!("Templater: função não suportada: {}", path.join("."))),
    })
}

fn find_note(root: &Path, name: &str) -> Option<std::path::PathBuf> {
    let want = name.trim_end_matches(".md").replace('\\', "/").to_lowercase();
    let mut stack = vec![root.to_path_buf()];
    let mut by_name = None;
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) == Some("md") {
                let rel = p.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/").trim_end_matches(".md").to_lowercase();
                if rel == want {
                    return Some(p);
                }
                if by_name.is_none() && rel.rsplit('/').next() == Some(want.as_str()) {
                    by_name = Some(p);
                }
            }
        }
    }
    by_name
}

static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<%([-_*]?)(.*?)([-_]?)%>").unwrap());

fn render_inner(src: &str, ctx: &FileCtx, prompt_idx: &mut usize, prompts: &mut Vec<Prompt>, depth: usize) -> Result<String, String> {
    let mut out = String::new();
    let mut last = 0;
    let mut trim_next: Option<&str> = None;
    for c in TAG.captures_iter(src) {
        let m = c.get(0).unwrap();
        let mut before = &src[last..m.start()];
        if let Some(mode) = trim_next.take() {
            before = if mode == "_" { before.trim_start() } else { before.strip_prefix("\r\n").or_else(|| before.strip_prefix('\n')).unwrap_or(before) };
        }
        let open = &c[1];
        let mut before = before.to_string();
        if open == "-" {
            if before.ends_with('\n') {
                before.pop();
                if before.ends_with('\r') {
                    before.pop();
                }
            }
        } else if open == "_" {
            before = before.trim_end().to_string();
        }
        out.push_str(&before);
        if open == "*" {
            out.push_str("<!-- Templater: blocos <%* %> (JavaScript) não são executados no Silex -->");
        } else {
            let (path, args, _) = parse_expr(&c[2]).ok_or_else(|| format!("Templater: expressão inválida: <%{}%>", &c[2]))?;
            out.push_str(&eval(&path, &args, ctx, prompt_idx, prompts, depth)?);
        }
        let close = &c[3];
        if !close.is_empty() {
            trim_next = Some(if close == "_" { "_" } else { "-" });
        }
        last = m.end();
    }
    let mut tail = &src[last..];
    if let Some(mode) = trim_next {
        tail = if mode == "_" { tail.trim_start() } else { tail.strip_prefix("\r\n").or_else(|| tail.strip_prefix('\n')).unwrap_or(tail) };
    }
    out.push_str(tail);
    Ok(out)
}

/// Renderiza o template. Devolve o texto e as perguntas encontradas (para o
/// frontend pedir as respostas e renderizar de novo com elas).
pub fn render(src: &str, ctx: &FileCtx) -> Result<(String, Vec<Prompt>), String> {
    let mut prompts = Vec::new();
    let mut idx = 0;
    let text = render_inner(src, ctx, &mut idx, &mut prompts, 0)?;
    Ok((text, prompts))
}

/// Pasta de templates: a configurada ou, se vazia, "templates"/"Templates" se existir.
pub fn templates_dir(root: &Path, s: &Settings) -> Option<std::path::PathBuf> {
    let configured = s.templates_folder.trim().trim_matches('/');
    let candidates: Vec<&str> = if configured.is_empty() { vec!["templates", "Templates", "Modelos"] } else { vec![configured] };
    candidates.into_iter().map(|c| root.join(c)).find(|p| p.is_dir())
}

/// Template configurado para a pasta de uma nova nota (a pasta mais específica vence).
pub fn folder_template(s: &Settings, rel_folder: &str) -> Option<String> {
    if !s.enable_folder_templates {
        return None;
    }
    let mut best: Option<(&FolderTemplate, usize)> = None;
    for ft in &s.folder_templates {
        let f = ft.folder.trim().trim_matches('/');
        if ft.template.trim().is_empty() {
            continue;
        }
        let matches = f.is_empty() || f == "/" || rel_folder == f || rel_folder.starts_with(&format!("{f}/"));
        if matches && best.map_or(true, |(_, len)| f.len() > len) {
            best = Some((ft, f.len()));
        }
    }
    best.map(|(ft, _)| ft.template.clone())
}


#[cfg(test)]
mod tests {
    use super::*;

    fn ctx<'a>(root: &'a Path, target: &'a Path, answers: Vec<String>) -> FileCtx<'a> {
        let d = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap().and_hms_opt(15, 4, 5).unwrap();
        FileCtx { root, target, frontmatter: serde_json::json!({ "status": "ok" }), tags: vec!["a".into()], created: d, modified: d, clipboard: "clip".into(), answers }
    }

    #[test]
    fn formata_datas_moment_em_portugues() {
        let d = NaiveDate::from_ymd_opt(2026, 9, 23).unwrap().and_hms_opt(15, 4, 5).unwrap();
        assert_eq!(format_moment(&d, "YYYY-MM-DD HH.mm.ss"), "2026-09-23 15.04.05");
        assert_eq!(format_moment(&d, "dddd, D [de] MMMM [de] YYYY"), "quarta-feira, 23 de setembro de 2026");
        assert_eq!(format_moment(&d, "ddd DD/MM/YY [semana] ww"), "qua 23/09/26 semana 39");
    }

    #[test]
    fn renderiza_funcoes_e_espacos() {
        let root = Path::new("/v");
        let target = Path::new("/v/Projetos/Nova nota.md");
        let c = ctx(root, target, vec![]);
        let (out, prompts) = render("# <% tp.file.title %>\npasta: <% tp.file.folder(true) %>\n<%- tp.frontmatter.status -%>\nfim<% tp.file.cursor() %>", &c).unwrap();
        // "<%-" apaga a quebra de linha antes da tag e "-%>" a de depois (como no Templater)
        assert_eq!(out, format!("# Nova nota\npasta: Projetosokfim{CURSOR}"));
        assert!(prompts.is_empty());
        let (out, _) = render(r#"<% tp.date.now("DD/MM/YYYY", 1, "2026-01-31", "YYYY-MM-DD") %>"#, &c).unwrap();
        assert_eq!(out, "01/02/2026");
        let (out, _) = render(r#"<% tp.date.now("YYYY-MM", "P1M", "2026-01-15", "YYYY-MM-DD") %>"#, &c).unwrap();
        assert_eq!(out, "2026-02");
    }

    #[test]
    fn perguntas_em_duas_fases() {
        let root = Path::new("/v");
        let target = Path::new("/v/n.md");
        let tpl = r#"Cliente: <% tp.system.prompt("Nome do cliente", "Ana") %> / <% tp.system.suggester(["Alta", "Baixa"], ["a", "b"]) %>"#;
        let (first, prompts) = render(tpl, &ctx(root, target, vec![])).unwrap();
        assert_eq!(prompts.len(), 2);
        assert_eq!(prompts[0].label, "Nome do cliente");
        assert_eq!(prompts[1].options, ["Alta", "Baixa"]);
        assert_eq!(first, "Cliente: Ana / ");
        let (out, _) = render(tpl, &ctx(root, target, vec!["Bruno".into(), "b".into()])).unwrap();
        assert_eq!(out, "Cliente: Bruno / b");
    }

    #[test]
    fn bloco_js_vira_aviso_e_funcao_desconhecida_e_erro() {
        let root = Path::new("/v");
        let target = Path::new("/v/n.md");
        let (out, _) = render("a<%* tR += 'x' %>b", &ctx(root, target, vec![])).unwrap();
        assert!(out.starts_with("a<!-- Templater") && out.ends_with("b"));
        assert!(render("<% tp.user.algo() %>", &ctx(root, target, vec![])).is_err());
    }
}
