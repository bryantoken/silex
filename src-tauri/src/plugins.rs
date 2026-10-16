//! Plugins do Silex: módulos WebAssembly compilados a partir de Rust.
//!
//! Cada plugin é um único `.wasm` em `<cofre>/.silex/plugins/`. Ele roda isolado
//! (sem imports do host: sem disco, rede ou relógio) e com limite de "combustível",
//! então um plugin com laço infinito é interrompido em vez de travar o app.
//!
//! ABI (ver `plugins/silex-plugin-sdk`): toda troca de dados é JSON em UTF-8.
//!   memory                               memória linear exportada
//!   silex_alloc(len: i32) -> i32         reserva `len` bytes para a entrada
//!   silex_manifest() -> i64              (ptr << 32 | len) do manifesto JSON
//!   silex_command(ptr, len) -> i64       executa um comando, devolve ações
//!   silex_render(ptr, len) -> i64        renderiza um bloco de código, devolve nós

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use wasmi::{Config, Engine, Instance, Linker, Module, Store};

const FUEL: u64 = 500_000_000;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CommandSpec {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub commands: Vec<CommandSpec>,
    /// Linguagens de bloco de código que o plugin renderiza (```lang)
    #[serde(default)]
    pub code_blocks: Vec<String>,
}

#[derive(Serialize, Debug, Clone)]
pub struct PluginInfo {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub file: String,
    pub enabled: bool,
    pub error: Option<String>,
}

struct Loaded {
    manifest: Manifest,
    module: Module,
}

pub struct PluginHost {
    engine: Engine,
    loaded: HashMap<String, Loaded>,
    infos: Vec<PluginInfo>,
}

impl Default for PluginHost {
    fn default() -> Self {
        let mut config = Config::default();
        config.consume_fuel(true);
        Self { engine: Engine::new(&config), loaded: HashMap::new(), infos: Vec::new() }
    }
}

pub fn plugins_dir(root: &Path) -> PathBuf {
    root.join(".silex").join("plugins")
}

fn unpack(packed: i64) -> (usize, usize) {
    (((packed as u64) >> 32) as usize, (packed as u64 & 0xffff_ffff) as usize)
}

impl PluginHost {
    fn instantiate(&self, module: &Module) -> Result<(Store<()>, Instance), String> {
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(FUEL).map_err(|e| e.to_string())?;
        let linker = Linker::<()>::new(&self.engine);
        let instance = linker
            .instantiate_and_start(&mut store, module)
            .map_err(|e| format!("instanciação: {e}"))?;
        Ok((store, instance))
    }

    fn read_out(store: &Store<()>, instance: &Instance, packed: i64) -> Result<Value, String> {
        let memory = instance.get_memory(store, "memory").ok_or("plugin sem memória exportada")?;
        let (ptr, len) = unpack(packed);
        let mut buf = vec![0u8; len];
        memory.read(store, ptr, &mut buf).map_err(|e| e.to_string())?;
        serde_json::from_slice(&buf).map_err(|e| format!("saída JSON inválida: {e}"))
    }

