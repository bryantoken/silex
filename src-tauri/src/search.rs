//! Índice de texto completo do cofre, construído sobre o Tantivy.
//!
//! - Índice invertido em memória com posições (necessário para frases e proximidade)
//! - Ranking BM25, com peso maior para o título
//! - Proximidade: `"termo1 termo2"~N` encontra os termos a até N posições de distância
//! - Tolerância a erros de digitação (distância de Levenshtein 1) quando não há resultado exato
//! - Normalização: minúsculas + remoção de acentos ("ação" == "acao")

use serde::Serialize;
use std::path::Path;
use std::sync::Mutex;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, Value, STORED, STRING,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::tokenizer::{
    AsciiFoldingFilter, LowerCaser, RemoveLongFilter, SimpleTokenizer, TextAnalyzer,
};
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument, Term};

const TOKENIZER: &str = "silex";
const WRITER_HEAP: usize = 64 * 1024 * 1024;
const SNIPPET_CHARS: usize = 160;

#[derive(Serialize, Debug)]
pub struct Segment {
    text: String,
    hl: bool,
}

#[derive(Serialize, Debug)]
pub struct SearchHit {
    path: String,
    title: String,
    score: f32,
    /// Trecho do corpo, já fatiado em partes destacadas/não destacadas
    /// (evita enviar HTML ou offsets em bytes UTF-8 para o frontend).
    snippet: Vec<Segment>,
}

pub struct SearchIndex {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    path: Field,
    title: Field,
    body: Field,
}

impl SearchIndex {
    pub fn new() -> tantivy::Result<Self> {
        let indexing = TextFieldIndexing::default()
            .set_tokenizer(TOKENIZER)
            .set_index_option(IndexRecordOption::WithFreqsAndPositions);
        let text = TextOptions::default()
            .set_indexing_options(indexing)
            .set_stored();

        let mut builder = Schema::builder();
        let path = builder.add_text_field("path", STRING | STORED);
        let title = builder.add_text_field("title", text.clone());
        let body = builder.add_text_field("body", text);
        let index = Index::create_in_ram(builder.build());

        index.tokenizers().register(
            TOKENIZER,
            TextAnalyzer::builder(SimpleTokenizer::default())
                .filter(RemoveLongFilter::limit(40))
                .filter(LowerCaser)
                .filter(AsciiFoldingFilter)
                .build(),
        );

        let writer = index.writer(WRITER_HEAP)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            path,
            title,
            body,
        })
    }

    /// Substitui todo o conteúdo do índice. `IndexWriter::add_document` é `&self`
    /// e internamente multi-thread, então os documentos podem vir de várias threads.
    pub fn rebuild<'a>(
        &self,
        docs: impl IntoIterator<Item = (&'a Path, &'a str)>,
    ) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|e| e.to_string())?;
        writer.delete_all_documents().map_err(|e| e.to_string())?;
        for (path, content) in docs {
            writer
                .add_document(self.make_doc(path, content))
                .map_err(|e| e.to_string())?;
        }
        writer.commit().map_err(|e| e.to_string())?;
        self.reader.reload().map_err(|e| e.to_string())
    }

    /// Insere ou atualiza uma única nota (chamado ao salvar/criar).
    pub fn upsert(&self, path: &Path, content: &str) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|e| e.to_string())?;
        writer.delete_term(Term::from_field_text(self.path, &path.to_string_lossy()));
        writer
            .add_document(self.make_doc(path, content))
            .map_err(|e| e.to_string())?;
        writer.commit().map_err(|e| e.to_string())?;
        self.reader.reload().map_err(|e| e.to_string())
    }

    pub fn remove(&self, path: &Path) -> Result<(), String> {
        let mut writer = self.writer.lock().map_err(|e| e.to_string())?;
        writer.delete_term(Term::from_field_text(self.path, &path.to_string_lossy()));
        writer.commit().map_err(|e| e.to_string())?;
        self.reader.reload().map_err(|e| e.to_string())
    }

    fn make_doc(&self, path: &Path, content: &str) -> TantivyDocument {
        let title = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        doc!(
            self.path => path.to_string_lossy().to_string(),
            self.title => title,
            self.body => content,
        )
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        let searcher = self.reader.searcher();

        let mut exact_parser = QueryParser::for_index(&self.index, vec![self.title, self.body]);
        exact_parser.set_conjunction_by_default();
        exact_parser.set_field_boost(self.title, 2.0);
        // Modo leniente: sintaxe inválida digitada pela metade não gera erro
        let (exact, _) = exact_parser.parse_query_lenient(query);

        let collector = TopDocs::with_limit(limit).order_by_score();
        let mut top = searcher.search(&exact, &collector).map_err(|e| e.to_string())?;

        // Sem resultado exato: tenta de novo tolerando um erro de digitação por termo.
        // Só como fallback, para não poluir buscas bem-sucedidas ("acao" ≠ "racao").
        if top.is_empty() {
            let mut fuzzy_parser =
                QueryParser::for_index(&self.index, vec![self.title, self.body]);
            fuzzy_parser.set_conjunction_by_default();
            fuzzy_parser.set_field_boost(self.title, 2.0);
            fuzzy_parser.set_field_fuzzy(self.title, true, 1, true);
            fuzzy_parser.set_field_fuzzy(self.body, true, 1, true);
            let (fuzzy, _) = fuzzy_parser.parse_query_lenient(query);
            top = searcher.search(&fuzzy, &collector).map_err(|e| e.to_string())?;
        }

        let mut snippets =
            SnippetGenerator::create(&searcher, &*exact, self.body).map_err(|e| e.to_string())?;
        snippets.set_max_num_chars(SNIPPET_CHARS);

        let mut hits = Vec::with_capacity(top.len());
        for (score, address) in top {
            let doc: TantivyDocument = searcher.doc(address).map_err(|e| e.to_string())?;
            let get = |f: Field| {
                doc.get_first(f)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let snippet = snippets.snippet_from_doc(&doc);
            hits.push(SearchHit {
                path: get(self.path),
                title: get(self.title),
                score,
                snippet: split_highlights(snippet.fragment(), snippet.highlighted()),
            });
        }
        Ok(hits)
    }
}

