//! Kleine JSON-Dateien im Datenordner der App (`%APPDATA%\de.nojo.streamdock\`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

pub fn read<T: for<'a> Deserialize<'a>>(app: &AppHandle, name: &str) -> Option<T> {
    let bytes = std::fs::read(dir(app).ok()?.join(name)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn write<T: Serialize>(app: &AppHandle, name: &str, v: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    let (target, tmp) = (dir(app)?.join(name), dir(app)?.join(format!("{name}.tmp")));
    // erst daneben schreiben, dann ersetzen: ein Absturz mittendrin lässt die alte Datei heil (wichtig beim Token)
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &target).map_err(|e| e.to_string())
}

pub fn remove(app: &AppHandle, name: &str) {
    if let Ok(d) = dir(app) {
        let _ = std::fs::remove_file(d.join(name));
    }
}
