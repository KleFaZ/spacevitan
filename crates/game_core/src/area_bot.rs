//! Der Flaechen-Bot (`flaeche`): spielt den Gebietsmodus ueber Land und Rohstoffe.
//!
//! Er rechnet nur einen Zug voraus, bewertet den Zustand danach aber mit der echten Engine
//! (Abtrennen, Umschliessen und Wiederanbinden sind also schon eingerechnet):
//! - **Land.** Jedes mit der Fabrik verbundene Feld zaehlt. Verbundenes Land des Gegners zaehlt
//!   gegen ihn.
//! - **Rohstoffe.** Ein Vorkommen auf eigenem verbundenem Land ist Geld wert, ein reifes mehr als
//!   eines, das noch reift. Ein Gegner, der ein reifes Vorkommen umfaerbt, verliert es sofort
//!   und beginnt bei null: Rohstofffelder sind deshalb Angriffsziele.
//! - **Figuren.** Er baut nur, wenn die neue Figur hoeher ist als seine beste (eine hoehere Stufe
//!   faerbt mehr Felder je Zug), und kauft Karten, die der naechsten Figur oder dem Koenig nuetzen.
//! - **Sicherheit.** Koenig, Fabrikfeld und Figuren auf bedrohten Feldern kosten Punkte.

use crate::action::Action;
use crate::bot::Bot;
use crate::cards::Card;
use crate::game::Game;
use crate::ids::PlayerId;
use crate::rng::Rng;
use crate::state::{Outcome, PieceKind};

const LAND: i64 = 10;
/// Wert einer Flaeche Kat-3-Bonus je Runde (ein Feld bringt je Runde eine Flaeche und ist 10 wert).
const BONUS_UNIT: i64 = 12;
/// Wert eines reifen Vorkommens je Kategorie (Kat 1, Kat 2, ...).
const DEPOSIT: [i64; 4] = [40, 90, 120, 150];
/// Wert einer Figur nach Stufe.
const UNIT: [i64; 5] = [0, 60, 150, 280, 450];
const WIN: i64 = 1_000_000_000;
const KING_LOST: i64 = 1_000_000;
const FACTORY_THREAT: i64 = 800;
/// Abzug, wenn der Koenig im naechsten Zug von der Fabrik getrennt werden kann (wehrlos, steht still).
const KING_CUT: i64 = 500;
/// Ab dieser Drohung (siehe `cut_value`) geht das Entschaerfen vor Bauen und Kartenkauf.
const URGENT_CUT: i64 = 300;
/// Abzug je Naehe-Einheit, wenn der Koenig hinter einer Engstelle steht, an die ein Gegner in zwei
/// Zuegen herankommt.
const TETHER: i64 = 40;
/// Abzug je Stufe einer Figur, die dabei von der Fabrik getrennt wuerde.
const CUT_UNIT: i64 = 60;
/// Was dem Bot eine Verbesserung durch eine Karte wert ist; die Sperre der Figur (Version 3)
/// darf hoechstens so viel kosten.
const CARD_VALUE: i64 = 30;

pub struct AreaBot {
    rng: Rng,
}

impl AreaBot {
    pub fn new(seed: u64) -> Self {
        AreaBot {
            rng: Rng::new(seed ^ 0x666c_6165_6368_6500),
        }
    }
}

/// Wert der Lage von `p`: verbundenes Land, Vorkommen darauf (gewichtet nach Reife), Figuren.
fn standing(g: &Game, p: PlayerId) -> i64 {
    standing_with(g, p, LAND, true)
}

/// Wie `standing`, aber mit frei gewaehltem Wert je Flaeche (`super-stark` zahlt mehr fuer Land,
/// solange noch Platz ist).
fn standing_with(g: &Game, p: PlayerId, land: i64, guard: bool) -> i64 {
    let mut v = 0;
    // Mit `terr_occupy` zaehlt der Vorkommen-Wert voll nur auf bearbeiteten Feldern.
    let worked = g.config.terr_occupy.then(|| g.worked_cells(p));
    // Ohne stehende Fabrik (zerstoert oder im Neubau) ist nichts verbunden: das Land gehoert ihm noch
    // und kommt mit der neuen Fabrik zurueck, die Wartezeit wird unten abgezogen.
    // (`guard` aus: so rechnete der Bot vor der Rangfolge Koenig > Fabrik, nur zum Messen.)
    let guard = guard && g.rebuild_enabled();
    let outage = if guard { outage_rounds(g, p) } else { 0 };
    let connected = if outage > 0 {
        let mut own = vec![false; g.map.board.len()];
        for c in g.map.board.cells() {
            own[c.idx()] = g.state.land[c.idx()] == Some(p);
        }
        Some(own)
    } else {
        g.connected_land(p)
    };
    if let Some(connected) = connected {
        for cell in g.map.board.cells().filter(|c| connected[c.idx()]) {
            // Mit Gelaende zaehlt ein Feld nach seinem Flaechenwert (Viertel), sonst voll.
            v += land * i64::from(g.land_quarters(cell)) / 4;
            for res in g.all_deposits(cell) {
                let cat = usize::from(g.map.category(res)).clamp(1, 4) - 1;
                // Kat 3 zaehlt mit Flaechenbonus nur ueber `2^k` unten, nicht als Vorkommen.
                if g.config.kat3_bonus_on() && cat == 2 {
                    continue;
                }
                if g.config.terr_occupy {
                    // Der Zaehler laeuft nur auf bearbeiteten Feldern (eigene Figur darauf, mit
                    // Kontrollfeldern auch im Einflussbereich): dann voller Wert, sonst ein Drittel
                    // (das Vorkommen ist da, aber es kommt nichts heraus).
                    v += if worked.as_ref().is_some_and(|w| w[cell.idx()]) {
                        DEPOSIT[cat]
                    } else {
                        DEPOSIT[cat] / 3
                    };
                    continue;
                }
                let need = g.config.terr_ripen[cat].max(1) as i64;
                let wait = g.ripen_wait(cell, res) as i64;
                // Reif = voller Wert, frisch gefaerbt ein gutes Drittel.
                v += DEPOSIT[cat] * (35 + 65 * (need - wait) / need) / 100;
            }
        }
        // Flaechenbonus: jede Runde `2^k` Flaeche zusaetzlich, das verdoppelt sich je Kat-3-Feld.
        v += BONUS_UNIT * i64::from(Game::kat3_bonus(g.connected_kat3(p)).min(2_000));
    }
    for piece in g.pieces_of(p) {
        if let PieceKind::Unit { level, .. } = piece.kind {
            v += UNIT[usize::from(level).min(4)];
        }
    }
    if guard {
        v -= outage_cost(g, p);
    } else if g.home_factory(p).is_none() {
        // Altes Verhalten: ohne Fabrik zaehlt das ganze Land ohne Zusammenhang weiter.
        v += g.land_count(p) as i64 * land;
    }
    v
}

/// Eigene Zuege, bis die Fabrik von `p` wieder steht (0 = sie steht). Im Neubau ist es die Restzeit;
/// nach einer Zerstoerung der Weg des Koenigs bis zum naechsten eigenen Feld plus die Bauzeit dort.
/// So lange sind alle seine Figuren wehrlos.
pub(crate) fn outage_rounds(g: &Game, p: PlayerId) -> i64 {
    match g.home_factory(p) {
        Some(f) => i64::from(f.building),
        None => wehrlos_rounds(g, p, g.state.lost_factory[p.idx()]),
    }
}

/// Wie viele eigene Zuege `p` ohne Fabrik waere, wenn sie (zuvor auf `old`) jetzt fehlte: der Koenig
/// geht zum naechsten eigenen Feld (er faerbt unterwegs nichts) und baut dort neu.
fn wehrlos_rounds(g: &Game, p: PlayerId, old: Option<crate::ids::CellId>) -> i64 {
    let Some(king) = g.king(p) else {
        return 0;
    };
    if g.config.terr_move == 0 {
        // Ohne Neubau-Regel kommt die Fabrik nie wieder.
        return 20;
    }
    let dist = g.map.board.distances_from(king.cell);
    let nearest = g
        .map
        .board
        .cells()
        .filter(|c| g.state.land[c.idx()] == Some(p))
        .min_by_key(|c| dist[c.idx()]);
    let (walk, to) = match nearest {
        Some(c) => (i64::from(dist[c.idx()]), c),
        None => (0, king.cell),
    };
    walk + i64::from(g.rebuild_rounds_from(old, to))
}

/// Was eine wehrlose Runde `p` kostet: ein Teil des Ertrags, ein Teil des Wertes seiner Figuren
/// (sie koennen nichts tun und sind leichte Beute) und die verlorene Zugzeit.
fn idle_per_round(g: &Game, p: PlayerId) -> i64 {
    let units: i64 = g
        .pieces_of(p)
        .map(|x| match x.kind {
            PieceKind::Unit { level, .. } => UNIT[usize::from(level).min(4)],
            PieceKind::King => 0,
        })
        .sum();
    i64::from(g.land_count(p)) * LAND / 8 + units / 4 + 60
}

