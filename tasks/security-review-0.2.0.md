# Differential Security Review: 0.1.0 → 0.2.0 (Arbeitsbaum gegen `202b6dc`)

Umfang: 37 Dateien, 401 Einfügungen, 77 Löschungen (davon 17 Symbole). Strategie: FOCUSED (Projekt klein, aber der Eingriff berührt die wichtigste Vertrauensgrenze: Fenster mit Fremdinhalt).
Frage: Kann eine Twitch-Seite (oder ein Skript darin: Drittanbieter-Code, kompromittierte Twitch-Domain, Erweiterung) mehr als ihre Fenstersteuerung erreichen, und gelangen Debug-Hilfen in den Release-Build?

## Risiko je Änderung

| Datei | Änderung | Risiko |
|---|---|---|
| `capabilities/twitch-windows.json` (neu) | Twitch-Fenster bekommen erstmals Zugriff auf die Tauri-Brücke (`remote.urls`), 6 Fensterbefehle | **HOCH** |
| `build.rs`, `capabilities/default.json` | App-Manifest: App-Befehle brauchen ausdrückliche Berechtigung, nur Hauptfenster | **HOCH** (Gegenmaßnahme zum vorigen) |
| `src/overlay.js` (neu), `stream.rs` | Skript wird in jede Seite der Twitch-Fenster eingesetzt, Fenster ohne Rahmen | **HOCH** |
| `store.rs` | Debug-Override des Datenordners (`STREAMDOCK_DATA_DIR`) | MITTEL (Token-Pfad) |
| `stream.rs`, `watch.rs` | Debug-Hilfen (Skript aus Datei ausführen, Probe, Attrappen-Feed) | MITTEL |
| `stats.rs`, `twitch.rs` | Schauzeit je Kanal lokal speichern, Sortierung | NIEDRIG (Datenschutz lokal) |
| Oberfläche, Symbol, Version | Anleitung, Farbspiel, Favoriten, Symbol | NIEDRIG |

## Befunde

**Kein Befund mit Ausnutzbarkeit über die Fenstersteuerung hinaus.** Beobachtungen:

1. **Twitch-Seiten können andere Fenster per Namen steuern (NIEDRIG).** Die Fensterbefehle nehmen ein `label`; die Berechtigung lässt sich in Tauri nicht auf das eigene Fenster einschränken. Eine Seite kann also auch das Hauptfenster (`main`) minimieren, auf Vollbild oder immer-oben setzen und `close` aufrufen (löst dort nur „in den Tray“ aus, siehe `on_window_event`). Kein Datenzugriff. *Gegenmaßnahme:* nicht möglich ohne eigenen Befehl; bewusst angenommen. Verschärfend wäre `hide`/`destroy`/`set_title`: nicht erlaubt (Test `twitch_fenster_haben_nur_fenstersteuerung`).
2. **Täuschung durch Vollbild + immer-oben (NIEDRIG).** Eine kompromittierte Twitch-Seite könnte ihr Fenster in den Vollbildmodus setzen und als „Streamdock“ wirkende Oberfläche zeigen. Voraussetzung ist Code auf `*.twitch.tv`; ein Browser hätte dasselbe Problem. Angenommen.
3. **Die Glas-Pille ist für die Seite sichtbar und klickbar (INFO).** Skripte der Seite können die Knöpfe auslösen (`shadowRoot` ist offen). Gleiche Wirkung wie Befund 1, keine neue Fähigkeit.
4. **Schauzeiten liegen als Klartext in `watch.json` (NIEDRIG, Datenschutz).** Sehverhalten je Kanal, nur im Profilordner des Benutzers, nie gesendet. In der README vermerkt.

## Was geprüft wurde (Beweise)

