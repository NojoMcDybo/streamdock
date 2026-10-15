fn main() {
    // App-Befehle brauchen eine ausdrückliche Berechtigung (capabilities/default.json, nur Hauptfenster): so kann ein
    // Fenster mit Twitch-Inhalt (capabilities/twitch-windows.json) nur seine Fenstersteuerung aufrufen, nie diese Befehle.
    // Neuer Befehl: hier eintragen UND in capabilities/default.json als "allow-<befehl-mit-bindestrichen>".
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
        "start_stream",
        "open_console",
        "update_state",
        "update_check",
        "update_install",
        "twitch_state",
        "set_client_id",
        "auth_start",
        "auth_cancel",
        "logout",
        "live_streams",
    ])))
    .expect("Tauri-Build fehlgeschlagen");
}
