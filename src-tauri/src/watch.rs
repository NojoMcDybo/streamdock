//! Hintergrundabfrage: meldet, wenn ein gefolgter Kanal live geht, und hält die Live-Liste im Tray-Menü aktuell.
//! Die Erkennung „neu live“ (`Seen`) ist rein und getestet.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::twitch::{self, Stream};

const INTERVAL: Duration = Duration::from_secs(60);
/// Ein Kanal, der kurz offline geht und wiederkommt (Verbindungsabbruch), meldet sich nicht ein zweites Mal.
const COOLDOWN: Duration = Duration::from_secs(30 * 60);

/// Merkt, wer bei der letzten Abfrage live war und wem schon gemeldet wurde.
#[derive(Default)]
pub struct Seen {
    prev: Option<HashSet<String>>,
    notified: HashMap<String, Instant>,
}

impl Seen {
    /// Vergisst alles (Abmeldung): die nächste Abfrage ist wieder nur die Ausgangslage.
    pub fn reset(&mut self) {
        *self = Seen::default();
    }

    /// Stellen in `current`, die seit der letzten Abfrage neu live sind und gemeldet werden sollen.
    /// Die erste Abfrage nach dem Start liefert nur die Ausgangslage, sonst bekäme man beim Start eine Meldung für jeden,
    /// der ohnehin schon live ist.
    pub fn update(&mut self, current: &[Stream], now: Instant) -> Vec<usize> {
        let live: HashSet<String> = current.iter().map(|s| s.login.clone()).collect();
        let Some(prev) = self.prev.replace(live) else { return Vec::new() };
        self.notified.retain(|_, t| now.saturating_duration_since(*t) < COOLDOWN);
        let mut new = Vec::new();
        for (i, s) in current.iter().enumerate() {
            if !prev.contains(&s.login) && !self.notified.contains_key(&s.login) {
                self.notified.insert(s.login.clone(), now);
                new.push(i);
            }
        }
        new
    }
}

fn notify(app: &AppHandle, s: &Stream) {
    let body: Vec<&str> = [s.game.as_str(), s.title.as_str()].into_iter().filter(|t| !t.is_empty()).collect();
    let body: String = body.join(" · ").chars().take(140).collect();
    let _ = app.notification().builder().title(format!("{} ist live", s.name)).body(body).show();
}

/// Nur in Debug-Builds mit `STREAMDOCK_FAKE_LIVE=1`: ein erfundener Verlauf, um Meldung und Tray-Menü ohne Twitch zu prüfen.
fn fake_list(step: u32) -> Vec<Stream> {
    let s = |login: &str, viewers: u64| Stream {
        login: login.into(),
        name: login.to_uppercase(),
        game: "Testspiel".into(),
        title: format!("Attrappe {login}"),
        viewers,
        thumbnail: String::new(),
        avatar: String::new(),
        started_at: String::new(),
        watched: 0,
    };
    let mut v = vec![s("alpha", 1200), s("bravo", 80)];
    if step >= 1 {
        v.push(s("charlie", 4300));
    }
    if step >= 3 {
        v.retain(|x| x.login != "bravo");
    }
    v
}

fn debug_env(name: &str) -> Option<String> {
    if cfg!(debug_assertions) { std::env::var(name).ok() } else { None }
}

/// `None` = nicht angemeldet, `Some(Err)` = Abfrage fehlgeschlagen (vorübergehend).
fn poll(app: &AppHandle, step: u32) -> Option<Result<Vec<Stream>, String>> {
    if debug_env("STREAMDOCK_FAKE_LIVE").is_some() {
        return Some(Ok(fake_list(step)));
    }
    twitch::connected(app).then(|| twitch::live_now(app))
}