    fn call(&self, module: &Module, export: &str, input: &Value) -> Result<Value, String> {
        let (mut store, instance) = self.instantiate(module)?;
        let bytes = serde_json::to_vec(input).map_err(|e| e.to_string())?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, "silex_alloc")
            .map_err(|e| format!("silex_alloc: {e}"))?;
        let ptr = alloc.call(&mut store, bytes.len() as i32).map_err(|e| e.to_string())?;
        let memory = instance.get_memory(&store, "memory").ok_or("plugin sem memória exportada")?;
        memory.write(&mut store, ptr as usize, &bytes).map_err(|e| e.to_string())?;
        let func = instance
            .get_typed_func::<(i32, i32), i64>(&store, export)
            .map_err(|e| format!("{export}: {e}"))?;
        let packed = func
            .call(&mut store, (ptr, bytes.len() as i32))
            .map_err(|e| format!("plugin falhou: {e}"))?;
        Self::read_out(&store, &instance, packed)
    }

    fn load_file(&self, path: &Path) -> Result<Loaded, String> {
        let bytes = fs::read(path).map_err(|e| e.to_string())?;
        let module = Module::new(&self.engine, &bytes[..]).map_err(|e| format!("wasm inválido: {e}"))?;
        let (mut store, instance) = self.instantiate(&module)?;
        let manifest_fn = instance
            .get_typed_func::<(), i64>(&store, "silex_manifest")
            .map_err(|e| format!("silex_manifest: {e}"))?;
        let packed = manifest_fn.call(&mut store, ()).map_err(|e| e.to_string())?;
        let manifest: Manifest = serde_json::from_value(Self::read_out(&store, &instance, packed)?)
            .map_err(|e| format!("manifesto inválido: {e}"))?;
        if manifest.id.is_empty() {
            return Err("manifesto sem id".into());
        }
        Ok(Loaded { manifest, module })
    }

    /// Recarrega todos os `.wasm` do cofre; só os habilitados ficam ativos.
    pub fn reload(&mut self, root: &Path, enabled: &[String]) {
        self.loaded.clear();
        self.infos.clear();
        let Ok(entries) = fs::read_dir(plugins_dir(root)) else { return };
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("wasm"))
            .collect();
        files.sort();
        for path in files {
            let file = path.file_name().unwrap().to_string_lossy().to_string();
            match self.load_file(&path) {
                Ok(loaded) => {
                    let on = enabled.contains(&loaded.manifest.id);
                    self.infos.push(PluginInfo { manifest: loaded.manifest.clone(), file, enabled: on, error: None });
                    if on {
                        self.loaded.insert(loaded.manifest.id.clone(), loaded);
                    }
                }
                Err(err) => self.infos.push(PluginInfo {
                    manifest: Manifest { id: file.clone(), name: file.clone(), ..Default::default() },
                    file,
                    enabled: false,
                    error: Some(err),
                }),
            }
        }
    }

    pub fn infos(&self) -> &[PluginInfo] {
        &self.infos
    }

    pub fn command(&self, plugin: &str, command: &str, note: Value) -> Result<Value, String> {
        let loaded = self.loaded.get(plugin).ok_or("plugin não está ativo")?;
        self.call(&loaded.module, "silex_command", &serde_json::json!({ "command": command, "note": note }))
    }

    pub fn render(&self, lang: &str, source: &str, note: Value) -> Option<Result<Value, String>> {
        let loaded = self.loaded.values().find(|l| l.manifest.code_blocks.iter().any(|b| b == lang))?;
        Some(self.call(
            &loaded.module,
            "silex_render",
            &serde_json::json!({ "lang": lang, "source": source, "note": note }),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Usa o plugin de exemplo compilado (cargo build --release --target wasm32-unknown-unknown
    /// em plugins/). Pulado se ainda não foi compilado.
    #[test]
    fn plugin_de_exemplo() {
        let wasm = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../plugins/target/wasm32-unknown-unknown/release/excel_table.wasm");
        if !wasm.exists() {
            eprintln!("plugin de exemplo não compilado; teste pulado");
            return;
        }
        let tmp = std::env::temp_dir().join(format!("silex-plugin-test-{}", std::process::id()));
        fs::create_dir_all(plugins_dir(&tmp)).unwrap();
        fs::copy(&wasm, plugins_dir(&tmp).join("excel_table.wasm")).unwrap();

        let mut host = PluginHost::default();
        host.reload(&tmp, &["excel-table".to_string()]);
        assert_eq!(host.infos()[0].error, None);
        assert_eq!(host.infos()[0].manifest.id, "excel-table");

        let note = serde_json::json!({ "path": "x.md", "content": "", "selection": "Nome\tValor\nA\t1" });
        let out = host.command("excel-table", "tsv-to-table", note).unwrap();
        let text = out["actions"][0]["text"].as_str().unwrap();
        assert!(text.starts_with("| Nome | Valor |\n| --- | --- |\n| A | 1 |"), "{text}");

        let nodes = host.render("tsv", "a\tb\n1\t2", serde_json::json!({})).unwrap().unwrap();
        assert_eq!(nodes["nodes"][0]["type"], "table");
        fs::remove_dir_all(&tmp).ok();
    }
}
