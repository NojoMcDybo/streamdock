//! Andock-Logik: reine Geometrie und Zustandsmaschine, Port von `twitch-dock.ahk`
//! (SnapSide, Dock, Tick). Keine Win32-Aufrufe; die Anbindung an die Fenster liegt in `dock_win.rs`.
//!
//! Rechtecke sind die *sichtbaren* Fenstergrenzen (ohne den unsichtbaren Win11-Rand).
//! Der Player ist das Hauptfenster, der Chat hängt an einer seiner vier Kanten.

/// Abstand in px, ab dem ein losgelassener Chat an einer Playerkante einrastet.
pub const SNAP: i32 = 20;
/// Abweichung in px von der Andockstelle, ab der sich der Chat löst.
pub const UNDOCK: i32 = 30;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Rect {
    pub l: i32,
    pub t: i32,
    pub r: i32,
    pub b: i32,
}

impl Rect {
    pub const fn new(l: i32, t: i32, r: i32, b: i32) -> Self {
        Rect { l, t, r, b }
    }
    pub const fn w(&self) -> i32 {
        self.r - self.l
    }
    pub const fn h(&self) -> i32 {
        self.b - self.t
    }
}

/// Seite des Players, an der der Chat hängt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Left,
    Bottom,
    Top,
}

/// Seite, an der der Chat `c` nahe genug (höchstens `snap` px) an einer Playerkante `p` liegt und die
/// Kanten sich überlappen; bei mehreren die nächste, sonst `None`.
pub fn snap_side(p: Rect, c: Rect, snap: i32) -> Option<Side> {
    let v_ov = c.t < p.b && c.b > p.t;
    let h_ov = c.l < p.r && c.r > p.l;
    [
        (Side::Right, (c.l - p.r).abs(), v_ov),
        (Side::Left, (c.r - p.l).abs(), v_ov),
        (Side::Bottom, (c.t - p.b).abs(), h_ov),
        (Side::Top, (c.b - p.t).abs(), h_ov),
    ]
    .into_iter()
    .filter(|&(_, d, ov)| ov && d <= snap)
    .min_by_key(|&(_, d, _)| d)
    .map(|(s, _, _)| s)
}

/// Soll-Rechteck des Chats: Kante am Player, Quermaß wie der Player, das Längsmaß behält der Chat.
pub fn dock_rect(side: Side, p: Rect, c: Rect) -> Rect {
    let (w, h) = (c.w(), c.h());
    match side {
        Side::Right => Rect::new(p.r, p.t, p.r + w, p.b),
        Side::Left => Rect::new(p.l - w, p.t, p.l, p.b),
        Side::Bottom => Rect::new(p.l, p.b, p.r, p.b + h),
        Side::Top => Rect::new(p.l, p.t - h, p.r, p.t),
    }
}

/// Abweichung der linken oberen Ecke in px (größere Achse).
pub fn dev(a: Rect, b: Rect) -> i32 {
    (a.l - b.l).abs().max((a.t - b.t).abs())
}

/// Ein Messpunkt der Fenster.
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub player: Rect,
    pub chat: Rect,
    /// Linke Maustaste gedrückt (irgendwo im System)
    pub mouse_down: bool,
}

/// Zustand des Andockens. `step` wird bei jeder Fensterbewegung und zyklisch aufgerufen.
#[derive(Debug)]
pub struct Dock {
    side: Option<Side>,
    p_prev: Rect,
    c_prev: Rect,
    dirty: bool,
    /// Wie oft nach dem Setzen schon nachgebessert wurde
    retries: u8,
}

impl Dock {
    pub fn new(player: Rect, chat: Rect, side: Option<Side>) -> Self {
        Dock { side, p_prev: player, c_prev: chat, dirty: false, retries: 0 }
    }

    /// Soll-Rechteck, falls der Chat noch nicht passt.
    fn want(&mut self, side: Side, p: Rect, c: Rect) -> Option<Rect> {
        let t = dock_rect(side, p, c);
        if t == c {
            self.retries = 0;
            return None;
        }
        Some(t)
    }

    /// Wartet etwas auf das Loslassen der Maus (dann lohnt schnelles Abfragen)?
    pub fn pending(&self) -> bool {
        self.dirty
    }

    #[cfg(test)]
    pub fn side(&self) -> Option<Side> {
        self.side
    }

