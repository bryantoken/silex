# rustview

Blocos ` ```rustview ` rodam um script dentro da nota e mostram o resultado no lugar do código. É o equivalente do Dataview no Silex.

O script usa **Rhai**, uma linguagem de script embutida, escrita em Rust e com sintaxe de Rust. Ela roda no próprio app, sem compilar nada, e em sandbox: sem acesso a disco ou rede, e com limite de operações (um laço infinito vira erro em vez de travar). Os dados vêm do índice de páginas em Rust e refletem também o que ainda não foi salvo.

## API

| Função | O que faz |
| --- | --- |
| `current()` | Página atual |
| `pages()` / `pages("#tag")` / `pages("pasta/sub")` | Páginas do cofre |
| `header(n, conteúdo)` | Título de nível `n` |
| `paragraph(conteúdo)` | Parágrafo |
| `list([itens])` | Lista |
| `table([cabeçalhos], [[linhas]])` | Tabela |
| `span(texto, cor)` · `bold(texto)` · `italic(texto)` · `link(alvo[, rótulo])` | Trechos formatados |
| `group_by(arr, \|x\| chave)` | Agrupa em `[#{key, rows}]` |
| `count(arr, \|x\| condição)` | Conta itens |
| `repeat("█", n)` | Repete texto |

"conteúdo" pode ser texto Markdown (`"**negrito** e [[link]]"`), um `span`/`link`, ou um array combinando-os.

Cada página tem: `path`, `name`, `folder`, `frontmatter` (mapa), `tags`, `links`, `headings` (`level`, `text`, `line`) e `tasks`.

Cada tarefa tem: `text`, `status` (`" "`, `"x"`, `"-"`, `"/"`, ...), `completed`, `line`, `section` (título acima da tarefa) e `indent`.

## Exemplos

Painel de progresso (equivalente ao `dataviewjs` de `folha_auditoria_202609.md`): veja [exemplos/painel_tarefas.rhai](exemplos/painel_tarefas.rhai).

Notas com uma tag, em tabela:

```rustview
let rows = [];
for p in pages("#folha-pagamento") {
    rows.push([link(p.name), p.frontmatter.status ?? "—", `${p.tasks.len()}`]);
}
table(["Nota", "Status", "Tarefas"], rows);
```
