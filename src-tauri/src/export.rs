//! Exportação de notas — reescrita em Rust do "Enhancing Export".
//!
//! - HTML autônomo (imagens embutidas em base64, estilo de impressão)
//! - DOCX nativo (sem Pandoc): títulos, parágrafos, negrito/itálico/código,
//!   listas, tarefas, citações/callouts, tabelas, blocos de código e imagens
//! - PDF: o frontend imprime o HTML pelo diálogo do sistema ("Salvar como PDF")
//! - Outros formatos via Pandoc, se estiver instalado

use crate::pages::split_frontmatter;
use docx_rs::*;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static WIKI_EMBED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]*))?\]\]").unwrap());
static WIKI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]*)?(?:\|([^\]]*))?\]\]").unwrap());
static HIGHLIGHT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"==([^=\n]+)==").unwrap());
static CALLOUT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^([ \t]*>[ \t]*)\[!([\w-]+)\][+-]?[ \t]*(.*)$").unwrap());
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)%%.*?%%").unwrap());

/// Localiza um anexo pelo nome como o Obsidian (caminho relativo ou só o nome).
pub fn find_attachment(root: &Path, name: &str) -> Option<PathBuf> {
    let want = name.trim().replace('\\', "/");
    let direct = root.join(&want);
    if direct.is_file() {
        return Some(direct);
    }
    let base = want.rsplit('/').next()?.to_lowercase();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(dir).ok()?.flatten() {
            let p = e.path();
            if p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase() == base) {
                return Some(p);
            }
        }
    }
    None
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

pub(crate) fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// Converte a sintaxe do Obsidian para Markdown padrão (antes de renderizar).
fn preprocess(md: &str, root: &Path, inline_images: bool) -> String {
    let (_, body, _) = split_frontmatter(md);
    let body = COMMENT.replace_all(body, "");
    let body = WIKI_EMBED.replace_all(&body, |c: &regex::Captures| {
        let name = c[1].trim();
        let alt = c.get(2).map(|m| m.as_str()).unwrap_or("");
        match find_attachment(root, name) {
            Some(p) if mime(&p).starts_with("image/") => {
                let src = if inline_images {
                    fs::read(&p).map(|b| format!("data:{};base64,{}", mime(&p), base64(&b))).unwrap_or_default()
                } else {
                    p.to_string_lossy().replace('\\', "/")
                };
                let width = alt.trim().parse::<u32>().ok().map(|w| format!(" width=\"{w}\"")).unwrap_or_default();
                format!("<img src=\"{src}\" alt=\"{}\"{width}>", html_escape(name))
            }
            _ => format!("*{}*", name),
        }
    });
    let body = WIKI.replace_all(&body, |c: &regex::Captures| c.get(2).map(|m| m.as_str()).unwrap_or(&c[1]).trim().to_string());
    let body = HIGHLIGHT.replace_all(&body, "<mark>$1</mark>");
    CALLOUT.replace_all(&body, |c: &regex::Captures| {
        let title = if c[3].trim().is_empty() { capitalize(&c[2]) } else { c[3].trim().to_string() };
        format!("{}**{}**", &c[1], title)
    })
    .into_owned()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str().to_lowercase().as_str()).unwrap_or_default()
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS | Options::ENABLE_FOOTNOTES
}

