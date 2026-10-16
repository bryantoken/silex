//! Clientes HTTP dos serviços de IA usados pelos plugins portados:
//! - Jev (TypeSafe): julgamentos tipados (choice / noul / score) com probabilidades
//! - Anthropic / OpenAI: geração de texto em JSON (Mesa)
//!
//! As chaves vêm do cofre de credenciais (secrets.rs) e nunca passam pelo frontend.

use crate::secrets;
use serde_json::{json, Value};
use std::sync::LazyLock;
use std::time::Duration;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent("Silex/0.1")
        .build()
        .expect("cliente HTTP")
});

const JEV_URL: &str = "https://api.typesafe.ai/v1/systemone";

#[derive(Debug, Clone, Default)]
pub struct JevReply {
    pub answers: Value,
    pub model: Option<String>,
    /// Custo informado pela API, se houver
    pub cost: Option<f64>,
    pub tokens_in: u64,
    pub tokens_out: u64,
}

/// Pergunta ao Jev. Repete em 429/529 com espera exponencial, como os plugins originais.
pub async fn jev_ask(model: &str, state: &Value, questions: &Value, timeout_secs: u64, retries: u32) -> Result<JevReply, String> {
    let key = secrets::get("typesafe").ok_or("Sem API key da TypeSafe: informe a chave nas configurações do plugin.")?;
    let body = json!({ "model": if model.is_empty() { "jev-latest" } else { model }, "state": state, "questions": questions });
    let mut attempt = 0;
    loop {
        let res = CLIENT
            .post(JEV_URL)
            .bearer_auth(&key)
            .json(&body)
            .timeout(Duration::from_secs(timeout_secs))
            .send()
            .await
            .map_err(|e| if e.is_timeout() { "Jev: tempo limite.".to_string() } else { format!("Jev: {e}") })?;
        let status = res.status().as_u16();
        if (status == 429 || status == 529) && attempt < retries {
            tokio::time::sleep(Duration::from_millis(1000 * 2u64.pow(attempt))).await;
            attempt += 1;
            continue;
        }
        let json: Value = res.json().await.unwrap_or(Value::Null);
        if status == 401 {
            return Err(format!(
                "Jev recusou a API key (HTTP 401). Chave salva com {} caracteres.",
                key.chars().count()
            ));
        }
        if status >= 400 {
            let detail = json.get("detail").map(|d| format!(": {}", d.to_string().chars().take(200).collect::<String>()));
            return Err(format!("Jev retornou HTTP {status}{}.", detail.unwrap_or_default()));
        }
        let answers = json.get("answers").cloned().ok_or("Jev: resposta sem answers.")?;
        let usage = json.get("usage").or_else(|| json.get("billing"));
        let cost = [json.get("cost"), usage.and_then(|u| u.get("cost")), usage.and_then(|u| u.get("total_cost")), usage.and_then(|u| u.get("cost_usd"))]
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .find(|v| *v >= 0.0);
        let num = |k: &str, alt: &str| usage.and_then(|u| u.get(k).or_else(|| u.get(alt))).and_then(Value::as_u64).unwrap_or(0);
        return Ok(JevReply {
            answers,
            model: json.get("model").and_then(Value::as_str).map(String::from),
            cost,
            tokens_in: num("input_tokens", "prompt_tokens"),
            tokens_out: num("output_tokens", "completion_tokens"),
        });
    }
}

#[derive(Debug, Clone, Default)]
pub struct LlmReply {
    pub value: Value,
    pub model: String,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

/// Chamada a Anthropic ou OpenAI pedindo resposta em JSON (mesmo protocolo da Mesa).
pub async fn llm_call(provider: &str, model: &str, system: &str, input: &Value) -> Result<LlmReply, String> {
    if model.is_empty() {
        return Err("Configure o modelo da API nas configurações da Mesa.".into());
    }
    let anthropic = provider == "anthropic";
    let key = secrets::get(if anthropic { "anthropic" } else { "openai" })
        .ok_or("Configure a API key do provedor nas configurações da Mesa.")?;
    let content = serde_json::to_string(input).map_err(|e| e.to_string())?;
    if content.len() > 60_000 {
        return Err("Contexto excede 60 mil caracteres. Reduza as notas vinculadas.".into());
    }
    let req = if anthropic {
        CLIENT
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &key)
            .header("anthropic-version", "2023-06-01")
            .json(&json!({ "model": model, "max_tokens": 4096, "system": system, "messages": [{ "role": "user", "content": content }] }))
    } else {
        CLIENT
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&key)
            .json(&json!({ "model": model, "messages": [{ "role": "system", "content": system }, { "role": "user", "content": content }], "response_format": { "type": "json_object" } }))
    };
    let res = req
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "Tempo limite: a resposta não será aplicada.".to_string() } else { e.to_string() })?;
    let status = res.status().as_u16();
    let json: Value = res.json().await.unwrap_or(Value::Null);
    if status >= 400 {
        let detail = json.pointer("/error/message").and_then(Value::as_str).map(|d| format!(": {d}")).unwrap_or_default();
        return Err(format!("Provedor retornou HTTP {status}{detail}. Confira chave, modelo e saldo."));
    }
    if anthropic && json.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") {
        return Err("Resposta da IA foi cortada por tamanho. Tente novamente.".into());
    }
    let text = if anthropic {
        json.get("content")
            .and_then(Value::as_array)
            .map(|parts| {
                parts
                    .iter()
                    .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .collect::<String>()
            })
    } else {
        json.pointer("/choices/0/message/content").and_then(Value::as_str).map(String::from)
    }
    .ok_or("Resposta vazia.")?;
    let u = json.get("usage");
    let n = |k: &str| u.and_then(|u| u.get(k)).and_then(Value::as_u64).unwrap_or(0);
    Ok(LlmReply {
        value: parse_json(&text)?,
        model: json.get("model").and_then(Value::as_str).unwrap_or(model).to_string(),
        tokens_in: if anthropic { n("input_tokens") } else { n("prompt_tokens") },
        tokens_out: if anthropic { n("output_tokens") } else { n("completion_tokens") },
        cache_write: n("cache_creation_input_tokens"),
        cache_read: n("cache_read_input_tokens"),
    })
}

/// Extrai JSON de uma resposta (aceita cercas ``` e texto em volta).
pub fn parse_json(text: &str) -> Result<Value, String> {
    let clean = text.trim();
    let clean = clean
        .strip_prefix("```json")
        .or_else(|| clean.strip_prefix("```"))
        .unwrap_or(clean)
        .trim()
        .trim_end_matches("```")
        .trim();
    if let Ok(v) = serde_json::from_str(clean) {
        return Ok(v);
    }
    if let (Some(a), Some(b)) = (clean.find('{'), clean.rfind('}')) {
        if b > a {
            if let Ok(v) = serde_json::from_str(&clean[a..=b]) {
                return Ok(v);
            }
        }
    }
    Err("A IA não retornou JSON válido. Tente novamente.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_json_de_respostas_variadas() {
        assert_eq!(parse_json("{\"ok\":true}").unwrap()["ok"], true);
        assert_eq!(parse_json("```json\n{\"a\":1}\n```").unwrap()["a"], 1);
        assert_eq!(parse_json("Claro! {\"b\":2} pronto").unwrap()["b"], 2);
        assert!(parse_json("sem json").is_err());
    }
}
