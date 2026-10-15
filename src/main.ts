import { getCurrentWindow } from "@tauri-apps/api/window";
import { glassLight, iconNode, lightScroller, liquid, windowControls, type IconName } from "./nojo/nojo-ui";
import { inTauri, loadApi, type Api, type AuthEvent, type Stream, type UpdateState } from "./api";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const win = inTauri ? getCurrentWindow() : null;

// Symbole aus der Bibliothek einsetzen (index.html hat Platzhalter <svg data-icon="…">)
document.querySelectorAll<SVGElement>("svg[data-icon]").forEach((s) => {
  const n = iconNode(s.dataset.icon as IconName, { cls: s.getAttribute("class") ?? undefined });
  s.replaceWith(n);
});

windowControls({ minimize: () => void win?.minimize(), close: () => void win?.hide(), closeTitle: "In den Tray" });
glassLight();
liquid();
lightScroller($("list"), { insetTop: 80 });

// ---------- kleine Bausteine (Texte aus Twitch immer als Text, nie als HTML) ----------

function h<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** `**fett**` im Text als <b>, alles andere als Text. */
function rich(parent: HTMLElement, text: string) {
  text.split("**").forEach((part, i) => parent.append(i % 2 ? h("b", "", part) : document.createTextNode(part)));
  return parent;
}

const CDN = /^https:\/\/[a-z0-9.-]+\.jtvnw\.net\//;
function image(src: string): HTMLImageElement | null {
  if (!CDN.test(src) && !src.startsWith("data:image/svg+xml,")) return null;
  const i = h("img");
  i.src = src;
  i.alt = "";
  i.loading = "lazy";
  i.draggable = false;
  i.addEventListener("error", () => i.remove());
  return i;
}

const fmt = (n: number) => (n >= 10_000 ? `${(n / 1000).toLocaleString("de", { maximumFractionDigits: 1 })} Tsd.` : n.toLocaleString("de"));

function toast(msg: string) {
  const t = h("div", "toast n-glass n-liquid panel", msg);
  document.body.append(t);
  setTimeout(() => t.remove(), 4500);
}

const view = $("view");
const count = $("count");
const show = (...n: Node[]) => view.replaceChildren(...n);

function note(title: string, ...body: Node[]) {
  const n = h("section", "note n-card");
  n.append(h("h2", "", title), ...body);
  return n;
}

function button(label: string, onClick: () => void, primary = false) {
  const b = h("button", primary ? "n-btn primary" : "n-btn", label);
  b.type = "button";
  b.addEventListener("click", onClick);
  return b;
}

// ---------- Ansichten ----------

let api: Api;
let timer = 0;

/** Zählt die Ansichtswechsel: eine Antwort von `refresh`, die nach einem Wechsel eintrifft, wird verworfen. */
let seq = 0;

function stopTimer() {
  clearInterval(timer);
  timer = 0;
  seq++;
}

function showSetup(msg = "") {
  stopTimer();
  count.textContent = "Einrichten";
  const intro = h("p", "", "Streamdock liest nur, welche deiner gefolgten Kanäle live sind. Dafür braucht es einmalig eine eigene Twitch-Anwendung, kostenlos.");
  const steps = h("ol");
  for (const s of [
    "Auf **dev.twitch.tv** eine Anwendung registrieren.",
    "Name **ohne „twitch“**, Weiterleitungs-URL **https://localhost**, Kategorie **Application Integration**, Clienttyp **Öffentlich**.",
    "Die **Client-ID** kopieren und hier einfügen.",
  ]) {
    const li = h("li");
    li.append(rich(h("span"), s)); // eigenes Element, damit der Zähler (::before) und der Text zwei Spalten bleiben
    steps.append(li);
  }
  const input = h("input", "n-input");
  input.placeholder = "Client-ID";
  input.spellcheck = false;
  input.setAttribute("aria-label", "Client-ID");
  const err = h("div", "msg", msg);
  const go = async () => {
    try {
      await api.setClientId(input.value);
      await route();
    } catch (e) {
      err.textContent = String(e);
    }
  };
  input.addEventListener("keydown", (e) => e.key === "Enter" && go());
  const row = h("div", "row");
  row.append(input, button("Verbinden", go, true));
  show(note("Mit Twitch verbinden", intro, steps, button("Konsole öffnen", () => void api.openConsole()), row, err));
  input.focus();
}

