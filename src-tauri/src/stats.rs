//! Wie lange welcher Kanal geschaut wurde: Grundlage für die Sortierung „am meisten geschaut zuerst“.
//! Gespeichert in `watch.json` (Kanal → Sekunden und Anzahl Starts). Gezählt wird, solange Player und Chat offen sind.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::store;

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Watched {
    #[serde(default)]
    pub seconds: u64,
    #[serde(default)]
    pub opens: u32,
}

pub type Stats = HashMap<String, Watched>;

/// Reine Buchung: `secs` Sekunden (und ggf. einen Start) auf den Kanal.
pub fn add(stats: &mut Stats, login: &str, secs: u64, opened: bool) {
    let e = stats.entry(login.to_string()).or_default();
    e.seconds += secs;
    if opened {
        e.opens += 1;
    }
}

/// Kanal und Zeitpunkt, ab dem noch nicht gebuchte Zeit läuft. Die Sperre bleibt über Lesen und Schreiben der Datei
/// gehalten, damit sich Minutentakt, Beenden und Start nicht überschreiben.
static ACTIVE: Mutex<Option<(String, Instant)>> = Mutex::new(None);

pub fn load(app: &AppHandle) -> Stats {
    store::read::<Stats>(app, "watch.json").unwrap_or_default()
}

fn save(app: &AppHandle, stats: &Stats) {
    let _ = store::write(app, "watch.json", stats);
}

/// Ein Stream beginnt: zählt als Start, ab jetzt läuft die Zeit.
pub fn begin(app: &AppHandle, login: &str) {
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    book(app, &mut active); // der vorige Stream (falls einer lief)
    let mut stats = load(app);
    add(&mut stats, login, 0, true);
    save(app, &stats);
    *active = Some((login.to_string(), Instant::now()));
}

fn book(app: &AppHandle, active: &mut Option<(String, Instant)>) {
    if let Some((login, since)) = active.as_mut() {
        let secs = since.elapsed().as_secs();
        if secs > 0 {
            let mut stats = load(app);
            add(&mut stats, login, secs, false);
            save(app, &stats);
            *since = Instant::now();
        }
    }
}

/// Bucht die seit dem letzten Mal gelaufene Zeit (jede Minute aus der Hintergrundabfrage, damit ein Absturz wenig verliert).
pub fn flush(app: &AppHandle) {
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    book(app, &mut active);
}

/// Der Stream ist zu Ende: Rest buchen, Zähler anhalten.
pub fn end(app: &AppHandle) {
    let mut active = ACTIVE.lock().unwrap_or_else(|e| e.into_inner());
    book(app, &mut active);
    *active = None;
}

/// Sekunden pro Kanal für die Sortierung der Liste.
pub fn seconds(stats: &Stats) -> HashMap<String, u64> {
    stats.iter().map(|(k, v)| (k.clone(), v.seconds)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buchen_addiert_pro_kanal() {
        let mut s = Stats::new();
        add(&mut s, "a", 0, true);
        add(&mut s, "a", 90, false);
        add(&mut s, "a", 30, false);
        add(&mut s, "b", 5, true);
        assert_eq!(s["a"], Watched { seconds: 120, opens: 1 });
        assert_eq!(s["b"], Watched { seconds: 5, opens: 1 });
        assert_eq!(seconds(&s)["a"], 120);
    }

    #[test]
    fn alte_oder_unvollstaendige_datei_wird_gelesen() {
        let s: Stats = serde_json::from_str(r#"{"a":{"seconds":7},"b":{}}"#).unwrap();
        assert_eq!(s["a"], Watched { seconds: 7, opens: 0 });
        assert_eq!(s["b"], Watched::default());
    }
}
