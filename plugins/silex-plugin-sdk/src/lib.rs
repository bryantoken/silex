//! SDK para plugins do Silex.
//!
//! Um plugin é uma crate `cdylib` compilada para `wasm32-unknown-unknown`.
//! Implemente [`Plugin`] e registre com [`export_plugin!`]:
//!
//! ```ignore
//! use silex_plugin_sdk::*;
//!
//! #[derive(Default)]
//! struct Contador;
//!
//! impl Plugin for Contador {
//!     fn manifest(&self) -> Manifest {
//!         Manifest::new("contador", "Contador de palavras")
//!             .command("contar", "Contar palavras da seleção")
//!     }
//!     fn command(&mut self, cmd: &str, note: &Note) -> Vec<Action> {
//!         let n = note.selection.split_whitespace().count();
//!         vec![Action::notice(format!("{n} palavras"))]
//!     }
//! }
//!
//! export_plugin!(Contador);
//! ```
//!
//! O plugin roda em sandbox: não há acesso a arquivos, rede ou relógio. Tudo o
//! que ele precisa chega em [`Note`] e tudo o que ele faz volta como [`Action`]
//! ou como nós de visualização ([`Node`]).

use serde::{Deserialize, Serialize};

pub use serde_json;

#[derive(Serialize, Debug, Clone, Default)]
pub struct CommandSpec {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Debug, Clone, Default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub commands: Vec<CommandSpec>,
    pub code_blocks: Vec<String>,
}

impl Manifest {
    pub fn new(id: &str, name: &str) -> Self {
        Self { id: id.into(), name: name.into(), version: "0.1.0".into(), ..Default::default() }
    }
    pub fn version(mut self, v: &str) -> Self {
        self.version = v.into();
        self
    }
    pub fn description(mut self, d: &str) -> Self {
        self.description = d.into();
        self
    }
    pub fn author(mut self, a: &str) -> Self {
        self.author = a.into();
        self
    }
    /// Comando que aparece na paleta (Ctrl+P)
    pub fn command(mut self, id: &str, name: &str) -> Self {
        self.commands.push(CommandSpec { id: id.into(), name: name.into() });
        self
    }
    /// Linguagem de bloco ```lang que este plugin renderiza
    pub fn code_block(mut self, lang: &str) -> Self {
        self.code_blocks.push(lang.into());
        self
    }
}

/// Nota ativa no momento da chamada.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct Note {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub content: String,
    /// Texto selecionado no editor (vazio se não houver seleção)
    #[serde(default)]
    pub selection: String,
}

/// O que o app deve fazer depois de um comando.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    /// Substitui a seleção (ou insere no cursor)
    ReplaceSelection { text: String },
    /// Substitui o conteúdo inteiro da nota
    SetContent { text: String },
    /// Mostra uma mensagem
    Notice { text: String },
    /// Abre uma nota pelo nome ou caminho relativo
    Open { target: String },
}

impl Action {
    pub fn replace_selection(text: impl Into<String>) -> Self {
        Self::ReplaceSelection { text: text.into() }
    }
    pub fn set_content(text: impl Into<String>) -> Self {
        Self::SetContent { text: text.into() }
    }
    pub fn notice(text: impl Into<String>) -> Self {
        Self::Notice { text: text.into() }
    }
    pub fn open(target: impl Into<String>) -> Self {
        Self::Open { target: target.into() }
    }
}

/// Trecho de texto dentro de um nó.
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Inline {
    /// Markdown inline (negrito, links, ==realce== ...)
    Md { text: String },
    Text { text: String },
    Span { text: String, color: String },
    Bold { text: String },
    Italic { text: String },
    Link { text: String, target: String },
}

impl From<&str> for Inline {
    fn from(s: &str) -> Self {
        Inline::Md { text: s.into() }
    }
}
impl From<String> for Inline {
    fn from(s: String) -> Self {
        Inline::Md { text: s }
    }
}

/// Bloco de visualização (mesmo formato do rustview).
#[derive(Serialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Node {
    Header { level: u8, content: Vec<Inline> },
    Paragraph { content: Vec<Inline> },
    List { items: Vec<Vec<Inline>> },
    Table { headers: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Error { message: String },
}

impl Node {
    pub fn paragraph(text: impl Into<Inline>) -> Self {
        Node::Paragraph { content: vec![text.into()] }
    }
    pub fn table(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Node::Table {
            headers: headers.into_iter().map(|h| vec![Inline::from(h)]).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(|c| vec![Inline::from(c)]).collect())
                .collect(),
        }
    }
}

pub trait Plugin: Default {
    fn manifest(&self) -> Manifest;
    fn command(&mut self, _command: &str, _note: &Note) -> Vec<Action> {
        Vec::new()
    }
    fn render(&mut self, _lang: &str, _source: &str, _note: &Note) -> Vec<Node> {
        Vec::new()
    }
}

// ---- Detalhes da ABI (usados pela macro) ----

#[doc(hidden)]
pub fn __alloc(len: i32) -> i32 {
    let mut buf = Vec::<u8>::with_capacity(len.max(0) as usize);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr as i32
}

#[doc(hidden)]
pub unsafe fn __input(ptr: i32, len: i32) -> serde_json::Value {
    let bytes = unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) };
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

#[doc(hidden)]
pub fn __output<T: Serialize>(value: &T) -> i64 {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"null".to_vec()).into_boxed_slice();
    let len = bytes.len() as u64;
    let ptr = Box::into_raw(bytes) as *mut u8 as u64;
    ((ptr << 32) | len) as i64
}

#[doc(hidden)]
#[derive(Deserialize, Default)]
pub struct __CommandInput {
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub note: Note,
}

#[doc(hidden)]
#[derive(Deserialize, Default)]
pub struct __RenderInput {
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub note: Note,
}

/// Exporta as funções da ABI do Silex para o tipo `$plugin`.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[no_mangle]
        pub extern "C" fn silex_alloc(len: i32) -> i32 {
            $crate::__alloc(len)
        }

        #[no_mangle]
        pub extern "C" fn silex_manifest() -> i64 {
            let p = <$plugin as ::core::default::Default>::default();
            $crate::__output(&$crate::Plugin::manifest(&p))
        }

        #[no_mangle]
        pub unsafe extern "C" fn silex_command(ptr: i32, len: i32) -> i64 {
            let input: $crate::__CommandInput =
                $crate::serde_json::from_value(unsafe { $crate::__input(ptr, len) }).unwrap_or_default();
            let mut p = <$plugin as ::core::default::Default>::default();
            let actions = $crate::Plugin::command(&mut p, &input.command, &input.note);
            $crate::__output(&$crate::serde_json::json!({ "actions": actions }))
        }

        #[no_mangle]
        pub unsafe extern "C" fn silex_render(ptr: i32, len: i32) -> i64 {
            let input: $crate::__RenderInput =
                $crate::serde_json::from_value(unsafe { $crate::__input(ptr, len) }).unwrap_or_default();
            let mut p = <$plugin as ::core::default::Default>::default();
            let nodes = $crate::Plugin::render(&mut p, &input.lang, &input.source, &input.note);
            $crate::__output(&$crate::serde_json::json!({ "nodes": nodes }))
        }
    };
}