fn split_highlights(fragment: &str, ranges: &[std::ops::Range<usize>]) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut cursor = 0;
    for r in ranges {
        if r.start > cursor {
            out.push(Segment { text: fragment[cursor..r.start].to_string(), hl: false });
        }
        out.push(Segment { text: fragment[r.clone()].to_string(), hl: true });
        cursor = r.end;
    }
    if cursor < fragment.len() {
        out.push(Segment { text: fragment[cursor..].to_string(), hl: false });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> SearchIndex {
        let idx = SearchIndex::new().unwrap();
        idx.rebuild([
            (Path::new("/v/Perto.md"), "o gato preto dormiu no sofá"),
            (Path::new("/v/Longe.md"), "o gato comeu ração, brincou no jardim e depois ficou preto de terra"),
            (Path::new("/v/Ação.md"), "plano de ação para o trimestre"),
        ])
        .unwrap();
        idx
    }

    fn titles(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.title.as_str()).collect()
    }

    #[test]
    fn proximidade_respeita_distancia() {
        let idx = index();
        assert_eq!(titles(&idx.search("\"gato preto\"~1", 10).unwrap()), ["Perto"]);
        let longe = idx.search("\"gato preto\"~20", 10).unwrap();
        assert_eq!(longe.len(), 2);
        assert_eq!(longe[0].title, "Perto", "o mais próximo deve ranquear primeiro");
    }

    #[test]
    fn ignora_acentos_e_caixa() {
        let idx = index();
        assert_eq!(titles(&idx.search("ACAO", 10).unwrap()), ["Ação"]);
    }

    #[test]
    fn tolera_erro_de_digitacao() {
        let idx = index();
        assert_eq!(titles(&idx.search("trimestee", 10).unwrap()), ["Ação"]);
    }

    #[test]
    fn upsert_atualiza_conteudo() {
        let idx = index();
        idx.upsert(Path::new("/v/Perto.md"), "agora fala de cachorros").unwrap();
        assert!(titles(&idx.search("sofá", 10).unwrap()).is_empty());
        assert_eq!(titles(&idx.search("cachorros", 10).unwrap()), ["Perto"]);
    }

    #[test]
    fn snippet_destaca_termos() {
        let idx = index();
        let hits = idx.search("jardim", 10).unwrap();
        assert!(hits[0].snippet.iter().any(|s| s.hl && s.text == "jardim"));
    }
}
