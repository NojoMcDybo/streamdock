// Attrappen nur für `npm run dev` im Browser: Design ansehen, ohne Twitch. Zustand per Adresse: ?state=setup|login|code|list|empty|error|loading
import type { Api } from "./api";
import type { AuthEvent, Stream, UpdateState } from "./api";

const q = new URLSearchParams(location.search).get("state") ?? "list";

const svg = (c1: string, c2: string, label = "") =>
  "data:image/svg+xml," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 440 248"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${c1}"/><stop offset="1" stop-color="${c2}"/></linearGradient></defs><rect width="440" height="248" fill="url(#g)"/><text x="50%" y="54%" text-anchor="middle" font-family="Segoe UI" font-size="64" font-weight="700" fill="rgba(255,255,255,.35)">${label}</text></svg>`,
  );

const people: Array<[string, string, string, string, number]> = [
  ["caedrel", "Caedrel", "League of Legends", "Worlds 2026 Watchparty – Playoffs Tag 3, später Soloqueue", 12345],
  ["jp_streamer", "日本語ストリーマー", "Chess", "Titel mit Umlauten: äöü und 日本語, dazu ein ziemlich langer Rest, der umbrechen muss", 800],
  ["kleiner", "Kleiner", "Just Chatting", "", 7],
  ["alpha", "Alpha", "Valorant", "Ranked Grind", 4300],
  ["bravo", "Bravo", "Minecraft", "Hardcore Tag 112", 2210],
  ["charlie", "Charlie", "Elden Ring", "Blind Run, kein Leveln", 1530],
  ["delta", "Delta", "Software und Spieleentwicklung", "Tauri-App bauen", 310],
  ["echo", "Echo", "IRL", "Spaziergang", 95],
];
const hues = ["#9146ff", "#2d8cff", "#ff6b3d", "#17c964", "#f5a524", "#f31260", "#7828c8", "#06b7db"];

const streams: Stream[] = people.map(([login, name, game, title, viewers], i) => ({
  login, name, game, title, viewers,
  thumbnail: svg(hues[i % 8], "#1c1c1e", name[0]),
  avatar: svg(hues[(i + 3) % 8], hues[i % 8], name[0]),
  startedAt: "2026-10-15T10:00:00Z",
  watched: [43200, 9000, 0, 2400, 0, 0, 0, 0][i] ?? 0,
})).sort((a, b) => b.watched - a.watched || b.viewers - a.viewers); // wie im Backend: Favoriten zuerst

let authCb: ((e: AuthEvent) => void) | null = null;

// ?update=available|current|error|downloading|checking (Vorgabe: current)
const upd = new URLSearchParams(location.search).get("update") ?? "current";
let updCb: ((u: UpdateState) => void) | null = null;
let updState: UpdateState = { status: upd as UpdateState["status"], current: "0.1.0", version: "0.2.0", message: "Twitch lieferte keine Antwort" };

export const mockApi: Api = {
  twitchState: async () => ({ clientId: q !== "setup", loggedIn: q !== "setup" && q !== "login" && q !== "code" }),
  setClientId: async () => {},
  authStart: async () => {
    setTimeout(() => authCb?.({ ok: true, message: "" }), 4000);
    return { userCode: "KDMHPQRS", verificationUri: "https://www.twitch.tv/activate" };
  },
  authCancel: async () => {},
  logout: async () => {},
  liveStreams: async () => {
    if (q === "loading") return new Promise(() => {});
    if (q === "error") throw "Twitch-API 500: interner Fehler";
    return q === "empty" ? [] : streams;
  },
  startStream: async (c) => console.log("startStream", c),
  openConsole: async () => {},
  onAuth: async (cb) => {
    authCb = cb;
    return () => { authCb = null; };
  },
  updateState: async () => updState,
  updateCheck: async () => {
    updState = { ...updState, status: "current" };
    updCb?.(updState);
    return updState;
  },
  updateInstall: async () => {
    updState = { ...updState, status: "downloading" };
    updCb?.(updState);
  },
  onUpdate: async (cb) => {
    updCb = cb;
    return () => { updCb = null; };
  },
};