/// Abzug fuer eine Fabrik, die fehlt oder im Neubau ist.
fn outage_cost(g: &Game, p: PlayerId) -> i64 {
    let rounds = outage_rounds(g, p);
    if rounds == 0 {
        return 0;
    }
    // Zerstoert heisst auch: Lager, Flaeche und Karte sind weg.
    let lost = if g.home_factory(p).is_none() {
        FACTORY_THREAT
    } else {
        0
    };
    lost + rounds.min(30) * idle_per_round(g, p)
}

/// Was es `me` kostet, wenn die Fabrik im naechsten gegnerischen Zug zerstoert wird: Lager und
/// Flaeche weg, dazu die Runden, in denen alle Figuren wehrlos sind, bis der Koenig sie neu gebaut hat.
pub(crate) fn factory_loss_cost(g: &Game, me: PlayerId) -> i64 {
    let Some(f) = g.standing_factory(me) else {
        return 0;
    };
    if !g.rebuild_enabled() {
        return FACTORY_THREAT;
    }
    FACTORY_THREAT + wehrlos_rounds(g, me, Some(f.cell)).min(30) * idle_per_round(g, me)
}

/// Wert der Lage fuer `me` gegen den Durchschnitt der lebenden Gegner.
fn evaluate(g: &Game, me: PlayerId) -> i64 {
    evaluate_with(g, me, |p| standing(g, p))
}

fn evaluate_with(g: &Game, me: PlayerId, stand: impl Fn(PlayerId) -> i64) -> i64 {
    if let Some(Outcome::Winner(w)) = g.outcome() {
        return if w == me { WIN } else { -WIN };
    }
    if !g.state.alive[me.idx()] {
        return -WIN;
    }
    let others: Vec<PlayerId> = g.alive_players().into_iter().filter(|&p| p != me).collect();
    let theirs: i64 = others.iter().map(|&p| stand(p)).sum::<i64>() / others.len().max(1) as i64;
    stand(me) - theirs
}

/// Stellschrauben von "super-stark" (`SearchBot::super_strong`): mehr Wert auf Land, solange noch
/// freier Platz da ist, und ein Aufschlag fuer eine Fabrik, die Richtung Brettmitte gerueckt ist.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drive {
    /// Zusaetzlicher Wert je Flaeche bei ganz leerem Brett; sinkt mit dem freien Anteil auf 0.
    pub land_extra: i64,
    /// Wert je Feldbreite, die die Fabrik naeher an der Mitte steht als die Startecke.
    pub centre: i64,
    /// Hoechstens so viele Feldbreiten zaehlen (rund drei bis vier Diagonalschritte).
    pub centre_cap: i64,
    /// Aufschlag, solange ein freies Diagonalfeld Richtung Mitte bereitsteht, die Karte im Markt
    /// liegt und bezahlbar ist.
    pub ready: i64,
    /// Kauft "Fabrik ruecken" gleich, sobald es kaufbar ist, die Fabrik damit naeher an die Mitte
    /// kommt und die Lage ruhig ist (0 = nur nach der Bewertung wie jede Karte).
    pub push: i64,
    /// Eroeffnung: der Koenig laeuft so viele Felder (1 oder 2) Richtung Mitte und baut dort die Fabrik neu
    /// (Umzug, kostenlos). 0 = aus.
    pub open: i64,
    /// Die Eroeffnung gilt nur bis zu dieser Runde.
    pub open_rounds: i64,
    /// Wert einer Umschliess-Chance (ein weiteres gefaerbtes Feld wuerde eine neutrale Flaeche ganz
    /// einschliessen), in Hundertsteln des Landwertes je Feld. 0 = aus.
    pub enclose: i64,
    /// Druck auf den Gegner: Punkte je Stufe und Feld Naehe zu gegnerischem Koenig und Fabrik, wachsend
    /// mit dem Fuellstand des Bretts. 0 = aus.
    pub press: i64,
    /// Angriffswert: Anteil (in Hundertsteln) dessen, was ich dem Gegner im naechsten Zug abtrennen
    /// koennte, wenn ich am Zug bin. 0 = aus.
    pub offence: i64,
    /// Schlagen zuerst: 0 = aus (altes Verhalten). Sonst gilt, solange ein sinnvoller Schlag moeglich ist,
    /// kein Kartenkauf und kein Fabrik-Rutschen vor der Zugsuche, und sinnvolle Schlaege bekommen den
    /// angegebenen Anteil (in Prozent) vom Wert der geschlagenen Figur als Zuschlag.
    pub hunt: i64,
    /// Rangfolge Koenig > Fabrik > eigene Macht > Koenigsjagd: eine fehlende oder im Neubau befindliche
    /// Fabrik kostet Runden wehrlos in der Bewertung, und bei bedrohter Fabrik prueft die Suche den
    /// Umzug des Koenigs. 0 = aus (altes Verhalten, nur zum Messen).
    pub guard: i64,
}

impl Drive {
    pub const DEFAULT: Drive = Drive {
        land_extra: 6,
        centre: 20,
        centre_cap: 6,
        ready: 30,
        push: 1,
        open: 2,
        open_rounds: 8,
        enclose: 100,
        press: 4,
        offence: 30,
        hunt: 50,
        guard: 1,
    };

    /// `land=6,centre=20,cap=6,ready=30,push=1` (nur zum Abstimmen).
    pub fn parse(list: &str) -> Result<Drive, String> {
        let mut d = Drive::DEFAULT;
        for part in list.split(',').filter(|p| !p.trim().is_empty()) {
            let (k, v) = part
                .split_once('=')
                .ok_or_else(|| format!("'{part}': name=wert erwartet"))?;
            let v: i64 = v
                .trim()
                .parse()
                .map_err(|_| format!("'{part}': Zahl erwartet"))?;
            match k.trim() {
                "land" => d.land_extra = v,
                "centre" | "center" => d.centre = v,
                "cap" => d.centre_cap = v,
                "ready" => d.ready = v,
                "push" => d.push = v,
                "open" => d.open = v,
                "orounds" => d.open_rounds = v,
                "enclose" => d.enclose = v,
                "press" => d.press = v,
                "offence" => d.offence = v,
                "hunt" => d.hunt = v,
                "guard" => d.guard = v,
                other => return Err(format!("unbekannte Stellschraube '{other}'")),
            }
        }
        Ok(d)
    }
}

/// Mittelpunkt des Bretts in Weltkoordinaten (Schwerpunkt des Umrisses).
fn board_centre(g: &Game) -> (f64, f64) {
    let outline = g.map.board.outline();
    let n = outline.len().max(1) as f64;
    let (sx, sy) = outline
        .iter()
        .fold((0.0, 0.0), |(x, y), p| (x + p.0, y + p.1));
    (sx / n, sy / n)
}

/// Wie viele Feldbreiten die Fabrik von `p` naeher an der Brettmitte steht als die Startecken
/// (in Tausendsteln, zwischen 0 und `cap` Feldbreiten).
fn centre_progress(g: &Game, p: PlayerId, cap: i64) -> i64 {
    g.home_factory(p)
        .map_or(0, |f| centre_progress_at(g, f.cell, cap))
}

/// Wie `centre_progress` fuer eine Fabrik auf `cell`.
fn centre_progress_at(g: &Game, cell: crate::ids::CellId, cap: i64) -> i64 {
    let board = &g.map.board;
    let (cx, cy) = board_centre(g);
    let dist = |c: crate::ids::CellId| {
        let (x, y) = board.pos(c);
        ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()
    };
    let corners = board.corners();
    if corners.is_empty() {
        return 0;
    }
    let start = corners.iter().map(|&c| dist(c)).sum::<f64>() / corners.len() as f64;
    // Feldbreite = Abstand zweier Nachbarfelder.
    let first = crate::ids::CellId(0);
    let spacing = board
        .neighbors(first)
        .next()
        .map(|n| {
            let (a, b) = (board.pos(first), board.pos(n));
            ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
        })
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0);
    let steps = (start - dist(cell)) / spacing;
    ((steps * 1000.0) as i64).clamp(0, cap * 1000)
}

/// Kann die Fabrik von `me` jetzt per "Fabrik ruecken" naeher an die Mitte (und noch unter der
/// Obergrenze)? Dann gibt es ein Diagonalfeld, das eigenes verbundenes Land ist, frei von Figuren.
pub(crate) fn can_advance(g: &Game, me: PlayerId, d: &Drive) -> bool {
    let Some(f) = g.standing_factory(me) else {
        return false;
    };
    let now = centre_progress_at(g, f.cell, d.centre_cap);
    now < d.centre_cap * 1000
        && g.factory_shift_cells(me)
            .into_iter()
            .any(|c| centre_progress_at(g, c, d.centre_cap) > now)
}

