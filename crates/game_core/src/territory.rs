//! Gebietsmodus (`Config::territory`): ein Spiel um Land.
//!
//! Alle Felder sind neutral, nur die Startecke gehoert dem Spieler und traegt seine einzige
//! Fabrik. Die Regeln im Ueberblick:
//!
//! - **Faerben.** Zieht eine Figur (auch der Koenig), bekommen alle Felder ihres Weges ihre
//!   Farbe, auch das Zielfeld. Ein Feld eines Gegners wird dabei umgefaerbt. Bei Spruengen
//!   (Pferd, Sprungfeder, Lanze, Laeuferschlag) kommt eine Bruecke dazu: ein Feld zwischen Start
//!   und Ziel, damit die Figur nicht auf einer Insel landet.
//! - **Zusammenhang.** Land zaehlt nur, solange es ueber Nachbarfelder mit der eigenen Fabrik
//!   verbunden ist. Wird es durchtrennt, geht alles Abgetrennte an den, der durchtrennt hat (mit
//!   `terr_cut_flips = false` wird es neutral). Eine Figur, die nicht auf verbundenem Land
//!   steht, faerbt nichts und kann nicht angreifen: sie ist wehrlos, bis sie wieder verbunden ist.
//!   Wer keine stehende Fabrik hat (zerstoert oder im Neubau), hat keinen Zusammenhang: alle seine
//!   Figuren sind wehrlos (und er bekommt auch keinen Ertrag), bis die Fabrik wieder steht. Sein
//!   Land bleibt dabei, wie es ist; es wird nicht abgetrennt. Das gilt mit Neubau-Regel (`terr_move > 0`,
//!   Version 3); ohne sie spielt, wer die Fabrik verloren hat, wie in Version 2 frei weiter.
//! - **Wieder anbinden.** Grenzt eine abgetrennte Figur (ueber eine Kante) an das verbundene Land
//!   ihres Besitzers, ist sie sofort wieder aktiv und faerbt das Feld, auf dem sie steht
//!   (`terr_reconnect`). Das gilt nach jedem Zug fuer alle Figuren, nicht nur fuer die des Ziehenden.
//! - **Umschliessen.** Eine neutrale Flaeche (zusammenhaengende neutrale Felder), deren saemtliche
//!   Nachbarfelder einer einzigen Farbe gehoeren, faellt ganz an diese Farbe (`terr_enclose`).
//!   Standardmaessig zaehlt der Brettrand nicht als Wand: eine Flaeche, die den Rand beruehrt, wird nie
//!   umschlossen. Mit `terr_enclose_edge=1` ist der Rand eine Wand (dann holt ein "L" aus der Ecke
//!   ein ganzes Quadrat).
//! - **Ertrag.** Am Ende des eigenen Zuges sammelt die Fabrik die verbundenen Felder ein: je Feld
//!   eine Einheit Flaeche, dazu `terr_yield` Einheiten je Rohstoffvorkommen auf diesen Feldern
//!   (Vorkommen werden nie abgebaut).
//! - **Bauen.** Statt zu ziehen baut der Spieler an der Fabrik eine Figur auf dem Fabrikfeld
//!   (es muss frei sein): Stufe 1 kostet `terr_pawn` Flaeche; Stufe 2 `terr_area` Flaeche und
//!   `terr_k1` Kat 1; Stufe 3 zusaetzlich `terr_k2` Kat 2; Stufe 4 zusaetzlich `terr_k3` Kat 3.
//!   Alles wird verbraucht. Stufe = Reichweite.
//! - **Karten.** Eine Karte (nur Figurenfaehigkeiten) wird neben der Fabrik gelagert und wirkt
//!   automatisch auf die naechste gebaute Figur. Bezahlt wird nach Kategorie aus dem Lager.
//! - **Fabrik.** Betritt eine verbundene gegnerische Figur das Fabrikfeld, ist die Fabrik
//!   zerstoert: Lager, Flaeche und gelagerte Karte sind weg. Das ist kein Spielende, aber es gibt
//!   keine neuen Figuren mehr. Nur der Koenig entscheidet das Spiel.

use crate::action::{Action, Event, PaySource};
use crate::cards::Card;
use crate::game::Game;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use crate::map::Map;
use crate::state::{Factory, Inventory, Piece, PieceKind};
use std::collections::VecDeque;

/// Was ein Zug im Gebietsmodus faerbt, berechnet vor dem Zug.
#[derive(Clone, Debug)]
pub(crate) struct MovePlan {
    /// Stand die Figur auf verbundenem Land (oder der Spieler hat keine Fabrik)?
    pub connected: bool,
    /// Die Felder des Weges (ohne Startfeld), leer wenn nicht verbunden.
    pub path: Vec<CellId>,
}

/// Eine Engstelle im verbundenen Land eines Spielers: faerbt ein Gegner dieses Feld um, haengt ein
/// Teil des Landes nicht mehr an der Fabrik. Die Fabrik selbst ist nie eine Engstelle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeakPoint {
    pub cell: CellId,
    /// Felder (ohne die Engstelle), die abgetrennt wuerden.
    pub cells: u32,
    /// Der Koenig steht auf dem abgetrennten Teil.
    pub king: bool,
    /// Summe der Stufen eigener Figuren auf dem abgetrennten Teil (der Koenig zaehlt nicht).
    pub levels: u32,
}

/// Preis einer Figur im Gebietsmodus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnitPrice {
    pub area: u32,
    pub k1: u32,
    pub k2: u32,
    pub k3: u32,
}

/// Nimmt bis zu `n` Einheiten der Kategorie `cat` (haeufigster Typ zuerst) und gibt zurueck,
/// was genommen wurde.
fn take_category(stock: &mut Inventory, map: &Map, cat: u8, mut n: u32) -> Inventory {
    let mut taken = Inventory::new();
    while n > 0 {
        let best = stock
            .iter()
            .filter(|&(r, _)| map.category(r) == cat)
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)));
        let Some((res, have)) = best else { break };
        let k = have.min(n);
        stock.take(res, k);
        taken.add(res, k);
        n -= k;
    }
    taken
}

/// Bezahlt `k1`, `k2`, `k3` Einheiten aus `stock`; `None`, wenn es nicht reicht.
fn plan_by_category(stock: &Inventory, map: &Map, k: [u32; 3]) -> Option<Inventory> {
    let mut left = stock.clone();
    let mut pay = Inventory::new();
    for (i, &n) in k.iter().enumerate() {
        let mut got = take_category(&mut left, map, i as u8 + 1, n);
        if got.total() < n as u64 {
            return None;
        }
        pay.absorb(&mut got);
    }
    Some(pay)
}

impl Game {
    /// Die einzige Fabrik des Spielers (im Gebietsmodus hoechstens eine).
    pub fn home_factory(&self, player: PlayerId) -> Option<&Factory> {
        self.factories_of(player).next()
    }

    /// Die stehende Fabrik des Spielers: eine, die nicht gerade (neu) gebaut wird.
    pub fn standing_factory(&self, player: PlayerId) -> Option<&Factory> {
        self.home_factory(player).filter(|f| f.building == 0)
    }

    /// Wird die Fabrik des Spielers gerade neu gebaut? Dann sammelt sie nichts, sein Land ist
    /// nicht verbunden (alle Figuren sind wehrlos) und der Koenig darf sich nicht bewegen.
    pub fn rebuilding(&self, player: PlayerId) -> bool {
        self.home_factory(player).is_some_and(|f| f.building > 0)
    }

    /// Felder, die ueber eigene Felder mit der Fabrik des Spielers zusammenhaengen. `None`, wenn
    /// er keine Fabrik hat: dann gibt es keinen Zusammenhang, auf den es ankaeme. Steht die Fabrik
    /// noch nicht (Neubau), ist nichts verbunden.
    pub fn connected_land(&self, player: PlayerId) -> Option<Vec<bool>> {
        let factory = self.home_factory(player)?;
        if factory.building > 0 {
            return Some(vec![false; self.map.board.len()]);
        }
        let mut seen = vec![false; self.map.board.len()];
        if self.state.land[factory.cell.idx()] != Some(player) {
            return Some(seen);
        }
        let mut queue = VecDeque::new();
        seen[factory.cell.idx()] = true;
        queue.push_back(factory.cell);
        while let Some(cur) = queue.pop_front() {
            for n in self.map.board.neighbors(cur) {
                if !seen[n.idx()] && self.state.land[n.idx()] == Some(player) {
                    seen[n.idx()] = true;
                    queue.push_back(n);
                }
            }
        }
        Some(seen)
    }

    /// Alle Engstellen des mit der Fabrik verbundenen Landes von `player` (Schnittpunkte des
    /// Nachbarschaftsgraphen, von der Fabrik aus gesehen), je mit dem Teil, der dahinter haengt.
    /// Leer ohne stehende Fabrik. Ein Durchlauf (Tarjan, ohne Rekursion).
    pub fn weak_points(&self, player: PlayerId) -> Vec<WeakPoint> {
        let Some(factory) = self.standing_factory(player) else {
            return Vec::new();
        };
        if self.state.land[factory.cell.idx()] != Some(player) {
            return Vec::new();
        }
        let board = &self.map.board;
        let n = board.len();
        let mut disc = vec![0u32; n];
        let mut low = vec![0u32; n];
        let mut size = vec![0u32; n];
        let mut king = vec![false; n];
        let mut levels = vec![0u32; n];
        let mut cut_cells = vec![0u32; n];
        let mut cut_king = vec![false; n];
        let mut cut_levels = vec![0u32; n];
        for piece in self.state.pieces.values().filter(|p| p.owner == player) {
            match piece.kind {
                PieceKind::King => king[piece.cell.idx()] = true,
                PieceKind::Unit { level, .. } => levels[piece.cell.idx()] += u32::from(level),
            }
        }
        let root = factory.cell.idx();
        let mut clock = 1u32;
        disc[root] = clock;
        low[root] = clock;
        size[root] = 1;
        // (Feld, Vorgaenger, Nachbarn, naechster Nachbar)
        let mut stack: Vec<(usize, usize, Vec<CellId>, usize)> =
            vec![(root, usize::MAX, board.neighbors(factory.cell).collect(), 0)];
        while let Some(top) = stack.last_mut() {
            let (v, parent) = (top.0, top.1);
            if top.3 < top.2.len() {
                let w = top.2[top.3];
                top.3 += 1;
                let wi = w.idx();
                if self.state.land[wi] != Some(player) || wi == parent {
                    continue;
                }
                if disc[wi] != 0 {
                    low[v] = low[v].min(disc[wi]);
                } else {
                    clock += 1;
                    disc[wi] = clock;
                    low[wi] = clock;
                    size[wi] = 1;
                    stack.push((wi, v, board.neighbors(w).collect(), 0));
                }
            } else {
                stack.pop();
                if parent != usize::MAX {
                    low[parent] = low[parent].min(low[v]);
                    size[parent] += size[v];
                    king[parent] |= king[v];
                    levels[parent] += levels[v];
                    if parent != root && low[v] >= disc[parent] {
                        cut_cells[parent] += size[v];
                        cut_king[parent] |= king[v];
                        cut_levels[parent] += levels[v];
                    }
                }
            }
        }
        (0..n)
            .filter(|&c| cut_cells[c] > 0)
            .map(|c| WeakPoint {
                cell: CellId(c as u32),
                cells: cut_cells[c],
                king: cut_king[c],
                levels: cut_levels[c],
            })
            .collect()
    }

    /// Summe `value(Engstelle, naehe)` ueber alle Engstellen von `player`. `naehe` ist 5, wenn eine
    /// gegnerische Figur, die faerben darf (auf verbundenem Land), direkt neben der Engstelle
    /// steht, und sinkt je Feld Abstand um eins bis 0 (ab Abstand 6 keine Gefahr, dann wird
    /// `value` nicht gerufen). `dist` liefert den Abstand zweier Felder auf dem Brett.
    pub fn weak_exposure(
        &self,
        player: PlayerId,
        mut dist: impl FnMut(CellId, CellId) -> u32,
        mut value: impl FnMut(&WeakPoint, i64) -> i64,
    ) -> i64 {
        let weak = self.weak_points(player);
        if weak.is_empty() {
            return 0;
        }
        let attackers: Vec<CellId> = self
            .state
            .pieces
            .values()
            .filter(|x| x.owner != player && self.piece_connected(x))
            .map(|x| x.cell)
            .collect();
        let mut total = 0;
        for wp in &weak {
            let near = attackers
                .iter()
                .map(|&a| dist(a, wp.cell))
                .min()
                .unwrap_or(u32::MAX);
            let close = 6u32.saturating_sub(near) as i64;
            if close > 0 {
                total += value(wp, close);
            }
        }
        total
    }

