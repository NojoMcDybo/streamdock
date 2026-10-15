//! Twitch-API: Gerätecode-Anmeldung, gefolgte Live-Streams, Profilbilder. Alles läuft in Rust, das Token
//! erreicht kein Skript einer Seite. Dateien liegen im Datenordner der App (`settings.json`, `token.json`).
//! Reine Parser stehen oben und sind getestet, die Netzwerkaufrufe darunter.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::store;
use tauri::{AppHandle, Emitter};

const ID: &str = "https://id.twitch.tv/oauth2/";
const API: &str = "https://api.twitch.tv/helix/";
const SCOPE: &str = "user:read:follows";

// ---------- reine Teile ----------

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Stream {
    pub login: String,
    pub name: String,
    pub game: String,
    pub title: String,
    pub viewers: u64,
    pub thumbnail: String,
    pub avatar: String,
    pub started_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
struct Token {
    access_token: String,
    refresh_token: String,
    #[serde(default)]
    user_id: String,
}

#[derive(Debug, PartialEq)]
struct Device {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

/// Client-ID aus der Twitch-Konsole: nur Kleinbuchstaben und Ziffern, 20 bis 40 Zeichen.
pub fn valid_client_id(id: &str) -> bool {
    (20..=40).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Die Anmeldeseite öffnen wir im Browser, aber nur wenn sie wirklich bei Twitch liegt.
pub fn safe_verification(uri: &str) -> bool {
    uri.parse::<tauri::Url>()
        .map(|u| u.scheme() == "https" && u.host_str().is_some_and(crate::stream::host_allowed))
        .unwrap_or(false)
}

fn api_message(v: &Value) -> String {
    let m = text(v, "message");
    if m.is_empty() { "unbekannter Fehler".into() } else { m }
}

fn parse_device(v: &Value) -> Result<Device, String> {
    let d = Device {
        device_code: text(v, "device_code"),
        user_code: text(v, "user_code"),
        verification_uri: text(v, "verification_uri"),
        expires_in: v.get("expires_in").and_then(Value::as_u64).unwrap_or(0),
        interval: v.get("interval").and_then(Value::as_u64).unwrap_or(5).max(1),
    };
    if d.device_code.is_empty() || d.user_code.is_empty() || d.verification_uri.is_empty() || d.expires_in == 0 {
        return Err(format!("Anmeldung nicht gestartet: {}", api_message(v)));
    }
    Ok(d)
}

fn parse_token(v: &Value) -> Result<Token, String> {
    let t = Token { access_token: text(v, "access_token"), refresh_token: text(v, "refresh_token"), user_id: String::new() };
    if t.access_token.is_empty() || t.refresh_token.is_empty() {
        return Err(api_message(v));
    }
    Ok(t)
}

struct Raw {
    id: String,
    login: String,
    name: String,
    game: String,
    title: String,
    viewers: u64,
    thumbnail: String,
    started_at: String,
}

/// Seite der gefolgten Streams und der Zeiger auf die nächste Seite.
fn parse_streams(v: &Value) -> (Vec<Raw>, Option<String>) {
    let rows = v.get("data").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let raw = rows
        .iter()
        .filter(|s| !text(s, "user_login").is_empty())
        .map(|s| Raw {
            id: text(s, "user_id"),
            login: text(s, "user_login"),
            name: text(s, "user_name"),
            game: text(s, "game_name"),
            title: text(s, "title"),
            viewers: s.get("viewer_count").and_then(Value::as_u64).unwrap_or(0),
            thumbnail: text(s, "thumbnail_url"),
            started_at: text(s, "started_at"),
        })
        .collect();
    let cursor = v.pointer("/pagination/cursor").and_then(Value::as_str).filter(|c| !c.is_empty()).map(String::from);
    (raw, cursor)
}

/// `{user_id: Profilbild-Adresse}` aus der Antwort von `users`.
fn parse_users(v: &Value) -> HashMap<String, String> {
    v.get("data")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .map(|u| (text(u, "id"), text(u, "profile_image_url")))
        .filter(|(id, url)| !id.is_empty() && !url.is_empty())
        .collect()
}

/// Twitch liefert Profilbilder in festen Größen (28, 50, 70, 150, 300, 600); 150 reicht für die Karten.
fn avatar_url(url: &str) -> String {
    match url.rfind('-') {
        Some(i) if url[i + 1..].split('.').next().is_some_and(|s| s.split('x').count() == 2 && s.split('x').all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))) => {
            let ext = url[i..].find('.').map_or("", |d| &url[i + d..]);
            format!("{}-150x150{}", &url[..i], ext)
        }
        _ => url.to_string(),
    }
}

fn thumb_url(template: &str) -> String {
    template.replace("{width}", "440").replace("{height}", "248")
}

/// Anzeigeliste: meistgesehene zuerst.
fn build_streams(raw: Vec<Raw>, avatars: &HashMap<String, String>) -> Vec<Stream> {
    let mut v: Vec<Stream> = raw
        .into_iter()
        .map(|r| Stream {
            avatar: avatars.get(&r.id).map(|u| avatar_url(u)).unwrap_or_default(),
            thumbnail: thumb_url(&r.thumbnail),
            name: if r.name.is_empty() { r.login.clone() } else { r.name },
            login: r.login,
            game: r.game,
            title: r.title,
            viewers: r.viewers,
            started_at: r.started_at,
        })
        .collect();
    v.sort_by(|a, b| b.viewers.cmp(&a.viewers).then_with(|| a.login.cmp(&b.login)));
    v
}

/// Prozentkodierung für Abfragewerte (Zeiger der Seitenblätterung).
fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

// ---------- Dateien ----------

fn client_id(app: &AppHandle) -> Option<String> {
    store::read::<Value>(app, "settings.json").map(|v| text(&v, "client_id")).filter(|c| valid_client_id(c))
}

fn load_token(app: &AppHandle) -> Option<Token> {
    store::read::<Token>(app, "token.json").filter(|t| !t.access_token.is_empty())
}

// ---------- Netzwerk ----------

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(15))).http_status_as_error(false).build().into()
}

