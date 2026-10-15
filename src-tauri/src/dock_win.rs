//! Anbindung des Andockens an echte Fenster (Win32): sichtbare Grenzen über DWM, sofortiges Nachziehen
//! über einen WinEvent-Hook, ein Zeitgeber fängt „Maustaste losgelassen“ ab. Die Entscheidungen trifft
//! `dock::Dock` (rein, getestet).

use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::WebviewWindow;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_RBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetSystemMetrics, GetWindowRect, IsIconic, IsWindow, MsgWaitForMultipleObjects, PeekMessageW, SetWindowPos,
    TranslateMessage, EVENT_OBJECT_LOCATIONCHANGE, MSG, OBJID_WINDOW, PM_REMOVE, QS_ALLINPUT, SWP_NOACTIVATE,
    SM_SWAPBUTTON, SWP_NOZORDER, WINEVENT_OUTOFCONTEXT,
};

use crate::dock::{dock_rect, Dock, Input, Rect, Side};

struct Session {
    dock: Dock,
    player: isize,
    chat: isize,
}

static SESSION: Mutex<Option<Session>> = Mutex::new(None);
/// Zählt die Sitzungen: eine neue (oder ein Ende) macht die Threads der alten gegenstandslos.
static GENERATION: AtomicU32 = AtomicU32::new(0);

fn hwnd(h: isize) -> HWND {
    HWND(h as *mut c_void)
}

/// Sichtbares Rechteck ohne den unsichtbaren Win11-Rand.
fn vis(h: isize) -> Option<Rect> {
    let mut r = RECT::default();
    unsafe {
        DwmGetWindowAttribute(hwnd(h), DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut c_void, size_of::<RECT>() as u32)
            .ok()?;
    }
    Some(Rect::new(r.left, r.top, r.right, r.bottom))
}

/// Setzt das sichtbare Rechteck eines Fensters und rechnet den unsichtbaren Rand ein.
fn move_vis(h: isize, t: Rect) {
    let Some(v) = vis(h) else { return };
    let mut w = RECT::default();
    unsafe {
        if GetWindowRect(hwnd(h), &mut w).is_err() {
            return;
        }
        let (il, it) = (v.l - w.left, v.t - w.top);
        let (ew, eh) = ((w.right - w.left) - v.w(), (w.bottom - w.top) - v.h());
        let _ = SetWindowPos(hwnd(h), None, t.l - il, t.t - it, t.w() + ew, t.h() + eh, SWP_NOZORDER | SWP_NOACTIVATE);
    }
}

fn end_session() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut g) = SESSION.lock() {
        *g = None;
    }
}

/// Ergebnis eines Durchlaufs: Sitzung zu Ende, nichts zu tun, oder es wartet etwas auf das Loslassen der Maus.
#[derive(PartialEq)]
enum Poll {
    End,
    Idle,
    Active,
}

/// Linke Maustaste gedrückt? Bei vertauschten Tasten (Linkshänder) ist die Hauptstaste die physisch rechte.
fn mouse_down() -> bool {
    let key = if unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0 { VK_RBUTTON } else { VK_LBUTTON };
    let state = unsafe { GetAsyncKeyState(key.0 as i32) };
    state < 0
}

