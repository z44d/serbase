mod commands;
mod engines;

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Manager;

use engines::{DatabaseEngine, LogCallback};

type EngineMap = Arc<Mutex<HashMap<String, Box<dyn DatabaseEngine>>>>;

#[derive(Clone, serde::Serialize)]
pub struct LogPayload {
    pub db_type: String,
    pub message: String,
    pub level: String,
}

#[derive(Clone, serde::Serialize)]
pub struct DbStatusPayload {
    pub db_type: String,
    pub running: bool,
    pub port: u16,
    pub host: String,
    pub name: String,
    pub username: String,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage::<EngineMap>(Arc::new(Mutex::new(HashMap::new())))
        .setup(|_app| {
            #[cfg(desktop)]
            {
                let img = image::load_from_memory(include_bytes!("../icons/32x32.png"))
                    .map_err(|e| format!("Failed to load icon: {}", e))?
                    .to_rgba8();
                let (w, h) = img.dimensions();
                let icon = tauri::image::Image::new_owned(img.into_raw(), w, h);

                let show = tauri::menu::MenuItem::with_id(_app, "show", "Open App", true, None::<&str>)?;
                let quit = tauri::menu::MenuItem::with_id(_app, "quit", "Quit", true, None::<&str>)?;
                let menu = tauri::menu::Menu::with_items(_app, &[&show, &quit])?;

                tauri::tray::TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&menu)
                    .tooltip("serbase")
                    .on_menu_event(move |app, event| {
                        match event.id().as_ref() {
                            "show" => {
                                if let Some(window) = app.get_webview_window("main") {
                                    let _ = window.show();
                                    let _ = window.set_focus();
                                }
                            }
                            "quit" => {
                                app.exit(0);
                            }
                            _ => {}
                        }
                    })
                    .build(_app)?;
            }

            Ok(())
        })
        .on_window_event(|_window, event| {
            if let tauri::WindowEvent::CloseRequested { api: _api, .. } = event {
                #[cfg(target_os = "macos")]
                {
                    let _ = _window.hide();
                    _api.prevent_close();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::create_database,
            commands::stop_database,
            commands::wipe_database,
            commands::get_db_status,
            commands::execute_query,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