// Margem da página zerada: sem espaço para cabeçalho/rodapé, o Chromium não
// imprime título, data, URL (tauri.localhost) nem numeração. A margem real vem
// do padding do body, repetido em cada página pelo box-decoration-break.
const PRINT_CSS: &str = r#"
@page { margin: 0; }
@media print {
  body { margin: 0 auto; padding: 18mm 16mm; max-width: 760px; box-decoration-break: clone; -webkit-box-decoration-break: clone; }
}
body { font-family: "Segoe UI", Inter, system-ui, sans-serif; color: #1d1d22; line-height: 1.6; max-width: 760px; margin: 32px auto; padding: 0 24px; font-size: 15px; }
h1, h2, h3, h4 { line-height: 1.25; margin: 1.4em 0 .5em; letter-spacing: -.01em; }
h1.title { font-size: 2.1em; margin-top: 0; }
a { color: #5b3fd1; }
code { font-family: "Cascadia Code", Consolas, monospace; font-size: .88em; background: #f2f1f6; padding: 1px 5px; border-radius: 4px; }
pre { background: #f6f5fa; border: 1px solid #e4e2ec; border-radius: 8px; padding: 12px 14px; overflow-x: auto; }
pre code { background: none; padding: 0; }
blockquote { margin: 1em 0; padding: 8px 14px; border-left: 3px solid #7a5af5; background: #f6f3ff; border-radius: 0 8px 8px 0; color: #3b3b46; }
table { border-collapse: collapse; margin: 1em 0; width: 100%; font-size: .94em; }
th, td { border: 1px solid #dcdbe3; padding: 6px 10px; text-align: left; vertical-align: top; }
th { background: #f3f2f8; }
img { max-width: 100%; border-radius: 6px; }
mark { background: #fff1a8; padding: 0 2px; border-radius: 2px; }
li.task { list-style: none; margin-left: -1.3em; }
hr { border: none; border-top: 1px solid #dcdbe3; margin: 2em 0; }
"#;

/// HTML completo e autônomo da nota.
pub fn to_html(root: &Path, path: &Path) -> Result<String, String> {
    let md = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let src = preprocess(&md, root, true);
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, Parser::new_ext(&src, options()));
    let body = body.replace("<li><input disabled=\"\" type=\"checkbox\"", "<li class=\"task\"><input disabled=\"\" type=\"checkbox\"");
    Ok(format!(
        "<!doctype html>\n<html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{t}</title><style>{PRINT_CSS}</style></head><body><h1 class=\"title\">{t}</h1>\n{body}</body></html>",
        t = html_escape(&title)
    ))
}

// ---------- DOCX ----------

#[derive(Default, Clone, Copy)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
}

fn run_of(text: &str, st: Style) -> Run {
    let mut r = Run::new().add_text(text);
    if st.bold {
        r = r.bold();
    }
    if st.italic {
        r = r.italic();
    }
    if st.strike {
        r = r.strike();
    }
    if st.code {
        r = r.fonts(RunFonts::new().ascii("Consolas").hi_ansi("Consolas")).size(19).highlight("lightGray");
    }
    r
}

pub fn to_docx(root: &Path, path: &Path, out: &Path) -> Result<(), String> {
    let md = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let title = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let src = preprocess(&md, root, false);
    // Imagens viram marcadores tratados abaixo; HTML inline vira texto
    let img_re = Regex::new(r#"<img src="([^"]+)" alt="[^"]*"(?: width="(\d+)")?>"#).unwrap();

    let mut doc = Docx::new().add_paragraph(Paragraph::new().add_run(Run::new().add_text(&title).size(44).bold()).style("Title"));
    let mut para: Option<Paragraph> = None;
    let mut st = Style::default();
    let mut list_depth: Vec<Option<u64>> = Vec::new();
    let mut quote = 0;
    let mut code_block: Option<String> = None;
    let mut table: Option<(Vec<TableRow>, Vec<TableCell>, Option<Paragraph>)> = None;
    let mut heading: Option<usize> = None;
    let mut task_prefix: Option<bool> = None;

    let flush = |doc: Docx, p: Option<Paragraph>| -> Docx { match p { Some(p) => doc.add_paragraph(p), None => doc } };

    for ev in Parser::new_ext(&src, options()) {
        let is_code = matches!(ev, Event::Code(_));
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(match level { HeadingLevel::H1 => 1, HeadingLevel::H2 => 2, HeadingLevel::H3 => 3, HeadingLevel::H4 => 4, _ => 5 });
                para = Some(Paragraph::new());
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(p) = para.take() {
                    let lvl = heading.take().unwrap_or(1);
                    doc = doc.add_paragraph(p.style(&format!("Heading{lvl}")));
                }
            }
            Event::Start(Tag::Paragraph) => {
                let mut p = Paragraph::new();
                if let Some(Some(n)) = list_depth.last().copied() {
                    let _ = n;
                }
                if quote > 0 {
                    p = p.indent(Some(560), None, None, None);
                }
                para = Some(p);
            }
            Event::End(TagEnd::Paragraph) => {
                if table.is_none() {
                    doc = flush(doc, para.take());
                }
            }
            Event::Start(Tag::BlockQuote(_)) => quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote -= 1,
            Event::Start(Tag::List(start)) => list_depth.push(start),
            Event::End(TagEnd::List(_)) => {
                list_depth.pop();
            }
            Event::Start(Tag::Item) => {
                let depth = list_depth.len().max(1);
                let marker = match list_depth.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}. ");
                        *n += 1;
                        m
                    }
                    _ => "• ".to_string(),
                };
                para = Some(Paragraph::new().indent(Some(360 * depth as i32), Some(SpecialIndentType::Hanging(280)), None, None).add_run(Run::new().add_text(&marker)));
            }
            Event::End(TagEnd::Item) => doc = flush(doc, para.take()),
            Event::TaskListMarker(done) => task_prefix = Some(done),
            Event::Start(Tag::Emphasis) => st.italic = true,
            Event::End(TagEnd::Emphasis) => st.italic = false,
            Event::Start(Tag::Strong) => st.bold = true,
            Event::End(TagEnd::Strong) => st.bold = false,
            Event::Start(Tag::Strikethrough) => st.strike = true,
            Event::End(TagEnd::Strikethrough) => st.strike = false,
            Event::Start(Tag::CodeBlock(kind)) => {
                let _ = matches!(kind, CodeBlockKind::Fenced(_));
                code_block = Some(String::new());
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(code) = code_block.take() {
                    for line in code.trim_end_matches('\n').split('\n') {
                        doc = doc.add_paragraph(
                            Paragraph::new()
                                .add_run(Run::new().add_text(line).fonts(RunFonts::new().ascii("Consolas").hi_ansi("Consolas")).size(19))
                                .indent(Some(280), None, None, None),
                        );
                    }
                }
            }
            Event::Start(Tag::Table(_)) => table = Some((Vec::new(), Vec::new(), None)),
            Event::End(TagEnd::Table) => {
                if let Some((rows, _, _)) = table.take() {
                    doc = doc.add_table(Table::new(rows));
                }
            }
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.1.clear();
                }
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                if let Some(t) = table.as_mut() {
                    let cells = std::mem::take(&mut t.1);
                    t.0.push(TableRow::new(cells));
                }
            }
            Event::Start(Tag::TableCell) => {
                if let Some(t) = table.as_mut() {
                    t.2 = Some(Paragraph::new());
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(t) = table.as_mut() {
                    let p = t.2.take().unwrap_or_default();
                    t.1.push(TableCell::new().add_paragraph(p));
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some(code) = code_block.as_mut() {
                    code.push_str(&text);
                    continue;
                }
                let mut style = st;
                style.code = is_code;
                let target = if let Some(t) = table.as_mut() { t.2.as_mut() } else { para.as_mut() };
                if let Some(p) = target {
                    let mut p2 = std::mem::take(p);
                    if let Some(done) = task_prefix.take() {
                        p2 = p2.add_run(Run::new().add_text(if done { "☑ " } else { "☐ " }));
                    }
                    if heading.is_some() {
                        style.bold = true;
                    }
                    *p = p2.add_run(run_of(&text, style));
                } else {
                    para = Some(Paragraph::new().add_run(run_of(&text, style)));
                }
            }
            Event::InlineHtml(html) | Event::Html(html) => {
                if let Some(c) = img_re.captures(&html) {
                    let file = PathBuf::from(&c[1]);
                    if let Ok(bytes) = fs::read(&file) {
                        let ext = file.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                        if ext == "png" || ext == "jpg" || ext == "jpeg" {
                            let mut pic = Pic::new(&bytes);
                            // Ajusta à largura da página (~16 cm)
                            let (w, h) = pic.size;
                            let max_w = c.get(2).and_then(|m| m.as_str().parse::<u32>().ok()).map(|px| px * 9525).unwrap_or(5_760_000).min(5_760_000);
                            if w > max_w && w > 0 {
                                pic = pic.size(max_w, (h as u64 * max_w as u64 / w as u64) as u32);
                            }
                            doc = flush(doc, para.take());
                            doc = doc.add_paragraph(Paragraph::new().add_run(Run::new().add_image(pic)));
                        }
                    }
                } else if html.contains("<mark>") || html.contains("</mark>") {
                    // realce: ignora a tag e mantém o texto
                }
            }
            Event::SoftBreak => {
                if let Some(p) = para.as_mut() {
                    *p = std::mem::take(p).add_run(Run::new().add_text(" "));
                }
            }
            Event::HardBreak => {
                if let Some(p) = para.as_mut() {
                    *p = std::mem::take(p).add_run(Run::new().add_break(BreakType::TextWrapping));
                }
            }
            Event::Rule => doc = doc.add_paragraph(Paragraph::new().add_run(Run::new().add_text("—————"))),
            _ => {}
        }
    }
    doc = flush(doc, para.take());
    let file = fs::File::create(out).map_err(|e| e.to_string())?;
    doc.build().pack(file).map_err(|e| e.to_string())
}

/// Pandoc instalado? Devolve a versão.
pub fn pandoc_version() -> Option<String> {
    let out = std::process::Command::new("pandoc").arg("--version").output().ok()?;
    String::from_utf8(out.stdout).ok()?.lines().next().map(String::from)
}

pub fn with_pandoc(root: &Path, path: &Path, out: &Path) -> Result<(), String> {
    let md = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let src = preprocess(&md, root, false);
    let tmp = std::env::temp_dir().join(format!("silex-export-{}.md", std::process::id()));
    fs::write(&tmp, src).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("pandoc")
        .arg(&tmp)
        .arg("-o")
        .arg(out)
        .arg("--resource-path")
        .arg(root)
        .arg("--standalone")
        .arg("--metadata")
        .arg(format!("title={}", path.file_stem().unwrap().to_string_lossy()))
        .status()
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&tmp);
    if status.success() { Ok(()) } else { Err(format!("Pandoc terminou com erro ({status}).")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporta_html_e_docx() {
        let dir = std::env::temp_dir().join(format!("silex-export-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let note = dir.join("Nota.md");
        fs::write(&note, "---\ntags: [x]\n---\n# Título\nTexto **forte** e ==realce== com [[Outra|link]].\n\n> [!warning]\n> cuidado\n\n- [x] feito\n- [ ] falta\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n").unwrap();
        let html = to_html(&dir, &note).unwrap();
        assert!(html.contains("<strong>forte</strong>") && html.contains("<mark>realce</mark>") && html.contains("link"));
        assert!(html.contains("<table>") && html.contains("Warning") && !html.contains("tags:"), "{html}");
        let out = dir.join("Nota.docx");
        to_docx(&dir, &note, &out).unwrap();
        assert!(fs::metadata(&out).unwrap().len() > 2000);
        fs::remove_dir_all(&dir).ok();
    }
}