/// Ein Durchlauf: Fenster lesen, Zustandsmaschine fragen, Chat setzen. Im Hook mit `blocking = false`:
/// ist der Zustand gerade belegt, holt der Zeitgeber es kurz darauf nach.
/// SetWindowPos läuft bewusst ohne gehaltene Sperre: es wartet auf den UI-Thread, und der darf beim Start
/// einer neuen Sitzung selbst auf die Sperre warten.
fn tick(generation: u32, blocking: bool) -> Poll {
    if GENERATION.load(Ordering::SeqCst) != generation {
        return Poll::End;
    }
    let (chat, target, pending) = {
        let mut guard = match if blocking { SESSION.lock().ok() } else { SESSION.try_lock().ok() } {
            Some(g) => g,
            None => return Poll::Active,
        };
        let Some(s) = guard.as_mut() else { return Poll::End };
        let (player, chat) = (s.player, s.chat);
        unsafe {
            if !IsWindow(Some(hwnd(player))).as_bool() || !IsWindow(Some(hwnd(chat))).as_bool() {
                drop(guard);
                end_session();
                return Poll::End;
            }
            if IsIconic(hwnd(player)).as_bool() || IsIconic(hwnd(chat)).as_bool() {
                return Poll::Idle;
            }
        }
        let (Some(p), Some(c)) = (vis(player), vis(chat)) else { return Poll::Idle };
        let down = mouse_down();
        let target = s.dock.step(Input { player: p, chat: c, mouse_down: down });
        (chat, target, s.dock.pending())
    };
    if let Some(t) = target {
        move_vis(chat, t);
        if let (Some(a), Ok(mut g)) = (vis(chat), SESSION.lock()) {
            if GENERATION.load(Ordering::SeqCst) == generation {
                if let Some(s) = g.as_mut() {
                    s.dock.applied(t, a);
                }
            }
        }
        return Poll::Active;
    }
    if pending { Poll::Active } else { Poll::Idle }
}

unsafe extern "system" fn on_event(_hook: HWINEVENTHOOK, _event: u32, h: HWND, id_object: i32, _id_child: i32, _thread: u32, _time: u32) {
    if id_object != OBJID_WINDOW.0 {
        return;
    }
    let raw = h.0 as isize;
    let ours = SESSION
        .try_lock()
        .ok()
        .and_then(|g| g.as_ref().map(|s| raw == s.player || raw == s.chat))
        .unwrap_or(false);
    if ours {
        tick(GENERATION.load(Ordering::SeqCst), false);
    }
}

/// Hook-Thread: jede Bewegung eines unserer Fenster löst sofort einen Durchlauf aus.
fn hook_thread(generation: u32) {
    unsafe {
        let hook = SetWinEventHook(
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(on_event),
            GetCurrentProcessId(),
            0,
            WINEVENT_OUTOFCONTEXT,
        );
        let mut msg = MSG::default();
        while GENERATION.load(Ordering::SeqCst) == generation {
            // höchstens 100 ms warten, damit das Ende der Sitzung bemerkt wird
            MsgWaitForMultipleObjects(None, false, 100, QS_ALLINPUT);
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let _ = UnhookWinEvent(hook);
    }
}

/// Zeitgeber-Thread: erkennt „Maustaste losgelassen“ (einrasten) und das Ende der Sitzung. Schnell (15 ms)
/// nur, solange etwas auf das Loslassen wartet, sonst genügt ein gelegentlicher Blick.
fn timer_thread(generation: u32) {
    loop {
        let wait = match tick(generation, true) {
            Poll::End => return,
            Poll::Active => 15,
            Poll::Idle => 100,
        };
        std::thread::sleep(Duration::from_millis(wait));
    }
}

/// Dockt den Chat rechts an den Player und hält ihn bis zum Wegziehen dran.
pub fn start(player: &WebviewWindow, chat: &WebviewWindow) -> tauri::Result<()> {
    let (p, c) = (player.hwnd()?.0 as isize, chat.hwnd()?.0 as isize);
    let (Some(pr), Some(cr)) = (vis(p), vis(c)) else { return Ok(()) };
    move_vis(c, dock_rect(Side::Right, pr, cr));
    let cr = vis(c).unwrap_or(cr);
    let generation = GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    if let Ok(mut g) = SESSION.lock() {
        *g = Some(Session { dock: Dock::new(pr, cr, Some(Side::Right)), player: p, chat: c });
    }
    std::thread::spawn(move || hook_thread(generation));
    std::thread::spawn(move || timer_thread(generation));
    Ok(())
}
