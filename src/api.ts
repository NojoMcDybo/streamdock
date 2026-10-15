// Brücke zur App (Rust). Im Browser ohne Tauri (npm run dev, nur zum Ansehen des Designs) kommen Attrappen aus mock.ts.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Stream = {
  login: string;
  name: string;
  game: string;
  title: string;
  viewers: number;
  thumbnail: string;
  avatar: string;
  startedAt: string;
};
export type TwitchState = { clientId: boolean; loggedIn: boolean };
export type AuthStart = { userCode: string; verificationUri: string };
export type AuthEvent = { ok: boolean; message: string };

export type UpdateState = {
  status: "idle" | "checking" | "current" | "available" | "downloading" | "error";
  current: string;
  version?: string;
  message?: string;
};

export const inTauri = "__TAURI_INTERNALS__" in window;

export type Api = {
  twitchState: () => Promise<TwitchState>;
  setClientId: (id: string) => Promise<void>;
  authStart: () => Promise<AuthStart>;
  authCancel: () => Promise<void>;
  logout: () => Promise<void>;
  liveStreams: () => Promise<Stream[]>;
  startStream: (channel: string) => Promise<void>;
  openConsole: () => Promise<void>;
  onAuth: (cb: (e: AuthEvent) => void) => Promise<() => void>;
  updateState: () => Promise<UpdateState>;
  updateCheck: () => Promise<UpdateState>;
  updateInstall: () => Promise<void>;
  onUpdate: (cb: (u: UpdateState) => void) => Promise<() => void>;
};

const real: Api = {
  twitchState: () => invoke("twitch_state"),
  setClientId: (id) => invoke("set_client_id", { id }),
  authStart: () => invoke("auth_start"),
  authCancel: () => invoke("auth_cancel"),
  logout: () => invoke("logout"),
  liveStreams: () => invoke("live_streams"),
  startStream: (channel) => invoke("start_stream", { channel }),
  openConsole: () => invoke("open_console"),
  onAuth: (cb) => listen<AuthEvent>("auth", (e) => cb(e.payload)),
  updateState: () => invoke("update_state"),
  updateCheck: () => invoke("update_check"),
  updateInstall: () => invoke("update_install"),
  onUpdate: (cb) => listen<UpdateState>("update", (e) => cb(e.payload)),
};

export async function loadApi(): Promise<Api> {
  if (inTauri || !import.meta.env.DEV) return real;
  return (await import("./mock")).mockApi;
}