fn finish(r: Result<ureq::http::Response<ureq::Body>, ureq::Error>) -> Result<(u16, Value), String> {
    let mut r = r.map_err(|e| format!("Keine Verbindung zu Twitch: {e}"))?;
    let status = r.status().as_u16();
    let v = r.body_mut().read_json::<Value>().unwrap_or(Value::Null);
    Ok((status, v))
}

fn post_form(url: &str, form: &[(&str, &str)]) -> Result<(u16, Value), String> {
    finish(agent().post(url).send_form(form.iter().copied()))
}

fn get_once(cid: &str, tok: &Token, path: &str) -> Result<(u16, Value), String> {
    finish(agent().get(&format!("{API}{path}")).header("Authorization", &format!("Bearer {}", tok.access_token)).header("Client-Id", cid).call())
}

fn api_get(app: &AppHandle, cid: &str, tok: &mut Token, path: &str) -> Result<Value, String> {
    for attempt in 0..2 {
        let (status, v) = get_once(cid, tok, path)?;
        if status == 200 {
            return Ok(v);
        }
        if status == 401 && attempt == 0 {
            refresh(app, cid, tok)?; // Zugangstoken läuft nach etwa 4 Stunden ab
            continue;
        }
        return Err(format!("Twitch-API {status}: {}", api_message(&v)));
    }
    Err("Twitch-API: unerwartet".into())
}

fn refresh(app: &AppHandle, cid: &str, tok: &mut Token) -> Result<(), String> {
    let (status, v) =
        post_form(&format!("{ID}token"), &[("client_id", cid), ("grant_type", "refresh_token"), ("refresh_token", &tok.refresh_token)])?;
    match (status, parse_token(&v)) {
        (200, Ok(new)) => {
            tok.access_token = new.access_token;
            tok.refresh_token = new.refresh_token; // einmal verwendbar: neues speichern
            store::write(app, "token.json", tok)
        }
        _ => {
            store::remove(app, "token.json"); // beim nächsten Versuch neu anmelden
            Err(format!("Anmeldung abgelaufen, bitte neu anmelden ({})", api_message(&v)))
        }
    }
}

