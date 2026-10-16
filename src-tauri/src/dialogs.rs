//! Diálogos de arquivo/pasta do sistema, abertos na thread principal.
//!
//! O tauri-plugin-dialog abre o diálogo numa thread separada com a janela como
//! dona. No Windows, dono e diálogo em threads diferentes acoplam as filas de
//! entrada das duas threads, e ao fechar o diálogo a janela ficava sem mouse
//! (cursor sumido, cliques ignorados). Aqui o diálogo modal roda na própria
//! thread da janela, como num app nativo; o modal tem seu próprio laço de
//! mensagens, então a janela continua sendo redesenhada.

use crate::{err, Res};
use std::path::PathBuf;
use tauri::{AppHandle, Webview, WebviewWindow};

async fn on_main<T: Send + 'static>(app: &AppHandle, f: impl FnOnce() -> T + Send + 'static) -> Res<T> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.run_on_main_thread(move || {
        let _ = tx.send(f());
    })
    .map_err(err)?;
    rx.await.map_err(err)
}

/// Devolve o foco à janela e ao WebView2 quando o diálogo fecha. Sem isso o
/// WebView2 podia continuar sem receber o mouse (só voltava trocando de janela).
fn refocus(window: &WebviewWindow) {
    let _ = window.set_focus();
    let _ = AsRef::<Webview>::as_ref(window).set_focus();
}

fn path_string(p: Option<PathBuf>) -> Option<String> {
    p.map(|p| p.to_string_lossy().to_string())
}

/// Escolher arquivo (ou pasta, com `directory`).
#[tauri::command]
pub async fn dialog_open(
    app: AppHandle,
    window: WebviewWindow,
    title: String,
    directory: bool,
    filter_name: Option<String>,
    extensions: Option<Vec<String>>,
) -> Res<Option<String>> {
    on_main(&app, move || {
        let mut d = rfd::FileDialog::new().set_parent(&window).set_title(&title);
        let out = if directory {
            d.pick_folder()
        } else {
            if let Some(exts) = extensions.as_ref().filter(|e| !e.is_empty()) {
                let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                d = d.add_filter(filter_name.as_deref().unwrap_or("Arquivos"), &exts);
            }
            d.pick_file()
        };
        refocus(&window);
        path_string(out)
    })
    .await
}

/// Escolher vários arquivos (anexos do chat).
#[tauri::command]
pub async fn dialog_open_many(app: AppHandle, window: WebviewWindow, title: String) -> Res<Vec<String>> {
    on_main(&app, move || {
        let out = rfd::FileDialog::new().set_parent(&window).set_title(&title).pick_files();
        refocus(&window);
        out.unwrap_or_default().into_iter().map(|p| p.to_string_lossy().to_string()).collect()
    })
    .await
}
