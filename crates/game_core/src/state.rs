//! Spielzustand: Figuren, Fabriken, Beute, Lager, Buchfuehrung.

use crate::cards::Card;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use std::collections::{BTreeMap, BTreeSet};

/// Rohstoffmengen nach Typ. Eintraege mit Menge 0 gibt es nicht.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inventory(BTreeMap<ResourceId, u32>);

impl Inventory {
    pub fn new() -> Self {
        Inventory(BTreeMap::new())
    }

    pub fn add(&mut self, res: ResourceId, amount: u32) {
        if amount > 0 {
            *self.0.entry(res).or_insert(0) += amount;
        }
    }

    pub fn get(&self, res: ResourceId) -> u32 {
        self.0.get(&res).copied().unwrap_or(0)
    }

    /// Entfernt `amount`, falls genug da ist. Sonst bleibt alles unveraendert und es ist `false`.
    pub fn take(&mut self, res: ResourceId, amount: u32) -> bool {
        let have = self.get(res);
        if have < amount {
            return false;
        }
        if have == amount {
            self.0.remove(&res);
        } else {
            self.0.insert(res, have - amount);
        }
        true
    }

    pub fn total(&self) -> u64 {
        self.0.values().map(|&n| n as u64).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ResourceId, u32)> + '_ {
        self.0.iter().map(|(&r, &n)| (r, n))
    }

    pub fn kinds(&self) -> impl Iterator<Item = ResourceId> + '_ {
        self.0.keys().copied()
    }

    /// Verschiebt alles aus `other` hierher; `other` ist danach leer.
    pub fn absorb(&mut self, other: &mut Inventory) {
        for (res, n) in std::mem::take(&mut other.0) {
            self.add(res, n);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PieceKind {
    King,
    Unit {
        /// Stufe = Zahl verschiedener Kat-1-Typen im Lager der aufgeloesten Fabrik.
        level: u8,
        /// Reichweite = hoechste Rohstoffkategorie im Lager.
        range: u8,
        /// Die Kat-1-Typen ("Farben"), aus denen die Figur entstand.
        colors: Vec<ResourceId>,
    },
}

/// Dauerhafte Verbesserungen einer Figur durch Karten (nur Angriff, keine Verteidigung).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Perks {
    /// Schlaegt zusaetzlich diagonal auf ein Nachbarfeld (Laeuferschlag).
    pub diagonal: bool,
    /// Zusaetzliche Reichweite durch "Lange Beine". Es gilt: Reichweite = Stufe + diese Zahl.
    pub extra_range: u8,
    /// Stufen durch Karten (Ausbildung, Runensteine), zusaetzlich zu Farben und Ausbau.
    pub card_levels: u8,
    /// Schlaegt bis zu zwei Felder weit geradeaus (Lanze).
    pub lance: bool,
    /// Springt ueber eine Nachbarfigur auf das Feld dahinter (Sprungfeder).
    pub spring: bool,
    /// Springt auf jedes Feld im Abstand 2 (Pferd).
    pub horse: bool,
}

impl Perks {
    pub fn any(&self) -> bool {
        *self != Perks::default()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub id: PieceId,
    pub owner: PlayerId,
    pub cell: CellId,
    pub kind: PieceKind,
    pub perks: Perks,
}

/// Was ein Spieler durch Karten dauerhaft hat oder gesammelt hat.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerPerks {
    /// Bergbau: Fabriken ernten Kat 2 doppelt.
    pub mining: bool,
    /// Ackerbau: Fabriken ernten Kat 3 doppelt.
    pub farming: bool,
    /// Gekaufte Runensteine seit dem letzten vollen Set.
    pub runes: u8,
    /// Gekaufte Sternsplitter seit dem letzten vollen Set.
    pub shards: u8,
    /// Wie viele Karten der Spieler insgesamt gekauft hat (Statistik).
    pub cards_bought: u32,
}

impl Piece {
    pub fn is_king(&self) -> bool {
        matches!(self.kind, PieceKind::King)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Factory {
    pub id: FactoryId,
    pub owner: PlayerId,
    pub cell: CellId,
    pub storage: Inventory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Winner(PlayerId),
    Draw,
}

/// Rohstoff-Buchfuehrung. Es gilt immer:
/// `created == in Gepaeck + in Lagern + als Beute + consumed + vanished + spent_on_cards`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Aus Vorkommen gesammelte Einheiten (Koenig, Fabrik, beim Fabrikbau verbrauchtes Kat 1).
    pub created: u64,
    /// Beim Aufloesen einer Fabrik in eine Figur uebergegangen.
    pub consumed_by_spawns: u64,
    /// Mit einem ausgeschiedenen Spieler verschwunden (Gepaeck und Fabrikenlager).
    pub vanished: u64,
    /// Fuer Karten bezahlt.
    pub spent_on_cards: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub pieces: BTreeMap<PieceId, Piece>,
    pub factories: BTreeMap<FactoryId, Factory>,
    /// Beute auf Feldern (nur ein Koenig kann sie aufnehmen).
    pub loot: BTreeMap<CellId, Inventory>,
    /// Was der Koenig jedes Spielers traegt.
    pub carried: Vec<Inventory>,
    pub alive: Vec<bool>,
    pub current: PlayerId,
    /// Laufende Runde, ab 1. Eine Runde = jeder lebende Spieler war einmal dran.
    pub round: u32,
    pub turns_played: u32,
    pub outcome: Option<Outcome>,
    pub ledger: Ledger,
    /// Vorkommen, die es nicht mehr gibt (Fabrik darauf gebaut oder Vorrat erschoepft).
    pub depleted: BTreeSet<(CellId, ResourceId)>,
    /// Abgebaute Vorkommen, die nachwachsen: ab dieser Runde sind sie wieder da.
    pub regrow: BTreeMap<(CellId, ResourceId), u32>,
    /// Bereits geerntete Menge je Vorkommen (nur bei begrenztem Vorrat gefuehrt).
    pub harvested: BTreeMap<(CellId, ResourceId), u32>,
    /// Die Karten, die in dieser Partie ausliegen (fest ab dem Start).
    pub market: Vec<Card>,
    /// Kartenwirkungen und Set-Zaehler je Spieler.
    pub player_perks: Vec<PlayerPerks>,
    /// Hat der Spieler am Zug in diesem Zug schon eine Karte gekauft?
    pub bought_this_turn: bool,
    /// Weitere Aktionen in diesem Zug (Doppelzug).
    pub extra_actions: u8,
    /// Sturmangriff: in diesem Zug haben alle Figuren des Spielers am Zug Reichweite +1.
    pub charge: bool,
    /// Vorkommen, die waehrend der Partie entstanden sind (Sternsplitter).
    pub extra_deposits: BTreeMap<CellId, Vec<ResourceId>>,
    pub(crate) occupant: Vec<Option<PieceId>>,
    pub(crate) factory_at: Vec<Option<FactoryId>>,
    next_piece: u32,
    next_factory: u32,
}

impl State {
    pub(crate) fn empty(players: usize, cells: usize) -> State {
        State {
            pieces: BTreeMap::new(),
            factories: BTreeMap::new(),
            loot: BTreeMap::new(),
            carried: vec![Inventory::new(); players],
            alive: vec![true; players],
            current: PlayerId(0),
            round: 1,
            turns_played: 0,
            outcome: None,
            ledger: Ledger::default(),
            depleted: BTreeSet::new(),
            regrow: BTreeMap::new(),
            harvested: BTreeMap::new(),
            market: Vec::new(),
            player_perks: vec![PlayerPerks::default(); players],
            bought_this_turn: false,
            extra_actions: 0,
            charge: false,
            extra_deposits: BTreeMap::new(),
            occupant: vec![None; cells],
            factory_at: vec![None; cells],
            next_piece: 0,
            next_factory: 0,
        }
    }

    pub fn piece_at(&self, cell: CellId) -> Option<&Piece> {
        self.occupant[cell.idx()].and_then(|id| self.pieces.get(&id))
    }

    pub fn factory_at(&self, cell: CellId) -> Option<&Factory> {
        self.factory_at[cell.idx()].and_then(|id| self.factories.get(&id))
    }

    pub(crate) fn insert_piece(
        &mut self,
        owner: PlayerId,
        cell: CellId,
        kind: PieceKind,
    ) -> PieceId {
        assert!(
            self.occupant[cell.idx()].is_none(),
            "Feld {cell:?} ist schon besetzt"
        );
        let id = PieceId(self.next_piece);
        self.next_piece += 1;
        self.occupant[cell.idx()] = Some(id);
        self.pieces.insert(
            id,
            Piece {
                id,
                owner,
                cell,
                kind,
                perks: Perks::default(),
            },
        );
        id
    }

    pub(crate) fn remove_piece(&mut self, id: PieceId) -> Option<Piece> {
        let piece = self.pieces.remove(&id)?;
        self.occupant[piece.cell.idx()] = None;
        Some(piece)
    }

    pub(crate) fn insert_factory(&mut self, owner: PlayerId, cell: CellId) -> FactoryId {
        assert!(
            self.factory_at[cell.idx()].is_none(),
            "Feld {cell:?} hat schon eine Fabrik"
        );
        let id = FactoryId(self.next_factory);
        self.next_factory += 1;
        self.factory_at[cell.idx()] = Some(id);
        self.factories.insert(
            id,
            Factory {
                id,
                owner,
                cell,
                storage: Inventory::new(),
            },
        );
        id
    }

    pub(crate) fn remove_factory(&mut self, id: FactoryId) -> Option<Factory> {
        let factory = self.factories.remove(&id)?;
        self.factory_at[factory.cell.idx()] = None;
        Some(factory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(n: u16) -> ResourceId {
        ResourceId(n)
    }

    #[test]
    fn inventory_add_get_and_total() {
        let mut inv = Inventory::new();
        assert!(inv.is_empty());
        inv.add(r(1), 2);
        inv.add(r(1), 3);
        inv.add(r(4), 1);
        assert_eq!(inv.get(r(1)), 5);
        assert_eq!(inv.get(r(4)), 1);
        assert_eq!(inv.get(r(9)), 0);
        assert_eq!(inv.total(), 6);
        assert!(!inv.is_empty());
    }

    #[test]
    fn adding_zero_creates_no_entry() {
        let mut inv = Inventory::new();
        inv.add(r(1), 0);
        assert!(inv.is_empty());
        assert_eq!(inv, Inventory::new());
    }

    #[test]
    fn take_succeeds_only_with_enough_stock() {
        let mut inv = Inventory::new();
        inv.add(r(1), 2);
        assert!(!inv.take(r(1), 3));
        assert_eq!(inv.get(r(1)), 2, "Fehlschlag aendert nichts");
        assert!(inv.take(r(1), 1));
        assert_eq!(inv.get(r(1)), 1);
        assert!(inv.take(r(1), 1));
        assert!(inv.is_empty(), "leere Eintraege werden entfernt");
        assert!(!inv.take(r(7), 1));
    }

    #[test]
    fn absorb_moves_everything() {
        let mut a = Inventory::new();
        let mut b = Inventory::new();
        a.add(r(1), 1);
        b.add(r(1), 2);
        b.add(r(2), 5);
        a.absorb(&mut b);
        assert_eq!(a.get(r(1)), 3);
        assert_eq!(a.get(r(2)), 5);
        assert!(b.is_empty());
    }

    #[test]
    fn kinds_are_sorted_by_id() {
        let mut inv = Inventory::new();
        inv.add(r(9), 1);
        inv.add(r(2), 1);
        inv.add(r(5), 1);
        assert_eq!(inv.kinds().collect::<Vec<_>>(), vec![r(2), r(5), r(9)]);
    }

    #[test]
    fn piece_and_factory_lookup_follow_insert_and_remove() {
        let mut s = State::empty(2, 9);
        let cell = CellId(4);
        assert!(s.piece_at(cell).is_none());
        let id = s.insert_piece(PlayerId(0), cell, PieceKind::King);
        assert_eq!(s.piece_at(cell).map(|p| p.id), Some(id));
        let fid = s.insert_factory(PlayerId(1), cell);
        assert_eq!(s.factory_at(cell).map(|f| f.id), Some(fid));
        assert!(s.remove_piece(id).is_some());
        assert!(s.piece_at(cell).is_none());
        assert!(s.remove_piece(id).is_none());
        assert!(s.remove_factory(fid).is_some());
        assert!(s.factory_at(cell).is_none());
    }

    #[test]
    #[should_panic]
    fn two_pieces_on_one_cell_are_rejected() {
        let mut s = State::empty(2, 4);
        s.insert_piece(PlayerId(0), CellId(1), PieceKind::King);
        s.insert_piece(PlayerId(1), CellId(1), PieceKind::King);
    }

    #[test]
    fn ids_are_never_reused() {
        let mut s = State::empty(1, 4);
        let a = s.insert_piece(PlayerId(0), CellId(0), PieceKind::King);
        s.remove_piece(a);
        let b = s.insert_piece(PlayerId(0), CellId(0), PieceKind::King);
        assert_ne!(a, b);
    }
}
