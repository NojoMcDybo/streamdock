//! Player- und Chat-Fenster. Beide zeigen Twitch-Seiten als Hauptdokument und haben bewusst
//! keine Tauri-Berechtigungen (siehe capabilities/default.json: nur das Fenster "main").

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::webview::NewWindowResponse;
use tauri::{
    AppHandle, LogicalSize, Manager, PhysicalPosition, Theme, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use crate::{stats, store};

/// Vorsilben der Fensternamen; jede Sitzung bekommt eine Nummer dahinter (`player-3`), weil ein eben zerstörtes
/// Fenster seinen Namen noch kurz belegt und ein Streamwechsel sonst scheitert.
pub const PLAYER: &str = "player";
pub const CHAT: &str = "chat";
static SESSION_NO: AtomicU32 = AtomicU32::new(0);
/// Marke „wird gerade beendet“ der laufenden Sitzung.
static CURRENT: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// Lage der Fenster beim letzten Schließen: Player außen links oben in physischen Pixeln (Bildschirmkoordinaten),\n/// Größen **logisch** (physisch geteilt durch die Skalierung des Monitors), damit sie auf einem Monitor mit anderer\n/// Skalierung nicht um deren Faktor wachsen oder schrumpfen. `w`/`h`: innere Größe des Players, `chat_w`: Chatbreite.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub chat_w: u32,
}

/// Gespeicherte Lage nur nutzen, wenn die Größen vernünftig sind und die linke obere Ecke noch auf einem
/// Monitor liegt (Monitore als `(x, y, Breite, Höhe)`). Fängt abgezogene Monitore und minimierte Fenster (-32000) ab.
pub fn layout_ok(l: &Layout, monitors: &[(i32, i32, u32, u32)]) -> bool {
    let sizes = (320..=10_000).contains(&l.w) && (200..=10_000).contains(&l.h) && (200..=4_000).contains(&l.chat_w);
    sizes && monitors.iter().any(|&(mx, my, mw, mh)| l.x >= mx - 20 && l.y >= my - 20 && l.x + 120 <= mx + mw as i32 && l.y + 80 <= my + mh as i32)
}

fn monitors(app: &AppHandle) -> Vec<(i32, i32, u32, u32)> {
    app.available_monitors().unwrap_or_default().iter().map(|m| (m.position().x, m.position().y, m.size().width, m.size().height)).collect()
}

fn save_layout(app: &AppHandle, player: &WebviewWindow, chat: &WebviewWindow) {
    if player.is_minimized().unwrap_or(true) || player.is_maximized().unwrap_or(true) {
        return;
    }
    let (Ok(pos), Ok(size), Ok(chat_size)) = (player.outer_position(), player.inner_size(), chat.inner_size()) else { return };
    let (Ok(ps), Ok(cs)) = (player.scale_factor(), chat.scale_factor()) else { return };
    let logical = |px: u32, scale: f64| (px as f64 / scale).round() as u32;
    let l = Layout { x: pos.x, y: pos.y, w: logical(size.width, ps), h: logical(size.height, ps), chat_w: logical(chat_size.width, cs) };
    let _ = store::write(app, "layout.json", &l);
}

