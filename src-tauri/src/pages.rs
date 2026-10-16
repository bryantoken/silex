//! Metadados estruturados de cada nota: frontmatter, tarefas, títulos, tags e links.
//! Base de dados do rustview (equivalente ao índice do Dataview).

use regex::Regex;
use serde::Serialize;
use std::path::Path;
use std::sync::LazyLock;

static LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[([^\]|#\n]+)(?:#[^\]|\n]*)?(?:\|[^\]\n]+)?\]\]").unwrap());
static TASK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)(?:[-*+]|\d+[.)])\s+\[(.)\]\s?(.*)$").unwrap());
static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(#{1,6})\s+(.+?)\s*#*\s*$").unwrap());
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)#([\p{L}\p{N}_][\p{L}\p{N}_/-]*)").unwrap());

#[derive(Serialize, Debug, Clone)]
pub struct Task {
    pub text: String,
    /// Caractere entre os colchetes: " ", "x", "-", "/", "?", "!" ...
    pub status: String,
    pub completed: bool,
    /// Linha (0-based) no arquivo
    pub line: usize,
    /// Título (heading) mais próximo acima da tarefa
    pub section: String,
    pub indent: usize,
}

#[derive(Serialize, Debug, Clone)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub line: usize,
}

#[derive(Serialize, Debug, Clone)]
pub struct Page {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub frontmatter: serde_json::Value,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub tasks: Vec<Task>,
    pub headings: Vec<Heading>,
    pub size: usize,
}

/// Separa o bloco YAML inicial (`---` ... `---`) do corpo.
pub fn split_frontmatter(content: &str) -> (Option<&str>, &str, usize) {
    let Some(rest) = content.strip_prefix("---\n").or_else(|| content.strip_prefix("---\r\n")) else {
        return (None, content, 0);
    };
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let yaml = &rest[..offset];
            let body_start = content.len() - rest.len() + offset + line.len();
            let fm_lines = yaml.lines().count() + 2;
            return (Some(yaml), &content[body_start..], fm_lines);
        }
        offset += line.len();
    }
    (None, content, 0)
}

pub fn parse_frontmatter(yaml: &str) -> serde_json::Value {
    serde_yaml::from_str::<serde_json::Value>(yaml)
        .ok()
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()))
}

pub fn parse_page(path: &Path, root: &Path, content: &str) -> Page {
    let (yaml, body, fm_lines) = split_frontmatter(content);
    let frontmatter = yaml.map(parse_frontmatter).unwrap_or_else(|| serde_json::json!({}));

    let mut tags: Vec<String> = match frontmatter.get("tags") {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim_start_matches('#').to_string()))
            .collect(),
        Some(serde_json::Value::String(s)) => s
            .split([',', ' '])
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches('#').to_string())
            .collect(),
        _ => Vec::new(),
    };

    let mut tasks = Vec::new();
    let mut headings = Vec::new();
    let mut section = String::new();
    let mut fence: Option<&str> = None;

    for (i, line) in body.lines().enumerate() {
        let line_no = i + fm_lines;
        let trimmed = line.trim_start();
        // Ignora conteúdo de blocos de código
        if let Some(f) = fence {
            if trimmed.starts_with(f) {
                fence = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            fence = Some("```");
            continue;
        }
        if trimmed.starts_with("~~~") {
            fence = Some("~~~");
            continue;
        }

        if let Some(c) = HEADING_RE.captures(line) {
            let text = c[2].to_string();
            headings.push(Heading { level: c[1].len(), text: text.clone(), line: line_no });
            section = text;
            continue;
        }
        if let Some(c) = TASK_RE.captures(line) {
            let status = c[2].to_string();
            tasks.push(Task {
                text: c[3].to_string(),
                completed: status.eq_ignore_ascii_case("x"),
                status,
                line: line_no,
                section: section.clone(),
                indent: c[1].chars().map(|ch| if ch == '\t' { 4 } else { 1 }).sum(),
            });
        }
        for c in TAG_RE.captures_iter(line) {
            let tag = c[1].to_string();
            if !tags.contains(&tag) {
                tags.push(tag);
            }
        }
    }

    let links = LINK_RE
        .captures_iter(body)
        .map(|c| c[1].trim().to_string())
        .collect();

    let rel = path.strip_prefix(root).unwrap_or(path);
    Page {
        path: path.to_string_lossy().to_string(),
        name: path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(),
        folder: rel
            .parent()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default(),
        frontmatter,
        tags,
        links,
        tasks,
        headings,
        size: content.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_frontmatter_tarefas_e_secoes() {
        let md = "---\nstatus: ok\ntags:\n  - a\n---\n# Fase 1\n- [x] feito\n- [-] n/a\n```\n- [ ] ignorada\n```\n## Fase 2\n\t- [/] meio #b\n";
        let p = parse_page(Path::new("/v/x/Nota.md"), Path::new("/v"), md);
        assert_eq!(p.frontmatter["status"], "ok");
        assert_eq!(p.tags, ["a", "b"]);
        assert_eq!(p.folder, "x");
        let st: Vec<_> = p.tasks.iter().map(|t| (t.status.as_str(), t.section.as_str())).collect();
        assert_eq!(st, [("x", "Fase 1"), ("-", "Fase 1"), ("/", "Fase 2")]);
        assert_eq!(p.tasks[0].line, 6);
    }
}
