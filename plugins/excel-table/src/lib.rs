//! Excel → tabela Markdown.
//! Reescrita em Rust do plugin "Excel to Markdown Table" do Obsidian.
//!
//! - Comando "Converter seleção do Excel em tabela Markdown"
//! - Bloco ```tsv renderizado como tabela

use silex_plugin_sdk::*;

#[derive(Default)]
struct ExcelTable;

/// Converte texto separado por tabulações (como o Excel copia) em tabela Markdown.
fn tsv_to_markdown(tsv: &str) -> Option<String> {
    let rows: Vec<Vec<String>> = tsv
        .trim_end_matches(['\r', '\n'])
        .lines()
        .map(|l| l.trim_end_matches('\r').split('\t').map(escape).collect())
        .collect();
    if rows.is_empty() || rows.iter().all(|r| r.len() < 2) {
        return None;
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let line = |cells: &[String]| {
        let mut out = String::from("|");
        for i in 0..cols {
            out.push(' ');
            out.push_str(cells.get(i).map(String::as_str).unwrap_or(""));
            out.push_str(" |");
        }
        out
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

fn escape(cell: &str) -> String {
    cell.trim().replace('|', "\\|").replace('\n', "<br>")
}

impl Plugin for ExcelTable {
    fn manifest(&self) -> Manifest {
        Manifest::new("excel-table", "Excel para tabela Markdown")
            .version("0.1.0")
            .author("Silex")
            .description("Converte tabelas copiadas do Excel em tabelas Markdown.")
            .command("tsv-to-table", "Converter seleção do Excel em tabela Markdown")
            .code_block("tsv")
    }

    fn command(&mut self, command: &str, note: &Note) -> Vec<Action> {
        if command != "tsv-to-table" {
            return vec![];
        }
        match tsv_to_markdown(&note.selection) {
            Some(md) => vec![Action::replace_selection(md)],
            None => vec![Action::notice("Selecione células copiadas do Excel (separadas por tabulação).")],
        }
    }

    fn render(&mut self, _lang: &str, source: &str, _note: &Note) -> Vec<Node> {
        let mut rows: Vec<Vec<String>> = source
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.split('\t').map(|c| c.trim().to_string()).collect())
            .collect();
        if rows.is_empty() {
            return vec![Node::paragraph("_Tabela vazia._")];
        }
        let headers = rows.remove(0);
        vec![Node::table(headers, rows)]
    }
}

export_plugin!(ExcelTable);