/// Zwei gleichzeitige Abfragen würden das einmal verwendbare Aktualisierungstoken verbrauchen.
static LIVE: Mutex<()> = Mutex::new(());

/// Gefolgte Live-Streams; `avatars` holt zusätzlich die Profilbild-Adressen (für die Oberfläche, nicht für die Hintergrundabfrage).
fn live_blocking(app: &AppHandle, avatars_wanted: bool) -> Result<Vec<Stream>, String> {
    let _one_at_a_time = LIVE.lock().map_err(|_| "intern: gesperrt".to_string())?;
    let cid = client_id(app).ok_or("Keine Client-ID hinterlegt")?;
    let mut tok = load_token(app).ok_or("Nicht bei Twitch angemeldet")?;
    if tok.user_id.is_empty() {
        let v = api_get(app, &cid, &mut tok, "users")?;
        tok.user_id = v.pointer("/data/0/id").and_then(Value::as_str).ok_or("Twitch lieferte keine Nutzer-ID")?.to_string();
        store::write(app, "token.json", &tok)?;
    }
    let (mut raw, mut after) = (Vec::new(), String::new());
    for _ in 0..10 {
        let path = format!("streams/followed?first=100&user_id={}{after}", tok.user_id);
        let v = api_get(app, &cid, &mut tok, &path)?;
        let (page, next) = parse_streams(&v);
        raw.extend(page);
        match next {
            Some(c) => after = format!("&after={}", urlenc(&c)),
            None => break,
        }
    }
    // Profilbilder: ein Fehler hier verhindert die Liste nicht
    let mut avatars = HashMap::new();
    let ids: Vec<&str> = raw.iter().map(|r| r.id.as_str()).filter(|i| avatars_wanted && !i.is_empty()).collect();
    for chunk in ids.chunks(100) {
        let q = chunk.iter().map(|i| format!("id={}", urlenc(i))).collect::<Vec<_>>().join("&");
        if let Ok(v) = api_get(app, &cid, &mut tok, &format!("users?{q}")) {
            avatars.extend(parse_users(&v));
        }
    }
    Ok(build_streams(raw, &avatars))
}

// ---------- Befehle (nur für das Hauptfenster) ----------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchState {
    client_id: bool,
    logged_in: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStart {
    user_code: String,
    verification_uri: String,
}

#[derive(Serialize, Clone)]
struct AuthEvent {
    ok: bool,
    message: String,
}

/// Zählt die Anmeldungen: eine neue oder ein Abbruch beendet die Abfrage der alten.
static JOB: AtomicU32 = AtomicU32::new(0);

#[tauri::command]
pub fn twitch_state(app: AppHandle) -> TwitchState {
    TwitchState { client_id: client_id(&app).is_some(), logged_in: load_token(&app).is_some() }
}

/// Dateien ändern nur unter der Sperre `LIVE`, sonst schreibt eine laufende Abfrage das Token wieder zurück.
/// Läuft in einem Hintergrund-Thread: ein Wartender darf die Oberfläche nicht blockieren.
async fn locked<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _one_at_a_time = LIVE.lock().unwrap_or_else(|e| e.into_inner());
        f()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_client_id(app: AppHandle, id: String) -> Result<(), String> {
    let id = id.trim().to_lowercase();
    if !valid_client_id(&id) {
        return Err("Das sieht nicht nach einer Client-ID aus (20 bis 40 Zeichen, nur Buchstaben und Ziffern).".into());
    }
    locked(move || {
        if client_id(&app).is_some_and(|old| old != id) {
            store::remove(&app, "token.json"); // das Token gehört zur alten Client-ID
        }
        set_setting(&app, "client_id", json!(id))
    })
    .await?
}