/// Gehoert `p` schon ein freies Diagonalfeld der Fabrik, das naeher an der Mitte liegt (nur Quadratbrett)?
/// Billige Vorstufe von `can_advance` fuer die Bewertung: Zusammenhang und freie Figurenfelder
/// prueft erst der Kauf.
fn owns_diagonal_ahead(g: &Game, p: PlayerId, d: &Drive) -> bool {
    let board = &g.map.board;
    if board.kind() != crate::board::GridKind::Square {
        return false;
    }
    let Some(f) = g.standing_factory(p) else {
        return false;
    };
    let now = centre_progress_at(g, f.cell, d.centre_cap);
    if now >= d.centre_cap * 1000 {
        return false;
    }
    let (x, y) = board.coord(f.cell);
    [(1, 1), (1, -1), (-1, 1), (-1, -1)]
        .into_iter()
        .filter_map(|(dx, dy)| board.cell_at(x + dx, y + dy))
        .any(|c| {
            g.state.land[c.idx()] == Some(p)
                && g.state.piece_at(c).is_none()
                && centre_progress_at(g, c, d.centre_cap) > now
        })
}

/// Ist die Lage ruhig genug, um einen Zug fuer das Rutschen der Fabrik herzugeben (nichts
/// Eigenes im naechsten gegnerischen Zug bedroht)?
pub(crate) fn calm(g: &Game, me: PlayerId) -> bool {
    risk(g, me) < URGENT_CUT
}

/// Anteil freier (herrenloser) Felder am Brett in Tausendsteln.
fn free_permille(g: &Game) -> i64 {
    let free = g.state.land.iter().filter(|l| l.is_none()).count() as i64;
    free * 1000 / (g.state.land.len().max(1) as i64)
}

/// Wie `evaluate`, aber mit dem Aufschlag von "super-stark".
fn evaluate_drive(g: &Game, me: PlayerId, d: &Drive) -> i64 {
    let land = LAND + d.land_extra * free_permille(g) / 1000;
    // Liegt "Fabrik ruecken" im Markt, zaehlt ein schon eigenes, verbundenes Diagonalfeld Richtung
    // Mitte als Vorbereitung: so lenkt die Suche Figuren dorthin, bevor die Karte gekauft wird.
    let shift_in_market = d.ready > 0 && g.state.market.contains(&Card::FactoryShift);
    evaluate_with(g, me, |p| {
        let mut v = standing_with(g, p, land, d.guard > 0)
            + d.centre * centre_progress(g, p, d.centre_cap) / 1000;
        if shift_in_market
            && owns_diagonal_ahead(g, p, d)
            && g.territory_card_payment(p, Card::FactoryShift).is_some()
        {
            v += d.ready;
        }
        if d.enclose > 0 {
            // Wer am Zug ist, kann die Chance gleich nutzen, der andere erst nach dessen Antwort.
            let side = if g.state.current == p { 100 } else { 35 };
            v += d.enclose * enclosure_chance(g, p) * land * side / 10_000;
        }
        if d.press > 0 {
            v += press_value(g, p, d.press);
        }
        v
    })
}

/// Wie viele neutrale Felder `p` mit einem einzigen weiteren gefaerbten Feld auf einmal umschliessen
/// koennte (das beste Feld neben seinem verbundenen Land). Nur eine Schaetzung: ob eine Figur das
/// Feld in einem Zug faerben kann, wird nicht geprueft.
pub(crate) fn enclosure_chance(g: &Game, p: PlayerId) -> i64 {
    if !g.config.terr_enclose {
        return 0;
    }
    let Some(conn) = g.connected_land(p) else {
        return 0;
    };
    let board = &g.map.board;
    let n = board.len();
    let full = board
        .cells()
        .map(|c| board.neighbors(c).count())
        .max()
        .unwrap_or(0);
    let edge_is_wall = g.config.terr_enclose_edge;
    let mut land: Vec<Option<PlayerId>> = g.state.land.clone();
    let mut best = 0usize;
    let mut seen = vec![0u32; n];
    let mut stamp = 0u32;
    let mut region: Vec<usize> = Vec::with_capacity(n);
    for c in board.cells() {
        // Nur neutrale Felder neben verbundenem eigenem Land kommen als Wand in Frage.
        if land[c.idx()].is_some() || !board.neighbors(c).any(|nb| conn[nb.idx()]) {
            continue;
        }
        land[c.idx()] = Some(p);
        stamp += 1;
        let mut gain = 0usize;
        // Neutrale Flaechen rund um `c` auf Einschluss durch genau `p` pruefen.
        for start in board.neighbors(c) {
            if land[start.idx()].is_some() || seen[start.idx()] == stamp {
                continue;
            }
            region.clear();
            region.push(start.idx());
            seen[start.idx()] = stamp;
            let mut owner: Option<PlayerId> = None;
            let mut mixed = false;
            let mut head = 0;
            while head < region.len() {
                let cur = crate::ids::CellId(region[head] as u32 as _);
                head += 1;
                if !edge_is_wall && board.neighbors(cur).count() < full {
                    mixed = true;
                }
                for nb in board.neighbors(cur) {
                    match land[nb.idx()] {
                        None => {
                            if seen[nb.idx()] != stamp {
                                seen[nb.idx()] = stamp;
                                region.push(nb.idx());
                            }
                        }
                        Some(q) => match owner {
                            None => owner = Some(q),
                            Some(o) if o != q => mixed = true,
                            _ => {}
                        },
                    }
                }
                if mixed {
                    break;
                }
            }
            if !mixed && owner == Some(p) {
                gain += region.len();
            }
        }
        land[c.idx()] = None;
        best = best.max(gain);
    }
    best as i64
}

/// Druck auf den Gegner: eigene Figuren (nach Stufe) nahe bei gegnerischem Koenig und gegnerischer
/// Fabrik, stärker bei vollem Brett (dann gibt es Land nur noch vom Gegner).
fn press_value(g: &Game, p: PlayerId, weight: i64) -> i64 {
    let board = &g.map.board;
    let mut targets: Vec<&[u32]> = Vec::new();
    for q in g.alive_players().into_iter().filter(|&q| q != p) {
        if let Some(k) = g.king(q) {
            targets.push(board.distances_from(k.cell));
        }
        if let Some(f) = g.home_factory(q) {
            targets.push(board.distances_from(f.cell));
        }
    }
    if targets.is_empty() {
        return 0;
    }
    let filled = 1000 - free_permille(g);
    let mut sum = 0i64;
    for piece in g.pieces_of(p) {
        let PieceKind::Unit { level, .. } = piece.kind else {
            continue;
        };
        let near = targets
            .iter()
            .map(|d| d[piece.cell.idx()])
            .min()
            .unwrap_or(99);
        sum += i64::from(level) * i64::from(6u32.saturating_sub(near));
    }
    weight * sum * filled / 1000
}

/// Eroeffnung ("super-stark" mit `open`): der Koenig geht ein oder zwei Felder Richtung Mitte und
/// baut dort die Fabrik neu (kostenlos, der Umzug dauert eine Runde). Fruehe Zuege sind wenig wert,
/// die Fabrik steht danach dauerhaft naeher an der Mitte. `None`, wenn nichts zu tun ist.
pub(crate) fn opening_move(g: &Game, me: PlayerId, d: &Drive, legal: &[Action]) -> Option<Action> {
    if d.open <= 0 || i64::from(g.state.round) > d.open_rounds {
        return None;
    }
    let king = g.king(me)?;
    let fac = g.home_factory(me)?;
    if fac.building > 0 || fac.cell != g.map.start_cell(me) {
        return None;
    }
    // Nichts Feindliches in der Naehe, sonst gilt die normale Suche.
    if !calm(g, me) {
        return None;
    }
    let board = &g.map.board;
    let from_fac = board.distances_from(fac.cell);
    let progress = |c: crate::ids::CellId| centre_progress_at(g, c, 100);
    let here = from_fac[king.cell.idx()] as i64;
    let better = progress(king.cell) > progress(fac.cell);
    if king.cell != fac.cell
        && better
        && (here >= d.open
            || legal.contains(&Action::RebuildFactory)
                && here >= 1
                && !has_step(g, king.cell, from_fac, &progress, legal, king.id))
    {
        if legal.contains(&Action::RebuildFactory) {
            return Some(Action::RebuildFactory);
        }
        return None;
    }
    // Ein Schritt weiter weg von der Fabrik und naeher an der Mitte.
    let mut best: Option<(i64, Action)> = None;
    for &a in legal {
        let Action::Move { piece, to } = a else {
            continue;
        };
        if piece != king.id
            || !board.neighbors(king.cell).any(|n| n == to)
            || g.state.piece_at(to).is_some()
            || i64::from(from_fac[to.idx()]) != here + 1
            || progress(to) <= progress(king.cell).max(progress(fac.cell))
        {
            continue;
        }
        if best.is_none_or(|(v, _)| progress(to) > v) {
            best = Some((progress(to), a));
        }
    }
    best.map(|(_, a)| a)
}

/// Gibt es fuer den Koenig noch einen Schritt weg von der Fabrik, der naeher an die Mitte fuehrt?
fn has_step(
    g: &Game,
    at: crate::ids::CellId,
    from_fac: &[u32],
    progress: &dyn Fn(crate::ids::CellId) -> i64,
    legal: &[Action],
    king: crate::ids::PieceId,
) -> bool {
    let board = &g.map.board;
    legal.iter().any(|a| match *a {
        Action::Move { piece, to } => {
            piece == king
                && board.neighbors(at).any(|n| n == to)
                && g.state.piece_at(to).is_none()
                && from_fac[to.idx()] > from_fac[at.idx()]
                && progress(to) > progress(at)
        }
        _ => false,
    })
}

