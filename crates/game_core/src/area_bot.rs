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
    standing_with(g, p, LAND)
}

/// Wie `standing`, aber mit frei gewaehltem Wert je Flaeche (`super-stark` zahlt mehr fuer Land,
/// solange noch Platz ist).
fn standing_with(g: &Game, p: PlayerId, land: i64) -> i64 {
    let mut v = 0;
    // Mit `terr_occupy` zaehlt der Vorkommen-Wert voll nur auf bearbeiteten Feldern.
    let worked = g.config.terr_occupy.then(|| g.worked_cells(p));
    if let Some(connected) = g.connected_land(p) {
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
    } else {
        v += g.land_count(p) as i64 * land;
    }
    for piece in g.pieces_of(p) {
        if let PieceKind::Unit { level, .. } = piece.kind {
            v += UNIT[usize::from(level).min(4)];
        }
    }
    v
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
}

impl Drive {
    pub const DEFAULT: Drive = Drive {
        land_extra: 6,
        centre: 20,
        centre_cap: 6,
        ready: 30,
        push: 1,
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
        let mut v =
            standing_with(g, p, land) + d.centre * centre_progress(g, p, d.centre_cap) / 1000;
        if shift_in_market
            && owns_diagonal_ahead(g, p, d)
            && g.territory_card_payment(p, Card::FactoryShift).is_some()
        {
            v += d.ready;
        }
        v
    })
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
        r += FACTORY_THREAT;
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
    let mut rows: std::collections::HashMap<crate::ids::CellId, Vec<u32>> = Default::default();
    g.weak_exposure(
        me,
        |a, b| rows.entry(a).or_insert_with(|| board.distances_from(a))[b.idx()],
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
        let legal = game.legal_actions();
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
        let mut bot = SearchBot::super_strong(3);
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
        let mut bot = SearchBot::super_strong(3);
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