function showLogin(msg = "") {
  stopTimer();
  count.textContent = "Anmelden";
  const err = h("div", "msg", msg);
  const start = button("Mit Twitch anmelden", async () => {
    try {
      showCode(await api.authStart());
    } catch (e) {
      err.textContent = String(e);
    }
  }, true);
  show(note("Bei Twitch anmelden", h("p", "", "Du bestätigst die Anmeldung kurz im Browser. Streamdock bekommt nur Leserechte auf deine gefolgten Kanäle."), start, err));
  start.focus();
}

function showCode(a: { userCode: string; verificationUri: string }) {
  stopTimer();
  count.textContent = "Anmelden";
  show(note(
    "Im Browser bestätigen",
    h("div", "big", a.userCode),
    h("p", "", "Der Browser wurde geöffnet. Bestätige dort die Anmeldung. Falls nichts erscheint, öffne twitch.tv/activate und gib den Code ein."),
    h("span", "pulse"),
    button("Abbrechen", () => void api.authCancel().then(() => showLogin())),
  ));
}

function showError(msg: string) {
  stopTimer();
  count.textContent = "Live";
  const n = note("Das hat nicht geklappt", h("p", "", msg), button("Erneut versuchen", () => void route(), true));
  n.classList.add("err");
  show(n);
}

function showEmpty() {
  count.textContent = "0 live";
  show(note("Gerade ist niemand live", h("p", "", "Von den Kanälen, denen du folgst, streamt im Moment keiner."), button("Aktualisieren", () => void refresh(true))));
}

function skeleton() {
  const g = h("div", "grid");
  for (let i = 0; i < 8; i++) g.append(h("div", "skel"));
  show(g);
}

function card(s: Stream) {
  const c = h("button", "card");
  c.type = "button";
  c.dataset.login = s.login;
  c.title = `${s.name} öffnen`;
  const thumb = h("div", "thumb");
  const t = image(s.thumbnail);
  if (t) thumb.append(t);
  thumb.append(h("span", "live-badge", "LIVE"), h("span", "viewers", fmt(s.viewers)));
  const ava = h("div", "ava");
  const a = image(s.avatar);
  if (a) ava.append(a);
  const txt = h("div", "txt");
  txt.append(h("span", "name", s.name), h("span", "game", s.game));
  const meta = h("div", "meta");
  meta.append(ava, txt);
  c.append(thumb, meta, h("p", "title", s.title));
  c.addEventListener("click", () => void launch(s.login));
  return c;
}

let shown = 0;

async function refresh(initial: boolean) {
  if (initial) {
    shown = 0;
    skeleton();
  }
  const mine = ++seq;
  try {
    const list = await api.liveStreams();
    if (mine !== seq) return;
    if (!list.length) return showEmpty();
    const focus = (document.activeElement as HTMLElement | null)?.dataset?.login;
    const grid = h("div", "grid");
    list.forEach((s) => grid.append(card(s)));
    show(grid);
    shown = list.length;
    count.textContent = `${list.length} live`;
    const typing = document.activeElement instanceof HTMLInputElement;
    const target = (focus && grid.querySelector<HTMLElement>(`[data-login="${CSS.escape(focus)}"]`)) || (!typing && initial ? grid.firstElementChild : null);
    (target as HTMLElement | null)?.focus({ preventScroll: true });
  } catch (e) {
    if (mine !== seq) return;
    const msg = String(e);
    if (/angemeldet|abgelaufen/i.test(msg)) return showLogin(msg);
    if (initial || !shown) return showError(msg);
    toast("Aktualisieren fehlgeschlagen: " + msg); // Liste bleibt stehen
  }
}

