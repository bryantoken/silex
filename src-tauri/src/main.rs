// Previne que uma janela de terminal abra no Windows em modo release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// A lógica fica em lib.rs (padrão Tauri v2, permite reutilizar no mobile)
fn main() {
    silex_lib::run()
}
