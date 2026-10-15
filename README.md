# Streamdock

Twitch-Player und Chat als zwei freie Fenster, die aneinander andocken. Beim Start zeigt ein Menü, welche deiner gefolgten Kanäle gerade live sind. Ersetzt das AutoHotkey-Skript in `D:\Dev\twitch-dock` (bleibt als Fallback liegen).

Design: Nojo UI (`D:\Dev\nojo-design`, per `tools\sync.ps1` nach `src/nojo/`), dazu Twitch-Akzente: Violett für Aktion und Auswahl, Rot für „live“. Flächen bleiben Nojo-Grau.

## Benutzen

- **Starten:** Programm öffnen → Menü mit den live gefolgten Kanälen (nach Zuschauern sortiert, Vorschaubild und Profilbild). Klick, oder Pfeiltasten und Enter, startet Player und Chat. Oben kann auch ein Kanalname oder Link eingegeben werden (geht ohne Twitch-Anmeldung).
- **Aus der Kommandozeile:** `streamdock.exe <Kanal oder Link>` öffnet direkt Player und Chat. Läuft die App schon, übernimmt die laufende Instanz den Kanal und wechselt den Stream.
- **Andocken:** Chat näher als 20 px an eine Playerkante loslassen → rastet ein (rechts, links, oben, unten). Angedockt folgt er dem Player beim Verschieben und Skalieren, Quermaß wie der Player. Mehr als 30 px wegziehen → löst sich. Schließt man ein Fenster, schließt das andere mit.
- **Fensterlage:** Beim Schließen merkt sich die App Position und Größe des Players und die Chatbreite.
- **Tray:** Schließen des Menüfensters legt die App in den Tray. Tray-Menü: oben die gerade live gefolgten Kanäle (bis zu 8, ein Klick startet Player und Chat), darunter Streamdock öffnen, Stream beenden, der Schalter „Melden, wenn jemand live geht“ und Streamdock beenden. Der Tooltip zeigt, wie viele live sind.
- **„X ist live“:** Im Hintergrund fragt die App jede Minute die gefolgten Live-Kanäle ab (nur wenn angemeldet) und zeigt eine Windows-Meldung, wenn ein Kanal neu live geht. Die erste Abfrage nach dem Start meldet nichts (sonst käme eine Meldung für jeden, der schon live ist), ein Kanal, der nur kurz offline war, meldet sich innerhalb von 30 Minuten nicht noch einmal. Ein Klick auf die Meldung tut nichts, gestartet wird über das Tray-Menü. Ausschalten: Schalter im Tray-Menü (wird gemerkt).
- **Chat anmelden:** Der Chat zeigt seine eigene „Anmelden“-Schaltfläche. Die Anmeldung gilt für Player und Chat und bleibt erhalten, ist aber vom Chrome-Profil getrennt.

## Einrichtung der Live-Liste (einmalig)

Twitch liefert die gefolgten Live-Kanäle nur über die offizielle API, dafür braucht es eine eigene (kostenlose) Twitch-Anwendung. Die App führt durch die Schritte:

1. Auf [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) eine Anwendung registrieren: Name **ohne „twitch“** (muss eindeutig sein), Weiterleitungs-URL `https://localhost`, Kategorie „Application Integration“, Clienttyp **Öffentlich**.
2. Die **Client-ID** in der App eintragen, dann „Mit Twitch anmelden“. Der Browser öffnet die Freigabeseite, dort „Autorisieren“ (nur Leserechte auf die gefolgten Kanäle).