#[tauri::command]
pub async fn logout(app: AppHandle) -> Result<(), String> {
    JOB.fetch_add(1, Ordering::SeqCst);
    locked(move || store::remove(&app, "token.json")).await
}

#[tauri::command]
pub fn auth_cancel() {
    JOB.fetch_add(1, Ordering::SeqCst);
}

/// Startet die Gerätecode-Anmeldung: Code und Adresse gehen an die Oberfläche, im Hintergrund wird abgefragt;
/// das Ergebnis kommt als Ereignis `auth`.
#[tauri::command]
pub async fn auth_start(app: AppHandle) -> Result<AuthStart, String> {
    let cid = client_id(&app).ok_or("Keine Client-ID hinterlegt")?;
    let c2 = cid.clone();
    let device = tauri::async_runtime::spawn_blocking(move || {
        let (_, v) = post_form(&format!("{ID}device"), &[("client_id", &c2), ("scopes", SCOPE)])?;
        parse_device(&v)
    })
    .await
    .map_err(|e| e.to_string())??;
    if !safe_verification(&device.verification_uri) {
        return Err("Twitch lieferte eine unerwartete Anmeldeadresse, Abbruch.".into());
    }
    let generation = JOB.fetch_add(1, Ordering::SeqCst) + 1;
    let _ = tauri_plugin_opener::open_url(&device.verification_uri, None::<&str>);
    let out = AuthStart { user_code: device.user_code.clone(), verification_uri: device.verification_uri.clone() };
    std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(device.expires_in);
        let emit = |ok: bool, message: &str| {
            let _ = app.emit("auth", AuthEvent { ok, message: message.into() });
        };
        while Instant::now() < end {
            std::thread::sleep(Duration::from_secs(device.interval));
            if JOB.load(Ordering::SeqCst) != generation {
                return; // abgebrochen oder durch eine neue Anmeldung ersetzt
            }
            let form = [("client_id", cid.as_str()), ("scopes", SCOPE), ("device_code", device.device_code.as_str()), ("grant_type", "urn:ietf:params:oauth:grant-type:device_code")];
            match post_form(&format!("{ID}token"), &form) {
                Ok((200, v)) => match parse_token(&v).and_then(|t| store::write(&app, "token.json", &t)) {
                    Ok(()) => return emit(true, ""),
                    Err(e) => return emit(false, &e),
                },
                Ok((_, v)) if text(&v, "message") == "authorization_pending" => {}
                Ok((_, v)) => return emit(false, &format!("Anmeldung abgelehnt: {}", api_message(&v))),
                Err(_) => {} // kurzer Netzfehler: nächste Runde
            }
        }
        emit(false, "Die Zeit für die Anmeldung ist abgelaufen.");
    });
    Ok(out)
}

#[tauri::command]
pub async fn live_streams(app: AppHandle) -> Result<Vec<Stream>, String> {
    tauri::async_runtime::spawn_blocking(move || live_blocking(&app, true)).await.map_err(|e| e.to_string())?
}

/// Für die Hintergrundabfrage (blockiert, ohne Profilbilder).
pub fn live_now(app: &AppHandle) -> Result<Vec<Stream>, String> {
    live_blocking(app, false)
}

/// Client-ID hinterlegt und angemeldet?
pub fn connected(app: &AppHandle) -> bool {
    client_id(app).is_some() && load_token(app).is_some()
}

/// Serialisiert Lesen-Ändern-Schreiben von `settings.json` (Tray-Schalter und Client-ID schreiben dieselbe Datei).
static SETTINGS: Mutex<()> = Mutex::new(());

pub fn setting_bool(app: &AppHandle, key: &str, default: bool) -> bool {
    store::read::<Value>(app, "settings.json").and_then(|v| v.get(key).and_then(Value::as_bool)).unwrap_or(default)
}

