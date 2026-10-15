# Plan: Streamdock

Fortschritt läuft über die Aufgabenliste der Claude-Sitzung (eine Quelle, Regel aus `~/.claude/CLAUDE.md`). Dieses Dokument hält Entscheidungen, Risiken und die Aufgabenschnitte fest, ohne eine zweite Checkliste (`tasks/todo.md` entfällt bewusst).

## Überblick

Eigenständige Tauri-2-App für Windows. Ersetzt das AutoHotkey-Skript `D:\Dev\twitch-dock` (bleibt als Fallback liegen).

- **Menü** der live gefolgten Twitch-Kanäle: Nojo-UI-Design (`D:\Dev\nojo-design`) mit Twitch-Akzenten.
- **Player** und **Chat** als zwei freie Fenster. Der Chat rastet an einer Playerkante ein und folgt dem Player, bis man ihn wegzieht.
- Eigener Browser-Container (WebView2): Twitch-Anmeldung einmalig in der App, getrennt von Chrome. Chrome-Erweiterungen fehlen dort (Werbeblocker, BTTV/7TV), siehe „Offen“.

## Entscheidungen

| Entscheidung | Begründung |
|---|---|
| Name **Streamdock**, Ordner `D:\Dev\streamdock`, Kennung `de.nojo.streamdock` | Twitch verbietet „twitch“ im Namen von Anwendungen; Namensschema wie bei den anderen Apps (kurz, ein Wort) |
| Stack wie Helio: Tauri 2, Vite 8, TypeScript 6, ohne UI-Framework | bekannte Vorlage, `nojo-ui` ist Vanilla-TS/CSS |
| Design über `nojo-design\tools\sync.ps1` nach `src/nojo/` (Streamdock dort eintragen) | so ist die Designsprache für alle Apps gleich; nicht von Hand ändern |
| Twitch-Akzente: Violett `#9146ff` als App-Akzent (Primäraktion, Auswahl), Rot für „live“ (Punkt, Zuschauer) | Nojo-Regel „Farbe nur mit Bedeutung“: Violett = Twitch/Aktion, Rot = live. Flächen bleiben Nojo-Grau, nicht Twitch-Schwarz |
| Player und Chat laden die Twitch-Seiten direkt als Hauptdokument (`player.twitch.tv/?channel=…&parent=twitch.tv`, `twitch.tv/popout/<Kanal>/chat`) | in Chrome bereits so erprobt; eingebettetes iframe bräuchte eine gültige `parent`-Domain für `tauri.localhost` |
| Fenster der Twitch-Seiten haben **keine** Tauri-Berechtigungen (kein IPC) und lassen nur Twitch-Hosts als Hauptseite zu | fremde Seiten dürfen nie Befehle der App aufrufen |
| Twitch-API (Gerätecode-Anmeldung, `streams/followed`, `users`) komplett in Rust; Token bleibt in Rust und in `%APPDATA%\de.nojo.streamdock\` | Token erreicht kein WebView-Skript; Dateiablage wie bei Helio (Credential Manager wäre die Steigerung) |
| Andocken in Rust, Win32 über das `windows`-Crate: sichtbare Fenstergrenzen (`DwmGetWindowAttribute`), `SetWinEventHook`-Thread für sofortiges Nachziehen, Zeitgeber für „Maus losgelassen“ | direkter Port der erprobten AutoHotkey-Logik; Zustandsmaschine als reine Funktion testbar |
| Profilbilder und Vorschaubilder lädt das Menüfenster direkt per `<img>` von Twitchs CDN | kein eigener Cache in Rust nötig |
| Wichtig auf diesem Rechner: von mir gestartete Apps schreiben `%APPDATA%` in den MSIX-Container der Claude-App (Speicher `claude-app-appdata-virtualization`) | Tests laufen isoliert; Anmeldung und Installer macht der Nutzer selbst |

## Aufgaben

### Phase 1: Fundament und Risiko zuerst

**1. Gerüst** (M) — Projekt aus Helio-Vorlage: `package.json`, `vite.config.ts` (anderer Port), `tsconfig.json`, `.npmrc` (`include=dev`), `.gitignore`, `src-tauri` mit `tauri.conf.json`, `capabilities`, Icons; `nojo-design\tools\sync.ps1` um Streamdock ergänzen und ausführen.
- [ ] `npm install`, `npm run build` und `cargo check` laufen sauber
- [ ] Leeres Hauptfenster mit Nojo-Rahmen (Ziehleiste, Fensterknöpfe) startet
- Abhängigkeiten: keine

**2. Spike: Player und Chat in WebView2** (M) — `streamdock.exe <Kanal>` öffnet zwei Fenster mit den Twitch-Seiten (Befehl `start_stream`), dunkle Titelleiste, keine Berechtigungen, Navigation auf Twitch-Hosts begrenzt.
- [ ] Player zeigt Bild und Ton eines Live-Kanals, Chat lädt und scrollt
- [ ] Die Fenster haben keinen Zugriff auf `window.__TAURI__`/IPC (im Seitenskript geprüft)
- [ ] Navigation auf fremde Hosts wird abgewiesen
- Abhängigkeiten: 1. **Bricht dieser Spike (Player spielt nicht, Chat unbenutzbar), ist Variante B nicht tragfähig: dann anhalten und mit dem Nutzer entscheiden.**

### Checkpoint 1
- [ ] Gerüst baut, Spike läuft, Entscheidung „weiter“

### Phase 2: Andocken

**3. Andock-Logik (rein)** (S) — `dock.rs`: `snap_side`, `dock_rect`, `dev` und Zustandsmaschine `step(...)` (Player bewegt, Chat bewegt, Maus losgelassen, Einrasten, Lösen) als Port von `twitch-dock.ahk`.
- [ ] `cargo test`: dieselben Fälle wie `SelfTest` der AutoHotkey-Version plus Zustandsübergänge (einrasten, mitziehen, lösen, zurückspringen)
- Abhängigkeiten: keine

**4. Andocken an die Fenster** (M) — Win32-Anbindung: Fenstergrenzen über DWM, Ereignis-Thread und Zeitgeber, `SetWindowPos`, Anbindung an die Fenster aus Aufgabe 2.
- [ ] Livetest mit `SetWindowPos`-Prüfstand: Mitziehen im Dauerlauf mittlerer Abstand ≤ 1 px (AutoHotkey-Messung: 0,3 px)
- [ ] Einrasten an allen vier Kanten, Lösen, Schließen eines Fensters beendet den Dock-Thread sauber
- Abhängigkeiten: 2, 3

### Checkpoint 2
- [ ] Player + Chat docken wie die AutoHotkey-Version

### Phase 3: Twitch und Oberfläche

**5. Twitch-Backend** (M) — `twitch.rs`: Einstellungen (Client-ID), Gerätecode-Anmeldung (Code und Adresse an die Oberfläche melden, im Hintergrund abfragen), Token speichern/erneuern, `live_streams` (inkl. Profilbild-Adressen), Fehler als Text. Reine Parser getrennt.
- [ ] `cargo test` für Parser (Streams, Nutzer, Token-Antworten, Paginierung, Fehler) und Bildadress-Umbau
- [ ] Fehlerfälle mit erfundenen Zugangsdaten gegen die echte Twitch-API: klare Meldung, kein Absturz
- Abhängigkeiten: 1

**6. Oberfläche Menü** (M) — Zustände Einrichten (Client-ID) → Anmelden (Code anzeigen) → Liste; Karten mit Profilbild (rund, Ring rot = live), Name, Spiel, Titel, Zuschauer, Vorschaubild; Suche/„Anderer Kanal“; Aktualisieren; Nojo-Glas, `lightScroller`; Dev-Modus mit Attrappen, damit sich das Design im Browser prüfen lässt.
- [ ] `npm run build` ohne Typfehler
- [ ] Screenshots des Menüs mit Attrappen in allen Zuständen (Einrichten, Anmelden, Liste, leer, Fehler)
- [ ] Tastatur: Pfeile und Enter starten einen Kanal, Fokus sichtbar
- Abhängigkeiten: 5

### Checkpoint 3
- [ ] Menü zeigt Attrappen schön; echte Anmeldung kann der Nutzer durchlaufen

### Phase 4: App-Gefühl und Härtung

**7. Tray, Einzelinstanz, Fensterlage** (S) — Tray-Symbol mit Menü, Schließen = in den Tray, nur eine Instanz, Größe/Position des Players merken.
- [ ] Zweiter Start holt das laufende Fenster nach vorn; Beenden aus dem Tray beendet auch Player und Chat

**8. Twitch-Anmeldung im Chat** (S) — Schaltfläche „Im Chat anmelden“ öffnet `twitch.tv/login` im Chat-Fenster-Profil; Zustand „angemeldet“ erkennbar.
- [ ] Anmeldung bleibt nach App-Neustart erhalten (Prüfung durch den Nutzer; mein Test läuft isoliert)

**9. Prüfung** (S) — `/code-review`; Differential-Review für Anmeldung, Berechtigungen, Vertrauensgrenzen (Fenster mit Fremdinhalt, Token, Befehle).
- [ ] Befunde behoben oder begründet zurückgestellt

**10. Übergabe** (S) — `tauri build --no-bundle`, README, Verknüpfung auf dem Desktop; Installer (NSIS) baut der Nutzer selbst, siehe MSIX-Hinweis.
- [ ] Release-Build startet; README mit Einrichtung und Grenzen

### Checkpoint 4 (fertig)
- [ ] Alle Kriterien erfüllt, offene Punkte benannt

## Risiken

| Risiko | Wirkung | Gegenmaßnahme |
|---|---|---|
| Twitch-Player oder -Chat verhalten sich in WebView2 anders als in Chrome (Werbung, Codec, Anmeldung) | hoch | Spike (Aufgabe 2) vor allem anderen; Abbruch- und Entscheidungspunkt |
| Chrome-Erweiterungen (Werbeblocker, BTTV/7TV) fehlen | mittel | als „Offen“ führen; später prüfen, ob WebView2/Tauri entpackte Erweiterungen laden kann |
| Unsichtbare Win11-Fensterränder verschieben die Andockkante | mittel | sichtbare Grenzen über DWM, wie bei der AutoHotkey-Version |
| Schleifen beim Nachziehen (eigene Bewegung löst Ereignis aus) | mittel | Zustandsmaschine merkt eigene Zielrechtecke, Test in Aufgabe 3 |
| Token liegt als Datei im Profilordner | mittel | Datei nur in `%APPDATA%\de.nojo.streamdock\`, nie in Seiten-Skripte; Credential Manager als Steigerung |
| Twitch ändert Seiten/Parameter (`parent`, Popout-Chat) | mittel | URLs an einer Stelle (`twitch.rs`/`windows.rs`), README nennt sie |
| Erste Rust-Übersetzung dauert lang | niedrig | im Hintergrund starten, parallel weiterarbeiten |
| Prüfung der Anmeldung nur teilweise möglich (kein echtes Konto in meiner Sitzung) | mittel | Parser/Fehlerfälle testen, Erfolgsfall vom Nutzer prüfen lassen und offen benennen |

## Stand nach 0.2.0

Erledigt seit dem ersten Plan: Tray mit Live-Liste und „X ist live“-Meldung, Selbst-Update, Mehrmonitor-Betrieb, Erweiterungsordner (Laden getestet), Fenster ohne Leiste mit Glas-Pille, Favoriten-Sortierung nach Schauzeit, Werbung stummschalten, neue Einrichtung (Kopierknöpfe, Farbspiel), neues Symbol. Rechte der Twitch-Fenster auf Fenstersteuerung beschränkt (App-Manifest), Prüfung in `tasks/security-review-0.2.0.md`.

## Offen

- Werbung: nur Stummschaltung (Erkennung mit echter Werbung nicht beobachtet, in 9 Minuten kam keine). Entfernen/Überspringen bewusst nicht gebaut (siehe README, Abschnitt Grenzen).
- Ziehen und Größe ändern der Fenster ohne Leiste mit echter Maus ungetestet (Schnittstelle erlaubt und geprüft, Knöpfe geklickt).
- Ideen (nach Wert und Aufwand in der Sitzung besprochen): Meldung anklickbar, Mit Windows starten, Zeitplan-Erinnerungen, Statistik der Lieblingskanäle, Sleep-Timer, globale Tastenkürzel, mehrere Streams.