- **Probe aus der Seite heraus** (Debug-Build, `STREAMDOCK_PROBE`): `start_stream`, `twitch_state`, `live_streams`, `logout`, `update_install`, `open_console`, `plugin:window|set_title`, `plugin:window|hide` (auf `main`), `plugin:event|listen` → **abgewiesen**; nur `set_always_on_top` und `set_fullscreen` → erlaubt. Gilt für Player und Chat.
- **Alle App-Befehle gesperrt für Fremdinhalt:** Test `befehle_sind_in_build_und_berechtigungen_eingetragen` (Handler = `build.rs` = `capabilities/default.json`); ein neuer Befehl ohne Eintrag ist für das Hauptfenster gesperrt, nie für Twitch-Fenster offen.
- **Rechte der Twitch-Fenster fest verdrahtet:** Test `twitch_fenster_haben_nur_fenstersteuerung` (nur `core:window:allow-*`, verbotene Befehle, URLs, Fenstermuster).
- **Navigation:** Hauptseite nur `https://*.twitch.tv` (`nav_allowed`, getestet gegen `eviltwitch.tv`, `twitch.tv.evil.com`, `@`-Tricks), neue Fenster abgelehnt.
- **Token:** nur in Rust, nie in einer Seite; `LIVE`-Sperre, atomares Schreiben; die Brücke erreicht weder Dateien noch Token (kein Dateisystem-, Shell-, HTTP-Plugin in den Berechtigungen).
- **Overlay-Skript:** keine Nutzdaten in `innerHTML` (nur feste Symbole), Stilblatt per `adoptedStyleSheets` (unabhängig von der Seiten-CSP), läuft nur im obersten Fenster der Seite, Tauri-Brücke wird nur benutzt, wenn vorhanden.
- **Debug-Hilfen:** alle hinter `cfg(debug_assertions)` bzw. `cfg!(debug_assertions)` (`stream.rs`, `watch.rs`, `store.rs`, `tray.rs`). **Release-Prüfung (Binärsuche in `target\release\streamdock.exe`, 15.10.2026 20:13, 8,7 MB):** keiner der Namen `STREAMDOCK_EVAL_FILE`, `STREAMDOCK_PROBE`, `STREAMDOCK_DATA_DIR`, `STREAMDOCK_MUTED`, `STREAMDOCK_FAKE_LIVE`, `STREAMDOCK_POLL_SECS` und kein Text mit `STREAMDOCK_` ist enthalten; Gegenprobe: im Debug-Build ist `STREAMDOCK_EVAL_FILE` vorhanden, die Suche erkennt also. Das Overlay-Skript (`__sdOverlay`) ist erwartungsgemäß enthalten. Der Release-Build kennt damit weder Datenordner-Umleitung noch Skript-Ausführung aus Dateien noch die Berechtigungs-Probe.

## Nachtrag: Zurück-Knopf (`show_menu`)

Der Zurück-Knopf im Player ruft den neuen App-Befehl `show_menu` auf. Das ist der **einzige** App-Befehl, den die Twitch-Fenster aufrufen dürfen (`allow-show-menu` in `capabilities/twitch-windows.json`). Er hat keinen Parameter und tut nur `show_main` (Menü entminimieren, zeigen, fokussieren); eine Twitch-Seite gewinnt damit keine Daten und keine Wahl des Ziels. Nebenwirkung für Seiten mit Schadcode: sie können das Menü in den Vordergrund holen (Belästigung, Täuschung wie Befund 2). Alternative wäre `core:window:allow-show`/`allow-set-focus` gewesen: die wirken auf jedes Fenster per Namen und wurden bewusst nicht erlaubt.
Belege: Test `twitch_fenster_haben_nur_fenstersteuerung` erlaubt jetzt genau `core:window:allow-*` und `allow-show-menu`; Probe im Debug-Build (`STREAMDOCK_PROBE`) aus Player und Chat: alle anderen App-Befehle weiter abgewiesen, `show_menu` erlaubt. Funktionsprüfung mit echten Mausklicks im Debug-Build: Menü kommt nach vorn und hat den Fokus, Vollbild und Immer-oben des Players werden aufgehoben, Player und Chat bleiben offen.

## Blast Radius

- `store::dir` / `local_dir`: 17 Aufrufer in `stream.rs` und `twitch.rs` (Token, Einstellungen, Fensterlage, Browserprofil, Erweiterungen). Der Override wirkt nur im Debug-Build, im Release liefert `override_dir` immer `None`.
- `build_streams` (Sortierung): ein Aufrufer (`live_blocking`), 4 Tests.
- Capability-Änderung: betrifft alle Fenster mit Fremdinhalt (Player, Chat) und das Hauptfenster (neue explizite Berechtigungen, 11 Befehle).

## Abdeckung und Vertrauen

Geprüft: gesamter Diff der sicherheitsrelevanten Dateien, Probe gegen echte Twitch-Seiten, Tests. **Nicht geprüft:** ein Angriff mit echtem Schadcode auf `*.twitch.tv` (Annahme: Twitch selbst ist nicht kompromittiert); Verhalten von Drittanbieter-Erweiterungen aus dem `extensions/`-Ordner (Nutzerentscheidung, in der README gewarnt); das Verhalten bei einer künftigen Tauri-Version (die Berechtigungsprüfung hängt an Tauri). Vertrauen: mittel bis hoch für die genannten Befunde.