/// Ein moeglicher Schlag des Spielers am Zug (siehe `captures`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Capture {
    pub action: Action,
    /// Wert der geschlagenen Figur (Koenig: `KING_LOST`).
    pub victim: i64,
    /// Sinnvoll: entscheidet die Partie, oder die Figur wird nicht zurueckgeschlagen, oder sie tauscht
    /// gegen etwas Wertvolleres. Sinnlos ist es, eine Figur in den Gegner laufen zu lassen, die dort
    /// fuer weniger Wert wieder verloren geht.
    pub sound: bool,
}

/// Alle Schlaege unter `moves` (Zuege auf ein Feld mit gegnerischer Figur), jeweils mit Pruefung, ob sie
/// sinnvoll sind.
pub(crate) fn captures(g: &Game, me: PlayerId, moves: &[Action]) -> Vec<Capture> {
    let value = |k: &PieceKind| match k {
        PieceKind::King => KING_LOST,
        PieceKind::Unit { level, .. } => UNIT[usize::from(*level).min(4)],
    };
    let mut out = Vec::new();
    for &a in moves {
        let Action::Move { piece, to } = a else {
            continue;
        };
        let Some(victim) = g.state.piece_at(to).filter(|v| v.owner != me) else {
            continue;
        };
        let Some(mover) = g.state.pieces.get(&piece) else {
            continue;
        };
        let (gain, own) = (value(&victim.kind), value(&mover.kind));
        let mut child = g.clone();
        child.apply_unchecked(a);
        let recaptured = child.outcome().is_none()
            && child.current_player() != me
            && child
                .legal_actions_for_bot()
                .iter()
                .any(|r| matches!(*r, Action::Move { to: t, .. } if t == to));
        out.push(Capture {
            action: a,
            victim: gain,
            sound: !recaptured || gain > own,
        });
    }
    out
}

/// Was ein Gegner im naechsten Zug an Land, Figuren und Koenig von `me` abtrennen koennte.
fn cut_value(g: &Game, me: PlayerId) -> i64 {
    g.cut_threat(me, |w| {
        let king = if w.king { KING_CUT } else { 0 };
        i64::from(w.cells) * 2 * LAND + i64::from(w.levels) * CUT_UNIT + king
    })
}

/// Abzug fuer alles, was der Gegner im naechsten Zug erreichen koennte.
fn risk(g: &Game, me: PlayerId) -> i64 {
    let threatened = g.threatened_cells(me);
    let mut r = 0;
    for piece in g.pieces_of(me) {
        if !threatened.contains(&piece.cell) {
            continue;
        }
        r += match piece.kind {
            PieceKind::King => KING_LOST,
            PieceKind::Unit { level, .. } => UNIT[usize::from(level).min(4)] * 9 / 10,
        };
    }
    if g.home_factory(me)
        .is_some_and(|f| threatened.contains(&f.cell))
    {
        r += factory_loss_cost(g, me);
    }
    // Eine duenne Verbindung: kann ein Gegner im naechsten Zug eine Engstelle umfaerben, fallen die
    // Felder dahinter an ihn (doppelter Landwert), Figuren darauf sind wehrlos und der Koenig steht
    // still, bis er wieder angebunden ist.
    r + cut_risk(g, me)
}

/// Anteilige Gefahr einer duennen Verbindung, die ein Gegner in zwei Zuegen erreicht: je naeher eine
/// gegnerische Figur, desto mehr (Naehe 5 = direkt daneben).
pub(crate) fn exposure(g: &Game, me: PlayerId) -> i64 {
    let board = &g.map.board;
    g.weak_exposure(
        me,
        |a, b| board.distances_from(a)[b.idx()],
        |w, close| {
            let king = if w.king { TETHER * close } else { 0 };
            i64::from(w.cells) * LAND * close / 6 + king
        },
    )
}

/// Was `me` an Land, Figuren und Koenig verliert, wenn ein Gegner eine duenne Verbindung kappt:
/// im naechsten Zug genau gerechnet, dazu anteilig die Zwei-Zug-Gefahr.
fn cut_risk(g: &Game, me: PlayerId) -> i64 {
    cut_value(g, me) + exposure(g, me)
}

/// Bewertung eines Blattes der Suche von "sehr stark" (`SearchBot::with_area_eval`): Lage von `me`
/// gegen die der Gegner wie bei `flaeche`. Eine duenne Verbindung, die ein Gegner kappen koennte,
/// kostet Punkte; Schlagen und Koenigsfang sieht die Suche selbst.
#[cfg(test)]
pub(crate) fn leaf_value(g: &Game, me: PlayerId) -> i64 {
    leaf_value_with(g, me, None)
}

/// `leaf_value`, mit `Some(drive)` nach der Spielweise von "super-stark".
pub(crate) fn leaf_value_with(g: &Game, me: PlayerId, drive: Option<&Drive>) -> i64 {
    let v = match drive {
        Some(d) => evaluate_drive(g, me, d),
        None => evaluate(g, me),
    };
    if v.abs() >= WIN / 2 {
        return v;
    }
    // Nur wenn der Gegner am Zug ist, zaehlt die Kappung: ist `me` am Zug, kann er sie noch abwehren.
    // (Gemessen: die anteilige Gefahr auch dann abzuziehen, machte "sehr stark" schwaecher, ohne
    // dass der Koenig seltener abgetrennt wurde.)
    if g.state.current != me {
        v - cut_risk(g, me)
    } else if drive.is_some_and(|d| d.offence > 0) {
        // Angriff: was ich dem Gegner jetzt abtrennen koennte (er hat es noch nicht abgewehrt).
        let best = g
            .alive_players()
            .into_iter()
            .filter(|&q| q != me)
            .map(|q| cut_value(g, q))
            .max()
            .unwrap_or(0);
        v + drive.map_or(0, |d| d.offence) * best / 100
    } else {
        v
    }
}

impl AreaBot {
    /// Version 3 (Karten auf gebaute Figuren): alle Kartenkaeufe in der Reihenfolge, in der der
    /// Bot sie haben will: Ecke, Sprung, Kuerzer, je Karte die hoechste Figur zuerst.
    fn cards_for_pieces(game: &Game, buys: &[Action]) -> Vec<Action> {
        let level_of = |id: crate::ids::PieceId| match game.state.pieces.get(&id).map(|p| &p.kind) {
            Some(PieceKind::Unit { level, .. }) => *level,
            _ => 0,
        };
        let mut out = Vec::new();
        for want in [Card::Bend, Card::Hop, Card::Shorten] {
            let mut of_card: Vec<Action> = buys
                .iter()
                .copied()
                .filter(
                    |a| matches!(a, Action::BuyCard { card, target: Some(_), .. } if *card == want),
                )
                .collect();
            of_card.sort_by_key(|a| match a {
                Action::BuyCard {
                    target: Some(id), ..
                } => (std::cmp::Reverse(level_of(*id)), *id),
                _ => (std::cmp::Reverse(0), crate::ids::PieceId(0)),
            });
            out.extend(of_card);
        }
        out
    }

    /// Bewertung eines Zugs: Lage danach minus Risiko (ohne die Sonderregeln von `best_by_value`).
    fn move_value(g: &Game, me: PlayerId, a: Action) -> i64 {
        let mut child = g.clone();
        child.apply_unchecked(a);
        let mut value = evaluate(&child, me);
        if child.outcome().is_none() {
            value -= risk(&child, me);
        }
        value
    }

    /// Version 3: was die Sperre kostet, die ein Kartenkauf mitbringt (die verbesserte Figur ruht
    /// diesen Zug, oder der Kauf beendet den Zug): bester Zug jetzt gegen bester Zug danach.
    fn lock_cost(game: &Game, me: PlayerId, buy: Action, options: &[Action]) -> i64 {
        if !game.config.territory || game.config.terr_cardlock == 0 {
            return 0;
        }
        let Some(now) = options.iter().map(|&a| Self::move_value(game, me, a)).max() else {
            return 0;
        };
        let mut child = game.clone();
        child.apply_unchecked(buy);
        let after = if child.current_player() != me || child.outcome().is_some() {
            // Der Kauf war der ganze Zug.
            let mut v = evaluate(&child, me);
            if child.outcome().is_none() {
                v -= risk(&child, me);
            }
            v
        } else {
            child
                .legal_actions()
                .into_iter()
                .filter(|a| {
                    !matches!(
                        a,
                        Action::BuyCard { .. } | Action::RebuildFactory | Action::TradeKat3 { .. }
                    )
                })
                .map(|a| Self::move_value(&child, me, a))
                .max()
                .unwrap_or(i64::MIN / 2)
        };
        (now - after).max(0)
    }

    /// Welche Karte nuetzt jetzt, wenn gleich gebaut wird? Rangfolge: Ecke, Sprung, Kuerzer.
    fn card_for_build(buys: &[Action]) -> Option<Action> {
        [Card::Bend, Card::Hop, Card::Shorten]
            .into_iter()
            .find_map(|want| {
                buys.iter()
                    .copied()
                    .find(|a| matches!(a, Action::BuyCard { card, .. } if *card == want))
            })
    }

