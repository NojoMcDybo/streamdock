mod dock;
mod dock_win;
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
        .run(tauri::generate_context!())
        .expect("Streamdock konnte nicht starten");
}
