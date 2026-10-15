//! Tray-Symbol: Menü mit den gerade live gefolgten Kanälen (Klick startet den Stream), Schalter für die
//! „ist live“-Meldung, Öffnen, Stream beenden, Beenden.

use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Wry};

use crate::twitch::{self, Stream};

const TRAY_ID: &str = "streamdock";
const MAX_LIVE: usize = 8;
const LIVE_PREFIX: &str = "live:";

/// `12345` -> `12.345`
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(c);
    }
    out
}

/// Menüzeilen `(Kennung, Text)` für die ersten `MAX_LIVE` Live-Kanäle; die Kennung trägt den Kanalnamen.
pub fn live_entries(list: &[Stream]) -> Vec<(String, String)> {
    list.iter()
        .take(MAX_LIVE)
        .map(|s| (format!("{LIVE_PREFIX}{}", s.login), format!("{}  ·  {}", s.name, thousands(s.viewers))))
        .collect()
}

pub fn tooltip(live: usize) -> String {
    if live == 0 { "Streamdock".into() } else { format!("Streamdock · {live} live") }
}

/// Kanalname aus einer Menükennung `live:<Kanal>`.
fn live_channel(id: &str) -> Option<&str> {
    id.strip_prefix(LIVE_PREFIX).filter(|c| !c.is_empty())
}

fn build_menu(app: &AppHandle, entries: &[(String, String)], notify: bool) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;
    if !entries.is_empty() {
        menu.append(&MenuItem::with_id(app, "none", "Live jetzt", false, None::<&str>)?)?;
        for (id, text) in entries {
            menu.append(&MenuItem::with_id(app, id, text, true, None::<&str>)?)?;
        }
        menu.append(&PredefinedMenuItem::separator(app)?)?;
    }
    menu.append(&MenuItem::with_id(app, "open", "Streamdock öffnen", true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "end", "Stream beenden", true, None::<&str>)?)?;
    menu.append(&CheckMenuItem::with_id(app, "notify", "Melden, wenn jemand live geht", true, notify, None::<&str>)?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", "Streamdock beenden", true, None::<&str>)?)?;
    Ok(menu)
}

/// Zuletzt gesetztes Menü (Zeilen, Schalter): nur bei Änderung neu bauen.
static LAST: Mutex<Option<(Vec<(String, String)>, bool)>> = Mutex::new(None);

fn apply(app: &AppHandle, entries: Vec<(String, String)>) {
    let notify = twitch::setting_bool(app, "notify", true);
    if let Ok(mut last) = LAST.lock() {
        if last.as_ref() == Some(&(entries.clone(), notify)) {
            return;
        }
        if let Some(tray) = app.tray_by_id(TRAY_ID) {
            if let Ok(menu) = build_menu(app, &entries, notify) {
                let ok = tray.set_menu(Some(menu)).is_ok();
                let _ = tray.set_tooltip(Some(tooltip(entries.len())));
                #[cfg(debug_assertions)]
                println!("[tray] Menü gesetzt ({ok}): {:?}", entries.iter().map(|e| e.1.as_str()).collect::<Vec<_>>());
                #[cfg(not(debug_assertions))]
                let _ = ok;
                *last = Some((entries, notify));
            }
        }
    }
}

/// Neue Live-Liste ins Tray-Menü übernehmen (aus dem Hintergrund-Thread).
pub fn update(app: &AppHandle, list: &[Stream]) {
    apply(app, live_entries(list));
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let menu = build_menu(app.handle(), &[], twitch::setting_bool(app.handle(), "notify", true))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().expect("Programmsymbol").clone())
        .tooltip(tooltip(0))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id().as_ref() {
            "open" => crate::show_main(app),
            "end" => crate::stop_in_background(app),
            "quit" => app.exit(0),
            "notify" => {
                let on = !twitch::setting_bool(app, "notify", true);
                let _ = twitch::set_setting(app, "notify", on.into());
            }
            id => {
                if let Some(channel) = live_channel(id) {
                    crate::start_in_background(app, channel.to_string());
                }
            }
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                crate::show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(login: &str, name: &str, viewers: u64) -> Stream {
        Stream {
            login: login.into(),
            name: name.into(),
            game: String::new(),
            title: String::new(),
            viewers,
            thumbnail: String::new(),
            avatar: String::new(),
            started_at: String::new(),
        }
    }

    #[test]
    fn tausenderpunkte() {
        for (n, t) in [(0, "0"), (7, "7"), (999, "999"), (1000, "1.000"), (12345, "12.345"), (1234567, "1.234.567")] {
            assert_eq!(thousands(n), t);
        }
    }

    #[test]
    fn menüzeilen_tragen_den_kanalnamen_und_sind_begrenzt() {
        let list: Vec<Stream> = (0..12).map(|i| s(&format!("k{i}"), &format!("K{i}"), 1000 + i)).collect();
        let e = live_entries(&list);
        assert_eq!(e.len(), MAX_LIVE);
        assert_eq!(e[0], ("live:k0".to_string(), "K0  ·  1.000".to_string()));
        assert!(live_entries(&[]).is_empty());
    }

    #[test]
    fn kennung_zurueck_zum_kanal() {
        assert_eq!(live_channel("live:caedrel"), Some("caedrel"));
        assert_eq!(live_channel("live:"), None);
        assert_eq!(live_channel("open"), None);
        assert_eq!(live_channel("quit"), None);
    }

    #[test]
    fn kurzinfo() {
        assert_eq!(tooltip(0), "Streamdock");
        assert_eq!(tooltip(3), "Streamdock · 3 live");
    }
}
