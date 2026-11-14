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
//!   Wer keine Fabrik mehr hat, braucht keinen Zusammenhang mehr (und bekommt auch keinen Ertrag).
//! - **Wieder anbinden.** Grenzt eine abgetrennte Figur (ueber eine Kante) an das verbundene Land
//!   ihres Besitzers, ist sie sofort wieder aktiv und faerbt das Feld, auf dem sie steht
//!   (`terr_reconnect`). Das gilt nach jedem Zug fuer alle Figuren, nicht nur fuer die des Ziehenden.
//! - **Umschliessen.** Eine neutrale Flaeche (zusammenhaengende neutrale Felder), deren saemtliche
//!   Nachbarfelder einer einzigen Farbe gehoeren, faellt ganz an diese Farbe (`terr_enclose`). Der
//!   Brettrand zaehlt nicht als Nachbar, schliesst also mit ein.
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
use crate::ids::{CellId, FactoryId, PieceId, PlayerId};
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

    /// Felder, die ueber eigene Felder mit der Fabrik des Spielers zusammenhaengen. `None`, wenn
    /// er keine Fabrik hat: dann gibt es keinen Zusammenhang, auf den es ankaeme.
    pub fn connected_land(&self, player: PlayerId) -> Option<Vec<bool>> {
        let factory = self.home_factory(player)?;
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

    /// Zahl der Felder, die mit der Fabrik zusammenhaengen (ohne Fabrik: alle eigenen Felder).
    pub fn connected_count(&self, player: PlayerId) -> u32 {
        match self.connected_land(player) {
            Some(c) => c.iter().filter(|&&b| b).count() as u32,
            None => self.land_count(player),
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

    /// Steht die Figur auf verbundenem Land? Ohne Fabrik gilt das immer.
    pub fn piece_connected(&self, piece: &Piece) -> bool {
        match self.connected_land(piece.owner) {
            None => true,
            Some(c) => c[piece.cell.idx()],
        }
    }

    /// Hoechste Stufe, die man bauen kann.
    pub fn max_unit_level(&self) -> u8 {
        self.config.corner_count().min(4) as u8
    }

    pub fn unit_price(&self, level: u8) -> UnitPrice {
        let c = &self.config;
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
        let Some(f) = self.home_factory(player) else {
            return false;
        };
        let price = self.unit_price(level);
        self.state.area[player.idx()] >= price.area
            && self.category_units(&f.storage, 1) >= price.k1
            && self.category_units(&f.storage, 2) >= price.k2
            && self.category_units(&f.storage, 3) >= price.k3
    }

    /// Kartenpreis nach Kategorien (Kat 1, 2, 3): die alten Preise, Kat 2 und 3 mal `terr_card_mult`.
    pub fn territory_card_price(&self, card: Card) -> [u32; 3] {
        let old = card.cost();
        let m = self.config.terr_card_mult;
        [
            (old.color + old.foreign_color) as u32 * m,
            old.k2 as u32 * m,
            old.k3 as u32 * m,
        ]
    }

    /// Was `player` fuer `card` aus dem Lager seiner Fabrik zahlen wuerde.
    pub fn territory_card_payment(&self, player: PlayerId, card: Card) -> Option<Inventory> {
        let f = self.home_factory(player)?;
        plan_by_category(&f.storage, &self.map, self.territory_card_price(card))
    }

    pub(crate) fn territory_build_actions(&self, me: PlayerId) -> Vec<Action> {
        let Some(f) = self.home_factory(me) else {
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

    pub(crate) fn territory_buy_actions(&self, me: PlayerId) -> Vec<Action> {
        let mut out = Vec::new();
        if self.state.bought_this_turn
            || self.state.market.is_empty()
            || self.state.stored_card[me.idx()].is_some()
        {
            return out;
        }
        let Some(f) = self.home_factory(me) else {
            return out;
        };
        for &card in &self.state.market {
            if self.territory_card_payment(me, card).is_some() {
                out.push(Action::BuyCard {
                    card,
                    from: PaySource::Factory(f.id),
                    target: None,
                });
            }
        }
        out
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

    /// Spruenge (Pferd, Sprungfeder, Lanze, Laeuferschlag) faerben eine Bruecke: ein Feld, das
    /// an Start und Ziel grenzt, und das Ziel. Sonst landete die Figur auf einem Inselfeld und
    /// waere sofort wehrlos. Unter mehreren Bruecken gewinnt die mit mehr umzufaerbenden Feldern,
    /// dann die mit der kleineren Nummer.
    fn jump_path(&self, owner: PlayerId, from: CellId, to: CellId) -> Vec<CellId> {
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
        self.territory_settle(actor, events);
    }

    /// Setzt die Farbe eines Feldes; steht dort eine gegnerische Fabrik, ist sie zerstoert.
    fn paint(&mut self, cell: CellId, player: PlayerId, events: &mut Vec<Event>) {
        self.state.land[cell.idx()] = Some(player);
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
    fn territory_settle(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
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
                self.state.land[c.idx()] = Some(player);
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
    fn cut_off(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        for p in 0..self.config.players {
            let player = PlayerId(p);
            if player == actor || !self.state.alive[player.idx()] {
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
                self.state.land[c.idx()] = to;
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
        for owner in self.state.land.iter_mut() {
            if *owner == Some(player) {
                *owner = None;
            }
        }
        self.state.area[player.idx()] = 0;
        self.state.stored_card[player.idx()] = None;
    }

    /// Ende des eigenen Zuges: die Fabrik sammelt die verbundenen Felder ein.
    pub(crate) fn territory_income(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let Some(connected) = self.connected_land(actor) else {
            return;
        };
        let Some(fid) = self.home_factory(actor).map(|f| f.id) else {
            return;
        };
        let yield_n = self.config.terr_yield;
        let mut gained = Inventory::new();
        let mut area = 0u32;
        for cell in self.map.board.cells() {
            if !connected[cell.idx()] {
                continue;
            }
            area += 1;
            for res in self.all_deposits(cell) {
                gained.add(res, yield_n);
            }
        }
        self.state.area[actor.idx()] += area;
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
            gained,
        });
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
        if let Some(card) = self.state.stored_card[actor.idx()].take() {
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
        self.state.stored_card[actor.idx()] = Some(card);
        events.push(Event::CardBought {
            player: actor,
            card,
            from,
            target: None,
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
        if st.land.len() != self.map.board.len() {
            return Err("land hat falsche Laenge".into());
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
        }
        if !st.loot.is_empty() || st.carried.iter().any(|c| !c.is_empty()) {
            return Err("Beute oder Gepaeck im Gebietsmodus".into());
        }
        if self.config.terr_enclose {
            let mut probe = self.clone();
            let mut events = Vec::new();
            if probe.enclose_neutral(&mut events) {
                return Err(format!(
                    "umschlossene neutrale Flaeche uebersehen: {events:?}"
                ));
            }
        }
        if self.config.terr_reconnect {
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