    /// Was ein einzelner Zug einer gegnerischen Figur im naechsten Zug von `player` abtrennen kann:
    /// der hoechste Wert, den `value` dem Teil gibt, der nach dem Umfaerben des Weges nicht mehr an
    /// der Fabrik haengt (`WeakPoint::cell` ist dabei das erste umgefaerbte Feld). Nur Figuren,
    /// die auf verbundenem Land stehen, faerben. 0, wenn nichts droht. Zuege auf die Fabrik oder
    /// auf den eigenen Koenig (Zerstoeren, Koenigsfang) zaehlen nicht mit, die kennen die Bots anders.
    pub fn cut_threat(&self, player: PlayerId, value: impl Fn(&WeakPoint) -> i64) -> i64 {
        let Some(factory) = self.standing_factory(player) else {
            return 0;
        };
        let Some(conn) = self.connected_land(player) else {
            return 0;
        };
        if !conn[factory.cell.idx()] {
            return 0;
        }
        let board = &self.map.board;
        let n = board.len();
        let mut hit = vec![false; n];
        let mut seen = vec![false; n];
        let mut queue = VecDeque::new();
        let mut worst = 0;
        for piece in self.state.pieces.values().filter(|p| p.owner != player) {
            if !self.piece_connected(piece) {
                continue;
            }
            for to in self.piece_destinations(piece.id) {
                if self.state.piece_at(to).is_some_and(|other| {
                    other.owner == player && matches!(other.kind, PieceKind::King)
                }) {
                    continue;
                }
                let path = self.move_path(piece, to);
                if !path.iter().any(|c| conn[c.idx()]) || path.contains(&factory.cell) {
                    continue;
                }
                for c in &path {
                    hit[c.idx()] = true;
                }
                seen.iter_mut().for_each(|x| *x = false);
                seen[factory.cell.idx()] = true;
                queue.push_back(factory.cell);
                while let Some(cur) = queue.pop_front() {
                    for nb in board.neighbors(cur) {
                        if !seen[nb.idx()]
                            && !hit[nb.idx()]
                            && self.state.land[nb.idx()] == Some(player)
                        {
                            seen[nb.idx()] = true;
                            queue.push_back(nb);
                        }
                    }
                }
                let lost = |i: usize| conn[i] && !seen[i] && !hit[i];
                let mut w = WeakPoint {
                    cell: path[0],
                    cells: (0..n).filter(|&i| lost(i)).count() as u32,
                    king: false,
                    levels: 0,
                };
                for mine in self.state.pieces.values().filter(|p| p.owner == player) {
                    if lost(mine.cell.idx()) {
                        match mine.kind {
                            PieceKind::King => w.king = true,
                            PieceKind::Unit { level, .. } => w.levels += u32::from(level),
                        }
                    }
                }
                for c in &path {
                    hit[c.idx()] = false;
                }
                if w.cells > 0 {
                    worst = worst.max(value(&w));
                }
            }
        }
        worst
    }

    /// Zahl der Felder, die mit der Fabrik zusammenhaengen (ohne Fabrik: alle eigenen Felder).
    pub fn connected_count(&self, player: PlayerId) -> u32 {
        match self.connected_land(player) {
            Some(c) => c.iter().filter(|&&b| b).count() as u32,
            None => self.land_count(player),
        }
    }

    /// Flaechenwert eines Feldes in Vierteln: 4 (voll), ohne Gelaende immer 4.
    pub(crate) fn land_quarters(&self, cell: CellId) -> u32 {
        if self.config.terr_terrain {
            self.map.terrain_at(cell).quarters()
        } else {
            4
        }
    }

    /// Alle Felder, die dem Spieler gehoeren, verbunden oder nicht.
    pub fn land_count(&self, player: PlayerId) -> u32 {
        self.state
            .land
            .iter()
            .filter(|&&o| o == Some(player))
            .count() as u32
    }

    /// Steht die Figur auf verbundenem Land? Ohne stehende Fabrik (zerstoert oder im Neubau) ist
    /// nichts verbunden: alle Figuren sind wehrlos, bis die Fabrik wieder steht. Das gilt, wenn die
    /// Fabrik neu gebaut werden kann (`terr_move > 0`, Version 3); ohne diese Regel kommt sie nie wieder,
    /// und wer sie verloren hat, spielt wie in Version 2 frei weiter (sonst waere er fuer immer wehrlos).
    pub fn piece_connected(&self, piece: &Piece) -> bool {
        match self.connected_land(piece.owner) {
            None => !self.rebuild_enabled(),
            Some(c) => c[piece.cell.idx()],
        }
    }

    /// Kann eine zerstoerte Fabrik neu gebaut werden (Version 3, `terr_move > 0`)?
    pub fn rebuild_enabled(&self) -> bool {
        self.config.territory && self.config.terr_move > 0
    }

    /// Hoechste Stufe, die man bauen kann.
    pub fn max_unit_level(&self) -> u8 {
        self.config.corner_count().min(4) as u8
    }

    pub fn unit_price(&self, level: u8) -> UnitPrice {
        let c = &self.config;
        if c.terr_grow >= 2 {
            // Version 2: nur Flaeche, jede Stufe `terr_grow` mal so teuer wie die davor.
            let steps = u32::from(level.max(1)) - 1;
            // Ab Stufe 2 zusaetzlich mal `terr_up_mult` (Version 3: 50 / 200 / 400 / 800).
            let up = if steps > 0 { c.terr_up_mult.max(1) } else { 1 };
            return UnitPrice {
                area: c
                    .terr_pawn
                    .saturating_mul(c.terr_grow.saturating_pow(steps))
                    .saturating_mul(up),
                k1: 0,
                k2: 0,
                k3: 0,
            };
        }
        match level {
            0 | 1 => UnitPrice {
                area: c.terr_pawn,
                k1: 0,
                k2: 0,
                k3: 0,
            },
            2 => UnitPrice {
                area: c.terr_area,
                k1: c.terr_k1,
                k2: 0,
                k3: 0,
            },
            3 => UnitPrice {
                area: c.terr_area + c.terr_area_step,
                k1: c.terr_k1,
                k2: c.terr_k2,
                k3: 0,
            },
            _ => UnitPrice {
                area: c.terr_area + 2 * c.terr_area_step,
                k1: c.terr_k1,
                k2: c.terr_k2,
                k3: c.terr_k3,
            },
        }
    }

    /// Einheiten einer Kategorie in einem Lager.
    pub fn category_units(&self, stock: &Inventory, cat: u8) -> u32 {
        stock
            .iter()
            .filter(|&(r, _)| self.map.category(r) == cat)
            .map(|(_, n)| n)
            .sum()
    }

    /// Kann der Spieler jetzt eine Figur dieser Stufe bezahlen?
    pub fn can_afford_unit(&self, player: PlayerId, level: u8) -> bool {
        let Some(f) = self.standing_factory(player) else {
            return false;
        };
        let price = self.unit_price(level);
        self.state.area[player.idx()] >= price.area
            && self.category_units(&f.storage, 1) >= price.k1
            && self.category_units(&f.storage, 2) >= price.k2
            && self.category_units(&f.storage, 3) >= price.k3
    }

    /// Kartenpreis nach Kategorien (Kat 1, 2, 3): die alten Preise, Kat 2 und 3 mal `terr_card_mult`;
    /// mit dem Kartensatz von Version 2 deren eigene Preise (Kartenfaktor 16 = Grundpreis).
    pub fn territory_card_price(&self, card: Card) -> [u32; 3] {
        if self.config.terr_cardset3 {
            // Version 3: feste Preise in Eisen / Gold / Brot, vom Spieler einstellbar (`price.<karte>`).
            return self
                .config
                .card_tweaks
                .price_override(card)
                .unwrap_or_else(|| {
                    if self.config.terr_bread > 0 {
                        card.v3_price_bread()
                    } else {
                        card.v3_price()
                    }
                });
        }
        let m = self.config.terr_card_mult;
        if self.config.terr_cards == 1 {
            let scale = |n: u32| if n == 0 { 0 } else { (n * m / 16).max(1) };
            let [a, b] = card.v2_price();
            return [scale(a), scale(b), 0];
        }
        let old = card.cost();
        [
            (old.color + old.foreign_color) as u32 * m,
            old.k2 as u32 * m,
            old.k3 as u32 * m,
        ]
    }

    /// Kartenpreis fuer einen bestimmten Spieler: der Koenigs-Boost kostet je Stufe mehr
    /// (Grundpreis mal die Stufe, die er bekaeme).
    pub fn territory_card_price_for(&self, player: PlayerId, card: Card) -> [u32; 3] {
        if card == Card::KingBoost && self.king_upgrade_on() {
            // Direktes Upgrade: Kat 2 (Gold) je zusaetzlichem Zugfeld.
            let next = self.state.player_perks[player.idx()].king_boost + 1;
            return [
                0,
                self.king_boost_extra(next) * self.config.terr_kingup_price,
                0,
            ];
        }
        let mut price = self.territory_card_price(card);
        if card == Card::KingBoost {
            let next = u32::from(self.state.player_perks[player.idx()].king_boost) + 1;
            for p in price.iter_mut() {
                *p *= next;
            }
        }
        price
    }

    /// Kann der Spieler die Karte jetzt ueberhaupt aufnehmen? Der Koenigs-Boost wirkt sofort und
    /// braucht den Kartenplatz nicht, dafuer hoechstens drei Stufen; alle anderen Karten
    /// brauchen den freien Platz neben der Fabrik.
    pub fn territory_card_takeable(&self, player: PlayerId, card: Card) -> bool {
        if card == Card::KingBoost {
            self.state.player_perks[player.idx()].king_boost < crate::state::MAX_KING_BOOST
        } else if self.instant_card(card) {
            self.config
                .card_tweaks
                .parts(card)
                .iter()
                .any(|&(part, _)| self.instant_has_room(player, part))
        } else if self.cards_go_on_pieces(card) {
            self.pieces_of(player)
                .any(|p| self.card_fits_piece(card, p))
        } else if card == Card::FactoryShift {
            // Die Karte wirkt sofort (die Fabrik muss rutschen), also nur kaufbar, wenn sie kann.
            self.state.stored_card[player.idx()].is_none()
                && !self.factory_shift_cells(player).is_empty()
        } else {
            self.state.stored_card[player.idx()].is_none()
        }
    }

    /// Version 3 (`terr_cardany`): wirkt `card` sofort auf eine gewaehlte eigene Einheit, statt
    /// neben der Fabrik zu liegen?
    pub fn cards_go_on_pieces(&self, card: Card) -> bool {
        self.config.territory
            && self.config.terr_cardany
            && self.config.card_tweaks.target(card) == crate::cards::CardTarget::Unit
    }

    /// Version 3: wirkt die Karte sofort beim Kauf auf den Spieler (Sturmangriff, Bergbau, Schmiede,
    /// Doppelzug, oder eine selbst gebaute Karte nur aus solchen Bausteinen)? Dann braucht sie weder
    /// eine Figur noch den Kartenplatz neben der Fabrik.
    pub fn instant_card(&self, card: Card) -> bool {
        self.config.territory
            && self.config.terr_cardset3
            && match card {
                Card::Custom(_) => {
                    self.config.card_tweaks.defined(card)
                        && self.config.card_tweaks.target(card) == crate::cards::CardTarget::Player
                }
                _ => card.is_instant(),
            }
    }

