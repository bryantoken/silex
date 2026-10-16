//! rustview: blocos ```rustview executados dentro da nota.
//!
//! Os scripts usam Rhai, uma linguagem de script embutida, escrita em Rust e com
//! sintaxe de Rust. Ela roda no próprio processo, sem compilação, e em sandbox:
//! sem acesso a disco/rede e com limite de operações. A API segue a do Dataview:
//!
//! ```text
//! current()              página atual (frontmatter, tasks, headings, tags, links...)
//! pages() / pages(src)   todas as páginas, ou filtradas por "#tag" ou "pasta/sub"
//! header(n, conteúdo)    paragraph(conteúdo)    list([itens])    table([cab], [[linhas]])
//! span(texto, cor)       bold(texto)            italic(texto)    link(alvo[, rótulo])
//! group_by(arr, |x| ..)  count(arr, |x| ..)     repeat("█", n)
//! ```
//!
//! "conteúdo" pode ser texto Markdown, um span/link, ou um array combinando-os.
//! A saída é uma lista de nós estruturados; o frontend os renderiza sem HTML cru.

use crate::pages::Page;
use rhai::serde::to_dynamic;
use rhai::{Array, Dynamic, Engine, EvalAltResult, FnPtr, Map, NativeCallContext, Scope};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

type Out = Arc<Mutex<Vec<Value>>>;

fn inline(d: &Dynamic) -> Vec<Value> {
    if d.is_unit() {
        return Vec::new();
    }
    if let Some(arr) = d.clone().try_cast::<Array>() {
        return arr.iter().flat_map(inline).collect();
    }
    if let Some(map) = d.clone().try_cast::<Map>() {
        if map.contains_key("kind") {
            let v: Value = rhai::serde::from_dynamic(d).unwrap_or(Value::Null);
            return vec![v];
        }
        return vec![json!({ "kind": "text", "text": d.to_string() })];
    }
    if d.is_string() {
        return vec![json!({ "kind": "md", "text": d.clone().into_string().unwrap_or_default() })];
    }
    vec![json!({ "kind": "text", "text": d.to_string() })]
}

fn span(kind: &str, text: &str) -> Map {
    let mut m = Map::new();
    m.insert("kind".into(), kind.into());
    m.insert("text".into(), text.into());
    m
}

fn group_key(d: &Dynamic) -> String {
    if d.is_unit() { String::new() } else { d.to_string() }
}