    /// Die Figur auf der eigenen Fabrik samt Wert der Figur, die man danach bauen koennte, wenn
    /// eine hoehere als jede vorhandene bezahlbar waere (Stufe 1 erst ab Runde 12, wie beim Bauen).
    fn blocking_piece(
        g: &Game,
        me: PlayerId,
        best_owned: u8,
    ) -> Option<(crate::ids::PieceId, i64)> {
        let fac = g.home_factory(me)?;
        let piece = g.state.piece_at(fac.cell).filter(|p| p.owner == me)?;
        let level = (best_owned + 1..=g.max_unit_level())
            .rev()
            .find(|&l| g.can_afford_unit(me, l) && (l >= 2 || g.state.round >= 12))?;
        Some((piece.id, UNIT[usize::from(level).min(4)] * 8 / 10))
    }

    fn best_by_value(
        &mut self,
        g: &Game,
        me: PlayerId,
        actions: &[Action],
        freeing: Option<(crate::ids::PieceId, i64)>,
    ) -> Option<Action> {
        let mut best: Vec<Action> = Vec::new();
        let mut best_value = i64::MIN;
        for &a in actions {
            let mut value = Self::move_value(g, me, a);
            // Die Fabrik zu rutschen ist selten sinnvoll; nur wenn die Bewertung es verlangt.
            if matches!(a, Action::ShiftFactory { .. }) {
                value -= 5;
            }
            if let (Some((blocker, bonus)), Action::Move { piece, .. }) = (freeing, a) {
                if piece == blocker {
                    value += bonus;
                }
            }
            match value.cmp(&best_value) {
                std::cmp::Ordering::Greater => {
                    best_value = value;
                    best = vec![a];
                }
                std::cmp::Ordering::Equal => best.push(a),
                std::cmp::Ordering::Less => {}
            }
        }
        self.rng.pick(&best).copied()
    }
}

impl Bot for AreaBot {
    fn name(&self) -> &'static str {
        "flaeche"
    }

    fn choose(&mut self, game: &Game) -> Action {
        if let Some(a) = game.auto_dissolve_action() {
            return a;
        }
        // Waehlt ein Mensch selbst, was sich aufloest (`terr_upkeep_pick`), rechnet der Bot seine Zuege
        // so, als loeste sich bei den Menschen automatisch die schwaechste Figur auf.
        let owned;
        let game = if game.config.terr_upkeep_pick != 0 {
            let mut copy = game.clone();
            copy.config.terr_upkeep_pick = 0;
            owned = copy;
            &owned
        } else {
            game
        };
        let legal = game.legal_actions_for_bot();
        if legal.len() == 1 || !game.config.territory {
            return legal[0];
        }
        let me = game.current_player();
        // Version 3: Kat 3 tauschen und eine zerstoerte Fabrik sofort neu bauen; freiwillig zieht
        // dieser Bot nicht um.
        if game.bot_should_trade(me) {
            if let Some(&t) = legal.iter().find(|a| matches!(a, Action::TradeKat3 { .. })) {
                return t;
            }
        }
        if game.home_factory(me).is_none() && legal.contains(&Action::RebuildFactory) {
            return Action::RebuildFactory;
        }
        let (buys, rest): (Vec<Action>, Vec<Action>) = legal
            .iter()
            .copied()
            .filter(|a| !matches!(a, Action::RebuildFactory | Action::TradeKat3 { .. }))
            .partition(|a| matches!(a, Action::BuyCard { .. }));

        // Zweite Rangstufe nach dem Koenig: die Fabrik. Kann ein Gegner sie im naechsten Zug zerstoeren,
        // geht ein Zug vor Bauen und Karten, der die Gefahr beseitigt (Schlag, Abtrennen, Umzug des
        // Koenigs). Der Umzug (Neubau) kostet ein paar wehrlose Runden; die Bewertung rechnet nach,
        // ob das billiger ist, als die Fabrik zu verlieren.
        if let Some(f) = game.standing_factory(me) {
            if game.threatened_cells(me).contains(&f.cell) {
                let mut cands: Vec<Action> = rest
                    .iter()
                    .copied()
                    .filter(|a| !matches!(a, Action::BuildUnit { .. }))
                    .collect();
                if legal.contains(&Action::RebuildFactory) {
                    cands.push(Action::RebuildFactory);
                }
                if let Some(a) = self.best_by_value(game, me, &cands, None) {
                    let mut child = game.clone();
                    child.apply_unchecked(a);
                    let safe = child.outcome().is_some()
                        || child
                            .home_factory(me)
                            .is_none_or(|nf| !child.threatened_cells(me).contains(&nf.cell));
                    if safe {
                        return a;
                    }
                }
            }
        }

        // Eine Figur bauen, wenn sie hoeher ist als jede, die er schon hat (oder er gar keine
        // hat und es schon spaet ist): hoehere Stufen faerben mehr Felder je Zug.
        let best_owned = game
            .pieces_of(me)
            .filter_map(|p| match p.kind {
                PieceKind::Unit { level, .. } => Some(level),
                PieceKind::King => None,
            })
            .max()
            .unwrap_or(0);
        let build = rest
            .iter()
            .copied()
            .filter_map(|a| match a {
                Action::BuildUnit { level, .. } => Some((level, a)),
                _ => None,
            })
            .max_by_key(|&(level, _)| level)
            .filter(|&(level, _)| level > best_owned && (level >= 2 || game.state.round >= 12));

        // Droht, dass ein Gegner im naechsten Zug Land oder den Koenig von der Fabrik trennt, und
        // laesst sich das mit einem Zug (nicht mit Bauen oder einem Kartenkauf) deutlich
        // entschaerfen, geht das vor.
        let threat = cut_value(game, me);
        if threat >= URGENT_CUT {
            let moves: Vec<Action> = rest
                .iter()
                .copied()
                .filter(|a| !matches!(a, Action::BuildUnit { .. }))
                .collect();
            if let Some(a) = self.best_by_value(game, me, &moves, None) {
                let mut child = game.clone();
                child.apply_unchecked(a);
                if cut_value(&child, me) + URGENT_CUT / 2 <= threat {
                    return a;
                }
            }
        }

        // Alles, was dieser Zug sonst tun koennte (fuer die Kosten der Sperre durch eine Karte).
        let options: Vec<Action> = rest.clone();
        // Karten vor dem Bauen: dem Koenig den Boost, der naechsten Figur eine Faehigkeit.
        if let Some(boost) = buys.iter().copied().find(|a| {
            matches!(
                a,
                Action::BuyCard {
                    card: Card::KingBoost,
                    ..
                }
            )
        }) {
            if Self::lock_cost(game, me, boost, &options) <= CARD_VALUE {
                return boost;
            }
        }
        if game.config.terr_cardany {
            if let Some(card) = Self::cards_for_pieces(game, &buys)
                .into_iter()
                .find(|&a| Self::lock_cost(game, me, a, &options) <= CARD_VALUE)
            {
                return card;
            }
        } else if build.is_some() && game.state.stored_card[me.idx()].is_none() {
            if let Some(card) = Self::card_for_build(&buys) {
                return card;
            }
        }
        if let Some((_, action)) = build {
            return action;
        }

        let moves: Vec<Action> = rest
            .into_iter()
            .filter(|a| !matches!(a, Action::BuildUnit { .. }))
            .collect();
        // Steht eine Figur auf der Fabrik und waere eine bessere Figur bezahlbar, lohnt es, sie
        // wegzuziehen (die Bewertung allein sieht den Gewinn nicht: er kommt erst im naechsten Zug).
        let freeing = if build.is_none() {
            Self::blocking_piece(game, me, best_owned)
        } else {
            None
        };
        if let Some(a) = self.best_by_value(game, me, &moves, freeing) {
            return a;
        }
        legal
            .iter()
            .copied()
            .find(|a| !matches!(a, Action::RebuildFactory | Action::TradeKat3 { .. }))
            .unwrap_or(legal[0])
    }
}

#[cfg(test)]
mod drive_tests {
    use super::*;
    use crate::action::Action;
    use crate::ai::SearchBot;
    use crate::config::Config;

    /// Die Stellschrauben vor den Aenderungen vom 19.11. (nur Land, Mitte, Fabrik-Rutschen).
    const OLD: Drive = Drive {
        open: 0,
        enclose: 0,
        press: 0,
        offence: 0,
        hunt: 0,
        ..Drive::DEFAULT
    };

    fn game() -> Game {
        let mut cfg = Config::new(2, 7);
        cfg.territory = true;
        cfg.set_v3(true);
        Game::new(cfg).unwrap()
    }

    #[test]
    fn the_drive_settings_can_be_tuned_by_text() {
        assert_eq!(Drive::parse("").unwrap(), Drive::DEFAULT);
        let d = Drive::parse("land=9, centre=30,cap=4").unwrap();
        assert_eq!((d.land_extra, d.centre, d.centre_cap), (9, 30, 4));
        assert!(Drive::parse("land").is_err());
        assert!(Drive::parse("tempo=1").is_err());
    }