pub fn set_setting(app: &AppHandle, key: &str, val: Value) -> Result<(), String> {
    let _g = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    let mut v = store::read::<Value>(app, "settings.json").filter(Value::is_object).unwrap_or_else(|| json!({}));
    v[key] = val;
    store::write(app, "settings.json", &v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn streams_json() -> Value {
        json!({
            "data": [
                {"id": "1", "user_id": "10", "user_login": "klein", "user_name": "Klein", "game_name": "", "title": "a\tb",
                 "viewer_count": 7, "started_at": "2026-10-15T10:00:00Z",
                 "thumbnail_url": "https://static-cdn.jtvnw.net/previews-ttv/live_user_klein-{width}x{height}.jpg"},
                {"id": "2", "user_id": "20", "user_login": "caedrel", "user_name": "Caedrel", "game_name": "League of Legends",
                 "title": "Worlds", "viewer_count": 12345, "thumbnail_url": "https://x/y-{width}x{height}.jpg"},
                {"user_id": "30", "viewer_count": 99}
            ],
            "pagination": {"cursor": "eyJiIjpudWxsfQ"}
        })
    }

    #[test]
    fn streams_parsen() {
        let (raw, cursor) = parse_streams(&streams_json());
        assert_eq!(raw.len(), 2, "Eintrag ohne user_login wird übergangen");
        assert_eq!(cursor.as_deref(), Some("eyJiIjpudWxsfQ"));
        assert_eq!((raw[1].login.as_str(), raw[1].viewers), ("caedrel", 12345));
    }

    #[test]
    fn streams_ohne_zeiger_und_kaputte_antworten() {
        assert_eq!(parse_streams(&json!({"data": [], "pagination": {}})).1, None);
        assert_eq!(parse_streams(&json!({"data": [], "pagination": {"cursor": ""}})).1, None);
        for bad in [Value::Null, json!({}), json!({"data": "x"}), json!({"data": [1, null, "a"]})] {
            let (raw, c) = parse_streams(&bad);
            assert!(raw.is_empty() && c.is_none(), "{bad}");
        }
    }

    #[test]
    fn anzeigeliste_sortiert_und_ergänzt() {
        let (raw, _) = parse_streams(&streams_json());
        let avatars = parse_users(&json!({"data": [
            {"id": "20", "profile_image_url": "https://static-cdn.jtvnw.net/jtv_user_pictures/ab-profile_image-300x300.png"},
            {"id": "", "profile_image_url": "x"}, {"id": "10", "profile_image_url": ""}]}));
        assert_eq!(avatars.len(), 1);
        let list = build_streams(raw, &avatars);
        assert_eq!(list.iter().map(|s| s.login.as_str()).collect::<Vec<_>>(), ["caedrel", "klein"]);
        assert_eq!(list[0].avatar, "https://static-cdn.jtvnw.net/jtv_user_pictures/ab-profile_image-150x150.png");
        assert_eq!(list[0].thumbnail, "https://x/y-440x248.jpg");
        assert_eq!(list[1].avatar, "", "ohne Bild bleibt leer");
        assert_eq!(list[1].name, "Klein");
    }

    #[test]
    fn gleiche_zuschauer_sortieren_nach_name() {
        let r = |l: &str| Raw { id: String::new(), login: l.into(), name: String::new(), game: String::new(), title: String::new(), viewers: 5, thumbnail: String::new(), started_at: String::new() };
        let list = build_streams(vec![r("b"), r("a")], &HashMap::new());
        assert_eq!(list[0].login, "a");
    }

    #[test]
    fn bildadressen() {
        assert_eq!(avatar_url("https://c/p/ab-profile_image-300x300.png"), "https://c/p/ab-profile_image-150x150.png");
        assert_eq!(avatar_url("https://c/p/ab-profile_image-70x70.jpeg"), "https://c/p/ab-profile_image-150x150.jpeg");
        assert_eq!(avatar_url("https://c/p/ohne-groesse.png"), "https://c/p/ohne-groesse.png");
        assert_eq!(avatar_url("https://c/p/x-axb.png"), "https://c/p/x-axb.png");
        assert_eq!(avatar_url(""), "");
        assert_eq!(thumb_url("u-{width}x{height}.jpg"), "u-440x248.jpg");
    }

    #[test]
    fn gerätecode_antwort() {
        let ok = json!({"device_code": "d", "user_code": "ABCDEFGH", "verification_uri": "https://www.twitch.tv/activate?public=true&device-code=ABCDEFGH", "expires_in": 1800, "interval": 5});
        let d = parse_device(&ok).unwrap();
        assert_eq!((d.user_code.as_str(), d.expires_in, d.interval), ("ABCDEFGH", 1800, 5));
        assert!(parse_device(&json!({"status": 400, "message": "invalid client"})).unwrap_err().contains("invalid client"));
        assert_eq!(parse_device(&json!({"device_code": "d", "user_code": "u", "verification_uri": "v", "expires_in": 1, "interval": 0})).unwrap().interval, 1);
    }

    #[test]
    fn token_antwort() {
        let t = parse_token(&json!({"access_token": "a", "refresh_token": "r", "expires_in": 14000, "scope": ["user:read:follows"], "token_type": "bearer"})).unwrap();
        assert_eq!((t.access_token.as_str(), t.refresh_token.as_str()), ("a", "r"));
        assert_eq!(parse_token(&json!({"status": 400, "message": "authorization_pending"})).unwrap_err(), "authorization_pending");
        assert!(parse_token(&json!({"access_token": "a"})).is_err());
    }

    #[test]
    fn client_id_prüfung() {
        assert!(valid_client_id("abcdefghij0123456789klmnopqrst"));
        assert!(!valid_client_id("kurz"));
        assert!(!valid_client_id(&"a".repeat(41)));
        assert!(!valid_client_id("ABCDEFGHIJ0123456789KLMNOPQRST"));
        assert!(!valid_client_id("abcdefghij0123456789klmnopqr&t"));
        assert!(!valid_client_id("abcdefghij0123456789klmnopqr t"));
    }

    #[test]
    fn nur_twitch_anmeldeadressen_werden_geöffnet() {
        assert!(safe_verification("https://www.twitch.tv/activate?public=true&device-code=ABCDEFGH"));
        assert!(!safe_verification("http://www.twitch.tv/activate"));
        assert!(!safe_verification("https://evil.example/activate"));
        assert!(!safe_verification("https://twitch.tv.evil.example/"));
        assert!(!safe_verification("file:///C:/Windows/System32/calc.exe"));
        assert!(!safe_verification("javascript:alert(1)"));
        assert!(!safe_verification(""));
    }

    /// Echte Twitch-Endpunkte mit erfundenen Zugangsdaten: klare Fehler statt Absturz.
    /// Läuft nur auf Wunsch: `cargo test -- --ignored`.
    #[test]
    #[ignore = "braucht Netz"]
    fn echte_api_mit_erfundenen_zugangsdaten() {
        let (s, v) = post_form(&format!("{ID}device"), &[("client_id", "erfunden"), ("scopes", SCOPE)]).unwrap();
        assert!(s >= 400, "device: {s}");
        assert!(parse_device(&v).is_err());

        let fake = Token { access_token: "x".into(), refresh_token: "y".into(), user_id: String::new() };
        let (s, v) = get_once("erfunden", &fake, "users").unwrap();
        assert_eq!(s, 401, "users: {v}");
        assert_ne!(api_message(&v), "unbekannter Fehler");

        let (s, v) = post_form(&format!("{ID}token"), &[("client_id", "erfunden"), ("grant_type", "refresh_token"), ("refresh_token", "y")]).unwrap();
        assert_ne!(s, 200, "token: {v}");
        assert!(parse_token(&v).is_err());
    }

    #[test]
    fn prozentkodierung() {
        assert_eq!(urlenc("abc-_.~123"), "abc-_.~123");
        assert_eq!(urlenc("a=b&c d/é"), "a%3Db%26c%20d%2F%C3%A9");
    }
}