/// Wartet (im Hintergrund-Thread), bis die Skalierung des Fensters zu dem Monitor passt, auf den es gerade verschoben
/// wurde: Windows rechnet die Größe beim Monitorwechsel selbst um, und eine davor gesetzte Größe würde mitwachsen.
fn settle(app: &AppHandle, win: &WebviewWindow, x: i32, y: i32) {
    let Ok(Some(monitor)) = app.monitor_from_point(x as f64 + 60.0, y as f64 + 60.0) else { return };
    for _ in 0..30 {
        if win.scale_factor().is_ok_and(|s| (s - monitor.scale_factor()).abs() < 0.01) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Gemerkte Lage anwenden: erst auf den Monitor schieben und die Skalierung abwarten, dann die Größe setzen.
fn restore(app: &AppHandle, player: &WebviewWindow, chat: &WebviewWindow, l: &Layout) {
    let _ = player.set_position(PhysicalPosition::new(l.x, l.y));
    settle(app, player, l.x, l.y);
    let _ = player.set_size(LogicalSize::new(l.w as f64, l.h as f64));
    // Der Chat kommt erst auf denselben Monitor, damit seine Breite in der richtigen Skalierung gesetzt wird
    let _ = chat.set_position(PhysicalPosition::new(l.x + 40, l.y + 40));
    settle(app, chat, l.x + 40, l.y + 40);
    let _ = chat.set_size(LogicalSize::new(l.chat_w as f64, l.h as f64));
}

/// Kanalname aus Eingabe oder Link: letzte Folge aus Buchstaben, Ziffern, `_` (wie in der AutoHotkey-Version),
/// 1 bis 25 Zeichen. Alles andere wird abgewiesen, damit nichts in die Adresse gelangt.
pub fn channel(input: &str) -> Result<String, String> {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_';
    let trimmed = input.trim_end_matches(|c: char| !word(c));
    // Beginn der letzten Folge: hinter dem letzten Nicht-Wortzeichen (Zeichenbreite beachten, nicht nur +1 Byte)
    let start = trimmed.char_indices().rev().find(|&(_, c)| !word(c)).map_or(0, |(i, c)| i + c.len_utf8());
    let name = &trimmed[start..];
    if name.is_empty() || name.len() > 25 {
        return Err(format!("Kein Kanalname erkannt: {input}"));
    }
    Ok(name.to_lowercase())
}

pub fn player_url(channel: &str) -> Url {
    format!("https://player.twitch.tv/?channel={channel}&parent=twitch.tv&autoplay=true&muted=false")
        .parse()
        .expect("Player-Adresse")
}

pub fn chat_url(channel: &str) -> Url {
    format!("https://www.twitch.tv/popout/{channel}/chat?popout=").parse().expect("Chat-Adresse")
}

/// Nur Twitch selbst (inkl. Unterdomänen wie `player.`, `passport.`) darf als Hauptseite geladen werden.
pub fn host_allowed(host: &str) -> bool {
    host == "twitch.tv" || host.ends_with(".twitch.tv")
}

fn nav_allowed(url: &Url) -> bool {
    url.scheme() == "https" && url.host_str().is_some_and(host_allowed)
}

/// Ordner für entpackte Browser-Erweiterungen (je Erweiterung ein Unterordner mit `manifest.json`), nur wenn
/// mindestens eine drin liegt. Ohne Ordner läuft alles wie bisher, ohne Erweiterungen.
fn extensions_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = store::local_dir(app).ok()?.join("extensions");
    let has_extension = std::fs::read_dir(&dir).ok()?.flatten().any(|e| e.path().join("manifest.json").is_file());
    has_extension.then_some(dir)
}

/// Oberfläche über dem Bild (overlay.js) mit den Nojo-Symbolen eingesetzt.
fn overlay_script() -> String {
    include_str!("overlay.js")
        .replace("__ICON_CLOSE__", include_str!("../../src/nojo/icons/close.svg").trim())
        .replace("__ICON_MIN__", include_str!("../../src/nojo/icons/min.svg").trim())
        .replace("__ICON_FULLSCREEN_EXIT__", include_str!("../../src/nojo/icons/fullscreen-exit.svg").trim())
        .replace("__ICON_FULLSCREEN__", include_str!("../../src/nojo/icons/fullscreen.svg").trim())
        .replace("__ICON_PIN__", include_str!("../../src/nojo/icons/pin.svg").trim())
}

fn make(app: &AppHandle, label: &str, url: Url, title: &str, (x, y, w, h): (f64, f64, f64, f64), ext: Option<&Path>) -> tauri::Result<WebviewWindow> {
    // Eigenes Browserprofil für Twitch: Anmeldung und Cookies bleiben von der App-Oberfläche getrennt
    let profile = store::local_dir(app).map_err(|e| tauri::Error::Io(std::io::Error::other(e)))?.join("twitch");
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::External(url))
        .title(title)
        .inner_size(w, h)
        .position(x, y)
        .min_inner_size(300.0, 200.0)
        .theme(Some(Theme::Dark))
        .decorations(false) // keine Titelleiste: Steuerung und Ziehen kommen aus overlay.js über dem Bild
        .shadow(true)
        .initialization_script(overlay_script())
        .data_directory(profile)
        .visible(false) // erst zeigen, wenn Lage und Andocken stehen
        .on_navigation(nav_allowed)
        .on_new_window(|_, _| NewWindowResponse::Deny);
    if let Some(dir) = ext {
        builder = builder.browser_extensions_enabled(true).extensions_path(dir);
    }
    #[cfg(debug_assertions)]
    let builder = {
        let label = label.to_string();
        builder.on_document_title_changed(move |_, t| println!("[titel] {label}: {t}"))
    };
    builder.build()
}

/// Fenster mit Twitch-Seite; schlägt das Laden der Erweiterungen fehl, geht es ohne sie weiter.
fn open(app: &AppHandle, label: &str, url: Url, title: &str, rect: (f64, f64, f64, f64)) -> tauri::Result<WebviewWindow> {
    match extensions_dir(app) {
        Some(dir) => make(app, label, url.clone(), title, rect, Some(&dir)).or_else(|e| {
            eprintln!("Erweiterungen konnten nicht geladen werden ({e}), starte ohne.");
            make(app, label, url, title, rect, None)
        }),
        None => make(app, label, url, title, rect, None),
    }
}