    /// Meldet nach dem Setzen, was verlangt war (`wanted`) und was Windows daraus gemacht hat (`actual`), damit die
    /// eigene Bewegung nicht als Bewegung des Nutzers zählt. Weicht das Ergebnis ab (Windows skaliert das Fenster
    /// beim Setzen auf einen Monitor mit anderer DPI, oder WebView2 begrenzt die Größe), wird beim nächsten
    /// Durchlauf nachgebessert, höchstens zweimal: bei einer festen Mindestgröße gäbe es sonst eine Endlosschleife.
    /// `wanted` kommt vom Aufrufer statt aus dem Zustand, weil sich Durchläufe aus zwei Threads überlappen.
    pub fn applied(&mut self, wanted: Rect, actual: Rect) {
        self.c_prev = actual;
        if actual != wanted && self.retries < 2 {
            self.retries += 1;
            self.dirty = true;
        }
    }
    /// Liefert das Rechteck, auf das der Chat zu setzen ist; danach `applied` aufrufen.
    pub fn step(&mut self, i: Input) -> Option<Rect> {
        let (p, c) = (i.player, i.chat);
        let p_moved = p != self.p_prev;
        let c_moved = c != self.c_prev;
        self.p_prev = p;
        self.c_prev = c;

        if let Some(side) = self.side {
            if p_moved {
                self.retries = 0;
                return self.want(side, p, c);
            }
            // Lösen nur beim Ziehen mit der Maus: eine Verschiebung durch Windows (neue Skalierung beim Wechsel auf
            // einen Monitor mit anderer DPI, Tastatur, Fenstermanager) ist kein Wegziehen; der Chat wird beim
            // nächsten Durchlauf wieder angedockt.
            if c_moved && i.mouse_down && dev(c, dock_rect(side, p, c)) > UNDOCK {
                self.side = None;
            }
        }
        if c_moved || (p_moved && self.side.is_none()) {
            self.dirty = true;
        } else if self.dirty && !i.mouse_down {
            // erst beim Loslassen einrasten
            self.dirty = false;
            if self.side.is_none() {
                self.side = snap_side(p, c, SNAP);
            }
            if let Some(side) = self.side {
                return self.want(side, p, c);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: Rect = Rect::new(100, 100, 1100, 700);

    #[test]
    fn snap_side_wie_in_der_ahk_version() {
        assert_eq!(snap_side(P, Rect::new(1110, 100, 1400, 700), 20), Some(Side::Right));
        assert_eq!(snap_side(P, Rect::new(1120, 100, 1400, 700), 20), Some(Side::Right)); // genau SNAP
        assert_eq!(snap_side(P, Rect::new(1121, 100, 1400, 700), 20), None); // knapp zu weit
        assert_eq!(snap_side(P, Rect::new(-200, 100, 90, 700), 20), Some(Side::Left));
        assert_eq!(snap_side(P, Rect::new(100, 710, 1100, 900), 20), Some(Side::Bottom));
        assert_eq!(snap_side(P, Rect::new(100, 20, 1100, 95), 20), Some(Side::Top));
        assert_eq!(snap_side(P, Rect::new(1110, 800, 1400, 1000), 20), None); // keine Überlappung entlang der Kante
    }

    #[test]
    fn dock_rect_wie_in_der_ahk_version() {
        let c = Rect::new(0, 0, 380, 50);
        assert_eq!(dock_rect(Side::Right, P, c), Rect::new(1100, 100, 1480, 700));
        assert_eq!(dock_rect(Side::Left, P, c), Rect::new(-280, 100, 100, 700));
        assert_eq!(dock_rect(Side::Bottom, P, c), Rect::new(100, 700, 1100, 750));
        assert_eq!(dock_rect(Side::Top, P, c), Rect::new(100, 50, 1100, 100));
        assert_eq!(dev(Rect::new(1110, 100, 0, 0), Rect::new(1100, 100, 0, 0)), 10);
    }

    /// Eingerastetes Rechteck: Kante bündig, und derselbe Chat rastet danach wieder an derselben Seite ein.
    #[test]
    fn dock_rect_ist_bündig_und_stabil() {
        let sizes = [(1, 1), (380, 700), (500, 50), (2000, 3)];
        for side in [Side::Right, Side::Left, Side::Bottom, Side::Top] {
            for p in [P, Rect::new(-500, -200, 40, 300), Rect::new(0, 0, 640, 360)] {
                for (w, h) in sizes {
                    let c = Rect::new(7, 9, 7 + w, 9 + h);
                    let d = dock_rect(side, p, c);
                    assert_eq!((d.w() > 0, d.h() > 0), (true, true));
                    match side {
                        Side::Right => assert_eq!((d.l, d.t, d.b, d.w()), (p.r, p.t, p.b, w)),
                        Side::Left => assert_eq!((d.r, d.t, d.b, d.w()), (p.l, p.t, p.b, w)),
                        Side::Bottom => assert_eq!((d.t, d.l, d.r, d.h()), (p.b, p.l, p.r, h)),
                        Side::Top => assert_eq!((d.b, d.l, d.r, d.h()), (p.t, p.l, p.r, h)),
                    }
                    assert_eq!(snap_side(p, d, 0), Some(side), "{side:?} {p:?} {c:?}");
                    assert_eq!(dock_rect(side, p, d), d, "Doppelt andocken ändert nichts");
                }
            }
        }
    }

    fn input(player: Rect, chat: Rect, mouse_down: bool) -> Input {
        Input { player, chat, mouse_down }
    }

    fn docked() -> (Dock, Rect, Rect) {
        let c = Rect::new(1100, 100, 1480, 700);
        (Dock::new(P, c, Some(Side::Right)), P, c)
    }

    #[test]
    fn ruhe_tut_nichts() {
        let (mut d, p, c) = docked();
        assert_eq!(d.step(input(p, c, false)), None);
        assert_eq!(d.step(input(p, c, false)), None);
    }

    #[test]
    fn chat_folgt_dem_player_sofort() {
        let (mut d, _, c) = docked();
        let p2 = Rect::new(300, 200, 1300, 800);
        let target = d.step(input(p2, c, false)).expect("Chat muss folgen");
        assert_eq!(target, Rect::new(1300, 200, 1680, 800));
        d.applied(target, target);
        assert_eq!(d.step(input(p2, target, false)), None, "eigene Bewegung löst nichts aus");
    }

    #[test]
    fn player_skaliert_chat_folgt_mit_gleicher_höhe() {
        let (mut d, _, c) = docked();
        let p2 = Rect::new(100, 100, 1200, 800);
        let target = d.step(input(p2, c, false)).unwrap();
        assert_eq!(target, Rect::new(1200, 100, 1580, 800));
    }

    #[test]
    fn kleines_wegziehen_springt_beim_loslassen_zurück() {
        let (mut d, p, c) = docked();
        let drag = Rect::new(1115, 100, 1495, 700); // 15 px, unter UNDOCK
        assert_eq!(d.step(input(p, drag, true)), None, "bei gedrückter Maustaste nichts tun");
        assert_eq!(d.side(), Some(Side::Right));
        assert_eq!(d.step(input(p, drag, false)), Some(c), "beim Loslassen zurück");
    }

    #[test]
    fn weites_wegziehen_löst_sofort_und_chat_folgt_nicht_mehr() {
        let (mut d, p, _) = docked();
        let away = Rect::new(1220, 140, 1600, 740); // 120 px
        assert_eq!(d.step(input(p, away, true)), None);
        assert_eq!(d.side(), None, "löst schon während des Ziehens");
        assert_eq!(d.step(input(p, away, false)), None, "weit weg: bleibt frei");
        let p2 = Rect::new(0, 0, 1000, 600);
        assert_eq!(d.step(input(p2, away, false)), None, "Player bewegt, Chat bleibt");
    }

    /// Windows verschiebt den Chat von selbst (DPI-Wechsel zwischen Monitoren): das ist kein Wegziehen.
    #[test]
    fn verschiebung_durch_windows_loest_nicht_und_dockt_wieder_an() {
        let (mut d, p, c) = docked();
        let moved_by_os = Rect::new(850, 100, 1330, 700); // 250 px daneben, ohne Maustaste
        assert_eq!(d.step(input(p, moved_by_os, false)), None);
        assert_eq!(d.side(), Some(Side::Right), "bleibt angedockt");
        assert_eq!(d.step(input(p, moved_by_os, false)), Some(Rect::new(1100, 100, 1580, 700)), "wieder bündig, Breite behalten");
        let _ = c;
    }

    /// Beide Fenster ändern sich nacheinander (Sprung auf einen anderen Monitor): am Ende sitzt der Chat bündig.
    #[test]
    fn monitorwechsel_mit_nachskalierung_endet_bündig() {
        let (mut d, _, c) = docked();
        let p1 = Rect::new(-2392, 100, -1158, 892); // Player springt auf Monitor mit 125 %
        let t1 = d.step(input(p1, c, false)).unwrap();
        // Windows skaliert und verschiebt den Chat schon beim Setzen: die Rückmeldung zeigt bereits das fremde Ergebnis
        let skewed = Rect::new(t1.l - 248, t1.t, t1.l - 248 + 479, t1.b + 199);
        d.applied(t1, skewed);
        let t2 = d.step(input(p1, skewed, false)).expect("weicht vom Verlangten ab: nachbessern");
        assert_eq!((t2.l, t2.t, t2.b), (p1.r, p1.t, p1.b));
        d.applied(t2, t2); // jetzt passt es
        assert_eq!(d.step(input(p1, t2, false)), None);
    }

    #[test]
    fn freier_chat_rastet_erst_beim_loslassen_ein() {
        let p = P;
        let mut d = Dock::new(p, Rect::new(1400, 100, 1780, 700), None);
        let near = Rect::new(1115, 100, 1495, 700); // 15 px an der rechten Kante
        assert_eq!(d.step(input(p, near, true)), None, "Maustaste gedrückt");
        assert_eq!(d.step(input(p, near, true)), None);
        assert_eq!(d.step(input(p, near, false)), Some(Rect::new(1100, 100, 1480, 700)));
        assert_eq!(d.side(), Some(Side::Right));
    }

    #[test]
    fn freier_chat_weit_weg_bleibt_frei() {
        let mut d = Dock::new(P, Rect::new(1400, 100, 1780, 700), None);
        let c = Rect::new(1300, 100, 1680, 700);
        assert_eq!(d.step(input(P, c, false)), None);
        assert_eq!(d.step(input(P, c, false)), None);
        assert_eq!(d.side(), None);
    }

    #[test]
    fn player_neben_den_chat_ziehen_dockt_den_chat_an() {
        let c = Rect::new(1130, 100, 1510, 700);
        let mut d = Dock::new(Rect::new(100, 100, 700, 700), c, None);
        let p2 = Rect::new(400, 100, 1120, 700); // rechte Kante 10 px vor dem Chat
        assert_eq!(d.step(input(p2, c, true)), None);
        assert_eq!(d.step(input(p2, c, false)), Some(Rect::new(1120, 100, 1500, 700)));
    }

    #[test]
    fn chat_breiter_ziehen_an_der_freien_kante_bleibt_angedockt() {
        let (mut d, p, _) = docked();
        let wider = Rect::new(1100, 100, 1540, 700); // rechte Kante des Chats +60
        assert_eq!(d.step(input(p, wider, false)), None);
        assert_eq!(d.side(), Some(Side::Right));
        assert_eq!(d.step(input(p, wider, false)), None, "kein Zurücksetzen der Breite");
    }

    #[test]
    fn chat_niedriger_gezogen_wird_auf_playerhöhe_gesetzt() {
        let (mut d, p, _) = docked();
        let shorter = Rect::new(1100, 100, 1480, 650);
        assert_eq!(d.step(input(p, shorter, false)), None);
        assert_eq!(d.step(input(p, shorter, false)), Some(Rect::new(1100, 100, 1480, 700)));
    }

    #[test]
    fn begrenzte_chatgröße_erzeugt_keine_endlosschleife() {
        // WebView2 begrenzt die Größe: nach `applied` mit dem tatsächlichen Rechteck herrscht Ruhe
        let (mut d, _, c) = docked();
        let p2 = Rect::new(100, 100, 1100, 500);
        let wanted = d.step(input(p2, c, false)).unwrap();
        assert_eq!(wanted.h(), 400);
        let actual = Rect::new(wanted.l, wanted.t, wanted.r, wanted.t + 450); // min. Höhe 450
        d.applied(wanted, actual);
        let mut corrections = 0;
        for _ in 0..10 {
            match d.step(input(p2, actual, false)) {
                Some(_) => {
                    corrections += 1;
                    d.applied(wanted, actual); // die Größe bleibt begrenzt
                }
                None => {}
            }
        }
        assert!(corrections <= 2, "höchstens zwei Nachbesserungen, dann Ruhe: {corrections}");
        assert_eq!(d.step(input(p2, actual, false)), None);
    }

    #[test]
    fn links_oben_unten_folgen_genauso() {
        for (side, c) in [
            (Side::Left, Rect::new(-280, 100, 100, 700)),
            (Side::Bottom, Rect::new(100, 700, 1100, 750)),
            (Side::Top, Rect::new(100, 50, 1100, 100)),
        ] {
            let mut d = Dock::new(P, c, Some(side));
            let p2 = Rect::new(P.l + 50, P.t + 30, P.r + 50, P.b + 30);
            let t = d.step(input(p2, c, false)).unwrap();
            assert_eq!(snap_side(p2, t, 0), Some(side));
        }
    }
}