async function route() {
  stopTimer();
  try {
    const st = await api.twitchState();
    if (!st.clientId) return showSetup();
    if (!st.loggedIn) return showLogin();
    await refresh(true);
    timer = window.setInterval(() => document.visibilityState === "visible" && void refresh(false), 60_000);
  } catch (e) {
    showError(String(e));
  }
}

async function launch(input: string) {
  if (!input.trim()) return;
  try {
    await api.startStream(input);
    await win?.hide();
  } catch (e) {
    toast(String(e));
  }
}

function onAuth(e: AuthEvent) {
  if (e.ok) void route();
  else showLogin(e.message);
}

// ---------- Selbst-Update (Rust: update.rs; prüft beim Start und alle 6 Stunden) ----------

let upd: UpdateState | null = null;

function paintUpdate() {
  const gear = $("gear");
  gear.querySelector(".n-dot")?.remove();
  if (upd?.status === "available") gear.append(h("i", "n-dot")); // es gibt eine neue Version
  const text = $("upd-text");
  const btn = $<HTMLButtonElement>("upd-btn");
  const v = upd?.current ?? "";
  const s = upd?.status ?? "idle";
  text.className = s === "error" ? "err" : "";
  text.textContent = {
    idle: `Version ${v} · sucht beim Start nach Updates`,
    checking: "Sucht …",
    current: `Version ${v} · aktuell`,
    available: `Version ${upd?.version ?? ""} ist da (installiert: ${v})`,
    downloading: "Lädt … Streamdock startet gleich neu",
    error: upd?.message ?? "Prüfung fehlgeschlagen",
  }[s];
  btn.textContent = s === "available" ? "Installieren" : "Nach Updates suchen";
  btn.classList.toggle("primary", s === "available");
  btn.disabled = s === "checking" || s === "downloading";
}

$("upd-btn").addEventListener("click", () => {
  if (upd?.status === "available") void api.updateInstall().catch((e) => toast(String(e)));
  else void api.updateCheck().then((u) => { upd = u; paintUpdate(); }).catch((e) => toast(String(e)));
});

// ---------- Bedienung ----------

$<HTMLFormElement>("open").addEventListener("submit", (e) => {
  e.preventDefault();
  const input = $<HTMLInputElement>("channel");
  void launch(input.value).then(() => (input.value = ""));
});
$("refresh").addEventListener("click", () => void route());

const pop = $("pop");
const closePop = () => (pop.hidden = true);
$("gear").addEventListener("click", (e) => {
  e.stopPropagation();
  pop.hidden = !pop.hidden;
});
document.addEventListener("click", (e) => !pop.contains(e.target as Node) && closePop());
$("change-id").addEventListener("click", () => {
  closePop();
  showSetup();
});
$("logout").addEventListener("click", async () => {
  closePop();
  await api.logout();
  showLogin();
});

// Pfeiltasten bewegen den Fokus durch das Raster, Enter startet (Knopf)
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") return closePop();
  const keys = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"];
  const cards = [...document.querySelectorAll<HTMLElement>(".card")];
  const i = cards.indexOf(document.activeElement as HTMLElement);
  if (!keys.includes(e.key) || i < 0) return;
  const cols = cards.filter((c) => c.offsetTop === cards[0].offsetTop).length || 1;
  const next = { ArrowLeft: i - 1, ArrowRight: i + 1, ArrowUp: i - cols, ArrowDown: i + cols }[e.key as "ArrowLeft"];
  if (next >= 0 && next < cards.length) {
    e.preventDefault();
    cards[next].focus();
  }
});

void (async () => {
  api = await loadApi();
  await api.onAuth(onAuth);
  await api.onUpdate((u) => { upd = u; paintUpdate(); });
  void api.updateState().then((u) => { upd = u; paintUpdate(); }).catch(() => {});
  await route();
})();
