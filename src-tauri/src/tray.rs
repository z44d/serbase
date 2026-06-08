use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Runtime,
};

pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Serbase", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let img = image::load_from_memory(include_bytes!("../icons/32x32.png"))?.into_rgba8();
    let (w, h) = img.dimensions();
    let rgba = img.into_raw();

    TrayIconBuilder::new()
        .icon(Image::new_owned(rgba, w, h))
        .menu(&menu)
        .tooltip("serbase - Database Server Manager")
        .on_menu_event(|app: &AppHandle<R>, event| match event.id.as_ref() {
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
        })
        .build(app)?;

    Ok(())
}
