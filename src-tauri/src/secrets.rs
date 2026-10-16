//! Chaves de API no cofre de credenciais do sistema (Windows Credential Manager,
//! Keychain no macOS, Secret Service no Linux). Nunca ficam nas notas nem no
//! frontend: a interface só sabe se a chave existe e quantos caracteres tem.

const SERVICE: &str = "silex";

/// Nomes aceitos (evita que o frontend crie entradas arbitrárias no cofre).
const NAMES: &[&str] = &["typesafe", "anthropic", "openai", "gemini"];

fn entry(name: &str) -> Result<keyring::Entry, String> {
    if !NAMES.contains(&name) {
        return Err(format!("Chave desconhecida: {name}"));
    }
    keyring::Entry::new(SERVICE, name).map_err(|e| e.to_string())
}

pub fn get(name: &str) -> Option<String> {
    entry(name)
        .ok()?
        .get_password()
        .ok()
        .map(|s| s.split_whitespace().collect::<String>())
        .filter(|s| !s.is_empty())
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    let e = entry(name)?;
    let clean: String = value.split_whitespace().collect();
    if clean.is_empty() {
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(err.to_string()),
        }
    } else {
        e.set_password(&clean).map_err(|e| e.to_string())
    }
}

/// Quantidade de caracteres da chave salva (0 = nenhuma).
pub fn len(name: &str) -> usize {
    get(name).map(|s| s.chars().count()).unwrap_or(0)
}
