mod dock;
mod dock_win;
mod stats;
mod store;
mod stream;
mod tray;
mod twitch;
mod update;
mod watch;

use tauri::{AppHandle, Manager, WindowEvent};

pub(crate) fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Kanal aus den Startargumenten (das erste ist der Programmpfad).
fn channel_arg(args: &[String]) -> Option<&String> {
    args.iter().skip(1).find(|a| !a.starts_with("--"))
}

/// Fenster dürfen nicht im UI-Thread zerstört und neu gebaut werden (Streamwechsel hängte dort fest, weil WebView2
/// beim Schließen den Nachrichtenfluss braucht). Darum: eigener Thread, der UI-Thread bleibt frei.
#[tauri::command]
async fn start_stream(app: AppHandle, channel: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stream::start(&app, &channel)).await.map_err(|e| e.to_string())?
}

/// Wie `start_stream`, aber ohne Antwort: für Tray und zweiten Programmstart, die im UI-Thread laufen.
pub(crate) fn start_in_background(app: &AppHandle, channel: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        if let Err(e) = stream::start(&app, &channel) {
            eprintln!("{e}");
            show_main(&app);
        }
    });
}

pub(crate) fn stop_in_background(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || stream::stop(&app));
}

/// Öffnet die Twitch-Entwicklerkonsole im Browser (feste Adresse, kein Parameter).
#[tauri::command]
fn open_console() {
    let _ = tauri_plugin_opener::open_url("https://dev.twitch.tv/console/apps", None::<&str>);
}

/// Holt das Menü nach vorn (Zurück-Knopf im Player). Der einzige App-Befehl, den Fenster mit Twitch-Inhalt aufrufen
/// dürfen: ohne Parameter, tut immer nur dies.
#[tauri::command]
fn show_menu(app: AppHandle) {
    show_main(&app);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Zweiter Start: mit Kanal direkt Player und Chat öffnen, sonst das Menü nach vorn holen
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| match channel_arg(&args) {
            Some(c) => start_in_background(app, c.clone()),
            None => show_main(app),
        }))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            start_stream,
            open_console,
            show_menu,
            update::update_state,
            update::update_check,
            update::update_install,
            twitch::twitch_state,
            twitch::set_client_id,
            twitch::auth_start,
            twitch::auth_cancel,
            twitch::logout,
            twitch::live_streams
        ])
        .setup(|app| {
            tray::setup(app)?;
            watch::spawn(app.handle().clone());
            update::spawn(app.handle().clone());

            // Schließen = in den Tray; beendet wird über das Tray-Menü
            if let Some(w) = app.get_webview_window("main") {
                let w2 = w.clone();
                w.on_window_event(move |ev| {
                    if let WindowEvent::CloseRequested { api, .. } = ev {
                        api.prevent_close();
                        let _ = w2.hide();
                    }
                });
            }

            // Kanal als Startargument: direkt Player und Chat öffnen, ohne Menü
            let args: Vec<String> = std::env::args().collect();
            // (im Hintergrund-Thread: das Wiederherstellen der Lage wartet auf Ereignisse des UI-Threads; bei einem
            // ungültigen Kanal zeigt `start_in_background` das Menü statt abzubrechen)
            match channel_arg(&args) {
                Some(c) => start_in_background(app.handle(), c.clone()),
                None => show_main(app.handle()),
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Streamdock konnte nicht starten")
        .run(|app, event| {
            // Beenden über das Tray-Menü: die laufende Schauzeit noch buchen
            if let tauri::RunEvent::Exit = event {
                stats::end(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    /// Wer einen Befehl ergänzt, muss ihn in `build.rs` (App-Manifest) UND in `capabilities/default.json` eintragen,
    /// sonst ist er für das Hauptfenster gesperrt. Umgekehrt darf nichts erlaubt sein, was es nicht gibt.
    #[test]
    fn befehle_sind_in_build_und_berechtigungen_eingetragen() {
        let src = include_str!("lib.rs");
        let handler: BTreeSet<String> = src
            .split("generate_handler![")
            .nth(1)
            .and_then(|s| s.split(']').next())
            .expect("generate_handler")
            .split(',')
            .map(|c| c.trim().rsplit("::").next().unwrap().to_string())
            .filter(|c| !c.is_empty())
            .collect();

        let build: BTreeSet<String> = include_str!("../build.rs")
            .split("commands(&[")
            .nth(1)
            .and_then(|s| s.split(']').next())
            .expect("commands")
            .split('"')
            .skip(1)
            .step_by(2)
            .map(String::from)
            .collect();

        let caps: serde_json::Value = serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let allowed: BTreeSet<String> = caps["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p.as_str()?.strip_prefix("allow-").map(|c| c.replace('-', "_")))
            .collect();

        assert_eq!(handler, build, "Befehle im Handler und in build.rs");
        assert_eq!(handler, allowed, "Befehle im Handler und in capabilities/default.json");
        assert!(handler.len() >= 10);
    }

    /// Fenster mit Twitch-Inhalt dürfen nur Fenstersteuerung und `show_menu`, keine anderen App-Befehle und keine Ereignisse.
    #[test]
    fn twitch_fenster_haben_nur_fenstersteuerung() {
        let caps: serde_json::Value = serde_json::from_str(include_str!("../capabilities/twitch-windows.json")).unwrap();
        let perms: Vec<&str> = caps["permissions"].as_array().unwrap().iter().filter_map(|p| p.as_str()).collect();
        assert!(!perms.is_empty());
        for p in &perms {
            assert!(p.starts_with("core:window:allow-") || *p == "allow-show-menu", "unerwartete Berechtigung für Twitch-Fenster: {p}");
        }
        for forbidden in ["allow-hide", "allow-set-title", "allow-destroy", "allow-set-focus", "allow-set-position"] {
            assert!(!perms.iter().any(|p| p.ends_with(forbidden)), "{forbidden} darf nicht erlaubt sein");
        }
        assert_eq!(caps["remote"]["urls"], serde_json::json!(["https://*.twitch.tv/*", "https://twitch.tv/*"]));
        assert_eq!(caps["windows"], serde_json::json!(["player-*", "chat-*"]));
    }
}