    /// Hat der Spieler fuer diesen sofort wirkenden Baustein noch etwas zu gewinnen? Sturmangriff
    /// nur einmal je Zug, Doppelzug nur eine zusaetzliche Aktion, Bergbau und Schmiede nur einmal.
    fn instant_has_room(&self, player: PlayerId, card: Card) -> bool {
        let pp = &self.state.player_perks[player.idx()];
        match card {
            Card::Charge if self.config.terr_delay => !self.state.armed_charge[player.idx()],
            Card::Charge => !self.state.charge,
            Card::Mining => !pp.mining,
            Card::Smelter => !pp.smelter,
            Card::DoubleMove if self.config.terr_delay => self.state.armed_extra[player.idx()] == 0,
            Card::DoubleMove => self.state.extra_actions == 0,
            _ => false,
        }
    }

    /// Name der Karte in dieser Partie (selbst gebaute Karten haben ihren eigenen).
    pub fn card_name(&self, card: Card) -> String {
        self.config.card_tweaks.name(card)
    }

    /// Text der Karte in dieser Partie: die Ecke beschreibt mit `terr_bend_tail` die neue Regel,
    /// eine selbst gebaute Karte nennt ihre Bausteine.
    pub fn card_text(&self, card: Card) -> String {
        if let Some(text) = self.config.card_tweaks.custom_text(card) {
            return text;
        }
        let mut text =
            if self.config.terr_cardset3 || (card == Card::Bend && self.config.terr_bend_tail) {
                card.v3_text().to_string()
            } else {
                card.text().to_string()
            };
        if self.config.terr_delay && matches!(card, Card::Charge | Card::DoubleMove) {
            text.push_str(" Wirkt erst in deinem nächsten Zug.");
        }
        text
    }

    /// Version 3 (Siedler): die Felder, die die Karte Siedler um diese Figur faerbt: alle
    /// Nachbarfelder ohne Figur, die nicht schon dem Besitzer gehoeren und keine gegnerische Fabrik
    /// tragen. Nur wenn die Figur auf verbundenem Land steht (sonst faerbt sie nichts).
    pub fn settle_cells(&self, piece: &Piece) -> Vec<CellId> {
        if !self.piece_connected(piece) {
            return Vec::new();
        }
        self.map
            .board
            .neighbors(piece.cell)
            .filter(|&c| {
                self.state.piece_at(c).is_none()
                    && self.state.land[c.idx()] != Some(piece.owner)
                    && self
                        .state
                        .factory_at(c)
                        .is_none_or(|f| f.owner == piece.owner)
            })
            .collect()
    }

    /// Wirkung der Karte Siedler: faerbt die Felder von `settle_cells` und loest dabei die Folgen
    /// eines Zuges aus (Abtrennen, Umschliessen).
    pub(crate) fn settle_around(
        &mut self,
        actor: PlayerId,
        target: Option<PieceId>,
        events: &mut Vec<Event>,
    ) {
        let Some(piece) = target.and_then(|t| self.state.pieces.get(&t)).cloned() else {
            return;
        };
        let path = self.settle_cells(&piece);
        if path.is_empty() {
            return;
        }
        let plan = MovePlan {
            connected: true,
            path,
        };
        self.territory_apply_move(actor, &plan, events);
    }

    /// Version 3: was die Fabrik des Spielers je Ernte mehr bekommt: Schmiede +1 Eisen (Kat 1),
    /// Bergbau +1 Gold (Kat 2). Nur im erweiterten Kartensatz.
    fn harvest_bonus(&self, player: PlayerId, res: ResourceId) -> u32 {
        if !self.config.terr_cardset3 {
            return 0;
        }
        let pp = &self.state.player_perks[player.idx()];
        match self.map.category(res) {
            1 => u32::from(pp.smelter),
            2 => u32::from(pp.mining),
            _ => 0,
        }
    }

    /// Version 3 (`terr_cardany`): kann `card` auf diese Figur gelegt werden? Nur auf Einheiten
    /// (nicht den Koenig) und erst ab Reichweite 2: eine Figur mit Reichweite 1 zieht ein Feld,
    /// da gibt es nichts zu kuerzen, keine Ecke und keinen Sprung ueber ein Zwischenfeld (und
    /// Figuren werden nie hoeher). Ohne `terr_stack` geht jede Karte nur einmal auf eine Figur;
    /// mit `terr_stack` so oft, wie es logisch Sinn hat: eine Figur der Reichweite `r` zieht
    /// hoechstens `r` Schritte, davon bleiben `r - 1` Schritte, die man verkuerzen, mit einer
    /// Ecke verlassen oder mit einem Sprung ueber eine Figur gehen kann (der letzte Schritt
    /// ist das Ziel).
    pub fn card_fits_piece(&self, card: Card, piece: &Piece) -> bool {
        let PieceKind::Unit { level, range, .. } = &piece.kind else {
            return false;
        };
        // Fort: auf jede eigene Einheit, wenn auf ihrem Feld ein Fort entstehen darf.
        if card == Card::Fort {
            return self.fort_fits(piece);
        }
        // Kamikaze: auf jede eigene Einheit, die noch nicht brennt.
        if card == Card::Kamikaze {
            return self.config.terr_kamikaze && piece.perks.kamikaze == 0;
        }
        // Diagonalzug: nur auf einem Bauern (Stufe 1) und nur einmal je Figur.
        if card == Card::PawnDiagonal {
            return *level == 1 && !piece.perks.pawn_diag;
        }
        if let Card::Custom(_) = card {
            // Eine selbst gebaute Karte passt, wenn mindestens ein Baustein auf die Figur passt.
            return self
                .config
                .card_tweaks
                .parts(card)
                .iter()
                .any(|&(part, _)| part != card && self.card_fits_piece(part, piece));
        }
        let have = match card {
            Card::Shorten => piece.perks.shorten,
            Card::Bend => piece.perks.bend,
            Card::Hop => piece.perks.hop,
            Card::Pass => piece.perks.pass,
            // Siedler wirkt sofort und bleibt nicht liegen: passt, solange es etwas zu faerben gibt.
            Card::Settle => return !self.settle_cells(piece).is_empty(),
            _ => return false,
        };
        let most = if self.config.terr_stack && card != Card::Pass {
            range.saturating_sub(1)
        } else {
            u8::from(*range >= 2)
        };
        have < most
    }

    /// Version 3.3: darf auf dem Feld dieser Figur ein Fort entstehen? Es braucht eigenes Land, das mit
    /// der stehenden Fabrik verbunden ist, kein Fabrikfeld und noch kein eigenes Fort dort.
    fn fort_fits(&self, piece: &Piece) -> bool {
        let owner = piece.owner;
        self.config.terr_fort
            && !piece.is_king()
            && self.state.land[piece.cell.idx()] == Some(owner)
            && self.state.forts[owner.idx()] != Some(piece.cell)
            && self.state.factory_at(piece.cell).is_none()
            && self
                .connected_land(owner)
                .is_some_and(|c| c[piece.cell.idx()])
    }

    /// Was `player` fuer `card` aus dem Lager seiner Fabrik zahlen wuerde.
    pub fn territory_card_payment(&self, player: PlayerId, card: Card) -> Option<Inventory> {
        let f = self.standing_factory(player)?;
        plan_by_category(
            &f.storage,
            &self.map,
            self.territory_card_price_for(player, card),
        )
    }

    pub(crate) fn territory_build_actions(&self, me: PlayerId) -> Vec<Action> {
        let Some(f) = self.standing_factory(me) else {
            return Vec::new();
        };
        if self.state.piece_at(f.cell).is_some() {
            return Vec::new();
        }
        (1..=self.max_unit_level())
            .filter(|&l| self.can_afford_unit(me, l))
            .map(|level| Action::BuildUnit {
                factory: f.id,
                level,
            })
            .collect()
    }

    /// Version 3 (`terr_kingup`): wird der Koenig direkt mit Kat 2 besser, ohne Karte aus dem
    /// Markt? Dann liegt der Koenigs-Boost immer aus (auf Quadrat- und Sechseckbrettern; auf dem
    /// Dreiecksbrett gibt es keine zusaetzlichen Zugfelder).
    pub fn king_upgrade_on(&self) -> bool {
        self.config.territory
            && self.config.terr_kingup
            && self.config.terr_cards == 1
            && self.king_boost_extra(1) > 0
    }

    /// Wie viele Zugfelder der Koenig mit der Boost-Stufe `level` (1 bis 3) dazubekommt, auf freiem
    /// Brett gezaehlt: Quadrat 4 (diagonal) / 4 (zwei geradeaus) / 8 (zwei vor, eins zur Seite),
    /// Sechseck 6 / 6 / 12. Auf dem Dreiecksbrett 0 (dort gibt es keinen Boost).
    pub fn king_boost_extra(&self, level: u8) -> u32 {
        let table: [u32; 3] = match self.map.board.kind() {
            crate::board::GridKind::Square => [4, 4, 8],
            crate::board::GridKind::Hex => [6, 6, 12],
            crate::board::GridKind::Triangle => [0, 0, 0],
        };
        match level {
            1..=3 => table[usize::from(level) - 1],
            _ => 0,
        }
    }

    /// Die Karten, die jetzt zu kaufen sind: der Markt, dazu mit `terr_kingup` immer der Koenigs-Boost.
    pub fn territory_offers(&self) -> Vec<Card> {
        let mut out = self.state.market.clone();
        if self.king_upgrade_on() && !out.contains(&Card::KingBoost) {
            out.push(Card::KingBoost);
        }
        out
    }

    pub(crate) fn territory_buy_actions(&self, me: PlayerId) -> Vec<Action> {
        let mut out = Vec::new();
        let offers = self.territory_offers();
        if (self.state.bought_this_turn && !self.config.terr_multibuy) || offers.is_empty() {
            return out;
        }
        let Some(f) = self.standing_factory(me) else {
            return out;
        };
        for &card in &offers {
            if !self.territory_card_takeable(me, card)
                || self.territory_card_payment(me, card).is_none()
            {
                continue;
            }
            let from = PaySource::Factory(f.id);
            if self.cards_go_on_pieces(card) {
                // Eine Aktion je Figur, auf die die Karte passt.
                for piece in self.pieces_of(me).filter(|p| self.card_fits_piece(card, p)) {
                    out.push(Action::BuyCard {
                        card,
                        from,
                        target: Some(piece.id),
                    });
                }
            } else {
                out.push(Action::BuyCard {
                    card,
                    from,
                    target: None,
                });
            }
        }
        out
    }

    // ----- Version 3: Umzug, Neubau und Kat 3 ----------------------------------------------------

    /// Bauzeit eines Umzugs von `from` nach `to` in Runden: die halbe Entfernung (Schritte ueber
    /// Nachbarfelder), aufgerundet, mindestens 1.
    pub fn relocation_rounds(&self, from: CellId, to: CellId) -> u32 {
        let d = self.map.board.distances_from(from)[to.idx()];
        d.div_ceil(2).max(1)
    }

    /// Was der Koenig mit `RebuildFactory` taete: (Bauzeit in Runden, Kosten Kat 1 und Kat 2).
    /// `None`, wenn er hier nicht bauen darf. Steht eine Fabrik, muss das Feld des Koenigs mit ihr
    /// verbunden sein, die Bauzeit ist `relocation_rounds` und das Lager muss den Preis hergeben;
    /// nach einer Zerstoerung geht es nur auf eigenem Land (hat er gar keines mehr, auch dort, wo er steht) und ohne Kosten, die Bauzeit ist mit
    /// `terr_rebuild_dist` wie beim Umzug die halbe Entfernung zum alten Platz (`rebuild_rounds_lost`),
    /// sonst fest `terr_rebuild` Runden.
    pub fn rebuild_plan(&self, me: PlayerId) -> Option<(u32, [u32; 2])> {
        let c = &self.config;
        if !c.territory || c.terr_move == 0 {
            return None;
        }
        let king = self.king(me)?;
        if self.state.factory_at(king.cell).is_some() {
            return None;
        }
        match self.home_factory(me) {
            Some(f) => {
                if f.building > 0 {
                    return None;
                }
                let price = [c.terr_move_k1, c.terr_move_k2];
                plan_by_category(&f.storage, &self.map, [price[0], price[1], 0])?;
                if !self.connected_land(me)?[king.cell.idx()] {
                    return None;
                }
                Some((self.relocation_rounds(f.cell, king.cell), price))
            }
            // Wehrlos kann der Koenig kein Land mehr holen: hat der Gegner ihm alles abgenommen,
            // darf er trotzdem dort neu bauen, wo er steht (sonst waere er fuer immer gesperrt).
            None => (self.state.land[king.cell.idx()] == Some(me) || self.land_count(me) == 0)
                .then(|| (self.rebuild_rounds_lost(me, king.cell), [0, 0])),
        }
    }