    #[test]
    fn a_factory_nearer_the_middle_scores_higher_for_super_stark_only() {
        let mut g = game();
        let me = PlayerId(0);
        let start = centre_progress(&g, me, 6);
        assert_eq!(start, 0, "in der Startecke gibt es nichts");
        // Die Fabrik rutscht zwei Diagonalschritte zur Mitte.
        let fid = g.home_factory(me).unwrap().id;
        let (x, y) = g.map.board.coord(g.home_factory(me).unwrap().cell);
        let side = g.map.board.side() as i32;
        let dir = if x == 0 { 1 } else { -1 };
        let diry = if y == 0 { 1 } else { -1 };
        assert!(side > 4);
        for step in 1..=2 {
            let to = g
                .map
                .board
                .cell_at(x + dir * step, y + diry * step)
                .unwrap();
            g.state.move_factory(fid, to);
        }
        let moved = centre_progress(&g, me, 6);
        assert!(
            moved > 2000,
            "zwei Diagonalschritte sind gut zwei Feldbreiten: {moved}"
        );
        let drive = Drive::DEFAULT;
        let plain = leaf_value_with(&g, me, None);
        let with = leaf_value_with(&g, me, Some(&drive));
        assert!(
            with > plain,
            "der Aufschlag zaehlt nur fuer super-stark ({with} gegen {plain})"
        );
        // Mehr als `cap` Feldbreiten zaehlen nicht.
        assert!(centre_progress(&g, me, 1) <= 1000);
    }

    /// P0 hat Land um die Startecke, Eisen fuer die Karte und "Fabrik ruecken" im Markt.
    fn shift_ready_game() -> Game {
        let mut g = game();
        g.state.market = vec![Card::FactoryShift];
        let me = PlayerId(0);
        for (x, y) in [(1, 0), (0, 1), (1, 1)] {
            let c = g.map.board.cell_at(x, y).unwrap();
            g.set_land(c, Some(me));
        }
        let iron = g
            .map
            .resource_types
            .iter()
            .find(|t| t.home_corner == Some(0) && t.category == 1)
            .unwrap()
            .id;
        let fid = g.home_factory(me).unwrap().id;
        g.setup_storage(fid, iron, 6);
        g
    }

    #[test]
    fn super_stark_buys_the_factory_shift_card_when_it_moves_the_factory_toward_the_middle() {
        let g = shift_ready_game();
        let me = PlayerId(0);
        assert!(can_advance(&g, me, &Drive::DEFAULT));
        assert!(calm(&g, me));
        let mut bot = SearchBot::super_strong(3).with_drive(OLD);
        let a = crate::bot::Bot::choose(&mut bot, &g);
        assert!(
            matches!(
                a,
                Action::BuyCard {
                    card: Card::FactoryShift,
                    ..
                }
            ),
            "{a:?}"
        );
        // "sehr-stark" hat diesen Drang nicht (kein Aufschlag fuer die Mitte).
        let mut plain = SearchBot::very_strong(3);
        let b = crate::bot::Bot::choose(&mut plain, &g);
        assert!(
            !matches!(
                b,
                Action::BuyCard {
                    card: Card::FactoryShift,
                    ..
                }
            ),
            "{b:?}"
        );
    }

    #[test]
    fn super_stark_then_shifts_toward_the_middle_and_stops_at_the_cap() {
        let mut g = shift_ready_game();
        let me = PlayerId(0);
        let fid = g.home_factory(me).unwrap().id;
        let buy = g
            .legal_actions()
            .into_iter()
            .find(|a| {
                matches!(
                    a,
                    Action::BuyCard {
                        card: Card::FactoryShift,
                        ..
                    }
                )
            })
            .unwrap();
        g.apply(buy).unwrap();
        let mut bot = SearchBot::super_strong(3).with_drive(OLD);
        let a = crate::bot::Bot::choose(&mut bot, &g);
        let Action::ShiftFactory { to, .. } = a else {
            panic!("erzwungenes Rutschen erwartet: {a:?}");
        };
        assert_eq!(to, g.map.board.cell_at(1, 1).unwrap());
        g.apply(a).unwrap();
        assert_eq!(g.home_factory(me).unwrap().id, fid);
        assert!(centre_progress(&g, me, 6) > 1000);
        // Mit der Obergrenze 1 ist die Fabrik schon "genug" gerueckt: kein Drang mehr.
        let tight = Drive {
            centre_cap: 1,
            ..Drive::DEFAULT
        };
        assert!(!can_advance(&g, me, &tight));
    }

    #[test]
    fn the_new_drive_keys_parse_and_the_defaults_are_the_measured_ones() {
        let d = Drive::parse("open=2,orounds=6,enclose=50,press=4,offence=30").unwrap();
        assert_eq!(
            (d.open, d.open_rounds, d.enclose, d.press, d.offence),
            (2, 6, 50, 4, 30)
        );
        // Ohne Angabe gelten die gemessenen Standardwerte (README, Version 3.6).
        let plain = Drive::parse("land=6").unwrap();
        assert_eq!(
            (
                plain.open,
                plain.open_rounds,
                plain.enclose,
                plain.press,
                plain.offence,
                plain.hunt
            ),
            (2, 8, 100, 4, 30, 50)
        );
    }

    fn colour(g: &mut Game, p: PlayerId, cells: &[(i32, i32)]) {
        for &(x, y) in cells {
            let c = g.map.board.cell_at(x, y).unwrap();
            g.state.land[c.idx()] = Some(p);
        }
    }

    #[test]
    fn a_missing_wall_cell_counts_as_an_enclosure_chance() {
        let me = PlayerId(0);
        // Ein Ring um (0,1) und (1,1); es fehlt nur (0,2). Mit Randwand sind das zwei Felder.
        let mut cfg = Config::new(2, 7);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.terr_enclose_edge = true;
        let mut g = Game::new(cfg).unwrap();
        colour(&mut g, me, &[(1, 0), (2, 0), (2, 1), (2, 2), (1, 2)]);
        assert_eq!(enclosure_chance(&g, me), 2);
        assert_eq!(enclosure_chance(&g, PlayerId(1)), 0);
        // Ohne Randwand schliesst nur noch das Innenfeld (1,1) (mit (0,1) als Wand).
        let mut plain = game();
        colour(&mut plain, me, &[(1, 0), (2, 0), (2, 1), (2, 2), (1, 2)]);
        assert_eq!(enclosure_chance(&plain, me), 1);
        // Ausgeschaltetes Umschliessen: keine Chance.
        let mut off = game();
        off.config.terr_enclose = false;
        colour(&mut off, me, &[(1, 0), (2, 0), (2, 1), (2, 2), (1, 2)]);
        assert_eq!(enclosure_chance(&off, me), 0);
    }

    #[test]
    fn the_enclosure_bonus_only_counts_when_asked() {
        let me = PlayerId(0);
        let mut g = game();
        colour(&mut g, me, &[(1, 0), (2, 0), (2, 1), (2, 2), (1, 2)]);
        let off = leaf_value_with(&g, me, Some(&OLD));
        let on = leaf_value_with(&g, me, Some(&Drive { enclose: 80, ..OLD }));
        assert!(on > off, "{on} > {off}");
        assert_eq!(leaf_value_with(&g, me, Some(&OLD)), off);
    }

    #[test]
    fn the_opening_walks_the_king_out_and_moves_the_factory() {
        let me = PlayerId(0);
        let d = Drive {
            open: 1,
            ..Drive::DEFAULT
        };
        let mut g = game();
        let legal = g.legal_actions_for_bot();
        // Aus: keine Eroeffnung.
        assert_eq!(opening_move(&g, me, &OLD, &legal), None);
        // An: der Koenig macht einen Schritt weg von der Fabrik, Richtung Mitte.
        let first = opening_move(&g, me, &d, &legal).expect("ein Eroeffnungszug");
        let Action::Move { piece, to } = first else {
            panic!("erst laeuft der Koenig: {first:?}");
        };
        assert_eq!(piece, g.king(me).unwrap().id);
        let fac = g.home_factory(me).unwrap().cell;
        assert!(g.map.board.neighbors(fac).any(|n| n == to));
        g.apply(first).unwrap();
        // Der Gegner zieht irgendwie.
        let reply = g.legal_actions_for_bot()[0];
        g.apply(reply).unwrap();
        // Danach: Fabrik am Koenig neu bauen (Umzug).
        let legal = g.legal_actions_for_bot();
        assert_eq!(
            opening_move(&g, me, &d, &legal),
            Some(Action::RebuildFactory)
        );
        g.apply(Action::RebuildFactory).unwrap();
        g.check_invariants().unwrap();
        // Spaeter in der Partie macht die Eroeffnung nichts mehr.
        let late = Drive {
            open_rounds: 0,
            ..d
        };
        let legal = g.legal_actions_for_bot();
        assert_eq!(opening_move(&g, me, &late, &legal), None);
    }

