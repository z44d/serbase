use std::path::PathBuf;
use std::sync::OnceLock;

static SIGNAL_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init(data_dir: PathBuf) {
    let _ = SIGNAL_DIR.set(data_dir);
}

pub fn set_servers_active(active: bool) {
    if let Some(dir) = SIGNAL_DIR.get() {
        let path = dir.join("servers_active.signal");
        if active {
            let _ = std::fs::write(&path, "");
        } else {
            let _ = std::fs::remove_file(&path);
        }
    }
}