    /// Bauzeit des Neubaus nach einer Zerstoerung, wenn der Koenig von `me` auf `to` steht: mit
    /// `terr_rebuild_dist` die Bauzeit eines Umzugs vom alten Platz der Fabrik (`relocation_rounds`),
    /// sonst (oder ohne bekannten alten Platz) fest `terr_rebuild`.
    pub fn rebuild_rounds_lost(&self, me: PlayerId, to: CellId) -> u32 {
        self.rebuild_rounds_from(self.state.lost_factory[me.idx()], to)
    }

    /// Wie `rebuild_rounds_lost`, fuer eine Fabrik, die auf `old` stand (die Bots rechnen damit
    /// durch, was eine Zerstoerung kosten wuerde).
    pub fn rebuild_rounds_from(&self, old: Option<CellId>, to: CellId) -> u32 {
        match old {
            Some(old) if self.config.terr_rebuild_dist => self.relocation_rounds(old, to),
            _ => self.config.terr_rebuild.max(1),
        }
    }

    pub(crate) fn territory_rebuild_actions(&self, me: PlayerId) -> Vec<Action> {
        if self.rebuild_plan(me).is_some() {
            vec![Action::RebuildFactory]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn do_rebuild_factory(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let (rounds, price) = self.rebuild_plan(actor).expect("Neubau ist erlaubt");
        let to = self.king(actor).expect("Koenig lebt").cell;
        let map = std::sync::Arc::clone(&self.map);
        let (fid, from) = match self.home_factory(actor).map(|f| (f.id, f.cell)) {
            Some((fid, from)) => {
                let storage = &mut self
                    .state
                    .factories
                    .get_mut(&fid)
                    .expect("Fabrik existiert")
                    .storage;
                let mut spent = 0;
                for (i, n) in price.into_iter().enumerate() {
                    spent += take_category(storage, &map, i as u8 + 1, n).total();
                }
                self.state.ledger.spent_on_moves += spent;
                self.state.move_factory(fid, to);
                (fid, Some(from))
            }
            None => {
                self.state.lost_factory[actor.idx()] = None;
                if self.state.land[to.idx()] != Some(actor) {
                    // Ohne jedes eigene Land: der Platz der neuen Fabrik wird eigenes Land.
                    self.set_land(to, Some(actor));
                }
                (self.state.insert_factory(actor, to), None)
            }
        };
        // Der Zug endet gleich mit dem Zugende, das schon einen Schritt zaehlt.
        self.state
            .factories
            .get_mut(&fid)
            .expect("Fabrik existiert")
            .building = rounds + 1;
        events.push(Event::FactoryRebuilding {
            factory: fid,
            owner: actor,
            from,
            to,
            rounds,
        });
    }

    /// Wie viel Flaeche `k` Einheiten Kat 3 ergeben: `2^k` (hoechstens `2^20`).
    pub fn trade_area(k: u32) -> u32 {
        1u32 << k.min(20)
    }

    /// Version 3: Flaechenbonus je Runde fuer `k` verbundene Kat-3-Felder: `2^k`, ohne Kat 3
    /// nichts (hoechstens `2^20`).
    pub fn kat3_bonus(k: u32) -> u32 {
        if k == 0 {
            0
        } else {
            1u32 << k.min(20)
        }
    }

    /// Version 3: wie viele Kat-3-Felder auf dem mit der stehenden Fabrik verbundenen Land von
    /// `me` liegen (0 ohne `terr_bonus`, ohne stehende Fabrik oder ohne Anbindung).
    pub fn connected_kat3(&self, me: PlayerId) -> u32 {
        if !self.config.kat3_bonus_on() || self.rebuilding(me) {
            return 0;
        }
        let Some(connected) = self.connected_land(me) else {
            return 0;
        };
        self.map
            .board
            .cells()
            .filter(|c| connected[c.idx()])
            .map(|c| {
                self.all_deposits(c)
                    .into_iter()
                    .filter(|&r| self.map.category(r) == 3)
                    .count() as u32
            })
            .sum()
    }

    /// Kat 3 im Lager der stehenden Fabrik.
    pub fn tradable_kat3(&self, me: PlayerId) -> u32 {
        if !self.config.terr_trade {
            return 0;
        }
        self.standing_factory(me)
            .map_or(0, |f| self.category_units(&f.storage, 3))
    }

    /// Faustregel der Bots: jetzt Kat 3 tauschen? Ja, wenn dadurch eine hoehere Stufe bezahlbar
    /// wird, bei fuenf oder mehr Stueck (mehr bringt wenig) oder kurz vor der Wertung.
    pub fn bot_should_trade(&self, me: PlayerId) -> bool {
        let k = self.tradable_kat3(me);
        if k == 0 {
            return false;
        }
        let Some(f) = self.standing_factory(me) else {
            return false;
        };
        let area_now = self.state.area[me.idx()];
        let area_after = area_now.saturating_add(Game::trade_area(k));
        let best = |area: u32| {
            (1..=self.max_unit_level())
                .filter(|&l| {
                    let p = self.unit_price(l);
                    area >= p.area
                        && self.category_units(&f.storage, 1) >= p.k1
                        && self.category_units(&f.storage, 2) >= p.k2
                })
                .max()
                .unwrap_or(0)
        };
        let late = self.config.score_after > 0 && self.state.round + 10 >= self.config.score_after;
        best(area_after) > best(area_now) || k >= 5 || late
    }

    pub(crate) fn territory_trade_actions(&self, me: PlayerId) -> Vec<Action> {
        match (self.standing_factory(me), self.tradable_kat3(me)) {
            (Some(f), n) if n > 0 => vec![Action::TradeKat3 { factory: f.id }],
            _ => Vec::new(),
        }
    }

    pub(crate) fn do_trade_kat3(
        &mut self,
        actor: PlayerId,
        fid: FactoryId,
        events: &mut Vec<Event>,
    ) {
        let map = std::sync::Arc::clone(&self.map);
        let have = self.category_units(&self.state.factories[&fid].storage, 3);
        let storage = &mut self
            .state
            .factories
            .get_mut(&fid)
            .expect("Fabrik existiert")
            .storage;
        let taken = take_category(storage, &map, 3, have).total();
        let count = taken as u32;
        let area = Game::trade_area(count);
        self.state.ledger.traded += taken;
        self.state.area[actor.idx()] += area;
        events.push(Event::Traded {
            player: actor,
            count,
            area,
        });
    }

    // ----- Version 2: Fabrik ruecken ------------------------------------------------------------

    /// Felder, auf die die Fabrik mit der Karte "Fabrik ruecken" rutschen darf: die vier
    /// Diagonalfelder (nur im Quadratgitter), wenn sie eigenes Land sind, das mit der Fabrik
    /// zusammenhaengt, und frei von Figuren. So bleibt der Zusammenhang des Landes erhalten.
    pub fn factory_shift_targets(&self, me: PlayerId) -> Vec<CellId> {
        if self.state.stored_card[me.idx()] != Some(Card::FactoryShift) {
            return Vec::new();
        }
        self.factory_shift_cells(me)
    }

    /// Wie `factory_shift_targets`, aber ohne die Karte vorauszusetzen: wohin die Fabrik rutschen
    /// koennte. Der Kauf der Karte braucht mindestens ein solches Feld (sie wirkt sofort).
    pub fn factory_shift_cells(&self, me: PlayerId) -> Vec<CellId> {
        let board = &self.map.board;
        if board.kind() != crate::board::GridKind::Square {
            return Vec::new();
        }
        let (Some(f), Some(connected)) = (self.standing_factory(me), self.connected_land(me))
        else {
            return Vec::new();
        };
        let (x, y) = board.coord(f.cell);
        let mut out: Vec<CellId> = [(1, 1), (1, -1), (-1, 1), (-1, -1)]
            .into_iter()
            .filter_map(|(dx, dy)| board.cell_at(x + dx, y + dy))
            .filter(|c| {
                connected[c.idx()]
                    && self.state.piece_at(*c).is_none()
                    && self.state.factory_at(*c).is_none()
            })
            .collect();
        out.sort();
        out
    }

    pub(crate) fn factory_shift_actions(&self, me: PlayerId) -> Vec<Action> {
        let Some(f) = self.standing_factory(me) else {
            return Vec::new();
        };
        self.factory_shift_targets(me)
            .into_iter()
            .map(|to| Action::ShiftFactory { factory: f.id, to })
            .collect()
    }

    pub(crate) fn do_shift_factory(
        &mut self,
        actor: PlayerId,
        fid: FactoryId,
        to: CellId,
        events: &mut Vec<Event>,
    ) {
        let from = self.state.factories[&fid].cell;
        self.state.stored_card[actor.idx()] = None;
        self.state.move_factory(fid, to);
        events.push(Event::FactoryShifted {
            factory: fid,
            owner: actor,
            from,
            to,
        });
    }

    /// Wehrlose Figuren (nicht auf verbundenem Land) duerfen nichts schlagen und keine
    /// gegnerische Fabrik betreten. Entfernt solche Ziele aus `out`.
    pub(crate) fn territory_filter(&self, piece: &Piece, out: &mut Vec<CellId>) {
        let owner = piece.owner;
        let attack = |c: CellId| {
            self.state.piece_at(c).is_some_and(|o| o.owner != owner)
                || self.state.factory_at(c).is_some_and(|f| f.owner != owner)
        };
        if !out.iter().any(|&c| attack(c)) || self.piece_connected(piece) {
            return;
        }
        out.retain(|&c| !attack(c));
    }

    // ----- Der Weg eines Zuges --------------------------------------------------------------

    /// Plan fuer den Zug von `id` nach `to`; vor dem Zug aufzurufen.
    pub(crate) fn territory_plan(&self, id: PieceId, to: CellId) -> MovePlan {
        let piece = &self.state.pieces[&id];
        let connected = self.piece_connected(piece);
        let path = if connected {
            self.move_path(piece, to)
        } else {
            Vec::new()
        };
        MovePlan { connected, path }
    }

    /// Wie viele Felder der Zug umfaerben wuerde (fuer die Bots).
    pub fn move_gain(&self, id: PieceId, to: CellId) -> u32 {
        let plan = self.territory_plan(id, to);
        let owner = self.state.pieces[&id].owner;
        if !plan.connected && self.config.terr_reconnect {
            // Eine wehrlose Figur, die an ihr Land heranzieht, faerbt das Zielfeld.
            let next_to_land = self
                .connected_land(owner)
                .is_some_and(|c| self.map.board.neighbors(to).any(|nb| c[nb.idx()]));
            return u32::from(next_to_land && self.state.land[to.idx()] != Some(owner));
        }
        plan.path
            .iter()
            .filter(|&&c| self.state.land[c.idx()] != Some(owner))
            .count() as u32
    }

    /// Die Felder, die ein Zug nach `to` faerbt. Gerade Zuege und Koenigsschritte haben einen
    /// eindeutigen Weg; bei freier Bewegung (ab Stufe 2) gilt unter den kuerzesten Wegen der,
    /// der die meisten Felder umfaerbt (bei Gleichstand der mit den kleineren Feldnummern).
    /// Spruenge faerben nur das Zielfeld.
    pub fn move_path(&self, piece: &Piece, to: CellId) -> Vec<CellId> {
        let board = &self.map.board;
        let from = piece.cell;
        let owner = piece.owner;
        match &piece.kind {
            PieceKind::King => {
                if board.neighbors(from).any(|n| n == to) {
                    return vec![to];
                }
                if let Some((_, path)) = self
                    .king_boost_moves(piece)
                    .into_iter()
                    .find(|(cell, _)| *cell == to)
                {
                    return path;
                }
            }
            PieceKind::Unit { .. } if self.config.terr_exact => {
                if let Some((_, path)) = self
                    .exact_moves(piece)
                    .into_iter()
                    .find(|(cell, _)| *cell == to)
                {
                    return path;
                }
            }
            PieceKind::Unit {
                level: 1, range, ..
            } => {
                let range = *range + self.charge_bonus(owner);
                for dir in 0..board.dir_count() {
                    let line = board.line(from, dir, range as usize);
                    if let Some(pos) = line.iter().position(|&c| c == to) {
                        if line[..pos]
                            .iter()
                            .all(|&c| self.state.piece_at(c).is_none())
                        {
                            return line[..=pos].to_vec();
                        }
                    }
                }
            }
            PieceKind::Unit { range, .. } => {
                let range = (*range + self.charge_bonus(owner)) as u32;
                if let Some(path) = self.best_path(owner, from, to, range) {
                    return path;
                }
            }
        }
        self.jump_path(owner, from, to)
    }

    /// Version 2: alle Zuege einer Einheit mit ihrem gefaerbten Weg. Pro Ziel gilt der Weg mit
    /// den meisten umzufaerbenden Feldern, bei Gleichstand der mit den kleineren Feldnummern.
    pub fn exact_moves(&self, piece: &Piece) -> Vec<(CellId, Vec<CellId>)> {
        let owner = piece.owner;
        let mut best: std::collections::BTreeMap<CellId, (std::cmp::Reverse<usize>, Vec<CellId>)> =
            std::collections::BTreeMap::new();
        self.exact_walk(piece, &mut |to, path| {
            let gain = path
                .iter()
                .filter(|&&c| self.state.land[c.idx()] != Some(owner))
                .count();
            let cand = (std::cmp::Reverse(gain), path.to_vec());
            match best.get(&to) {
                Some(old) if *old <= cand => {}
                _ => {
                    best.insert(to, cand);
                }
            }
        });
        best.into_iter().map(|(to, (_, path))| (to, path)).collect()
    }

    // ----- Version 3.6: Kamikaze ------------------------------------------------------------------

    /// Alle Angriffe einer brennenden Figur: das Feld einer gegnerischen Figur, die genau in
    /// Reichweite liegt (wie `exact_walk`, eine gegnerische Figur davor darf uebersprungen werden),
    /// mit dem Weg. Fuehren mehrere Wege zum selben Ziel, gilt der mit den meisten gegnerischen
    /// Figuren, bei Gleichstand der mit den kleineren Feldnummern. Wehrlose Figuren (nicht auf
    /// verbundenem Land) greifen nicht an.
    pub fn kamikaze_strikes(&self, piece: &Piece) -> Vec<(CellId, Vec<CellId>)> {
        if piece.perks.kamikaze != 2 || !self.piece_connected(piece) {
            return Vec::new();
        }
        let owner = piece.owner;
        let kills = |path: &[CellId]| {
            path.iter()
                .filter(|&&c| self.state.piece_at(c).is_some_and(|p| p.owner != owner))
                .count()
        };
        let mut best: std::collections::BTreeMap<CellId, Vec<CellId>> =
            std::collections::BTreeMap::new();
        self.exact_walk_with(piece, true, &mut |to, path| {
            if !self.state.piece_at(to).is_some_and(|p| p.owner != owner) {
                return;
            }
            match best.get(&to) {
                Some(old)
                    if (std::cmp::Reverse(kills(old)), old.as_slice())
                        <= (std::cmp::Reverse(kills(path)), path) => {}
                _ => {
                    best.insert(to, path.to_vec());
                }
            }
        });
        best.into_iter().collect()
    }

    /// Der Angriff der brennenden Figur `id` auf das Feld `to` (aus `kamikaze_strikes`): alle
    /// gegnerischen Figuren auf dem Weg sterben, die Figur selbst auch, alle Felder des Weges
    /// werden neutral (eine gegnerische Fabrik dort ist zerstoert, eigene Fabriken bleiben) und
    /// die Folgen laufen wie bei jedem Zug: abgetrenntes Land wird neutral, Umschliessen faerbt.
    pub(crate) fn do_kamikaze(
        &mut self,
        actor: PlayerId,
        id: PieceId,
        to: CellId,
        events: &mut Vec<Event>,
    ) {
        let piece = self.state.pieces[&id].clone();
        let path = self
            .kamikaze_strikes(&piece)
            .into_iter()
            .find(|(cell, _)| *cell == to)
            .map(|(_, path)| path)
            .expect("Angriff in kamikaze_strikes");
        let victims: Vec<(PieceId, PlayerId, CellId)> = path
            .iter()
            .filter_map(|&c| {
                self.state
                    .piece_at(c)
                    .filter(|p| p.owner != actor)
                    .map(|p| (p.id, p.owner, c))
            })
            .collect();
        self.state.remove_piece(id);
        events.push(Event::KamikazeStrike {
            piece: id,
            owner: actor,
            from: piece.cell,
            to,
            path: path.clone(),
            killed: victims.iter().map(|v| v.0).collect(),
        });
        for (victim, owner, at) in victims {
            let Some(gone) = self.state.remove_piece(victim) else {
                continue;
            };
            events.push(Event::Captured {
                piece: victim,
                owner,
                at,
                by: actor,
            });
            if gone.is_king() {
                self.eliminate(owner, events);
            }
        }
        // Der Weg wird neutral; eigene Fabriken behalten ihr Land, gegnerische sind zerstoert.
        for &cell in &path {
            if let Some(fid) = self.state.factory_at[cell.idx()] {
                if self.state.factories[&fid].owner == actor {
                    continue;
                }
                self.destroy_factory(fid, actor, events);
            }
            self.set_land(cell, None);
        }
        // Wer durch die Luecke vom eigenen Land abgeschnitten ist, verliert es, auch der Angreifer.
        if self.alive_players().len() > 1 {
            self.cut_off_neutral(events);
            self.territory_settle(actor, events);
        }
    }

    /// Land, das nicht mehr mit der Fabrik seines Besitzers zusammenhaengt, wird neutral, bei
    /// allen Spielern (auch dem Ziehenden).
    fn cut_off_neutral(&mut self, events: &mut Vec<Event>) {
        for p in 0..self.config.players {
            let player = PlayerId(p);
            if !self.state.alive[player.idx()] || self.rebuilding(player) {
                continue;
            }
            let Some(connected) = self.connected_land(player) else {
                continue;
            };
            let cells: Vec<CellId> = self
                .map
                .board
                .cells()
                .filter(|c| self.state.land[c.idx()] == Some(player) && !connected[c.idx()])
                .collect();
            if cells.is_empty() {
                continue;
            }
            for c in &cells {
                self.set_land(*c, None);
            }
            events.push(Event::CutOff {
                from: player,
                to: None,
                cells,
            });
        }
    }

    /// Am Ende des Zuges von `actor`: die brennende Figur, die nicht angegriffen hat, ist weg;
    /// eine frisch gekaufte (Pause) fängt Feuer.
    pub(crate) fn kamikaze_end_of_turn(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        if !self.config.terr_kamikaze {
            return;
        }
        let ids: Vec<PieceId> = self
            .pieces_of(actor)
            .filter(|p| p.perks.kamikaze > 0)
            .map(|p| p.id)
            .collect();
        for id in ids {
            let (state, cell) = {
                let p = &self.state.pieces[&id];
                (p.perks.kamikaze, p.cell)
            };
            if state >= 2 {
                self.state.remove_piece(id);
                events.push(Event::KamikazeGone {
                    piece: id,
                    owner: actor,
                    cell,
                });
            } else if let Some(p) = self.state.pieces.get_mut(&id) {
                p.perks.kamikaze = 2;
            }
        }
    }

    /// Geht alle Wege einer Einheit im Modus "genau n Felder" ab und meldet jedes Ziel samt Weg
    /// (Felder ohne Startfeld). Eine Figur der Stufe n zieht geradeaus genau n Felder; Karten
    /// erlauben einen Schritt weniger (`shorten`), eine Ecke (`bend`) oder einen Sprung ueber
    /// eine Figur (`hop`). Ziel darf ein Gegner sein (wird geschlagen), nie eine eigene Figur.
    pub(crate) fn exact_walk(&self, piece: &Piece, emit: &mut dyn FnMut(CellId, &[CellId])) {
        self.exact_walk_with(piece, false, emit);
    }

    /// Wie `exact_walk`; mit `skip` darf der Weg zusaetzlich einmal eine gegnerische Figur
    /// ueberspringen (Kamikaze).
    fn exact_walk_with(&self, piece: &Piece, skip: bool, emit: &mut dyn FnMut(CellId, &[CellId])) {
        let PieceKind::Unit { range, .. } = &piece.kind else {
            return;
        };
        let base = *range + self.charge_bonus(piece.owner);
        // Die Laenge `base`, dazu je Karte Kuerzer ein Schritt weniger (mindestens 1 Feld); auf dem
        // Fort (Version 3.3) darf es auch einer mehr sein.
        let shorter = usize::from(piece.perks.shorten).min(usize::from(base).saturating_sub(1));
        let longest = usize::from(base) + usize::from(self.fort_bonus(piece));
        let lengths: Vec<usize> = (0..=shorter + longest - usize::from(base))
            .map(|i| longest - i)
            .collect();
        let mut path = Vec::with_capacity(5);
        for len in lengths {
            self.walk_from(
                piece.owner,
                piece.cell,
                None,
                len,
                piece.perks.bend,
                piece.perks.hop,
                piece.perks.pass > 0,
                skip,
                &mut path,
                emit,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn walk_from(
        &self,
        owner: PlayerId,
        cell: CellId,
        dir: Option<usize>,
        left: usize,
        bends: u8,
        hops: u8,
        pass: bool,
        skip: bool,
        path: &mut Vec<CellId>,
        emit: &mut dyn FnMut(CellId, &[CellId]),
    ) {
        if left == 0 {
            return;
        }
        let board = &self.map.board;
        let tail = self.config.terr_bend_tail;
        for d in 0..board.dir_count() {
            let turning = match dir {
                None => false,
                Some(prev) if prev == d => false,
                Some(prev) => {
                    // Version 3: die Ecke gilt nur fuer die letzten `bends` Felder des Weges (mit
                    // einer Karte nur das letzte, mit zwei das letzte und vorletzte, ...).
                    if bends == 0
                        || d == board.opposite(prev)
                        || (tail && usize::from(bends) < left)
                    {
                        continue;
                    }
                    true
                }
            };
            let Some(next) = board.step(cell, d) else {
                continue;
            };
            let occupant = self.state.piece_at(next);
            path.push(next);
            if left == 1 {
                if !occupant.is_some_and(|p| p.owner == owner) {
                    emit(next, path);
                }
            } else {
                // Mit der Schwanz-Regel verbraucht eine Ecke nichts: die letzten `bends` Felder duerfen alle abbiegen.
                let bends_left = if turning && !tail { bends - 1 } else { bends };
                let mut go = |hops: u8, skip: bool, path: &mut Vec<CellId>| {
                    self.walk_from(
                        owner,
                        next,
                        Some(d),
                        left - 1,
                        bends_left,
                        hops,
                        pass,
                        skip,
                        path,
                        emit,
                    )
                };
                match occupant {
                    None => go(hops, skip, path),
                    // Durchzug: eigene Figuren versperren den Weg nicht.
                    Some(p) if pass && p.owner == owner => go(hops, skip, path),
                    Some(p) => {
                        // Kamikaze: eine gegnerische Figur darf einmal frei uebersprungen werden.
                        if skip && p.owner != owner {
                            go(hops, false, path);
                        }
                        if hops > 0 {
                            go(hops - 1, skip, path);
                        }
                    }
                }
            }
            path.pop();
        }
    }

    /// Version 2: zusaetzliche Zuege des Koenigs durch den Boost (nur Quadratgitter), je mit
    /// ihrem gefaerbten Weg. Stufe 1: ein Schritt diagonal. Stufe 2: zwei Felder geradeaus.
    /// Stufe 3: zwei vor und eins zur Seite (der Weg ist ein L). Die Stufen gelten zusammen.
    pub fn king_boost_moves(&self, piece: &Piece) -> Vec<(CellId, Vec<CellId>)> {
        use crate::board::GridKind;
        let owner = piece.owner;
        let boost = self.state.player_perks[owner.idx()].king_boost;
        let board = &self.map.board;
        if !piece.is_king() || boost == 0 {
            return Vec::new();
        }
        let kind = board.kind();
        // Auf dem Dreiecksbrett gibt es keine zusaetzlichen Zuege; auf dem Sechseckbrett nur mit
        // `terr_kingup` (die Karte selbst gibt es dort ohne den Schalter nicht).
        if kind == GridKind::Triangle || (kind == GridKind::Hex && !self.king_upgrade_on()) {
            return Vec::new();
        }
        let mut out: Vec<(CellId, Vec<CellId>)> = Vec::new();
        let from = piece.cell;
        let dirs = board.dir_count();
        let free = |c: CellId| self.state.piece_at(c).is_none();
        let ok = |c: CellId| !self.own_piece_at(c, owner);
        // Stufe 1: "diagonal". Quadrat: die vier Diagonalfelder; Sechseck: ein Schritt in einer
        // Richtung und einer in der benachbarten (die sechs Felder zwischen den Geraden).
        if kind == GridKind::Square {
            let (x, y) = board.coord(from);
            for (dx, dy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                if let Some(to) = board.cell_at(x + dx, y + dy).filter(|&c| ok(c)) {
                    out.push((to, self.jump_path(owner, from, to)));
                }
            }
        } else {
            for dir in 0..dirs {
                let to = board
                    .step(from, dir)
                    .and_then(|a| board.step(a, (dir + 1) % dirs))
                    .filter(|&c| ok(c));
                if let Some(to) = to {
                    out.push((to, self.jump_path(owner, from, to)));
                }
            }
        }
        if boost >= 2 {
            for dir in 0..dirs {
                let line = board.line(from, dir, 2);
                if line.len() == 2 && free(line[0]) && ok(line[1]) {
                    out.push((line[1], line));
                }
            }
        }
        if boost >= 3 {
            for dir in 0..dirs {
                let line = board.line(from, dir, 2);
                if line.len() < 2 || !free(line[0]) || !free(line[1]) {
                    continue;
                }
                for side in [(dir + 1) % dirs, (dir + dirs - 1) % dirs] {
                    if let Some(to) = board.step(line[1], side).filter(|&c| ok(c)) {
                        out.push((to, vec![line[0], line[1], to]));
                    }
                }
            }
        }
        out.sort();
        out.dedup_by_key(|(to, _)| *to);
        out
    }

    /// Spruenge (Pferd, Sprungfeder, Lanze, Laeuferschlag) faerben eine Bruecke: ein Feld, das
    /// an Start und Ziel grenzt, und das Ziel. Sonst landete die Figur auf einem Inselfeld und
    /// waere sofort wehrlos. Unter mehreren Bruecken gewinnt die mit mehr umzufaerbenden Feldern,
    /// dann die mit der kleineren Nummer.
    pub(crate) fn jump_path(&self, owner: PlayerId, from: CellId, to: CellId) -> Vec<CellId> {
        let board = &self.map.board;
        if board.neighbors(from).any(|n| n == to) {
            return vec![to];
        }
        let near: Vec<CellId> = board.neighbors(from).collect();
        let bridge = board
            .neighbors(to)
            .filter(|n| near.contains(n))
            .max_by_key(|&n| {
                (
                    i32::from(self.state.land[n.idx()] != Some(owner)),
                    std::cmp::Reverse(n),
                )
            });
        match bridge {
            Some(mid) => vec![mid, to],
            None => vec![to],
        }
    }

    /// Kuerzester Weg von `from` nach `to` ueber freie Felder mit den meisten umzufaerbenden
    /// Feldern, hoechstens `range` Schritte.
    fn best_path(
        &self,
        owner: PlayerId,
        from: CellId,
        to: CellId,
        range: u32,
    ) -> Option<Vec<CellId>> {
        let board = &self.map.board;
        let n = board.len();
        let none = CellId(u32::MAX);
        let mut dist = vec![u32::MAX; n];
        let mut gain = vec![i32::MIN; n];
        let mut pred = vec![none; n];
        let mut queue = VecDeque::new();
        dist[from.idx()] = 0;
        gain[from.idx()] = 0;
        queue.push_back(from);
        let new_cell = |c: CellId| i32::from(self.state.land[c.idx()] != Some(owner));
        while let Some(cur) = queue.pop_front() {
            let d = dist[cur.idx()];
            if d >= range {
                continue;
            }
            for nb in board.neighbors(cur) {
                if dist[nb.idx()] == u32::MAX {
                    dist[nb.idx()] = d + 1;
                    if self.state.piece_at(nb).is_none() {
                        queue.push_back(nb);
                    }
                }
                if dist[nb.idx()] == d + 1 {
                    let g = gain[cur.idx()] + new_cell(nb);
                    if g > gain[nb.idx()] || (g == gain[nb.idx()] && cur < pred[nb.idx()]) {
                        gain[nb.idx()] = g;
                        pred[nb.idx()] = cur;
                    }
                }
            }
        }
        if dist[to.idx()] == u32::MAX || dist[to.idx()] > range {
            return None;
        }
        let mut path = vec![to];
        let mut cur = to;
        while pred[cur.idx()] != from {
            cur = pred[cur.idx()];
            if cur == none {
                return None;
            }
            path.push(cur);
        }
        path.reverse();
        Some(path)
    }

    // ----- Wirkung eines Zuges ----------------------------------------------------------------

    /// Faerbt den Weg, zerstoert dabei erreichte gegnerische Fabriken und trennt Land ab.
    pub(crate) fn territory_apply_move(
        &mut self,
        actor: PlayerId,
        plan: &MovePlan,
        events: &mut Vec<Event>,
    ) {
        if plan.connected {
            let mut changed = Vec::new();
            for &cell in &plan.path {
                if self.state.land[cell.idx()] == Some(actor) {
                    continue;
                }
                self.paint(cell, actor, events);
                changed.push(cell);
            }
            if !changed.is_empty() {
                events.push(Event::Painted {
                    player: actor,
                    cells: changed,
                });
            }
            self.cut_off(actor, events);
        }
        // Ist das Spiel mit diesem Zug entschieden (letzter Koenig der Gegner gefallen), bleibt das
        // Brett so, wie es ist: das Land des Verlierers ist weiss und wird nicht mehr verteilt.
        if self.alive_players().len() > 1 {
            self.territory_settle(actor, events);
        }
    }

    /// Aendert den Besitzer eines Feldes. Wechselt er, faengt die Reifezeit des Feldes neu an.
    pub(crate) fn set_land(&mut self, cell: CellId, owner: Option<PlayerId>) {
        if self.state.land[cell.idx()] != owner {
            // Ein Fort steht nur auf eigenem Land: faellt das Feld, ist es zerstoert.
            for (i, fort) in self.state.forts.iter_mut().enumerate() {
                if *fort == Some(cell) && owner != Some(PlayerId(i as u8)) {
                    *fort = None;
                }
            }
            self.state.land[cell.idx()] = owner;
            self.state.owned_since[cell.idx()] = self.state.round;
            self.state.age[cell.idx()] = 0;
        }
    }

    /// Setzt die Farbe eines Feldes; steht dort eine gegnerische Fabrik, ist sie zerstoert.
    fn paint(&mut self, cell: CellId, player: PlayerId, events: &mut Vec<Event>) {
        self.set_land(cell, Some(player));
        if let Some(fid) = self.state.factory_at[cell.idx()] {
            if self.state.factories[&fid].owner != player {
                self.destroy_factory(fid, player, events);
            }
        }
    }

    /// Folgen eines Zuges, die sich gegenseitig anstossen koennen: Umschliessen faerbt neutrale
    /// Flaechen, Wiederanbinden faerbt Felder unter abgetrennten Figuren (und kann dabei fremdes
    /// Land durchtrennen). Wiederholt, bis nichts mehr passiert; jede Figur wird je Zug hoechstens
    /// einmal wieder angebunden, damit es sicher endet.
    pub(crate) fn territory_settle(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let mut done: Vec<PieceId> = Vec::new();
        let limit = self.map.board.len() + self.state.pieces.len() + 4;
        for _ in 0..limit {
            let mut changed = self.config.terr_enclose && self.enclose_neutral(events);
            if self.config.terr_reconnect {
                changed |= self.reconnect_one(actor, &mut done, events);
            }
            if !changed {
                return;
            }
        }
    }

    /// Neutrale Flaechen, die nur an eine einzige Farbe grenzen, gehoeren jetzt dieser Farbe.
    fn enclose_neutral(&mut self, events: &mut Vec<Event>) -> bool {
        let board = &self.map.board;
        let n = board.len();
        let mut seen = vec![false; n];
        let mut found: Vec<(PlayerId, Vec<CellId>)> = Vec::new();
        // Randfelder haben weniger Nachbarn als ein Feld mitten im Brett.
        let full = board
            .cells()
            .map(|c| board.neighbors(c).count())
            .max()
            .unwrap_or(0);
        let edge_is_wall = self.config.terr_enclose_edge;
        for start in board.cells() {
            if seen[start.idx()] || self.state.land[start.idx()].is_some() {
                continue;
            }
            let mut region = vec![start];
            seen[start.idx()] = true;
            let mut owner: Option<PlayerId> = None;
            let mut mixed = false;
            let mut head = 0;
            while head < region.len() {
                let cur = region[head];
                head += 1;
                if !edge_is_wall && board.neighbors(cur).count() < full {
                    mixed = true;
                }
                for nb in board.neighbors(cur) {
                    match self.state.land[nb.idx()] {
                        None => {
                            if !seen[nb.idx()] {
                                seen[nb.idx()] = true;
                                region.push(nb);
                            }
                        }
                        Some(p) => match owner {
                            None => owner = Some(p),
                            Some(o) if o != p => mixed = true,
                            _ => {}
                        },
                    }
                }
            }
            if let (Some(p), false) = (owner, mixed) {
                found.push((p, region));
            }
        }
        let any = !found.is_empty();
        for (player, mut cells) in found {
            cells.sort();
            for c in &cells {
                self.set_land(*c, Some(player));
            }
            events.push(Event::Enclosed { player, cells });
        }
        any
    }

    /// Findet eine abgetrennte Figur, die an das verbundene Land ihres Besitzers grenzt, und
    /// faerbt ihr Feld. Der Ziehende kommt zuerst, sonst die kleinere Figurennummer.
    fn reconnect_one(
        &mut self,
        actor: PlayerId,
        done: &mut Vec<PieceId>,
        events: &mut Vec<Event>,
    ) -> bool {
        let mut ids: Vec<(bool, PieceId)> = self
            .state
            .pieces
            .values()
            .map(|p| (p.owner != actor, p.id))
            .collect();
        ids.sort();
        for (_, id) in ids {
            if done.contains(&id) {
                continue;
            }
            let (owner, cell) = {
                let p = &self.state.pieces[&id];
                (p.owner, p.cell)
            };
            let Some(connected) = self.connected_land(owner) else {
                continue;
            };
            if connected[cell.idx()]
                || !self.map.board.neighbors(cell).any(|nb| connected[nb.idx()])
            {
                continue;
            }
            done.push(id);
            self.paint(cell, owner, events);
            events.push(Event::Reconnected {
                piece: id,
                owner,
                cell,
            });
            self.cut_off(owner, events);
            return true;
        }
        false
    }

    /// Fabrik zerstoert: Lager, Flaeche und gelagerte Karte sind verloren.
    fn destroy_factory(&mut self, fid: FactoryId, by: PlayerId, events: &mut Vec<Event>) {
        let factory = self.state.remove_factory(fid).expect("Fabrik existiert");
        self.state.ledger.vanished += factory.storage.total();
        self.state.stored_card[factory.owner.idx()] = None;
        self.state.area[factory.owner.idx()] = 0;
        self.state.area_rest[factory.owner.idx()] = 0;
        self.state.lost_factory[factory.owner.idx()] = Some(factory.cell);
        events.push(Event::FactoryDestroyed {
            factory: factory.id,
            owner: factory.owner,
            cell: factory.cell,
            by,
            loot: Inventory::new(),
        });
    }

    /// Land, das nicht mehr mit der Fabrik seines Besitzers zusammenhaengt, geht an `actor`
    /// (oder wird neutral).
    pub(crate) fn cut_off(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        for p in 0..self.config.players {
            let player = PlayerId(p);
            if player == actor || !self.state.alive[player.idx()] || self.rebuilding(player) {
                continue;
            }
            let Some(connected) = self.connected_land(player) else {
                continue;
            };
            let cells: Vec<CellId> = self
                .map
                .board
                .cells()
                .filter(|c| self.state.land[c.idx()] == Some(player) && !connected[c.idx()])
                .collect();
            if cells.is_empty() {
                continue;
            }
            let to = self.config.terr_cut_flips.then_some(actor);
            for c in &cells {
                self.set_land(*c, to);
            }
            events.push(Event::CutOff {
                from: player,
                to,
                cells,
            });
        }
    }

    /// Land eines ausgeschiedenen Spielers wird neutral.
    pub(crate) fn territory_forget(&mut self, player: PlayerId) {
        for cell in self.map.board.cells() {
            if self.state.land[cell.idx()] == Some(player) {
                self.set_land(cell, None);
            }
        }
        self.state.area[player.idx()] = 0;
        self.state.area_rest[player.idx()] = 0;
        self.state.stored_card[player.idx()] = None;
    }

    /// Wie viele Runden das Feld seinem Besitzer noch gehoeren muss, bevor das Vorkommen `res`
    /// Ertrag gibt (0 = reif). Die Reifezeit steht in `Config::terr_ripen`, je Kategorie.
    pub fn ripen_wait(&self, cell: CellId, res: crate::ids::ResourceId) -> u32 {
        let cat = usize::from(self.map.category(res));
        let need = self
            .config
            .terr_ripen
            .get(cat.wrapping_sub(1))
            .copied()
            .unwrap_or(0);
        let held = self
            .state
            .round
            .saturating_sub(self.state.owned_since[cell.idx()]);
        need.saturating_sub(held)
    }

    /// Version 3: Erntezeit in Runden (`terr_ripen` der Kategorie), 0 = jede Runde.
    pub fn harvest_period(&self, res: crate::ids::ResourceId) -> u32 {
        let cat = usize::from(self.map.category(res));
        // Mit Brot-Lager (`terr_bread`) hat Kat 3 seinen eigenen Takt.
        if cat == 3 && self.config.terr_bread > 0 {
            return self.config.terr_bread;
        }
        self.config
            .terr_ripen
            .get(cat.wrapping_sub(1))
            .copied()
            .unwrap_or(0)
    }

    /// Version 3.1 (`terr_yield_lvl`): so oft zaehlt eine Ernte auf diesem Feld. Steht eine eigene
    /// Figur der Stufe `n` darauf, erntet sie `n` Stueck; der Koenig und leere Felder zaehlen 1.
    pub fn yield_multiplier(&self, owner: PlayerId, cell: CellId) -> u32 {
        if !self.config.terr_yield_lvl {
            return 1;
        }
        self.state.occupant[cell.idx()]
            .and_then(|id| self.state.pieces.get(&id))
            .filter(|p| p.owner == owner)
            .map_or(1, |p| u32::from(p.level()).max(1))
    }

    /// Version 3: ist das Vorkommen nach dem jetzigen Stand des Erntezaehlers dran?
    fn harvest_due(&self, cell: CellId, res: crate::ids::ResourceId) -> bool {
        match self.harvest_period(res) {
            0 => true,
            p => self.state.age[cell.idx()].is_multiple_of(p),
        }
    }

    /// Version 3: wie viele Ernten das Vorkommen noch bis zur naechsten Ernte braucht (1 = die
    /// naechste Runde mit stehender Fabrik). Ein frisch geernteter oder umgefaerbter Zaehler steht
    /// wieder auf der vollen Zeit.
    pub fn harvest_in(&self, cell: CellId, res: crate::ids::ResourceId) -> u32 {
        match self.harvest_period(res) {
            0 => 1,
            p => p - self.state.age[cell.idx()] % p,
        }
    }

    /// Ende des eigenen Zuges: die Fabrik sammelt die verbundenen Felder ein.
    pub(crate) fn territory_income(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let Some(fid) = self.home_factory(actor).map(|f| f.id) else {
            return;
        };
        // Version 3: eine Fabrik im Bau zaehlt ihre Runden herunter und sammelt nichts.
        if self.rebuilding(actor) {
            let f = self
                .state
                .factories
                .get_mut(&fid)
                .expect("Fabrik existiert");
            f.building -= 1;
            if f.building == 0 {
                events.push(Event::FactoryStands {
                    factory: fid,
                    owner: actor,
                    cell: f.cell,
                });
                // Das Land ist wieder verbunden: wartende Figuren binden sich sofort wieder an.
                if self.alive_players().len() > 1 {
                    self.territory_settle(actor, events);
                }
            }
            return;
        }
        let Some(connected) = self.connected_land(actor) else {
            return;
        };
        let yield_n = self.config.terr_yield;
        let cycle = self.config.terr_cycle;
        let bonus_on = self.config.kat3_bonus_on();
        // Mit `terr_occupy` zaehlt und erntet ein Feld nur, solange es bearbeitet wird (eigene Figur
        // darauf, mit `terr_control` auch im Einflussbereich einer eigenen Figur).
        let worked_cells = self.config.terr_occupy.then(|| self.worked_cells(actor));
        let mut gained = Inventory::new();
        let mut area = 0u32;
        // Mit Gelaende zaehlt jedes Feld nach seinem Flaechenwert in Vierteln (Feld 4, Erde 2, Moor 1).
        let terrain_on = self.config.terr_terrain;
        let mut quarters = 0u32;
        let mut kat3 = 0u32;
        for cell in self.map.board.cells() {
            if !connected[cell.idx()] {
                continue;
            }
            if terrain_on {
                quarters += self.map.terrain_at(cell).quarters();
            } else {
                area += 1;
            }
            let worked = worked_cells.as_ref().is_none_or(|w| w[cell.idx()]);
            if cycle && worked {
                self.state.age[cell.idx()] += 1;
            }
            for res in self.all_deposits(cell) {
                // Mit Flaechenbonus wird Kat 3 nicht geerntet, sondern nur gezaehlt.
                if bonus_on && self.map.category(res) == 3 {
                    kat3 += 1;
                    continue;
                }
                let ready = if cycle {
                    worked && self.harvest_due(cell, res)
                } else {
                    self.ripen_wait(cell, res) == 0
                };
                if ready {
                    let mult = self.yield_multiplier(actor, cell);
                    gained.add(res, yield_n * mult + self.harvest_bonus(actor, res));
                }
            }
        }
        if terrain_on {
            let total = quarters + u32::from(self.state.area_rest[actor.idx()]);
            area = total / 4;
            self.state.area_rest[actor.idx()] = (total % 4) as u8;
        }
        let bonus = Game::kat3_bonus(kat3);
        self.state.area[actor.idx()] += area + bonus;
        let upkeep = self.territory_upkeep(actor);
        self.state.ledger.created += gained.total();
        let mut copy = gained.clone();
        self.state
            .factories
            .get_mut(&fid)
            .expect("Fabrik existiert")
            .storage
            .absorb(&mut copy);
        events.push(Event::Income {
            player: actor,
            area,
            kat3,
            bonus,
            gained,
        });
        if let Some(upkeep) = upkeep.filter(|&u| u > 0) {
            self.pay_upkeep(actor, upkeep, events);
        }
    }

    /// Version 3 (`terr_upkeep`): Unterhalt aller Figuren des Spielers (Summe der Stufen, der
    /// Koenig zahlt nichts). `None`, wenn die Regel aus ist.
    pub fn territory_upkeep(&self, player: PlayerId) -> Option<u32> {
        self.config.terr_upkeep.then(|| {
            if self.config.terr_upkeep_cnt > 0 {
                let n = self.pieces_of(player).filter(|p| !p.is_king()).count() as u32;
                return Game::count_upkeep(self.upkeep_k(), n);
            }
            let sq = self.config.terr_upkeep_sq;
            self.pieces_of(player).map(|p| p.upkeep(sq)).sum()
        })
    }

    /// Version 3.10 (`terr_famine`): ist die laufende Runde eine Hungersnot-Runde?
    pub fn famine_round(&self) -> bool {
        self.config.terr_famine > 0 && self.state.round.is_multiple_of(self.config.terr_famine)
    }

    /// Das k des Unterhalts nach Anzahl in dieser Runde: in einer Hungersnot `terr_famine_k`, sonst
    /// `terr_upkeep_cnt`.
    pub fn upkeep_k(&self) -> u32 {
        if self.famine_round() {
            self.config.terr_famine_k
        } else {
            self.config.terr_upkeep_cnt
        }
    }

    /// Version 3.9 (`terr_upkeep_cnt = k`): Unterhalt von `n` Figuren, `(k * n)^2`.
    pub fn count_upkeep(k: u32, n: u32) -> u32 {
        (k * n) * (k * n)
    }

    /// Zieht den Unterhalt von der Flaeche ab. Reicht sie nicht, loest sich Figur um Figur auf, bis
    /// sie reicht (`upkeep_victim`): zuerst die kleinste Figur, die den fehlenden Betrag allein deckt,
    /// gibt es keine, die groesste (bei gleicher Stufe die zuletzt gebaute); bei `terr_upkeep_cnt` die
    /// schwaechste. Jede aufgeloeste Figur spart ihren Unterhalt, es wird nichts zurueckerstattet.
    /// Version 3.9 (`terr_upkeep_pick`): waehlt der Spieler selbst, wartet das Spiel stattdessen auf
    /// `Action::Dissolve` (`State::dissolve_due`).
    fn pay_upkeep(&mut self, actor: PlayerId, upkeep: u32, events: &mut Vec<Event>) {
        if upkeep > self.state.area[actor.idx()]
            && self.config.terr_upkeep_pick >> actor.idx() & 1 == 1
            && self.pieces_of(actor).any(|p| !p.is_king())
        {
            self.state.dissolve_due = true;
            events.push(Event::DissolveDue {
                player: actor,
                upkeep,
                area: self.state.area[actor.idx()],
            });
            return;
        }
        self.settle_upkeep(actor, events);
    }

    /// Loest automatisch auf, was die Flaeche nicht traegt, und zahlt den Rest.
    fn settle_upkeep(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let have = self.state.area[actor.idx()];
        let mut upkeep = self.territory_upkeep(actor).unwrap_or(0);
        while upkeep > have {
            let Some(id) = self.upkeep_victim(actor, upkeep - have) else {
                break;
            };
            self.dissolve_piece(actor, id, events);
            upkeep = self.territory_upkeep(actor).unwrap_or(0);
        }
        self.state.dissolve_due = false;
        self.state.area[actor.idx()] = have - upkeep.min(have);
        events.push(Event::Upkeep {
            player: actor,
            paid: upkeep,
        });
    }

    fn dissolve_piece(&mut self, actor: PlayerId, id: PieceId, events: &mut Vec<Event>) {
        let (cell, level) = {
            let p = &self.state.pieces[&id];
            (p.cell, p.level())
        };
        self.state.remove_piece(id);
        events.push(Event::Dissolved {
            piece: id,
            owner: actor,
            cell,
            level,
        });
    }

    /// Die Figur, die sich als naechste aufloest, wenn der Spieler nicht waehlt: ohne
    /// `terr_upkeep_cnt` die kleinste, die den fehlenden Betrag `missing` allein deckt, sonst die
    /// groesste; mit `terr_upkeep_cnt` die schwaechste. Bei gleicher Stufe die zuletzt gebaute.
    pub fn upkeep_victim(&self, actor: PlayerId, missing: u32) -> Option<PieceId> {
        if self.config.terr_upkeep_cnt > 0 {
            return self
                .pieces_of(actor)
                .filter(|p| !p.is_king())
                .map(|p| (p.level(), std::cmp::Reverse(p.id)))
                .min()
                .map(|(_, std::cmp::Reverse(id))| id);
        }
        let sq = self.config.terr_upkeep_sq;
        let mut units: Vec<(u32, PieceId)> = self
            .pieces_of(actor)
            .filter(|p| p.upkeep(sq) > 0)
            .map(|p| (p.upkeep(sq), p.id))
            .collect();
        units.sort_by_key(|&(cost, id)| (cost, std::cmp::Reverse(id)));
        let covering = units.iter().find(|&&(cost, _)| cost >= missing);
        covering.or(units.last()).map(|&(_, id)| id)
    }

    /// Version 3.9: die Figuren, unter denen der Spieler waehlen darf (alle ausser dem Koenig).
    pub(crate) fn dissolve_actions(&self, me: PlayerId) -> Vec<Action> {
        self.pieces_of(me)
            .filter(|p| !p.is_king())
            .map(|p| Action::Dissolve { piece: p.id })
            .collect()
    }

    /// Version 3.9: die gewaehlte Figur loest sich auf; reicht die Flaeche jetzt, wird der Rest gezahlt
    /// und der Zug ist zu Ende, sonst muss noch eine Figur gewaehlt werden.
    pub(crate) fn do_dissolve(&mut self, actor: PlayerId, piece: PieceId, events: &mut Vec<Event>) {
        self.dissolve_piece(actor, piece, events);
        let upkeep = self.territory_upkeep(actor).unwrap_or(0);
        if upkeep <= self.state.area[actor.idx()] || !self.pieces_of(actor).any(|p| !p.is_king()) {
            self.settle_upkeep(actor, events);
        }
    }

    /// Version 3.9: der Spielzug, den ein Bot im Wartezustand `dissolve_due` macht (ein Bot hat keine
    /// Wahl und loest automatisch auf, wie ohne `terr_upkeep_pick`).
    pub fn auto_dissolve_action(&self) -> Option<Action> {
        if !self.state.dissolve_due {
            return None;
        }
        let actor = self.state.current;
        let upkeep = self.territory_upkeep(actor).unwrap_or(0);
        let missing = upkeep.saturating_sub(self.state.area[actor.idx()]).max(1);
        self.upkeep_victim(actor, missing)
            .map(|piece| Action::Dissolve { piece })
    }

    /// Felder, die `player` am Ende seines Zuges bearbeitet (Voraussetzung fuer Erntezaehler und
    /// Ernte mit `terr_occupy`): das Feld jeder eigenen Figur; mit `terr_control` dazu alle Felder
    /// im Einflussbereich einer eigenen Figur auf verbundenem Land. Der Einflussbereich einer
    /// Einheit sind die Felder geradeaus bis zur Reichweite, bis zur ersten Figur (diese
    /// eingeschlossen; mit Sprungfeder eine Figur weiter), der des Koenigs seine Nachbarfelder.
    pub fn worked_cells(&self, player: PlayerId) -> Vec<bool> {
        let board = &self.map.board;
        let mut out = vec![false; board.len()];
        let connected = if self.config.terr_control {
            self.connected_land(player)
        } else {
            None
        };
        for piece in self.pieces_of(player) {
            out[piece.cell.idx()] = true;
            if !self.config.terr_control {
                continue;
            }
            // Wehrlose Figuren (nicht auf verbundenem Land) beherrschen nichts.
            if connected.as_ref().is_some_and(|c| !c[piece.cell.idx()]) {
                continue;
            }
            match &piece.kind {
                PieceKind::King => {
                    for n in board.neighbors(piece.cell) {
                        out[n.idx()] = true;
                    }
                }
                PieceKind::Unit { range, .. } => {
                    for dir in 0..board.dir_count() {
                        let mut hops = piece.perks.hop;
                        for cell in board.line(piece.cell, dir, *range as usize) {
                            out[cell.idx()] = true;
                            if self.state.piece_at(cell).is_some() {
                                if hops > 0 {
                                    hops -= 1;
                                } else {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Eine Figur der Stufe `level` auf dem Fabrikfeld bauen; eine gelagerte Karte wirkt auf sie.
    pub(crate) fn do_build_unit(
        &mut self,
        actor: PlayerId,
        fid: FactoryId,
        level: u8,
        events: &mut Vec<Event>,
    ) {
        let price = self.unit_price(level);
        let map = std::sync::Arc::clone(&self.map);
        self.state.area[actor.idx()] -= price.area;
        let factory = self
            .state
            .factories
            .get_mut(&fid)
            .expect("Fabrik existiert");
        let mut spent = 0u64;
        for (i, n) in [price.k1, price.k2, price.k3].into_iter().enumerate() {
            spent += take_category(&mut factory.storage, &map, i as u8 + 1, n).total();
        }
        let cell = factory.cell;
        self.state.ledger.consumed_by_spawns += spent;
        let piece = self.state.insert_piece(
            actor,
            cell,
            PieceKind::Unit {
                level,
                range: level,
                colors: vec![self.map.own_color(actor)],
            },
        );
        events.push(Event::Spawned {
            piece,
            owner: actor,
            factory: fid,
            cell,
            level,
            range: level,
        });
        // "Fabrik ruecken" bleibt liegen, bis sie benutzt wird; alle anderen Karten wirken auf
        // die neue Figur.
        let unit_card = self.state.stored_card[actor.idx()].filter(|&c| c != Card::FactoryShift);
        if let Some(card) = unit_card {
            self.state.stored_card[actor.idx()] = None;
            self.apply_card(
                actor,
                card,
                PaySource::Factory(fid),
                Some(piece),
                None,
                events,
            );
        }
    }

    /// Karte kaufen: bezahlt wird nach Kategorie aus dem Lager, die Karte liegt neben der Fabrik.
    pub(crate) fn do_buy_territory(
        &mut self,
        actor: PlayerId,
        card: Card,
        from: PaySource,
        target: Option<PieceId>,
        events: &mut Vec<Event>,
    ) {
        let paid = self.territory_card_payment(actor, card).expect("bezahlbar");
        let PaySource::Factory(fid) = from else {
            unreachable!("im Gebietsmodus wird aus der Fabrik bezahlt")
        };
        let storage = &mut self
            .state
            .factories
            .get_mut(&fid)
            .expect("Fabrik existiert")
            .storage;
        for (res, n) in paid.iter() {
            assert!(storage.take(res, n), "Bezahlung geplant, aber nicht da");
        }
        self.state.ledger.spent_on_cards += paid.total();
        self.state.bought_this_turn = true;
        self.state.player_perks[actor.idx()].cards_bought += 1;
        let on_piece = self.cards_go_on_pieces(card);
        if card == Card::KingBoost {
            self.apply_card(actor, card, from, None, None, events);
        } else if self.instant_card(card) {
            // Version 3: Sturmangriff, Bergbau, Schmiede, Doppelzug wirken sofort.
            for (part, _) in self.config.card_tweaks.parts(card) {
                if self.instant_has_room(actor, part) {
                    self.apply_card(actor, part, from, None, None, events);
                }
            }
        } else if on_piece {
            // Version 3: die Karte wirkt sofort auf die gewaehlte Einheit.
            self.apply_card(actor, card, from, target, None, events);
        } else {
            self.state.stored_card[actor.idx()] = Some(card);
        }
        // Version 3 (`terr_cardlock = 1`): die verbesserte Figur darf in diesem Zug nicht ziehen.
        if self.config.terr_cardlock == 1 {
            let upgraded = if card == Card::KingBoost {
                self.king(actor).map(|k| k.id)
            } else if on_piece {
                target
            } else {
                None
            };
            if let Some(piece) = upgraded {
                // Mit Mehrfachkauf ruhen auch die Figuren, die vorher eine Karte bekamen.
                if let Some(before) = self.state.card_lock.filter(|&b| b != piece) {
                    if !self.state.card_locks.contains(&before) {
                        self.state.card_locks.push(before);
                    }
                }
                self.state.card_lock = Some(piece);
            }
        }
        events.push(Event::CardBought {
            player: actor,
            card,
            from,
            target: if on_piece { target } else { None },
            paid,
        });
    }

    /// Wertung im Gebietsmodus: eigene Felder plus Summe der Figurenstufen.
    pub fn territory_score(&self, player: PlayerId) -> u32 {
        let levels: u32 = self
            .pieces_of(player)
            .map(|p| match &p.kind {
                PieceKind::Unit { level, .. } => u32::from(*level),
                PieceKind::King => 0,
            })
            .sum();
        self.land_count(player) + levels
    }

    /// Zusatzpruefungen der Invarianten im Gebietsmodus.
    pub(crate) fn territory_check_invariants(&self) -> Result<(), String> {
        let st = &self.state;
        if st.land.len() != self.map.board.len() || st.owned_since.len() != st.land.len() {
            return Err("land hat falsche Laenge".into());
        }
        if self.config.terr_cardany
            && st
                .stored_card
                .iter()
                .flatten()
                .any(|&c| self.cards_go_on_pieces(c))
        {
            return Err("eine Figurenkarte liegt neben der Fabrik, sie wirkt sofort".into());
        }
        for (i, fort) in st.forts.iter().enumerate() {
            if let Some(cell) = fort {
                if st.land[cell.idx()] != Some(PlayerId(i as u8)) {
                    return Err(format!(
                        "das Fort von Spieler {i} steht nicht auf seinem Land"
                    ));
                }
            }
        }
        if st.stored_card.contains(&Some(Card::KingBoost)) {
            return Err("der Koenigs-Boost liegt nicht neben der Fabrik, er wirkt sofort".into());
        }
        if st
            .player_perks
            .iter()
            .any(|pp| pp.king_boost > crate::state::MAX_KING_BOOST)
        {
            return Err("Koenigs-Boost ueber der hoechsten Stufe".into());
        }
        for p in 0..self.config.players {
            let player = PlayerId(p);
            let n = self.factories_of(player).count();
            if n > 1 {
                return Err(format!("{player:?} hat {n} Fabriken"));
            }
            if !st.alive[p as usize] {
                if st.land.contains(&Some(player)) {
                    return Err(format!("{player:?} ist ausgeschieden, hat aber noch Land"));
                }
                continue;
            }
            if n == 0 && st.stored_card[p as usize].is_some() {
                return Err(format!("{player:?}: Karte ohne Fabrik"));
            }
        }
        for f in st.factories.values() {
            if st.land[f.cell.idx()] != Some(f.owner) {
                return Err(format!("{:?} steht auf fremdem oder neutralem Land", f.id));
            }
            if f.building > 0 {
                if self.config.terr_move == 0 {
                    return Err(format!("{:?} wird gebaut, ohne Umzugsregel", f.id));
                }
                if self.king(f.owner).map(|k| k.cell) != Some(f.cell) {
                    return Err(format!(
                        "{:?} im Bau, aber der Koenig steht nicht auf dem Bauplatz",
                        f.id
                    ));
                }
            }
        }
        if !st.loot.is_empty() || st.carried.iter().any(|c| !c.is_empty()) {
            return Err("Beute oder Gepaeck im Gebietsmodus".into());
        }
        // Im entschiedenen Spiel bleibt das weisse Land des Verlierers, wie es ist.
        let decided = self.alive_players().len() <= 1;
        if self.config.terr_enclose && !decided {
            let mut probe = self.clone();
            let mut events = Vec::new();
            if probe.enclose_neutral(&mut events) {
                return Err(format!(
                    "umschlossene neutrale Flaeche uebersehen: {events:?}"
                ));
            }
        }
        if self.config.terr_reconnect && !decided {
            for piece in st.pieces.values() {
                let Some(connected) = self.connected_land(piece.owner) else {
                    continue;
                };
                if !connected[piece.cell.idx()]
                    && self
                        .map
                        .board
                        .neighbors(piece.cell)
                        .any(|nb| connected[nb.idx()])
                {
                    return Err(format!(
                        "{:?} grenzt an verbundenes Land, ist aber nicht angebunden",
                        piece.id
                    ));
                }
            }
        }
        Ok(())
    }
}
