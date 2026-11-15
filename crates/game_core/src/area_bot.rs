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
    let mut v = 0;
    if let Some(connected) = g.connected_land(p) {
        for cell in g.map.board.cells().filter(|c| connected[c.idx()]) {
            v += LAND;
            for res in g.all_deposits(cell) {
                let cat = usize::from(g.map.category(res)).clamp(1, 4) - 1;
                // Kat 3 zaehlt mit Flaechenbonus nur ueber `2^k` unten, nicht als Vorkommen.
                if g.config.terr_bonus && cat == 2 {
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
        v += g.land_count(p) as i64 * LAND;
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
    if let Some(Outcome::Winner(w)) = g.outcome() {
        return if w == me { WIN } else { -WIN };
    }
    if !g.state.alive[me.idx()] {
        return -WIN;
    }
    let others: Vec<PlayerId> = g.alive_players().into_iter().filter(|&p| p != me).collect();
    let theirs: i64 =
        others.iter().map(|&p| standing(g, p)).sum::<i64>() / others.len().max(1) as i64;
    standing(g, me) - theirs
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
    r
}

impl AreaBot {
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
            let mut child = g.clone();
            child.apply_unchecked(a);
            let mut value = evaluate(&child, me);
            if child.outcome().is_none() {
                value -= risk(&child, me);
            }
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
            return boost;
        }
        if build.is_some() && game.state.stored_card[me.idx()].is_none() {
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
