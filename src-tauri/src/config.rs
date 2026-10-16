//! Persistência de configurações.
//!
//! - Global (pasta de configuração do app): lista de cofres salvos.
//! - Por cofre (`<cofre>/.silex/config.json`): tema, snippets, plugins, editor.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SavedVault {
    pub path: String,
    pub name: String,
    /// Segundos desde a época Unix
    pub last_opened: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub vaults: Vec<SavedVault>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(default)]
pub struct VaultConfig {
    /// Nome do tema em `.silex/themes/<nome>.css`; vazio = tema padrão do Silex
    pub theme: String,
    pub snippets: Vec<String>,
    pub plugins: Vec<String>,
    pub accent_hue: u16,
    /// "dark", "light" ou "system"
    pub color_scheme: String,
    pub font_size: u16,
    pub line_width: u16,
    pub readable_line_width: bool,
    /// Plugins nativos ativos (None = conjunto padrão)
    pub core_plugins: Option<Vec<String>>,
    /// Configurações de cada plugin nativo (id → JSON)
    pub plugin_settings: serde_json::Map<String, serde_json::Value>,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            theme: String::new(),
            snippets: Vec::new(),
            plugins: Vec::new(),
            accent_hue: 220,
            color_scheme: "dark".into(),
            font_size: 16,
            line_width: 720,
            readable_line_width: true,
            core_plugins: None,
            plugin_settings: serde_json::Map::new(),
        }
    }
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    // Escrita atômica: grava num temporário e renomeia
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn app_config_path(dir: &Path) -> PathBuf {
    dir.join("vaults.json")
}

pub fn load_app(dir: &Path) -> AppConfig {
    read_json(&app_config_path(dir))
}

pub fn save_app(dir: &Path, cfg: &AppConfig) -> Result<(), String> {
    write_json(&app_config_path(dir), cfg)
}

/// Registra o cofre como aberto agora (no topo da lista).
pub fn touch_vault(dir: &Path, root: &Path) -> Result<AppConfig, String> {
    let mut cfg = load_app(dir);
    let path = root.to_string_lossy().to_string();
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
    cfg.vaults.retain(|v| v.path != path);
    cfg.vaults.insert(0, SavedVault { path, name, last_opened: now });
    save_app(dir, &cfg)?;
    Ok(cfg)
}

pub fn vault_config_path(root: &Path) -> PathBuf {
    root.join(".silex").join("config.json")
}

pub fn load_vault(root: &Path) -> VaultConfig {
    read_json(&vault_config_path(root))
}

pub fn save_vault(root: &Path, cfg: &VaultConfig) -> Result<(), String> {
    write_json(&vault_config_path(root), cfg)
}

impl VaultConfig {
    /// Configuração de um plugin nativo (padrão se ausente ou inválida).
    pub fn plugin<T: for<'de> Deserialize<'de> + Default>(&self, id: &str) -> T {
        self.plugin_settings.get(id).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default()
    }
}
