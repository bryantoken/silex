# Plugins do Silex

Plugins são escritos em **Rust** e compilados para **WebAssembly**. Cada um é um único arquivo `.wasm` em `<cofre>/.silex/plugins/`.

- **Rápidos:** o host (wasmi, em Rust) carrega o módulo em milissegundos. Não há JavaScript nem um runtime pesado.
- **Isolados:** o plugin não acessa disco, rede nem relógio. Recebe a nota ativa e devolve ações ou conteúdo para exibir.
- **Seguros:** há limite de processamento por chamada. Um laço infinito é interrompido e o app não trava.

## Criar um plugin

```sh
rustup target add wasm32-unknown-unknown   # uma vez
cargo new --lib meu-plugin
```

`Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
silex-plugin-sdk = { path = "../silex-plugin-sdk" }
```

`src/lib.rs`:

```rust
use silex_plugin_sdk::*;

#[derive(Default)]
struct Contador;

impl Plugin for Contador {
    fn manifest(&self) -> Manifest {
        Manifest::new("contador", "Contador de palavras")
            .command("contar", "Contar palavras da seleção")
            .code_block("contagem")
    }

    fn command(&mut self, _cmd: &str, note: &Note) -> Vec<Action> {
        let n = note.selection.split_whitespace().count();
        vec![Action::notice(format!("{n} palavras"))]
    }

    fn render(&mut self, _lang: &str, source: &str, _note: &Note) -> Vec<Node> {
        vec![Node::paragraph(format!("**{}** palavras", source.split_whitespace().count()))]
    }
}

export_plugin!(Contador);
```

Para compilar:

```sh
cargo build --release --target wasm32-unknown-unknown
```

Depois, instale o arquivo pelo app em **Configurações → Plugins → Instalar plugin**.

## O que um plugin pode fazer

- **Comandos** (`.command(id, nome)`): aparecem na paleta (Ctrl+P). Recebem `Note { path, content, selection }` e devolvem ações:
  - `Action::replace_selection`
  - `Action::set_content`
  - `Action::notice`
  - `Action::open`
- **Blocos de código** (`.code_block("lang")`): um bloco ` ```lang ` na nota é renderizado pelo plugin. A saída usa os mesmos nós do rustview: `Header`, `Paragraph`, `List`, `Table`.

## Exemplo completo

`plugins/excel-table` é uma reescrita em Rust do plugin "Excel to Markdown Table" do Obsidian. Ele oferece:

- um comando que converte células copiadas do Excel em tabela Markdown;
- um bloco ` ```tsv ` renderizado como tabela.