pub fn run(script: &str, current: &Page, pages: &[Page]) -> Vec<Value> {
    let out: Out = Arc::new(Mutex::new(Vec::new()));
    let mut engine = Engine::new();
    engine.set_max_operations(5_000_000);
    engine.set_max_call_levels(64);
    engine.set_max_string_size(1 << 20);
    engine.set_max_array_size(200_000);
    engine.set_max_map_size(50_000);

    let current_dyn = to_dynamic(current).unwrap_or(Dynamic::UNIT);
    engine.register_fn("current", move || current_dyn.clone());

    // Conversão para valores do Rhai só quando o script chama pages()
    let all = Arc::new(pages.to_vec());
    let all_src = all.clone();
    engine.register_fn("pages", move || -> Array {
        all.iter().filter_map(|p| to_dynamic(p).ok()).collect()
    });
    engine.register_fn("pages", move |src: &str| -> Array {
        let src = src.trim().trim_matches('"');
        all_src
            .iter()
            .filter(|p| {
                if let Some(tag) = src.strip_prefix('#') {
                    p.tags.iter().any(|t| t == tag || t.starts_with(&format!("{tag}/")))
                } else {
                    let src = src.trim_matches('/');
                    src.is_empty() || p.folder == src || p.folder.starts_with(&format!("{src}/"))
                }
            })
            .filter_map(|p| to_dynamic(p).ok())
            .collect()
    });

    let o = out.clone();
    engine.register_fn("header", move |level: i64, content: Dynamic| {
        o.lock().unwrap().push(json!({ "type": "header", "level": level.clamp(1, 6), "content": inline(&content) }));
    });
    let o = out.clone();
    engine.register_fn("paragraph", move |content: Dynamic| {
        o.lock().unwrap().push(json!({ "type": "paragraph", "content": inline(&content) }));
    });
    let o = out.clone();
    engine.register_fn("list", move |items: Array| {
        let items: Vec<_> = items.iter().map(inline).collect();
        o.lock().unwrap().push(json!({ "type": "list", "items": items }));
    });
    let o = out.clone();
    engine.register_fn("table", move |headers: Array, rows: Array| {
        let headers: Vec<_> = headers.iter().map(inline).collect();
        let rows: Vec<Vec<_>> = rows
            .iter()
            .map(|r| match r.clone().try_cast::<Array>() {
                Some(cells) => cells.iter().map(inline).collect(),
                None => vec![inline(r)],
            })
            .collect();
        o.lock().unwrap().push(json!({ "type": "table", "headers": headers, "rows": rows }));
    });

    engine.register_fn("span", |text: &str, color: &str| -> Map {
        let mut m = span("span", text);
        m.insert("color".into(), color.into());
        m
    });
    engine.register_fn("bold", |text: &str| -> Map { span("bold", text) });
    engine.register_fn("italic", |text: &str| -> Map { span("italic", text) });
    engine.register_fn("link", |target: &str| -> Map {
        let mut m = span("link", target);
        m.insert("target".into(), target.into());
        m
    });
    engine.register_fn("link", |target: &str, label: &str| -> Map {
        let mut m = span("link", label);
        m.insert("target".into(), target.into());
        m
    });
    engine.register_fn("repeat", |s: &str, n: i64| -> String { s.repeat(n.clamp(0, 10_000) as usize) });

    engine.register_fn(
        "group_by",
        |ctx: NativeCallContext, arr: Array, f: FnPtr| -> Result<Array, Box<EvalAltResult>> {
            let mut groups: Vec<(String, Dynamic, Array)> = Vec::new();
            for item in arr {
                let key: Dynamic = f.call_within_context(&ctx, (item.clone(),))?;
                let k = group_key(&key);
                match groups.iter_mut().find(|(gk, _, _)| *gk == k) {
                    Some((_, _, rows)) => rows.push(item),
                    None => groups.push((k, key, vec![item])),
                }
            }
            Ok(groups
                .into_iter()
                .map(|(_, key, rows)| {
                    let mut m = Map::new();
                    m.insert("key".into(), key);
                    m.insert("rows".into(), rows.into());
                    Dynamic::from_map(m)
                })
                .collect())
        },
    );
    engine.register_fn(
        "count",
        |ctx: NativeCallContext, arr: Array, f: FnPtr| -> Result<i64, Box<EvalAltResult>> {
            let mut n = 0;
            for item in arr {
                if f.call_within_context::<bool>(&ctx, (item,))? {
                    n += 1;
                }
            }
            Ok(n)
        },
    );

    let mut scope = Scope::new();
    if let Err(err) = engine.run_with_scope(&mut scope, script) {
        // `return` no nível superior encerra o script normalmente
        if !matches!(*err, EvalAltResult::Return(..)) {
            out.lock().unwrap().push(json!({ "type": "error", "message": err.to_string() }));
        }
    }
    let nodes = std::mem::take(&mut *out.lock().unwrap());
    nodes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::parse_page;
    use std::path::Path;

    /// Equivalente em rustview do painel dataviewjs da nota folha_auditoria.
    pub const PAINEL: &str = include_str!("../../docs/exemplos/painel_tarefas.rhai");

    #[test]
    fn painel_de_tarefas() {
        let md = "# Antes\n- [x] a\n- [x] b\n- [ ] c\n# Contratos\n- [-] d\n- [x] e\n";
        let page = parse_page(Path::new("/v/n.md"), Path::new("/v"), md);
        let out = run(PAINEL, &page, &[]);
        let text = serde_json::to_string(&out).unwrap();
        assert!(!text.contains("\"error\""), "{text}");
        assert_eq!(out[0]["type"], "header");
        assert!(text.contains("Progresso geral — 3/4 (75%) · 1 N/A"), "{text}");
        assert_eq!(out[2]["type"], "table");
        assert_eq!(out[2]["rows"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn sem_tarefas_e_erros() {
        let page = parse_page(Path::new("/v/n.md"), Path::new("/v"), "texto");
        let out = run(PAINEL, &page, &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "paragraph");

        let out = run("let x = ;", &page, &[]);
        assert_eq!(out[0]["type"], "error");

        let out = run("loop {}", &page, &[]);
        assert_eq!(out[0]["type"], "error", "laços infinitos são interrompidos");
    }

    #[test]
    fn filtra_pages_por_tag_e_pasta() {
        let a = parse_page(Path::new("/v/x/a.md"), Path::new("/v"), "#proj");
        let b = parse_page(Path::new("/v/y/b.md"), Path::new("/v"), "");
        let pages = [a.clone(), b];
        let out = run(r##"paragraph(`${pages("#proj").len()} ${pages("y").len()} ${pages().len()}`)"##, &a, &pages);
        assert_eq!(out[0]["content"][0]["text"], "1 1 2");
    }
}

#[cfg(test)]
mod sample {
    /// SILEX_SAMPLE=<nota.md> cargo test --lib sample -- --nocapture
    #[test]
    fn nota_real() {
        let Ok(path) = std::env::var("SILEX_SAMPLE") else { return };
        let content = std::fs::read_to_string(&path).unwrap();
        let p = std::path::Path::new(&path);
        let page = crate::pages::parse_page(p, p.parent().unwrap(), &content);
        let t = std::time::Instant::now();
        let out = super::run(super::tests::PAINEL, &page, &[]);
        println!("{} tarefas, {:?}\n{}", page.tasks.len(), t.elapsed(), serde_json::to_string_pretty(&out).unwrap());
    }
}