pub fn spawn(app: AppHandle) {
    let every = debug_env("STREAMDOCK_POLL_SECS").and_then(|s| s.parse().ok()).map_or(INTERVAL, Duration::from_secs);
    std::thread::spawn(move || {
        let mut seen = Seen::default();
        let mut step = 0;
        std::thread::sleep(every.min(Duration::from_secs(8))); // die App erst hochkommen lassen
        loop {
            crate::stats::flush(&app); // Schauzeit des laufenden Streams buchen (ein Absturz verliert so höchstens eine Minute)
            match poll(&app, step) {
                Some(Ok(list)) => {
                    let new = seen.update(&list, Instant::now());
                    if twitch::setting_bool(&app, "notify", true) {
                        new.into_iter().for_each(|i| notify(&app, &list[i]));
                    }
                    crate::tray::update(&app, &list);
                }
                Some(Err(_)) => {} // vorübergehend (Netz, Twitch): die letzte Liste bleibt, nächste Runde
                None => {
                    seen.reset();
                    crate::tray::update(&app, &[]);
                }
            }
            step += 1;
            std::thread::sleep(every);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(login: &str) -> Stream {
        Stream {
            login: login.into(),
            name: login.into(),
            game: String::new(),
            title: String::new(),
            viewers: 0,
            thumbnail: String::new(),
            avatar: String::new(),
            started_at: String::new(),
            watched: 0,
        }
    }

    fn logins(list: &[Stream], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| list[i].login.clone()).collect()
    }

    #[test]
    fn erste_abfrage_meldet_nichts() {
        let mut seen = Seen::default();
        assert!(seen.update(&[s("a"), s("b")], Instant::now()).is_empty());
    }

    #[test]
    fn neu_live_wird_einmal_gemeldet() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[s("a")], t0);
        let list = [s("a"), s("b")];
        assert_eq!(logins(&list, &seen.update(&list, t0)), ["b"]);
        assert!(seen.update(&list, t0).is_empty(), "bleibt live: keine zweite Meldung");
    }

    #[test]
    fn mehrere_neue_auf_einmal() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[], t0);
        let list = [s("a"), s("b"), s("c")];
        assert_eq!(logins(&list, &seen.update(&list, t0)), ["a", "b", "c"]);
    }

    #[test]
    fn kurzes_offline_und_zurueck_meldet_nicht_noch_einmal() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[s("a")], t0);
        let both = [s("a"), s("b")];
        assert_eq!(seen.update(&both, t0).len(), 1);
        seen.update(&[s("a")], t0 + Duration::from_secs(60)); // b offline
        assert!(seen.update(&both, t0 + Duration::from_secs(120)).is_empty(), "innerhalb der Sperrzeit");
    }

    #[test]
    fn nach_der_sperrzeit_wird_wieder_gemeldet() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[s("a")], t0);
        let both = [s("a"), s("b")];
        seen.update(&both, t0);
        seen.update(&[s("a")], t0 + Duration::from_secs(60));
        assert_eq!(seen.update(&both, t0 + COOLDOWN + Duration::from_secs(61)).len(), 1);
    }

    #[test]
    fn lange_live_laufende_werden_nach_der_sperrzeit_nicht_erneut_gemeldet() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[s("a")], t0);
        let both = [s("a"), s("b")];
        seen.update(&both, t0);
        assert!(seen.update(&both, t0 + COOLDOWN * 3).is_empty());
    }

    #[test]
    fn abmelden_setzt_zurueck() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[s("a")], t0);
        seen.reset();
        assert!(seen.update(&[s("a"), s("b")], t0).is_empty(), "wieder nur Ausgangslage");
    }

    #[test]
    fn leere_antwort_zwischendurch_haelt_den_zustand_sauber() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        seen.update(&[s("a")], t0);
        assert!(seen.update(&[], t0).is_empty());
        let list = [s("a")];
        assert_eq!(seen.update(&list, t0).len(), 1, "a war weg und ist wieder da, nie gemeldet");
    }

    #[test]
    fn attrappe_hat_den_erwarteten_verlauf() {
        let (mut seen, t0) = (Seen::default(), Instant::now());
        assert!(seen.update(&fake_list(0), t0).is_empty());
        let l1 = fake_list(1);
        assert_eq!(logins(&l1, &seen.update(&l1, t0)), ["charlie"]);
        assert_eq!(fake_list(3).len(), 2, "bravo ist wieder offline");
    }
}
