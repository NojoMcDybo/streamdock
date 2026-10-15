//! Kleine JSON-Dateien im Datenordner der App (`%APPDATA%\de.nojo.streamdock\`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// Nur Debug-Build: `STREAMDOCK_DATA_DIR` verlegt alle Daten (Einstellungen, Token, Browserprofil der Twitch-Fenster)
/// in einen eigenen Ordner. So rühren Tests nie die echte Anmeldung an (ein Test, der ein Token erneuert, würde das
/// einmal verwendbare Aktualisierungstoken der installierten App verbrauchen).
#[cfg(debug_assertions)]
fn override_dir(sub: &str) -> Option<PathBuf> {
    std::env::var_os("STREAMDOCK_DATA_DIR").map(|d| PathBuf::from(d).join(sub))
}

#[cfg(not(debug_assertions))]
fn override_dir(_sub: &str) -> Option<PathBuf> {
    None
}

/// Datenordner (roaming): Einstellungen, Token, Fensterlage, Schauzeiten.
pub fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = match override_dir("roaming") {
        Some(d) => d,
        None => app.path().app_data_dir().map_err(|e| e.to_string())?,
    };
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    Ok(d)
}

/// Lokaler Datenordner: Browserprofil der Twitch-Fenster und `extensions/`.
pub fn local_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let d = match override_dir("local") {
        Some(d) => d,
        None => app.path().app_local_data_dir().map_err(|e| e.to_string())?,
    };
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
