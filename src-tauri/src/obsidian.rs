//! Importação de temas, snippets e plugins de um cofre do Obsidian (`.obsidian/`).
//!
//! Temas e snippets são CSS e são copiados para `.silex/`. O Silex mapeia as
//! variáveis de cor do Obsidian (`--background-primary`, `--text-normal`,
//! `--interactive-accent`...) para os próprios tokens, então paleta e tipografia
//! do tema são aplicadas; regras que dependem da estrutura HTML do Obsidian não.
//!
//! Plugins do Obsidian são JavaScript e não rodam no Silex. Para cada um, o
//! relatório diz qual é o equivalente nativo em Rust, quando existe.

use serde::Serialize;
use std::fs;
use std::path::Path;

#[derive(Serialize, Debug)]
pub struct PluginReport {
    pub id: String,
    pub name: String,
    pub enabled_in_obsidian: bool,
    /// "native" | "plugin" | "replaced" | "unneeded" | "pending"
    pub status: &'static str,
    pub note: String,
}

#[derive(Serialize, Debug, Default)]
pub struct ImportReport {
    /// Configurações importadas para os plugins nativos (id nativo → JSON)
    #[serde(skip)]
    pub settings: serde_json::Map<String, serde_json::Value>,
    /// Plugins nativos a ativar (estavam ativos no Obsidian)
    pub enable: Vec<String>,
    pub themes: Vec<String>,
    pub snippets: Vec<String>,
    pub active_theme: Option<String>,
    pub enabled_snippets: Vec<String>,
    pub plugins: Vec<PluginReport>,
}

/// Plugin do Obsidian → (situação, plugin nativo do Silex, observação)
fn equivalent(id: &str) -> (&'static str, Option<&'static str>, &'static str) {
    match id {
        "dataview" => ("native", Some("rustview"), "Nativo: blocos ```rustview (sintaxe Rust, índice de páginas em Rust). Blocos dataviewjs precisam ser reescritos em rustview."),
        "obsidian-excel-to-markdown-table" => ("native", Some("excel-paste"), "Nativo: colar células do Excel gera tabela Markdown. Também existe como plugin WASM de exemplo."),
        "templater-obsidian" => ("native", Some("templater"), "Nativo em Rust: <% tp.* %> (datas, arquivo, frontmatter, prompt, include). Blocos <%* %> com JavaScript não rodam."),
        "numerals" => ("native", Some("numerals"), "Nativo em Rust: blocos ```math com variáveis, porcentagem, @prev, @total e moeda."),
        "obsidian-enhancing-export" => ("native", Some("export"), "Nativo em Rust: PDF, HTML e Word (.docx) sem Pandoc; outros formatos se o Pandoc estiver instalado."),
        "obsidian-excalidraw-plugin" => ("native", Some("excalidraw"), "Integrado: abre e salva os desenhos .excalidraw.md no mesmo formato, com o editor oficial do Excalidraw."),
        "jev-home-ob" => ("native", Some("jev-home"), "Nativo em Rust: página inicial com busca local + Jev (TypeSafe). Informe a API key nas configurações."),
        "jev-organizer" => ("native", Some("jev-organizer"), "Nativo em Rust: sugestões de pasta e propriedades com o Jev; sugestões pendentes importadas."),
        "mesa" => ("native", Some("mesa"), "Nativo: dados, histórico, consumo e IA em Rust; mesma pasta Mesa/ e mesmo formato."),
        "obsidian-style-settings" | "obsidian-minimal-settings" | "minimalist-settings-view" => {
            ("replaced", None, "Substituído por Configurações → Aparência (tema, cor de destaque, fonte, largura).")
        }
        "obsidian-hider" => ("replaced", None, "A interface do Silex já é enxuta; trilho e barra lateral são alternáveis (Ctrl+B)."),
        "lazy-plugins" => ("unneeded", None, "Desnecessário: plugins carregam em milissegundos."),
        "vault-optimizer" => ("pending", None, "Ainda não portado."),
        "realclaudian" => ("pending", None, "Ainda não portado (chat com IA)."),
        _ => ("pending", None, "Ainda não portado. Pode ser reescrito como plugin WASM com o SDK do Silex."),
    }
}

