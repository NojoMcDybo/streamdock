// Wird in jede Twitch-Seite der Player- und Chat-Fenster eingesetzt (Rust: stream.rs, `initialization_script`).
// Die Fenster haben keine Titelleiste. Stattdessen:
//  - Beim Hovern am oberen Rand fährt eine Glas-Pille herunter (Zurück zur Auswahl, Anheften, Vollbild, Minimieren, Schließen); der Streifen
//    darunter zieht das Fenster, Ränder und Ecken ändern die Größe.
//  - Im Player wird Werbung erkannt und der Ton stumm geschaltet (Hinweis „Werbung · stumm“), danach kommt der Ton zurück.
// Die Seite darf über die Tauri-Brücke nur Fenstersteuerung aufrufen (capabilities/twitch-windows.json), keine App-Befehle.
(() => {
  if (window.top !== window || window.__sdOverlay) return;
  const bridge = window.__TAURI_INTERNALS__;
  if (!bridge || !bridge.invoke) return;
  window.__sdOverlay = true;

  const label = (bridge.metadata && bridge.metadata.currentWindow && bridge.metadata.currentWindow.label) || '';
  const isChat = label.startsWith('chat-');
  const win = (cmd, args) => bridge.invoke('plugin:window|' + cmd, Object.assign({ label }, args)).catch(() => undefined);

  const ICON = {
    back: `__ICON_BACK__`,
    close: `__ICON_CLOSE__`,
    min: `__ICON_MIN__`,
    full: `__ICON_FULLSCREEN__`,
    fullExit: `__ICON_FULLSCREEN_EXIT__`,
    pin: `__ICON_PIN__`,
  };

  // Nojo Liquid Glass (nojo-ui.css .n-glass/.n-liquid) als eigene, von der Seite getrennte Oberfläche
  const css = `
    :host { all: initial; }
    .pill { position: fixed; top: 10px; right: 12px; z-index: 2147483646; display: flex; gap: 2px; padding: 4px; border-radius: 18px;
      background: rgba(58, 58, 62, 0.52); color: #f5f5f7;
      -webkit-backdrop-filter: blur(22px) saturate(170%) brightness(1.04); backdrop-filter: blur(22px) saturate(170%) brightness(1.04);
      box-shadow: inset 0 1px 0 rgba(255,255,255,.22), inset 0 0 0 1px rgba(255,255,255,.1), 0 6px 22px rgba(0,0,0,.28);
      transform: translateY(-72px); opacity: 0; pointer-events: none;
      transition: transform .42s cubic-bezier(.22,1.18,.36,1), opacity .22s ease; }
    .pill.show { transform: none; opacity: 1; pointer-events: auto; }
    .pill::after { content: ""; position: absolute; inset: 0; border-radius: inherit; pointer-events: none;
      background: radial-gradient(120px circle at 20% -40%, rgba(255,255,255,.2), transparent 70%); }
    button { all: unset; box-sizing: border-box; width: 30px; height: 30px; border-radius: 50%; display: grid; place-items: center; cursor: pointer; color: inherit;
      transition: background .2s ease, transform .3s cubic-bezier(.22,1.18,.36,1), color .2s ease; }
    button svg { width: 15px; height: 15px; pointer-events: none; }
    button:hover { background: rgba(255,255,255,.1); }
    button:active { transform: scale(.92); }
    button.on { color: #cdb0ff; background: rgba(145,70,255,.28); }
    button.close:hover { background: #df4944; color: #fff; }
    .drag { position: fixed; top: 0; left: 0; right: 0; height: 38px; z-index: 2147483645; display: none; cursor: grab; }
    .drag.on { display: block; }
    .edge { position: fixed; z-index: 2147483647; }
    .edge.n { top: 0; left: 10px; right: 10px; height: 4px; cursor: ns-resize; }
    .edge.s { bottom: 0; left: 10px; right: 10px; height: 4px; cursor: ns-resize; }
    .edge.w { left: 0; top: 10px; bottom: 10px; width: 4px; cursor: ew-resize; }
    .edge.e { right: 0; top: 10px; bottom: 10px; width: 4px; cursor: ew-resize; }
    .edge.nw { top: 0; left: 0; width: 10px; height: 10px; cursor: nwse-resize; }
    .edge.se { bottom: 0; right: 0; width: 10px; height: 10px; cursor: nwse-resize; }
    .edge.ne { top: 0; right: 0; width: 10px; height: 10px; cursor: nesw-resize; }
    .edge.sw { bottom: 0; left: 0; width: 10px; height: 10px; cursor: nesw-resize; }
    :host(.full) .edge { display: none; }
    .badge { position: fixed; left: 14px; bottom: 56px; z-index: 2147483646; display: none; align-items: center; gap: 8px; padding: 7px 13px; border-radius: 999px;
      font: 600 12px/1 "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif; color: #f5f5f7;
      background: rgba(58, 58, 62, 0.52); -webkit-backdrop-filter: blur(22px) saturate(170%); backdrop-filter: blur(22px) saturate(170%);
      box-shadow: inset 0 1px 0 rgba(255,255,255,.2), inset 0 0 0 1px rgba(255,255,255,.1), 0 6px 22px rgba(0,0,0,.28); }
    .badge.on { display: inline-flex; }
    .badge i { width: 7px; height: 7px; border-radius: 50%; background: #ff4a4a; box-shadow: 0 0 8px #ff4a4a; }
    @media (prefers-reduced-motion: reduce) { .pill { transition: none; } }
  `;

  const host = document.createElement('div');
  host.id = 'sd-overlay';
  const root = host.attachShadow({ mode: 'open' });
  try {
    const sheet = new CSSStyleSheet(); // Stilblatt ohne <style>: von der Content-Security-Policy der Seite unberührt
    sheet.replaceSync(css);
    root.adoptedStyleSheets = [sheet];
  } catch (e) {
    const s = document.createElement('style');
    s.textContent = css;
    root.append(s);
  }

  const el = (tag, cls, html) => { const e = document.createElement(tag); if (cls) e.className = cls; if (html) e.innerHTML = html; return e; };
  const pill = el('div', 'pill');
  const drag = el('div', 'drag');
  const button = (name, cls, title, icon, fn) => {
    const b = el('button', cls, icon);
    b.title = title; b.setAttribute('aria-label', title); b.dataset.name = name;
    b.addEventListener('click', (ev) => { ev.stopPropagation(); fn(b); });
    pill.append(b);
    return b;
  };

  // Zurück zur Auswahl (nur Player): das Menü kommt nach vorn, der Stream läuft weiter, bis dort ein anderer gewählt wird.
  // Vorher Vollbild und Immer-oben aufheben, sonst bliebe das Menü hinter dem Player.
  if (!isChat) {
    button('back', '', 'Zurück zur Auswahl', ICON.back, async () => {
      if (full) await setFull(false);
      if (pinned) await setPin(false);
      bridge.invoke('show_menu').catch(() => undefined);
    });
  }

  let pinned = false;
  const setPin = (value) => {
    pinned = value;
    pill.querySelector('[data-name=pin]').classList.toggle('on', pinned);
    return win('set_always_on_top', { value });
  };
  button('pin', '', 'Immer im Vordergrund', ICON.pin, () => setPin(!pinned));

  let full = false;
  const setFull = (value) => {
    full = value;
    host.classList.toggle('full', full);
    const b = pill.querySelector('[data-name=full]');
    if (b) b.innerHTML = full ? ICON.fullExit : ICON.full;
    return win('set_fullscreen', { value });
  };
  if (!isChat) button('full', '', 'Vollbild', ICON.full, () => setFull(!full));
  button('min', '', 'Minimieren', ICON.min, () => win('minimize'));
  button('close', 'close', 'Schließen', ICON.close, () => win('close'));

  drag.addEventListener('mousedown', (e) => { if (e.button === 0) win('start_dragging'); });
  root.append(drag, pill);
  for (const d of ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw']) {
    const edge = el('div', 'edge ' + d);
    const dir = { n: 'North', s: 'South', e: 'East', w: 'West', ne: 'NorthEast', nw: 'NorthWest', se: 'SouthEast', sw: 'SouthWest' }[d];
    edge.addEventListener('mousedown', (e) => { if (e.button === 0) { e.preventDefault(); win('start_resize_dragging', { value: dir }); } });
    root.append(edge);
  }

  // Einblenden, solange die Maus oben ist oder auf der Pille liegt; Ausblenden kurz nach dem Verlassen
  let hideTimer = 0;
  let inside = false;
  const hide = () => { if (!inside) { pill.classList.remove('show'); drag.classList.remove('on'); } };
  const arm = () => { clearTimeout(hideTimer); hideTimer = setTimeout(hide, 1600); };
  const show = () => { pill.classList.add('show'); drag.classList.add('on'); arm(); };
  for (const part of [pill, drag]) {
    part.addEventListener('mouseenter', () => { inside = true; clearTimeout(hideTimer); });
    part.addEventListener('mouseleave', () => { inside = false; arm(); });
  }
  document.addEventListener('mousemove', (e) => { if (e.clientY < 46) show(); }, true);
  // (nicht am documentElement: das Skript läuft vor dem Aufbau der Seite, dort gibt es ihn noch nicht)
  window.addEventListener('mouseout', (e) => { if (!e.relatedTarget) { inside = false; arm(); } });
  document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && full) setFull(false); }, true);

  // ---------- Werbung stumm schalten (nur Player) ----------
  // Erkennung über die Kennungen der Werbe-Oberfläche des Twitch-Players (data-a-target="video-ad-label" u. ä.):
  // ein Wortteil "ad" in data-a-target / data-test-selector eines sichtbaren Elements.
  const badge = el('div', 'badge', '<i></i>Werbung · stumm');
  const AD_TOKEN = /(^|[-_])ad([-_]|$)/i;
  // Zweiter Weg, falls Twitch die Kennungen ändert: ein sichtbares Blatt-Element, das nur die Werbe-Beschriftung zeigt,
  // z. B. „Werbung“, „Ad 1 of 2“, „Werbung 0:15“
  const AD_TEXT = /^(werbung|anzeige|advertisement|ad)(\s*[:·•-]?\s*(\d+\s*(von|of|\/)\s*\d+|\d+:\d{2}|\d+\s*(s|sek)\.?))?$/i;
  const adPresent = () => {
    for (const e of document.querySelectorAll('[data-a-target], [data-test-selector]')) {
      const t = (e.getAttribute('data-a-target') || '') + ' ' + (e.getAttribute('data-test-selector') || '');
      if (t.split(/\s+/).some((x) => AD_TOKEN.test(x)) && e.getClientRects().length > 0) return true;
    }
    for (const e of document.querySelectorAll('span, div, p')) {
      if (e.childElementCount === 0 && e.textContent.length < 24 && AD_TEXT.test(e.textContent.trim()) && e.getClientRects().length > 0) return true;
    }
    return false;
  };
  let adOn = false;
  let wasMuted = null;
  const setAd = (on) => {
    const videos = document.querySelectorAll('video');
    if (on && !adOn) {
      const first = videos[0];
      wasMuted = first ? first.muted : null;
      badge.classList.add('on');
    }
    if (!on && adOn) {
      if (wasMuted === false) videos.forEach((v) => { v.muted = false; });
      wasMuted = null;
      badge.classList.remove('on');
    }
    adOn = on;
    if (on) videos.forEach((v) => { v.muted = true; });
  };
  if (!isChat) {
    root.append(badge);
    // der Player stellt die Lautstärke selbst wieder her: während der Werbung sofort wieder stumm
    document.addEventListener('volumechange', (e) => { if (adOn && e.target instanceof HTMLVideoElement && !e.target.muted) e.target.muted = true; }, true);
    setInterval(() => setAd(adPresent()), 500);
  }

  const mount = () => { if (!host.isConnected) (document.body || document.documentElement).append(host); };
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', mount); else mount();
  // Twitch baut die Seite um (Navigation innerhalb der App): wieder anhängen, falls entfernt
  setInterval(mount, 2000);
})();