Widerrufen: [twitch.tv/settings/connections](https://www.twitch.tv/settings/connections) und in der App „Von Twitch abmelden“. Das Aktualisierungstoken läuft bei Twitch nach 30 Tagen ohne Nutzung ab, dann meldet die App „Anmeldung abgelaufen“ und führt zur Neuanmeldung.

## Browser-Erweiterungen (optional)

Player und Chat laufen in einem eigenen WebView2-Profil (`%LOCALAPPDATA%\de.nojo.streamdock\twitch`), nicht in Chrome. Chrome-Erweiterungen fehlen deshalb, lassen sich aber nachrüsten: entpackte Erweiterungen (je ein Ordner mit `manifest.json`) in

```
%LOCALAPPDATA%\de.nojo.streamdock\extensions\<Name>\
```

legen und die App neu starten. Geprüft: Eine eigene Test-Erweiterung (Content-Script auf `*.twitch.tv`) wird in Player und Chat geladen. Nicht geprüft: ob eine bestimmte Erweiterung (etwa ein Werbeblocker) gegen Twitch wirkt; Werbeblocker sind ein Wettlauf mit Twitch und nicht Teil der App. Schlägt das Laden fehl, startet die App ohne Erweiterungen. Wer hier schreiben kann, kann Code in die Twitch-Fenster bringen: nur eigene, geprüfte Erweiterungen ablegen.

## Updates und Veröffentlichen (wie Arena, Notch und Kursbuch)

Streamdock aktualisiert sich selbst: 20 s nach dem Start und dann alle 6 h fragt es `https://github.com/NojoMcDybo/streamdock/releases/latest/download/latest.json`. Gibt es eine neue Version, leuchtet ein Punkt am Zahnrad; im Menü darunter steht **Installieren** (lädt, prüft die Signatur mit dem öffentlichen Schlüssel aus `tauri.conf.json`, derselbe wie bei Arena, Notch und Kursbuch, und startet neu). Dort steht auch die Version und „Nach Updates suchen“. Solange es das Repository oder ein Release nicht gibt, bleibt die Prüfung still.

Veröffentlichen: Version in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml` und `src-tauri/tauri.conf.json` erhöhen, committen, Tag `v<version>` pushen. `.github/workflows/release.yml` testet, baut, signiert (Secret `TAURI_SIGNING_PRIVATE_KEY`; der private Schlüssel liegt nur unter `%USERPROFILE%\.tauri\nojo-updater.key`) und hängt Installer und `latest.json` an das Release. **Einmalig:** Repository `NojoMcDybo/streamdock` auf GitHub anlegen und das Secret setzen (`gh secret set TAURI_SIGNING_PRIVATE_KEY < %USERPROFILE%\.tauri\nojo-updater.key`).

Lokal bauen (ohne den privaten Schlüssel): `npx tauri build --config "{\"bundle\":{\"createUpdaterArtifacts\":false}}"`. Ohne diese Option verlangt `tauri build` den Schlüssel, weil `createUpdaterArtifacts` in der Konfiguration an ist. Der lokal gebaute Installer hat keine Signatur, taugt also nur zum Installieren, nicht als Update.

## Mehrere Monitore

Beim Wechsel auf einen Monitor mit anderer Skalierung (hier 100 % und 125 %) rechnet Windows beide Fenster selbst um und verschiebt dabei den Chat. Das zählt nicht als Wegziehen: Ein Chat löst sich nur, wenn man ihn mit gedrückter Maustaste wegzieht, nach einem Monitorwechsel wird er wieder bündig angedockt (bis zu zweimal nachgebessert, dann Ruhe). Die gemerkte Fensterlage speichert Größen logisch (ohne Skalierung) und stellt sie erst her, nachdem das Fenster auf dem Monitor angekommen ist.

## Daten und Sicherheit

- `%APPDATA%\de.nojo.streamdock\`: `settings.json` (Client-ID, kein Geheimnis), `token.json` (Zugangs- und Aktualisierungstoken, **vertraulich**, nur für den Windows-Benutzer lesbar, Klartext), `layout.json` (Fensterlage).
- Das Token bleibt in Rust und erreicht kein Seiten-Skript. Die Fenster mit Twitch-Inhalt (Player, Chat) haben **keine** Tauri-Berechtigungen (geprüft: Befehle werden abgewiesen) und lassen als Hauptseite nur `https://*.twitch.tv` zu; neue Fenster (`window.open`) werden abgelehnt.
- Die Anmeldeadresse aus der Twitch-Antwort wird nur geöffnet, wenn sie `https` und eine Twitch-Domain ist. Kanalnamen werden auf `[A-Za-z0-9_]`, höchstens 25 Zeichen geprüft, bevor sie in eine Adresse kommen.

## Entwickeln

```powershell
npm install
npm run build            # Typen + Frontend
cd src-tauri; cargo test # 47 Tests (Andock-Logik, Kanalnamen, Twitch-Antworten, Fensterlage, „neu live“, Tray-Zeilen)
cargo test -- --ignored  # zusätzlich: echte Twitch-Endpunkte mit erfundenen Zugangsdaten (braucht Netz)
npx tauri build          # Release-Build und NSIS-Installer
```

- Meldung und Tray-Menü ohne Twitch prüfen (nur Debug-Build): `STREAMDOCK_FAKE_LIVE=1` und `STREAMDOCK_POLL_SECS=6` setzen, `streamdock.exe` starten. Ein erfundener Verlauf lässt „CHARLIE“ live gehen und „BRAVO“ offline.
- `npm run dev` im Browser zeigt die Oberfläche mit Attrappen (`?state=setup|login|code|list|empty|error|loading`), ohne Twitch.
- Design aktualisieren: `D:\Dev\nojo-design\tools\sync.ps1 -Only streamdock`.
- Installieren: `tools\install-streamdock.cmd` per Doppelklick im Explorer (nicht aus der Claude-App, die schreibt AppData in einen umgeleiteten Ordner).

Aufbau (`src-tauri/src`): `dock.rs` (reine Andock-Logik, getestet), `dock_win.rs` (Win32: sichtbare Fenstergrenzen, WinEvent-Hook, Zeitgeber), `stream.rs` (Player/Chat-Fenster, Lage, Navigation), `twitch.rs` (Anmeldung per Gerätecode, `streams/followed`, `users`), `store.rs` (JSON-Dateien), `watch.rs` (Hintergrundabfrage, Erkennung „neu live“, Meldung), `tray.rs` (Tray-Menü), `update.rs` (Selbst-Update), `lib.rs` (Einzelinstanz, Start). Entscheidungen und Risiken: `tasks/plan.md`.

## Herkunft und Abhängigkeiten

Tauri 2, Vite 8, TypeScript 6 (wie Helio); Rust-Crates `ureq` 3, `tauri-plugin-opener`, `tauri-plugin-single-instance`, `tauri-plugin-notification`, `tauri-plugin-updater`, `windows` 0.62. Die Andock-Logik ist ein Port von `twitch-dock.ahk`. Twitch-API: Gerätecode-Anmeldung, `GET helix/streams/followed` (Scope `user:read:follows`), `GET helix/users`.

## Grenzen

- Mit echtem Twitch-Konto ungetestet: Anmeldung bis zum Token, `streams/followed` mit echten Daten, Profilbilder von Twitchs CDN (die Parser sind mit Beispielantworten getestet, die Fehlerfälle gegen die echte API).
- Die Meldung ist mit einem erfundenen Verlauf getestet (Windows-Meldung erscheint, Tray-Menü wird aktualisiert). Im Debug-Build erscheint sie unter „Windows PowerShell“, wie der installierte Build erscheint, ist ungeprüft. Mit echten Twitch-Daten ungetestet.
- Auf zwei Monitoren (1920×1080 mit 100 %, 2560×1440 mit 125 %) getestet: 20 Sprünge zwischen den Monitoren endeten alle bündig angedockt (vorher 10 von 16), langsames Ziehen über die Grenze ohne Lücke, Fensterlage auf beiden Monitoren richtig wiederhergestellt. Beim Dauerziehen auf dem 125-%-Monitor hinkt der Chat im Mittel 3 px nach (auf dem 100-%-Monitor 0).
- Nicht getestet: vertauschte Maustasten, Windows-Abmeldung mit laufender App, die Update-Installation (es gibt noch kein Release; Oberfläche und Zustände sind mit Attrappen geprüft, die Prüfung gegen GitHub ohne Release meldet still einen Fehler).
- Die Fenster haben die normale Windows-Titelleiste (Fremdseiten lassen sich nicht mit eigenem Rahmen versehen).
