//! Selbst-Update über GitHub Releases (wie Arena, Notch und Kursbuch; NojoMcDybo/streamdock): prüft 20 s nach dem Start
//! und dann alle 6 h, auf Wunsch sofort. Installiert wird nur, was mit dem Schlüssel aus tauri.conf.json
//! (plugins.updater.pubkey) signiert ist; das erledigt das Updater-Plugin. Der Zustand geht als Ereignis "update"
//! ans Fenster (Punkt am Zahnrad, Zeile im Einstellungsmenü).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

/// Läuft gerade ein Download/eine Installation: keine weiteren Prüfungen.
static BUSY: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<Value>> = Mutex::new(None);

fn set(app: &AppHandle, mut v: Value) {
    v["current"] = app.package_info().version.to_string().into();
    *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(v.clone());
    let _ = app.emit("update", v);
}

fn state(app: &AppHandle) -> Value {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_else(|| json!({ "status": "idle", "current": app.package_info().version.to_string() }))
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(20));
        loop {
            tauri::async_runtime::block_on(check(&app, false));
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}

/// `manual`: auch Fehler zeigen; beim automatischen Prüfen (offline o. Ä.) still bleiben.
async fn check(app: &AppHandle, manual: bool) -> Value {
    if BUSY.load(Ordering::SeqCst) {
        return state(app);
    }
    if manual {
        set(app, json!({ "status": "checking" }));
    }
    let res = match app.updater() {
        Ok(u) => u.check().await.map_err(|e| e.to_string()),
        Err(e) => Err(e.to_string()),
    };
    match res {
        Ok(Some(u)) => set(app, json!({ "status": "available", "version": u.version })),
        Ok(None) => set(app, json!({ "status": "current" })),
        Err(e) if manual => set(app, json!({ "status": "error", "message": e })),
        Err(e) => eprintln!("[streamdock] Update-Prüfung: {e}"),
    }
    state(app)
}

#[tauri::command]
pub fn update_state(app: AppHandle) -> Value {
    state(&app)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Value {
    check(&app, true).await
}

#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    set(&app, json!({ "status": "downloading" }));
    let res: Result<(), String> = async {
        let update = app.updater().map_err(|e| e.to_string())?.check().await.map_err(|e| e.to_string())?.ok_or("Kein Update mehr verfügbar")?;
        update.download_and_install(|_, _| {}, || {}).await.map_err(|e| e.to_string())
    }
    .await;
    match res {
        // Windows: der Installer beendet Streamdock selbst und startet es danach neu
        Ok(()) => app.restart(),
        Err(e) => {
            BUSY.store(false, Ordering::SeqCst);
            set(&app, json!({ "status": "error", "message": e }));
            Err(e)
        }
    }
}
