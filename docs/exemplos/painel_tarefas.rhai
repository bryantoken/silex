// Painel de progresso das tarefas da nota atual.
// Equivalente em rustview do bloco dataviewjs de folha_auditoria_202609.md.
// Barra: branco = feito [x], vermelho = não se aplica [-], vazio = falta [ ]

let tasks = current().tasks;
if tasks.is_empty() {
    paragraph("_Sem tarefas neste arquivo._");
    return;
}

fn is_done(t) { t.status == "x" || t.status == "X" }
fn is_na(t) { t.status == "-" }

fn bar(done, na, total) {
    let size = 10;
    if total == 0 { return []; }
    let d = (done.to_float() / total.to_float() * size.to_float()).round().to_int();
    let n = (na.to_float() / total.to_float() * size.to_float()).round().to_int();
    if d + n > size { n = size - d; }
    let e = size - d - n;
    [span(repeat("█", d), "#e8e8e8"), span(repeat("█", n), "#e5534b"), span(repeat("░", e), "#555")]
}

let total = tasks.len();
let done = count(tasks, |t| is_done(t));
let na = count(tasks, |t| is_na(t));
let ap = total - na; // aplicáveis
let pct = if ap > 0 { (done.to_float() / ap.to_float() * 100.0).round().to_int() } else { 100 };

let titulo = `Progresso geral — ${done}/${ap} (${pct}%)`;
if na > 0 { titulo += ` · ${na} N/A`; }
header(3, titulo);
paragraph(bar(done, na, total));

let rows = [];
for g in group_by(tasks, |t| if t.section == "" { "Geral" } else { t.section }) {
    let t = g.rows.len();
    let d = count(g.rows, |x| is_done(x));
    let n = count(g.rows, |x| is_na(x));
    let a = t - n;
    let feito = `${d}/${a}`;
    if n > 0 { feito += ` (${n} N/A)`; }
    rows.push([g.key, bar(d, n, t), feito]);
}
table(["Fase", "Progresso", "Concluído"], rows);