/// data.json do Obsidian → configuração do plugin nativo.
fn import_settings(native: &str, data: &serde_json::Value) -> Option<serde_json::Value> {
    match native {
        "jev-home" | "jev-organizer" => data.get("settings").cloned(),
        "mesa" => Some(data.clone()),
        "templater" => Some(serde_json::json!({
            "templates_folder": data.get("templates_folder").cloned().unwrap_or_default(),
            "trigger_on_file_creation": data.get("trigger_on_file_creation").cloned().unwrap_or(serde_json::json!(false)),
            "enable_folder_templates": data.get("enable_folder_templates").cloned().unwrap_or(serde_json::json!(true)),
            "folder_templates": data.get("folder_templates").cloned().unwrap_or(serde_json::json!([])),
            "auto_jump_to_cursor": data.get("auto_jump_to_cursor").cloned().unwrap_or(serde_json::json!(true)),
        })),
        "excalidraw" => Some(serde_json::json!({
            "folder": data.get("folder").cloned().unwrap_or(serde_json::json!("Excalidraw")),
            "compress": data.get("compress").cloned().unwrap_or(serde_json::json!(true)),
            "filenamePrefix": data.get("drawingFilenamePrefix").cloned().unwrap_or(serde_json::json!("Drawing ")),
            "filenameDateTime": data.get("drawingFilenameDateTime").cloned().unwrap_or(serde_json::json!("YYYY-MM-DD HH.mm.ss")),
        })),
        "export" => Some(serde_json::json!({ "openAfter": data.get("openExportedFile").cloned().unwrap_or(serde_json::json!(true)) })),
        _ => None,
    }
}

fn copy_css(src_dir: &Path, dest_dir: &Path, out: &mut Vec<String>, from_theme_dirs: bool) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(src_dir) else { return Ok(()) };
    fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let path = entry.path();
        let (name, css) = if from_theme_dirs && path.is_dir() {
            // .obsidian/themes/<Nome>/theme.css
            (entry.file_name().to_string_lossy().to_string(), path.join("theme.css"))
        } else if path.extension().and_then(|s| s.to_str()) == Some("css") {
            (path.file_stem().unwrap().to_string_lossy().to_string(), path.clone())
        } else {
            continue;
        };
        if css.is_file() {
            fs::copy(&css, dest_dir.join(format!("{name}.css"))).map_err(|e| e.to_string())?;
            out.push(name);
        }
    }
    out.sort();
    Ok(())
}

pub fn import(root: &Path) -> Result<ImportReport, String> {
    let obs = root.join(".obsidian");
    if !obs.is_dir() {
        return Err("Este cofre não tem pasta .obsidian.".into());
    }
    let silex = root.join(".silex");
    let mut report = ImportReport::default();

    copy_css(&obs.join("themes"), &silex.join("themes"), &mut report.themes, true)?;
    // Temas antigos do Obsidian ficavam soltos como .css na pasta themes
    copy_css(&obs.join("themes"), &silex.join("themes"), &mut report.themes, false)?;
    report.themes.dedup();
    copy_css(&obs.join("snippets"), &silex.join("snippets"), &mut report.snippets, false)?;

    let appearance: serde_json::Value = fs::read(obs.join("appearance.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    report.active_theme = appearance["cssTheme"].as_str().filter(|s| !s.is_empty()).map(String::from);
    report.enabled_snippets = appearance["enabledCssSnippets"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    let enabled: Vec<String> = fs::read(obs.join("community-plugins.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    if let Ok(entries) = fs::read_dir(obs.join("plugins")) {
        for entry in entries.flatten() {
            let manifest: serde_json::Value = fs::read(entry.path().join("manifest.json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_default();
            let id = manifest["id"]
                .as_str()
                .map(String::from)
                .unwrap_or_else(|| entry.file_name().to_string_lossy().to_string());
            let (status, native, note) = equivalent(&id);
            let on = enabled.contains(&id);
            if let Some(native) = native {
                let data: serde_json::Value = fs::read(entry.path().join("data.json"))
                    .ok()
                    .and_then(|b| serde_json::from_slice(&b).ok())
                    .unwrap_or_default();
                if let Some(cfg) = import_settings(native, &data) {
                    report.settings.insert(native.to_string(), cfg);
                }
                if native == "jev-organizer" {
                    if let Some(sugg) = data.get("suggestions").filter(|s| s.is_object()) {
                        report.settings.insert("jev-organizer:suggestions".into(), sugg.clone());
                    }
                }
                if on || native == "rustview" {
                    report.enable.push(native.to_string());
                }
            }
            report.plugins.push(PluginReport {
                name: manifest["name"].as_str().unwrap_or(&id).to_string(),
                enabled_in_obsidian: on,
                id,
                status,
                note: note.to_string(),
            });
        }
    }
    report.plugins.sort_by(|a, b| a.status.cmp(b.status).then(a.name.cmp(&b.name)));
    Ok(report)
}