/// Nur Debug-Build mit `STREAMDOCK_MUTED=1`: Tests ohne Ton.
fn debug_muted(url: Url) -> Url {
    if cfg!(debug_assertions) && std::env::var_os("STREAMDOCK_MUTED").is_some() {
        return url.as_str().replace("muted=false", "muted=true").parse().unwrap_or(url);
    }
    url
}

/// Beendet Player und Chat.
pub fn stop(app: &AppHandle) {
    stats::end(app);
    if let Some(flag) = CURRENT.lock().ok().and_then(|mut g| g.take()) {
        flag.store(true, Ordering::SeqCst); // die Fenster werden gleich beide von hier zerstört
    }
    for (label, w) in app.webview_windows() {
        if label.starts_with("player-") || label.starts_with("chat-") {
            let _ = w.destroy();
        }
    }
}

/// Öffnet Player und Chat für einen Kanal (ersetzt laufende). Schließt man eins, schließt das andere mit;
/// die Lage wird beim Schließen gemerkt.
pub fn start(app: &AppHandle, input: &str) -> Result<(), String> {
    let ch = channel(input)?;
    stop(app);
    let err = |e: tauri::Error| e.to_string();
    let n = SESSION_NO.fetch_add(1, Ordering::SeqCst) + 1;
    let player = open(app, &format!("{PLAYER}-{n}"), debug_muted(player_url(&ch)), &format!("{ch} – Streamdock"), (40.0, 40.0, 1120.0, 700.0)).map_err(err)?;
    let chat = open(app, &format!("{CHAT}-{n}"), chat_url(&ch), &format!("Chat {ch}"), (1160.0, 40.0, 380.0, 700.0)).map_err(err)?;
    if let Some(l) = store::read::<Layout>(app, "layout.json").filter(|l| layout_ok(l, &monitors(app))) {
        restore(app, &player, &chat, &l);
    }
    crate::dock_win::start(&player, &chat).map_err(err)?;
    let _ = player.show();
    let _ = chat.show();
    let _ = player.set_focus();
    stats::begin(app, &ch); // ab jetzt zählt die Schauzeit dieses Kanals
    // Schließt eins, schließt das andere mit. Die gemeinsame Marke verhindert, dass sich beide Fenster
    // gegenseitig mitten im Zerstören noch einmal zerstören (das hing den UI-Thread beim Streamwechsel auf).
    let closing = Arc::new(AtomicBool::new(false));
    if let Ok(mut g) = CURRENT.lock() {
        *g = Some(closing.clone());
    }
    for (this, other) in [(&player, &chat), (&chat, &player)] {
        let (app, p, c, other, closing) = (app.clone(), player.clone(), chat.clone(), other.clone(), closing.clone());
        this.on_window_event(move |e| match e {
            WindowEvent::CloseRequested { .. } => save_layout(&app, &p, &c),
            WindowEvent::Destroyed => {
                if !closing.swap(true, Ordering::SeqCst) {
                    stats::end(&app);
                    let _ = other.destroy();
                }
            }
            _ => {}
        });
    }
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("STREAMDOCK_EVAL_FILE") {
        // Nur Debug-Build: jedes Mal, wenn die Datei sich ändert, wird ihr Inhalt in Player und Chat ausgeführt
        // (Tests ohne Maus; Ergebnisse als Seitentitel im Log).
        let windows = [player.clone(), chat.clone()];
        std::thread::spawn(move || {
            let mut last = None;
            while windows.iter().all(|w| w.is_visible().is_ok()) {
                let stamp = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                if stamp.is_some() && stamp != last {
                    last = stamp;
                    if let Ok(js) = std::fs::read_to_string(&path) {
                        windows.iter().for_each(|w| drop(w.eval(&js)));
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(400));
            }
        });
    }
    #[cfg(debug_assertions)]
    if std::env::var_os("STREAMDOCK_PROBE").is_some() {
        // Prüft, was die Twitch-Seite über die Brücke aufrufen darf: Ergebnis erscheint als Seitentitel im Log.
        // Erwartet: App-Befehle und Ereignisse abgewiesen, Fenstersteuerung erlaubt.
        let probe = r#"(async()=>{const I=window.__TAURI_INTERNALS__;if(!I||!I.invoke){document.title='PROBE kein-IPC';return}
            const label=I.metadata.currentWindow.label;const out=[];
            const tries=[['start_stream',{channel:'%%'}],['twitch_state',{}],['live_streams',{}],['logout',{}],['update_install',{}],['open_console',{}],
              ['plugin:window|set_title',{label,value:'x'}],['plugin:window|hide',{label:'main'}],['plugin:event|listen',{event:'auth',target:{kind:'Any'},handler:1}],
              ['plugin:window|set_always_on_top',{label,value:false}],['plugin:window|set_fullscreen',{label,value:false}]];
            for(const [c,a] of tries){try{await I.invoke(c,a);out.push(c+'=ERLAUBT')}catch(e){out.push(c+'='+(/not allowed|denied|Plugin not found|permission/i.test(String(e))?'abgewiesen':'Fehler:'+String(e).slice(0,30)))}}
            document.title='PROBE '+out.join(' | ');})()"#;
        let _ = player.eval(probe);
        let _ = chat.eval(probe);
    }    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_aus_name_und_link() {
        assert_eq!(channel("caedrel").unwrap(), "caedrel");
        assert_eq!(channel("  Caedrel ").unwrap(), "caedrel");
        assert_eq!(channel("https://www.twitch.tv/caedrel/").unwrap(), "caedrel");
        assert_eq!(channel("twitch.tv/Some_User_1?x=").unwrap(), "x"); // Abfragezeichen zählt zur Folge am Ende, wie in der AHK-Version
        assert_eq!(channel("a_b9").unwrap(), "a_b9");
    }

    #[test]
    fn channel_lehnt_unbrauchbares_ab() {
        assert!(channel("").is_err());
        assert!(channel("///").is_err());
        assert!(channel(&"a".repeat(26)).is_err());
        assert!(channel("a b").unwrap() == "b"); // nur die letzte Folge
    }

    #[test]
    fn channel_mit_nicht_ascii_zeichen_davor_stürzt_nicht_ab() {
        assert_eq!(channel("Café1").unwrap(), "1");
        assert_eq!(channel("日本1").unwrap(), "1");
        assert_eq!(channel("日本語/caedrel").unwrap(), "caedrel");
        assert_eq!(channel("ü_x").unwrap(), "_x");
        assert!(channel("é").is_err());
        assert!(channel("日本語").is_err());
    }

    #[test]
    fn channel_kann_die_adresse_nicht_verlassen() {
        for bad in ["x&parent=evil.com", "x/../y", "x#frag", "x%20y"] {
            let c = channel(bad).unwrap();
            assert!(c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'), "{bad} -> {c}");
        }
    }

    #[test]
    fn adressen() {
        assert_eq!(
            player_url("caedrel").as_str(),
            "https://player.twitch.tv/?channel=caedrel&parent=twitch.tv&autoplay=true&muted=false"
        );
        assert_eq!(chat_url("caedrel").as_str(), "https://www.twitch.tv/popout/caedrel/chat?popout=");
    }

    #[test]
    fn gespeicherte_lage_wird_geprüft() {
        let monitors = [(0, 0, 1920, 1080), (-2560, 0, 2560, 1440)];
        let l = |x, y, w, h, c| Layout { x, y, w, h, chat_w: c };
        assert!(layout_ok(&l(40, 40, 1120, 700, 380), &monitors));
        assert!(layout_ok(&l(-7, 0, 1120, 700, 380), &monitors), "Win11-Rand links");
        assert!(layout_ok(&l(-2000, 100, 1120, 700, 380), &monitors), "zweiter Monitor links");
        assert!(!layout_ok(&l(5000, 40, 1120, 700, 380), &monitors), "Monitor abgezogen");
        assert!(!layout_ok(&l(-32000, -32000, 1120, 700, 380), &monitors), "minimiert gespeichert");
        assert!(!layout_ok(&l(1900, 40, 1120, 700, 380), &monitors), "kaum noch sichtbar");
        assert!(!layout_ok(&l(40, 40, 10, 10, 380), &monitors), "zu klein");
        assert!(!layout_ok(&l(40, 40, 1120, 700, 5000), &monitors), "Chat zu breit");
        assert!(!layout_ok(&l(40, 40, 1120, 700, 380), &[]), "ohne Monitore");
    }

    #[test]
    fn nur_twitch_hosts() {
        assert!(host_allowed("twitch.tv"));
        assert!(host_allowed("www.twitch.tv"));
        assert!(host_allowed("player.twitch.tv"));
        assert!(!host_allowed("eviltwitch.tv"));
        assert!(!host_allowed("twitch.tv.evil.com"));
        assert!(!host_allowed("twitch.tvx"));
        assert!(!host_allowed(""));
        let u = |s: &str| s.parse::<Url>().unwrap();
        assert!(nav_allowed(&u("https://www.twitch.tv/login")));
        assert!(!nav_allowed(&u("http://www.twitch.tv/login")));
        assert!(!nav_allowed(&u("https://twitch.tv@evil.com/")));
        assert!(!nav_allowed(&u("https://evil.com/?twitch.tv")));
    }
}
