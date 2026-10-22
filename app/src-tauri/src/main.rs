#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(any(target_os = "linux", test))]
mod downloads;
mod runtime;
use runtime::{Runtime, ShellState};
use std::sync::Arc;
use tauri::{Manager, RunEvent};

fn trusted_origin(url: &tauri::Url) -> bool {
    url.port().is_none()
        && (url.scheme() == "tauri" && url.host_str() == Some("localhost")
            || url.scheme() == "http" && url.host_str() == Some("tauri.localhost")
            || url.scheme() == "https" && url.host_str() == Some("tauri.localhost"))
}

#[tauri::command]
fn get_runtime(
    window: tauri::WebviewWindow,
    state: tauri::State<'_, Arc<ShellState>>,
) -> Result<Runtime, String> {
    let url = window
        .url()
        .map_err(|_| "Cannot identify requesting view")?;
    let trusted = trusted_origin(&url);
    #[cfg(debug_assertions)]
    let trusted = trusted
        || url.scheme() == "http"
            && url.host_str() == Some("localhost")
            && url.port() == Some(5173);
    if window.label() != "main" || !trusted {
        return Err("Runtime access denied".into());
    }
    state.runtime.lock().unwrap().clone()
}

fn main() {
    let state = ShellState::new();
    let setup_state = state.clone();
    let builder = tauri::Builder::default()
        // Plugin registration precedes setup: a duplicate process never starts a backend.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }));
    #[cfg(feature = "native-e2e")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder
        .manage(state)
        .invoke_handler(tauri::generate_handler![get_runtime])
        .setup(move |app| {
            let config = app
                .config()
                .app
                .windows
                .iter()
                .find(|window| window.label == "main")
                .ok_or_else(|| std::io::Error::other("Main window configuration missing"))?;
            let window = tauri::WebviewWindowBuilder::from_config(app, config)?;
            #[cfg(target_os = "linux")]
            let window = window.on_download(|webview, event| match event {
                tauri::webview::DownloadEvent::Requested { url, destination } => {
                    if !downloads::trusted_backup_url(&url) {
                        return false;
                    }
                    let directory = webview.app_handle().path().download_dir().or_else(|_| {
                        webview
                            .app_handle()
                            .path()
                            .home_dir()
                            .map(|home| home.join("Downloads"))
                    });
                    match directory
                        .ok()
                        .and_then(|directory| downloads::destination(&directory).ok())
                    {
                        Some(path) => {
                            *destination = path;
                            true
                        }
                        None => {
                            eprintln!("momo: cannot prepare backup download destination");
                            false
                        }
                    }
                }
                tauri::webview::DownloadEvent::Finished { success: false, .. } => {
                    eprintln!("momo: backup download failed");
                    true
                }
                _ => true,
            });
            window.build()?;
            let resources = app.path().resource_dir()?;
            let data = app
                .path()
                .app_data_dir()?
                .join(if cfg!(feature = "native-e2e") {
                    "v2-e2e"
                } else if cfg!(debug_assertions) {
                    "v2-development"
                } else {
                    "v2"
                });
            std::thread::spawn(move || {
                if let Err(error) = runtime::start(setup_state.clone(), &resources, &data) {
                    eprintln!("momo: {error}");
                    *setup_state.runtime.lock().unwrap() = Err(error);
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == "main" {
                window.app_handle().exit(0);
            }
        })
        .build(tauri::generate_context!())
        .expect("Cannot initialize momo desktop")
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                app.state::<Arc<ShellState>>().shutdown();
            }
        });
}

#[cfg(test)]
mod origin_tests {
    use super::trusted_origin;
    #[test]
    fn accepts_only_the_packaged_native_origins() {
        for origin in [
            "tauri://localhost/",
            "http://tauri.localhost/",
            "https://tauri.localhost/",
        ] {
            assert!(trusted_origin(&origin.parse().unwrap()));
        }
        for origin in [
            "http://tauri.localhost:8080/",
            "https://tauri.localhost:8080/",
            "tauri://localhost:8080/",
            "https://example.com/",
            "http://localhost:5173/",
        ] {
            assert!(!trusted_origin(&origin.parse().unwrap()), "{origin}");
        }
    }
}