    /// Messhilfe (nicht im Normallauf): wie oft haette ein Bot schlagen koennen, und was tat er stattdessen?
    #[test]
    #[ignore]
    fn capture_audit() {
        use crate::ai::SearchParams;
        let budget: u64 = std::env::var("AUDIT_NODES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1500);
        for kind in ["sehr-stark", "super-stark"] {
            let (mut avail, mut took, mut bought, mut other) = (0, 0, 0, 0);
            let (mut safe_avail, mut safe_took) = (0, 0);
            let mut shown = 0;
            for seed in 1..=4u64 {
                let mut cfg = Config::new(2, seed);
                cfg.territory = true;
                cfg.set_v3(true);
                cfg.score_after = 100;
                let mut g = Game::new(cfg).unwrap();
                let mk = |s: u64| {
                    let b = SearchBot::with_params(s, SearchParams::with_budget(budget));
                    if kind == "super-stark" {
                        let list = std::env::var("AUDIT_DRIVE").unwrap_or_default();
                        b.with_drive(Drive::parse(&list).unwrap())
                            .with_name("super-stark")
                    } else {
                        b
                    }
                };
                let mut bots = [mk(seed * 2), mk(seed * 2 + 1)];
                for _ in 0..400 {
                    if g.outcome().is_some() {
                        break;
                    }
                    let me = g.current_player();
                    let legal = g.legal_actions_for_bot();
                    let captures = |l: &[Action]| {
                        l.iter()
                            .filter(|a| match **a {
                                Action::Move { to, .. } => {
                                    g.state.piece_at(to).is_some_and(|p| p.owner != me)
                                }
                                _ => false,
                            })
                            .count()
                    };
                    // Sicherer Schlag: der Gegner kann das Zielfeld danach nicht zurueckschlagen.
                    let safe_capture = |l: &[Action]| {
                        l.iter().any(|a| {
                            let Action::Move { to, .. } = *a else {
                                return false;
                            };
                            let Some(v) = g.state.piece_at(to) else {
                                return false;
                            };
                            if v.owner == me {
                                return false;
                            }
                            let mut c = g.clone();
                            c.state.current = me;
                            c.apply_unchecked(*a);
                            if c.outcome().is_some() {
                                return true;
                            }
                            !c.legal_actions_for_bot()
                                .iter()
                                .any(|r| matches!(*r, Action::Move { to: t2, .. } if t2 == to))
                        })
                    };
                    let n = captures(&legal);
                    let safe = n > 0 && safe_capture(&legal);
                    let a: Action = crate::bot::Bot::choose(&mut bots[me.idx()], &g);
                    if safe {
                        safe_avail += 1;
                    }
                    if n > 0 {
                        avail += 1;
                        let is_capture = matches!(a, Action::Move { to, .. } if g.state.piece_at(to).is_some_and(|p| p.owner != me));
                        if is_capture {
                            took += 1;
                            if safe {
                                safe_took += 1;
                            }
                        } else if matches!(a, Action::BuyCard { .. }) {
                            bought += 1;
                            if shown < 6 {
                                shown += 1;
                                println!("  [{kind}] Runde {} P{}: {n} Schlaege moeglich, gekauft: {a:?}", g.state.round, me.idx());
                            }
                        } else {
                            other += 1;
                            if shown < 6 {
                                shown += 1;
                                println!("  [{kind}] Runde {} P{}: {n} Schlaege moeglich, gezogen: {a:?}", g.state.round, me.idx());
                            }
                        }
                    }
                    g.apply(a).unwrap();
                }
            }
            println!("{kind}: Schlag moeglich {avail}x, geschlagen {took}x, stattdessen gekauft {bought}x, anders gezogen {other}x; davon Zuege mit sicherem Schlag {safe_avail}x, dabei geschlagen {safe_took}x");
        }
    }

    fn cell(g: &Game, x: i32, y: i32) -> crate::ids::CellId {
        g.map.board.cell_at(x, y).unwrap()
    }

    #[test]
    fn a_free_capture_is_sound_and_a_losing_trade_is_not() {
        let me = PlayerId(0);
        let foe = PlayerId(1);
        let mut g = game();
        // Ein Streifen eigenen Landes zieht sich an der Unterkante entlang; die Figur ist angebunden.
        for x in 0..=7 {
            g.set_land(cell(&g, x, 0), Some(me));
        }
        let hunter = g.setup_unit(me, cell(&g, 7, 0), 1, 1);
        let target = g.setup_unit(foe, cell(&g, 8, 0), 1, 1);
        let grab = Action::Move {
            piece: hunter,
            to: cell(&g, 8, 0),
        };
        let found = captures(&g, me, &[grab]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].victim, UNIT[1]);
        assert!(found[0].sound, "niemand kann zurueckschlagen");
        // Eine verteidigte Figur des Gegners (Stufe 2, genau zwei Felder entfernt) schlaegt zurueck:
        // Bauer gegen Bauer und danach verloren ist kein Gewinn.
        for y in 2..=9 {
            g.set_land(cell(&g, 9, y), Some(foe));
        }
        g.set_land(cell(&g, 8, 2), Some(foe));
        g.setup_unit(foe, cell(&g, 8, 2), 2, 2);
        let found = captures(&g, me, &[grab]);
        assert_eq!(found.len(), 1);
        assert!(!found[0].sound, "wird zurueckgeschlagen, kein Gewinn");
        // Dasselbe gegen eine wertvollere Figur lohnt sich.
        g.state.remove_piece(target);
        let better = g.setup_unit(foe, cell(&g, 8, 0), 2, 2);
        let _ = better;
        let found = captures(&g, me, &[grab]);
        assert!(found[0].sound, "Stufe 2 fuer Stufe 1 ist ein guter Tausch");
        // Zuege ohne gegnerische Figur am Ziel sind keine Schlaege.
        let step = Action::Move {
            piece: hunter,
            to: cell(&g, 7, 1),
        };
        assert!(captures(&g, me, &[step]).is_empty());
    }

    #[test]
    fn hunt_takes_a_sound_capture_before_shifting_the_factory() {
        use crate::ai::SearchParams;
        let me = PlayerId(0);
        let mut g = shift_ready_game();
        let hunter = g.setup_unit(me, cell(&g, 1, 0), 1, 1);
        g.setup_unit(PlayerId(1), cell(&g, 2, 0), 1, 1);
        let grab = Action::Move {
            piece: hunter,
            to: cell(&g, 2, 0),
        };
        assert!(g.legal_actions_for_bot().contains(&grab));
        // Ohne "hunt": der Bot rutscht zuerst mit der Fabrik (altes Verhalten).
        let old = OLD;
        let mut bot = SearchBot::with_params(3, SearchParams::with_budget(1500))
            .with_drive(old)
            .with_name("super-stark");
        let a = crate::bot::Bot::choose(&mut bot, &g);
        assert!(
            matches!(
                a,
                Action::BuyCard {
                    card: Card::FactoryShift,
                    ..
                }
            ),
            "Ausgangslage: der Bot kauft erst Fabrik ruecken: {a:?}"
        );
        // Mit "hunt": der Schlag kommt zuerst.
        let hunting = Drive { hunt: 100, ..old };
        let mut bot = SearchBot::with_params(3, SearchParams::with_budget(1500))
            .with_drive(hunting)
            .with_name("super-stark");
        let a = crate::bot::Bot::choose(&mut bot, &g);
        assert_eq!(a, grab, "der Bot soll schlagen");
    }

    // ----- Rangfolge: Koenig vor Fabrik vor eigener Macht vor Koenigsjagd --------------------------

    /// Alle Bots, die den Gebietsmodus rechnen (jeweils mit kleinem Budget, damit die Tests schnell sind).
    fn thinking_bots(seed: u64) -> Vec<(&'static str, Box<dyn crate::bot::Bot>)> {
        use crate::ai::SearchParams;
        let p = || SearchParams::with_budget(2500);
        vec![
            (
                "stark",
                Box::new(SearchBot::with_params(seed, p()).with_name("stark")),
            ),
            (
                "sehr-stark",
                Box::new(SearchBot::with_params(seed, p()).with_area_eval()),
            ),
            (
                "super-stark",
                Box::new(
                    SearchBot::with_params(seed, p())
                        .with_drive(Drive::DEFAULT)
                        .with_name("super-stark"),
                ),
            ),
            ("flaeche", Box::new(AreaBot::new(seed))),
        ]
    }

    /// Kann ein Gegner nach `a` im naechsten Zug den Koenig von `me` schlagen oder dessen Fabrik zerstoeren?
    /// (`king`: Koenigsfang, sonst Zerstoerung der Fabrik.)
    fn exposed_after(g: &Game, me: PlayerId, a: Action, king: bool) -> bool {
        let mut child = g.clone();
        child.apply_unchecked(a);
        if child.outcome().is_some() {
            return king && child.outcome() != Some(Outcome::Winner(me));
        }
        if child.current_player() == me {
            return false;
        }
        child.legal_actions_for_bot().into_iter().any(|b| {
            let mut c2 = child.clone();
            c2.apply_unchecked(b);
            if king {
                c2.king(me).is_none()
            } else {
                c2.home_factory(me).is_none() && c2.state.alive[me.idx()]
            }
        })
    }

