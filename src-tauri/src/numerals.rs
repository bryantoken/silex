//! Numerals — reescrita em Rust do plugin "Numerals" do Obsidian.
//!
//! Blocos ```math (ou ```numerals) viram uma calculadora linha a linha:
//!   preco = R$ 1.250          variáveis
//!   preco * 3 + 10%           porcentagem de calculadora (soma 10%)
//!   @prev / 2                 resultado da linha anterior
//!   @total                    soma desde o último título/linha em branco
//!   sqrt(16), round(2.345, 2), min(), max(), sum(), avg(), abs(), floor(), ceil(), ln(), log()
//!   # Título                  separa seções (zera @total)
//!   // comentário             ignorado
//! Números com ponto decimal; "_" separa milhares (1_000). Resultados em pt-BR.

use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Num {
    v: f64,
    pct: bool,
    cur: Option<&'static str>,
}

impl Num {
    fn plain(v: f64) -> Self {
        Self { v, pct: false, cur: None }
    }
    fn merge_cur(a: Num, b: Num) -> Option<&'static str> {
        a.cur.or(b.cur)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Op(char),
    Pct,
    Cur(&'static str),
    LParen,
    RParen,
    Comma,
}

const CURRENCIES: [&str; 5] = ["R$", "US$", "$", "€", "£"];

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let rest: String = chars[i..].iter().take(3).collect();
        if let Some(cur) = CURRENCIES.iter().find(|cur| rest.starts_with(**cur)) {
            out.push(Tok::Cur(cur));
            i += cur.chars().count();
            continue;
        }
        if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let mut j = i;
            let mut text = String::new();
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.' || chars[j] == '_') {
                if chars[j] != '_' {
                    text.push(chars[j]);
                }
                j += 1;
            }
            // notação científica 1e3
            if j < chars.len() && (chars[j] == 'e' || chars[j] == 'E') && chars.get(j + 1).is_some_and(|d| d.is_ascii_digit() || *d == '-') {
                text.push('e');
                j += 1;
                while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '-') {
                    text.push(chars[j]);
                    j += 1;
                }
            }
            out.push(Tok::Num(text.parse().map_err(|_| format!("número inválido: {text}"))?));
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == '_' || c == '@' {
            let mut j = i;
            let mut name = String::new();
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || (j == i && chars[j] == '@')) {
                name.push(chars[j]);
                j += 1;
            }
            out.push(Tok::Ident(name));
            i = j;
            continue;
        }
        out.push(match c {
            '+' | '-' | '*' | '/' | '^' | '×' | '÷' => Tok::Op(match c { '×' => '*', '÷' => '/', _ => c }),
            '%' => Tok::Pct,
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            ',' | ';' => Tok::Comma,
            _ => return Err(format!("caractere inesperado: {c}")),
        });
        i += 1;
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    vars: &'a HashMap<String, Num>,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        self.pos += 1;
        t
    }

    fn expr(&mut self) -> Result<Num, String> {
        let mut left = self.term()?;
        while let Some(Tok::Op(op @ ('+' | '-'))) = self.peek().cloned() {
            self.next();
            let right = self.term()?;
            let cur = Num::merge_cur(left, right);
            // 100 + 10% = 110 (porcentagem aplicada sobre o valor à esquerda)
            let delta = if right.pct && !left.pct { left.v * right.v } else { right.v };
            left = Num { v: if op == '+' { left.v + delta } else { left.v - delta }, pct: left.pct && right.pct, cur };
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Num, String> {
        let mut left = self.power()?;
        loop {
            match self.peek().cloned() {
                Some(Tok::Op(op @ ('*' | '/'))) => {
                    self.next();
                    let right = self.power()?;
                    if op == '/' && right.v == 0.0 {
                        return Err("divisão por zero".into());
                    }
                    left = Num { v: if op == '*' { left.v * right.v } else { left.v / right.v }, pct: false, cur: Num::merge_cur(left, right) };
                }
                Some(Tok::Ident(w)) if w == "mod" => {
                    self.next();
                    let right = self.power()?;
                    left = Num { v: left.v.rem_euclid(right.v), pct: false, cur: left.cur };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn power(&mut self) -> Result<Num, String> {
        let base = self.unary()?;
        if let Some(Tok::Op('^')) = self.peek() {
            self.next();
            let exp = self.power()?;
            return Ok(Num { v: base.v.powf(exp.v), pct: false, cur: base.cur });
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<Num, String> {
        match self.peek() {
            Some(Tok::Op('-')) => {
                self.next();
                let n = self.unary()?;
                Ok(Num { v: -n.v, ..n })
            }
            Some(Tok::Op('+')) => {
                self.next();
                self.unary()
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Num, String> {
        let mut n = self.atom()?;
        if let Some(Tok::Pct) = self.peek() {
            self.next();
            n = Num { v: n.v / 100.0, pct: true, cur: None };
        }
        Ok(n)
    }

    fn args(&mut self) -> Result<Vec<Num>, String> {
        let mut out = Vec::new();
        if let Some(Tok::RParen) = self.peek() {
            self.next();
            return Ok(out);
        }
        loop {
            out.push(self.expr()?);
            match self.next() {
                Some(Tok::Comma) => continue,
                Some(Tok::RParen) => return Ok(out),
                _ => return Err("faltou fechar parênteses".into()),
            }
        }
    }

    fn atom(&mut self) -> Result<Num, String> {
        match self.next() {
            Some(Tok::Cur(c)) => {
                let n = self.postfix()?;
                Ok(Num { cur: Some(c), ..n })
            }
            Some(Tok::Num(v)) => {
                if let Some(Tok::Cur(c)) = self.peek().cloned() {
                    self.next();
                    return Ok(Num { v, pct: false, cur: Some(c) });
                }
                Ok(Num::plain(v))
            }
            Some(Tok::LParen) => {
                let n = self.expr()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(n),
                    _ => Err("faltou fechar parênteses".into()),
                }
            }
            Some(Tok::Ident(name)) => {
                if let Some(Tok::LParen) = self.peek() {
                    self.next();
                    let a = self.args()?;
                    return call(&name, &a);
                }
                match name.as_str() {
                    "pi" | "PI" => Ok(Num::plain(std::f64::consts::PI)),
                    "e" | "E" => Ok(Num::plain(std::f64::consts::E)),
                    _ => self.vars.get(&name).copied().ok_or_else(|| format!("variável desconhecida: {name}")),
                }
            }
            Some(t) => Err(format!("símbolo inesperado: {t:?}")),
            None => Err("expressão incompleta".into()),
        }
    }
}

fn call(name: &str, a: &[Num]) -> Result<Num, String> {
    let need = |n: usize| if a.len() < n { Err(format!("{name}() precisa de {n} argumento(s)")) } else { Ok(()) };
    let cur = a.iter().find_map(|x| x.cur);
    let one = |f: fn(f64) -> f64| -> Result<Num, String> {
        need(1)?;
        Ok(Num { v: f(a[0].v), pct: false, cur })
    };
    match name {
        "sqrt" => one(f64::sqrt),
        "abs" => one(f64::abs),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "ln" => one(f64::ln),
        "log" | "log10" => one(f64::log10),
        "exp" => one(f64::exp),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "round" => {
            need(1)?;
            let d = a.get(1).map(|x| x.v as i32).unwrap_or(0);
            let f = 10f64.powi(d);
            Ok(Num { v: (a[0].v * f).round() / f, pct: false, cur })
        }
        "pow" => {
            need(2)?;
            Ok(Num { v: a[0].v.powf(a[1].v), pct: false, cur })
        }
        "min" => { need(1)?; Ok(Num { v: a.iter().map(|x| x.v).fold(f64::INFINITY, f64::min), pct: false, cur }) }
        "max" => { need(1)?; Ok(Num { v: a.iter().map(|x| x.v).fold(f64::NEG_INFINITY, f64::max), pct: false, cur }) }
        "sum" => Ok(Num { v: a.iter().map(|x| x.v).sum(), pct: false, cur }),
        "avg" | "mean" => { need(1)?; Ok(Num { v: a.iter().map(|x| x.v).sum::<f64>() / a.len() as f64, pct: false, cur }) }
        _ => Err(format!("função desconhecida: {name}()")),
    }
}

/// Formata no padrão brasileiro: 1.234.567,89 (até 6 casas, sem zeros à direita).
fn format_br(n: Num) -> String {
    if n.v.is_nan() {
        return "NaN".into();
    }
    if n.v.is_infinite() {
        return if n.v > 0.0 { "∞".into() } else { "−∞".into() };
    }
    let (v, suffix) = if n.pct { (n.v * 100.0, "%") } else { (n.v, "") };
    let decimals = if n.cur.is_some() { 2 } else { 6 };
    let s = format!("{:.*}", decimals, v.abs());
    let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
    let frac = if n.cur.is_some() { frac.to_string() } else { frac.trim_end_matches('0').to_string() };
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(c);
    }
    let sign = if v < 0.0 && (grouped != "0" || !frac.chars().all(|c| c == '0')) { "-" } else { "" };
    let num = if frac.is_empty() { grouped } else { format!("{grouped},{frac}") };
    match n.cur {
        Some(c) => format!("{sign}{c} {num}"),
        None => format!("{sign}{num}{suffix}"),
    }
}

/// Avalia o bloco e devolve um nó "calc" com o resultado de cada linha.
pub fn run(source: &str) -> Vec<Value> {
    let mut vars: HashMap<String, Num> = HashMap::new();
    let mut prev: Option<Num> = None;
    let mut section: Vec<Num> = Vec::new();
    let mut rows = Vec::new();
    for raw in source.lines() {
        let line = raw.trim();
        if line.is_empty() {
            section.clear();
            rows.push(json!({ "input": "", "kind": "blank" }));
            continue;
        }
        if line.starts_with('#') {
            section.clear();
            rows.push(json!({ "input": line.trim_start_matches('#').trim(), "kind": "heading" }));
            continue;
        }
        if line.starts_with("//") {
            rows.push(json!({ "input": line, "kind": "comment" }));
            continue;
        }
        // Comentário no fim da linha: "expr // nota" ou "expr # nota"
        let expr_part = line.split(" //").next().unwrap_or(line).split(" #").next().unwrap_or(line).trim();
        let (target, expr) = match expr_part.split_once('=') {
            Some((name, rhs)) if !name.trim().is_empty() && name.trim().chars().all(|c| c.is_alphanumeric() || c == '_' || c == ' ') && !rhs.starts_with('=') => {
                (Some(name.trim().replace(' ', "_")), rhs.trim())
            }
            _ => (None, expr_part),
        };
        let mut scope = vars.clone();
        if let Some(p) = prev {
            scope.insert("@prev".into(), p);
        }
        let total = Num { v: section.iter().map(|n| n.v).sum(), pct: false, cur: section.iter().find_map(|n| n.cur) };
        scope.insert("@total".into(), total);
        scope.insert("@sum".into(), total);
        let result = lex(expr).and_then(|toks| {
            let mut p = Parser { toks, pos: 0, vars: &scope };
            let n = p.expr()?;
            if p.pos < p.toks.len() {
                return Err(format!("sobrou texto depois da expressão: {:?}", p.toks[p.pos]));
            }
            Ok(n)
        });
        match result {
            Ok(n) => {
                if let Some(name) = &target {
                    vars.insert(name.clone(), n);
                }
                prev = Some(n);
                section.push(n);
                rows.push(json!({ "input": raw.trim_end(), "result": format_br(n), "kind": if target.is_some() { "assign" } else { "expr" } }));
            }
            Err(e) => rows.push(json!({ "input": raw.trim_end(), "error": e, "kind": "error" })),
        }
    }
    vec![json!({ "type": "calc", "rows": rows })]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn results(src: &str) -> Vec<String> {
        run(src)[0]["rows"].as_array().unwrap().iter().map(|r| r.get("result").or(r.get("error")).and_then(Value::as_str).unwrap_or("").to_string()).collect()
    }

    #[test]
    fn calcula_como_calculadora() {
        assert_eq!(results("2 + 3 * 4\n(2 + 3) * 4\n2 ^ 10\n10 / 4"), ["14", "20", "1.024", "2,5"]);
        assert_eq!(results("100 + 10%\n100 - 25%\n20%\n200 * 15%"), ["110", "75", "20%", "30"]);
        assert_eq!(results("x = 1_500\nx * 2\n@prev + 1"), ["1.500", "3.000", "3.001"]);
        assert_eq!(results("# Gastos\n10\n20\n@total\n\n5\n@total"), ["", "10", "20", "30", "", "5", "5"]);
    }

    #[test]
    fn moeda_funcoes_e_erros() {
        assert_eq!(results("preco = R$ 1250.5\npreco * 2"), ["R$ 1.250,50", "R$ 2.501,00"]);
        assert_eq!(results("sqrt(16)\nround(2.3456, 2)\nmax(1, 7, 3)\navg(2, 4)"), ["4", "2,35", "7", "3"]);
        let r = results("y + 1\n1 / 0");
        assert!(r[0].contains("desconhecida") && r[1].contains("zero"), "{r:?}");
    }
}