    /// P0: Land entlang der Unterkante, Fabrik auf (0,0), Koenig auf (4,0); P1 hat seine Fabrik in der
    /// Naehe (auf (4,4)) und einen Angreifer der Stufe 2 auf (4,2), der den Koenig schlagen kann.
    fn king_in_danger() -> (Game, crate::ids::PieceId, crate::ids::PieceId) {
        let me = PlayerId(0);
        let foe = PlayerId(1);
        let mut g = game();
        for x in 0..=7 {
            g.set_land(cell(&g, x, 0), Some(me));
        }
        let king = g.king(me).unwrap().id;
        g.setup_teleport(king, cell(&g, 4, 0));
        // Die Fabrik des Gegners steht nahe, sein Land haengt zusammen.
        let ff = g.home_factory(foe).unwrap().id;
        g.state.move_factory(ff, cell(&g, 4, 4));
        for y in 2..=4 {
            g.set_land(cell(&g, 4, y), Some(foe));
        }
        let attacker = g.setup_unit(foe, cell(&g, 4, 2), 2, 2);
        // Ein Koeder: eine Figur der Stufe 4 des Gegners, die meine Figur auf (7,0) frei schlagen koennte.
        g.set_land(cell(&g, 8, 0), Some(foe));
        let bait = g.setup_unit(foe, cell(&g, 8, 0), 4, 4);
        let hunter = g.setup_unit(me, cell(&g, 7, 0), 1, 1);
        let _ = hunter;
        g.setup_current(me);
        (g, attacker, bait)
    }

    #[test]
    fn the_king_comes_first_for_every_thinking_bot() {
        let me = PlayerId(0);
        let (g, attacker, _bait) = king_in_danger();
        let threats = g.piece_destinations(attacker);
        let kc = g.king(me).unwrap().cell;
        assert!(
            threats.contains(&kc),
            "Ausgangslage: der Koenig ist bedroht"
        );
        let bait_capture = g
            .legal_actions_for_bot()
            .into_iter()
            .find(|a| matches!(*a, Action::Move { to, .. } if to == cell(&g, 8, 0)))
            .expect("der Koeder ist schlagbar");
        assert!(
            exposed_after(&g, me, bait_capture, true),
            "den Koeder zu schlagen laesst den Koenig stehen"
        );
        let safe = g
            .legal_actions_for_bot()
            .into_iter()
            .any(|a| !exposed_after(&g, me, a, true));
        assert!(safe, "es gibt eine Rettung");
        for (name, mut bot) in thinking_bots(5) {
            let a = bot.choose(&g);
            assert!(
                !exposed_after(&g, me, a, true),
                "{name} laesst den Koenig stehen: {a:?}"
            );
        }
    }

    /// P0: Land entlang der Unterkante bis (6,0), Fabrik auf (0,0) mit Lager, Koenig auf (3,0).
    /// P1 hat seine Fabrik auf (0,4) und einen Angreifer der Stufe 2 auf (0,2): im naechsten Zug
    /// steht er auf (0,0). Optional steht ein Verteidiger von P0 bereit, der den Angreifer schlagen kann.
    fn factory_in_danger(with_defender: bool) -> Game {
        let me = PlayerId(0);
        let foe = PlayerId(1);
        let mut g = game();
        for x in 0..=6 {
            g.set_land(cell(&g, x, 0), Some(me));
        }
        let iron = g
            .map
            .resource_types
            .iter()
            .find(|t| t.home_corner == Some(0) && t.category == 1)
            .unwrap()
            .id;
        let fid = g.home_factory(me).unwrap().id;
        g.setup_storage(fid, iron, 6);
        let king = g.king(me).unwrap().id;
        g.setup_teleport(king, cell(&g, 3, 0));
        let ff = g.home_factory(foe).unwrap().id;
        g.state.move_factory(ff, cell(&g, 0, 4));
        for y in 2..=4 {
            g.set_land(cell(&g, 0, y), Some(foe));
        }
        g.setup_unit(foe, cell(&g, 0, 2), 2, 2);
        if with_defender {
            // Eine Figur auf (1,2) neben dem Angreifer: sie schlaegt ihn mit einem Schritt
            // (sie steht auf eigenem, verbundenem Land).
            for y in 1..=2 {
                g.set_land(cell(&g, 1, y), Some(me));
            }
            g.set_land(cell(&g, 0, 1), Some(me));
            g.setup_unit(me, cell(&g, 1, 2), 1, 1);
        }
        g.setup_current(me);
        g
    }

    #[test]
    fn the_factory_comes_second_every_thinking_bot_saves_it_when_it_can() {
        let me = PlayerId(0);
        let g = factory_in_danger(true);
        let fcell = g.home_factory(me).unwrap().cell;
        assert!(
            g.threatened_cells(me).contains(&fcell),
            "Ausgangslage: die Fabrik ist bedroht"
        );
        assert!(
            g.legal_actions_for_bot()
                .iter()
                .any(|a| !exposed_after(&g, me, *a, false)),
            "es gibt eine Rettung"
        );
        for (name, mut bot) in thinking_bots(9) {
            let a = bot.choose(&g);
            assert!(
                !exposed_after(&g, me, a, false),
                "{name} laesst die Fabrik stehen: {a:?}"
            );
        }
    }

    #[test]
    fn a_doomed_factory_is_moved_when_that_is_cheaper_than_losing_it() {
        let me = PlayerId(0);
        let g = factory_in_danger(false);
        let fcell = g.home_factory(me).unwrap().cell;
        assert!(g.threatened_cells(me).contains(&fcell));
        assert!(
            g.legal_actions_for_bot().contains(&Action::RebuildFactory),
            "der Koenig darf umziehen"
        );
        // Ohne Umzug geht die Fabrik verloren, was immer der Bot sonst zieht.
        assert!(g
            .legal_actions_for_bot()
            .iter()
            .filter(|a| **a != Action::RebuildFactory)
            .all(|a| exposed_after(&g, me, *a, false)));
        for (name, mut bot) in thinking_bots(11) {
            let a = bot.choose(&g);
            assert_eq!(
                a,
                Action::RebuildFactory,
                "{name}: der Umzug ist billiger als die Zerstoerung"
            );
        }
    }

    #[test]
    fn without_a_factory_the_king_walks_back_to_his_land_and_rebuilds() {
        let me = PlayerId(0);
        let mut g = game();
        for x in 0..=4 {
            g.set_land(cell(&g, x, 0), Some(me));
        }
        // Fabrik zerstoert (wie `destroy_factory`); der Koenig steht abseits auf neutralem Land.
        let fid = g.home_factory(me).unwrap().id;
        let old = g.home_factory(me).unwrap().cell;
        g.state.remove_factory(fid);
        g.state.area[me.idx()] = 0;
        g.state.lost_factory[me.idx()] = Some(old);
        let king = g.king(me).unwrap().id;
        g.setup_teleport(king, cell(&g, 4, 3));
        g.setup_current(me);
        let dist = |g: &Game| {
            let k = g.king(me).unwrap().cell;
            let d = g.map.board.distances_from(k);
            g.map
                .board
                .cells()
                .filter(|c| g.state.land[c.idx()] == Some(me))
                .map(|c| d[c.idx()])
                .min()
                .unwrap()
        };
        let before = dist(&g);
        assert_eq!(before, 3);
        assert!(!g.legal_actions_for_bot().contains(&Action::RebuildFactory));
        for (name, mut bot) in thinking_bots(3) {
            let a = bot.choose(&g);
            let mut child = g.clone();
            child.apply_unchecked(a);
            assert!(dist(&child) < before, "{name} geht nicht zurueck: {a:?}");
        }
        // Wehrlos zaehlt in der Bewertung: dieselbe Lage mit Fabrik ist deutlich besser.
        let mut with_factory = g.clone();
        with_factory.state.lost_factory[me.idx()] = None;
        with_factory.setup_factory(me, old);
        assert!(
            evaluate(&with_factory, me) > evaluate(&g, me) + 500,
            "ohne Fabrik: {} gegen mit Fabrik: {}",
            evaluate(&g, me),
            evaluate(&with_factory, me)
        );
    }

    #[test]
    fn free_room_makes_land_worth_more() {
        let g = game();
        let free = free_permille(&g);
        assert!(free > 900, "am Anfang ist fast alles frei: {free}");
    }

    #[test]
    fn super_stark_plays_legal_moves_and_never_stalls() {
        use crate::ai::SearchParams;
        let mut g = game();
        // Kleines Budget, damit der Test schnell bleibt; die Spielweise ist dieselbe.
        let mut bots: Vec<SearchBot> = (1..=2)
            .map(|seed| {
                SearchBot::with_params(seed, SearchParams::with_budget(600))
                    .with_drive(Drive::DEFAULT)
                    .with_name("super-stark")
            })
            .collect();
        for _ in 0..40 {
            if g.outcome().is_some() {
                break;
            }
            let who = g.current_player().idx();
            let a: Action = crate::bot::Bot::choose(&mut bots[who], &g);
            assert!(g.legal_actions().contains(&a), "{a:?}");
            g.apply(a).unwrap();
        }
        g.check_invariants().unwrap();
        assert_eq!(crate::bot::Bot::name(&bots[0]), "super-stark");
        assert_eq!(
            crate::bot::Bot::name(&SearchBot::super_strong(1)),
            "super-stark"
        );
    }
}
