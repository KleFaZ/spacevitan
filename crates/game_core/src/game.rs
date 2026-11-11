//! Regel-Engine: erlaubte Aktionen erzeugen, Aktionen ausfuehren, Zugende abwickeln.
//!
//! Quelle der Wahrheit fuer Zulaessigkeit ist `legal_actions`; `apply` akzeptiert genau diese
//! Aktionen. Die Regeln stammen aus dem Regelwerk (Abschnitte 4 bis 8).

use crate::action::{Action, Event, IllegalReason, RuleError};
use crate::config::Config;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use crate::map::{Map, MapError};
use crate::state::{Factory, Inventory, Outcome, Piece, PieceKind, State};
use std::collections::{BTreeSet, VecDeque};

#[derive(Clone, Debug)]
pub struct Game {
    pub config: Config,
    pub map: Map,
    pub state: State,
}

impl Game {
    pub fn new(config: Config) -> Result<Game, MapError> {
        let map = Map::generate(&config)?;
        Ok(Game::with_map(config, map))
    }

    /// Spiel auf einer vorgegebenen Karte (fuer Tests und Szenarien).
    pub fn with_map(config: Config, map: Map) -> Game {
        assert_eq!(
            map.player_corners.len(),
            config.players as usize,
            "Karte und Spielerzahl passen nicht zusammen"
        );
        let mut state = State::empty(config.players as usize, map.board.len());
        for p in 0..config.players {
            let player = PlayerId(p);
            state.insert_piece(player, map.start_cell(player), PieceKind::King);
        }
        Game { config, map, state }
    }

    // ----- Abfragen -------------------------------------------------------------------------

    pub fn current_player(&self) -> PlayerId {
        self.state.current
    }

    pub fn outcome(&self) -> Option<Outcome> {
        self.state.outcome
    }

    pub fn king(&self, player: PlayerId) -> Option<&Piece> {
        self.state
            .pieces
            .values()
            .find(|p| p.owner == player && p.is_king())
    }

    pub fn pieces_of(&self, player: PlayerId) -> impl Iterator<Item = &Piece> {
        self.state
            .pieces
            .values()
            .filter(move |p| p.owner == player)
    }

    pub fn factories_of(&self, player: PlayerId) -> impl Iterator<Item = &Factory> {
        self.state
            .factories
            .values()
            .filter(move |f| f.owner == player)
    }

    pub fn alive_players(&self) -> Vec<PlayerId> {
        (0..self.config.players)
            .map(PlayerId)
            .filter(|p| self.state.alive[p.idx()])
            .collect()
    }

    /// Stufe und Reichweite, die eine Fabrik beim Aufloesen ergaeben wuerde.
    pub fn spawn_preview(&self, factory: &Factory) -> Option<(u8, u8, Vec<ResourceId>)> {
        let colors: Vec<ResourceId> = factory
            .storage
            .kinds()
            .filter(|&r| self.map.category(r) == 1)
            .collect();
        if colors.is_empty() {
            return None;
        }
        // Reichweite = Stufe: ein Bauer (Stufe 1) zieht ein Feld weit.
        let level = colors.len() as u8;
        Some((level, level, colors))
    }

    /// Felder, auf die die Figur ziehen darf (inklusive Schlagen), sortiert.
    pub fn piece_destinations(&self, id: PieceId) -> Vec<CellId> {
        let Some(piece) = self.state.pieces.get(&id) else {
            return Vec::new();
        };
        let board = &self.map.board;
        let owner = piece.owner;
        let mut out: Vec<CellId> = Vec::new();

        match &piece.kind {
            PieceKind::King => {
                for n in board.neighbors(piece.cell) {
                    if !self.own_piece_at(n, owner) {
                        out.push(n);
                    }
                }
            }
            PieceKind::Unit {
                level: 1, range, ..
            } => {
                // Stufe 1: nur geradeaus, nicht ueber Figuren hinweg.
                for dir in 0..board.dir_count() {
                    for cell in board.line(piece.cell, dir, *range as usize) {
                        match self.state.piece_at(cell) {
                            None => out.push(cell),
                            Some(other) if other.owner != owner => {
                                out.push(cell);
                                break;
                            }
                            Some(_) => break,
                        }
                    }
                }
            }
            PieceKind::Unit { range, .. } => {
                // Ab Stufe 2: beliebige Wege bis `range` Schritte, nicht durch Figuren hindurch.
                let mut dist = vec![u32::MAX; board.len()];
                let mut queue = VecDeque::new();
                dist[piece.cell.idx()] = 0;
                queue.push_back(piece.cell);
                while let Some(cur) = queue.pop_front() {
                    let d = dist[cur.idx()];
                    if d >= *range as u32 {
                        continue;
                    }
                    for n in board.neighbors(cur) {
                        if dist[n.idx()] != u32::MAX {
                            continue;
                        }
                        dist[n.idx()] = d + 1;
                        match self.state.piece_at(n) {
                            None => {
                                out.push(n);
                                queue.push_back(n);
                            }
                            Some(other) if other.owner != owner => out.push(n),
                            Some(_) => {}
                        }
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Alle Felder, auf die Figuren anderer Spieler im naechsten Zug ziehen oder schlagen koennten.
    pub fn threatened_cells(&self, player: PlayerId) -> BTreeSet<CellId> {
        let mut set = BTreeSet::new();
        for piece in self.state.pieces.values().filter(|p| p.owner != player) {
            set.extend(self.piece_destinations(piece.id));
        }
        set
    }

    /// Vorkommen auf dem Feld, die es noch gibt (nicht verbaut, nicht erschoepft).
    pub fn available_deposits(&self, cell: CellId) -> Vec<ResourceId> {
        self.map
            .deposits_at(cell)
            .iter()
            .copied()
            .filter(|&r| !self.state.depleted.contains(&(cell, r)))
            .collect()
    }

    /// Wie viel ein Vorkommen noch hergibt (`None` = unerschoepflich, 0 = weg).
    pub fn deposit_remaining(&self, cell: CellId, res: ResourceId) -> Option<u32> {
        if self.state.depleted.contains(&(cell, res)) {
            return Some(0);
        }
        match self.config.deposit_stock {
            0 => None,
            stock => Some(stock - self.state.harvested.get(&(cell, res)).copied().unwrap_or(0)),
        }
    }

    /// Erntet bis zu `want` Einheiten; erschoepft das Vorkommen, wenn der Vorrat aufgebraucht ist.
    fn harvest(&mut self, cell: CellId, res: ResourceId, want: u32) -> u32 {
        let amount = match self.deposit_remaining(cell, res) {
            None => want,
            Some(left) => {
                let take = want.min(left);
                let done = self.state.harvested.entry((cell, res)).or_insert(0);
                *done += take;
                if *done >= self.config.deposit_stock {
                    self.state.depleted.insert((cell, res));
                }
                take
            }
        };
        self.state.ledger.created += amount as u64;
        amount
    }

    fn own_piece_at(&self, cell: CellId, owner: PlayerId) -> bool {
        self.state.piece_at(cell).is_some_and(|p| p.owner == owner)
    }

    fn factory_distance_ok(&self, cell: CellId) -> bool {
        if self.state.factories.is_empty() {
            return true;
        }
        let dist = self.map.board.distances_from(cell);
        self.state
            .factories
            .values()
            .all(|f| dist[f.cell.idx()] >= self.config.min_factory_distance)
    }

    fn can_spawn(&self, factory: &Factory) -> bool {
        factory.owner == self.state.current
            && self.spawn_preview(factory).is_some()
            && self.state.piece_at(factory.cell).is_none()
    }

    // ----- Erlaubte Aktionen ----------------------------------------------------------------

    pub fn legal_actions(&self) -> Vec<Action> {
        if self.state.outcome.is_some() {
            return Vec::new();
        }
        let me = self.state.current;
        let mut actions = Vec::new();

        for piece in self.pieces_of(me) {
            for to in self.piece_destinations(piece.id) {
                actions.push(Action::Move {
                    piece: piece.id,
                    to,
                });
            }
        }

        if let Some(king) = self.king(me) {
            if self.state.factory_at(king.cell).is_none() && self.factory_distance_ok(king.cell) {
                for res in self.available_deposits(king.cell) {
                    if self.map.category(res) == 1 {
                        actions.push(Action::BuildFactory { pay: res });
                    }
                }
            }
        }

        for factory in self.factories_of(me) {
            if self.can_spawn(factory) {
                actions.push(Action::SpawnPiece {
                    factory: factory.id,
                });
            }
        }

        if actions.is_empty() {
            actions.push(Action::Pass);
        }
        actions
    }

    /// Erklaert, warum eine Aktion nicht erlaubt ist (best effort, nur fuer Fehlermeldungen).
    fn diagnose(&self, action: &Action) -> IllegalReason {
        let me = self.state.current;
        match *action {
            Action::Move { piece, to } => {
                let Some(p) = self.state.pieces.get(&piece) else {
                    return IllegalReason::NoSuchPiece;
                };
                if p.owner != me {
                    return IllegalReason::NotYourPiece;
                }
                if to.idx() >= self.map.board.len() {
                    return IllegalReason::CellOutsideBoard;
                }
                if self.own_piece_at(to, me) {
                    return IllegalReason::OccupiedByOwnPiece;
                }
                if !self.piece_destinations(piece).contains(&to) {
                    return IllegalReason::CannotReach;
                }
            }
            Action::BuildFactory { pay } => {
                if pay.idx() >= self.map.resource_types.len() {
                    return IllegalReason::UnknownResource;
                }
                if self.map.category(pay) != 1 {
                    return IllegalReason::PayTypeIsNotCategory1;
                }
                if let Some(king) = self.king(me) {
                    if !self.available_deposits(king.cell).contains(&pay) {
                        return IllegalReason::NoCategory1DepositHere;
                    }
                    if self.state.factory_at(king.cell).is_some() {
                        return IllegalReason::FactoryAlreadyHere;
                    }
                    if !self.factory_distance_ok(king.cell) {
                        return IllegalReason::TooCloseToAnotherFactory;
                    }
                }
            }
            Action::SpawnPiece { factory } => {
                let Some(f) = self.state.factories.get(&factory) else {
                    return IllegalReason::NoSuchFactory;
                };
                if f.owner != me {
                    return IllegalReason::NotYourFactory;
                }
                if self.spawn_preview(f).is_none() {
                    return IllegalReason::FactoryHasNoCategory1;
                }
                if self.state.piece_at(f.cell).is_some() {
                    return IllegalReason::SpawnCellOccupied;
                }
            }
            Action::Pass => return IllegalReason::PassNotAllowed,
        }
        IllegalReason::Unspecified
    }

    // ----- Aktion ausfuehren ----------------------------------------------------------------

    pub fn apply(&mut self, action: Action) -> Result<Vec<Event>, RuleError> {
        if self.state.outcome.is_some() {
            return Err(RuleError::GameOver);
        }
        if !self.legal_actions().contains(&action) {
            return Err(RuleError::Illegal(self.diagnose(&action)));
        }
        let actor = self.state.current;
        let mut events = Vec::new();

        match action {
            Action::Move { piece, to } => self.do_move(actor, piece, to, &mut events),
            Action::BuildFactory { pay } => self.do_build(actor, pay, &mut events),
            Action::SpawnPiece { factory } => self.do_spawn(actor, factory, &mut events),
            Action::Pass => events.push(Event::Passed { player: actor }),
        }

        if self.state.outcome.is_none() {
            self.end_of_turn(actor, &mut events);
        }
        self.state.turns_played += 1;
        if self.state.outcome.is_none() {
            self.advance_turn(&mut events);
        }
        Ok(events)
    }

    fn do_move(&mut self, actor: PlayerId, id: PieceId, to: CellId, events: &mut Vec<Event>) {
        let from = self.state.pieces[&id].cell;

        if let Some(victim_id) = self.state.occupant[to.idx()] {
            let victim = self.state.remove_piece(victim_id).expect("Opfer existiert");
            events.push(Event::Captured {
                piece: victim.id,
                owner: victim.owner,
                at: to,
                by: actor,
            });
            if victim.is_king() {
                self.eliminate(victim.owner, events);
            }
        }

        self.state.occupant[from.idx()] = None;
        self.state.occupant[to.idx()] = Some(id);
        self.state
            .pieces
            .get_mut(&id)
            .expect("Figur existiert")
            .cell = to;
        events.push(Event::Moved {
            piece: id,
            from,
            to,
        });

        // Eine feindliche Fabrik auf dem Zielfeld wird eingenommen: zerstoert, Lager wird Beute.
        if let Some(fid) = self.state.factory_at[to.idx()] {
            if self.state.factories[&fid].owner != actor {
                let factory = self.state.remove_factory(fid).expect("Fabrik existiert");
                let mut storage = factory.storage.clone();
                if !storage.is_empty() {
                    self.state.loot.entry(to).or_default().absorb(&mut storage);
                }
                events.push(Event::FactoryDestroyed {
                    factory: factory.id,
                    owner: factory.owner,
                    cell: to,
                    by: actor,
                    loot: factory.storage,
                });
            }
        }

        self.check_winner(events);
    }

    fn do_build(&mut self, actor: PlayerId, pay: ResourceId, events: &mut Vec<Event>) {
        let cell = self.king(actor).expect("Koenig lebt").cell;
        // Das Kat-1-Vorkommen geht in der Fabrik auf: es ist dauerhaft weg, liegt aber als
        // eine Einheit im Lager. Sofort aufgeloest ergibt das einen Bauern.
        self.state.depleted.insert((cell, pay));
        self.state.ledger.created += 1;
        let factory = self.state.insert_factory(actor, cell);
        self.state
            .factories
            .get_mut(&factory)
            .expect("Fabrik existiert")
            .storage
            .add(pay, 1);
        events.push(Event::FactoryBuilt {
            factory,
            owner: actor,
            cell,
        });
    }

    fn do_spawn(&mut self, actor: PlayerId, id: FactoryId, events: &mut Vec<Event>) {
        let factory = self.state.factories[&id].clone();
        let (level, range, colors) = self.spawn_preview(&factory).expect("Fabrik ist aufloesbar");
        self.state.remove_factory(id);
        self.state.ledger.consumed_by_spawns += factory.storage.total();
        let piece = self.state.insert_piece(
            actor,
            factory.cell,
            PieceKind::Unit {
                level,
                range,
                colors,
            },
        );
        events.push(Event::Spawned {
            piece,
            owner: actor,
            factory: id,
            cell: factory.cell,
            level,
            range,
        });
    }

    /// Spieler scheidet aus: alle Figuren und Fabriken verschwinden samt Gepaeck und Lagern.
    fn eliminate(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let piece_ids: Vec<PieceId> = self.pieces_of(player).map(|p| p.id).collect();
        for id in piece_ids {
            self.state.remove_piece(id);
        }
        let factory_ids: Vec<FactoryId> = self.factories_of(player).map(|f| f.id).collect();
        for id in factory_ids {
            let factory = self.state.remove_factory(id).expect("Fabrik existiert");
            self.state.ledger.vanished += factory.storage.total();
        }
        let carried = std::mem::take(&mut self.state.carried[player.idx()]);
        self.state.ledger.vanished += carried.total();
        self.state.alive[player.idx()] = false;
        events.push(Event::PlayerEliminated { player });
    }

    fn check_winner(&mut self, events: &mut Vec<Event>) {
        let alive = self.alive_players();
        if alive.len() == 1 && self.state.outcome.is_none() {
            let outcome = Outcome::Winner(alive[0]);
            self.state.outcome = Some(outcome);
            events.push(Event::GameOver { outcome });
        }
    }

    /// Automatisch am Ende des eigenen Zuges (keine Aktion): Koenig sammelt und laedt ab,
    /// dann erzeugen die Fabriken des Spielers ihren Ertrag.
    fn end_of_turn(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let yield_n = self.config.yield_per_round;

        if let Some(king_cell) = self.king(actor).map(|k| k.cell) {
            let mut gained = Inventory::new();
            for res in self.available_deposits(king_cell) {
                let amount = self.harvest(king_cell, res, yield_n);
                gained.add(res, amount);
            }
            if let Some(mut pile) = self.state.loot.remove(&king_cell) {
                gained.absorb(&mut pile);
            }
            if !gained.is_empty() {
                let mut copy = gained.clone();
                self.state.carried[actor.idx()].absorb(&mut copy);
                events.push(Event::Collected {
                    player: actor,
                    cell: king_cell,
                    gained,
                });
            }

            if let Some(fid) = self.state.factory_at[king_cell.idx()] {
                if self.state.factories[&fid].owner == actor
                    && !self.state.carried[actor.idx()].is_empty()
                {
                    let mut load = std::mem::take(&mut self.state.carried[actor.idx()]);
                    let amount = load.clone();
                    self.state
                        .factories
                        .get_mut(&fid)
                        .expect("Fabrik existiert")
                        .storage
                        .absorb(&mut load);
                    events.push(Event::Unloaded {
                        player: actor,
                        factory: fid,
                        amount,
                    });
                }
            }
        }

        let own: Vec<(FactoryId, CellId)> =
            self.factories_of(actor).map(|f| (f.id, f.cell)).collect();
        for (fid, cell) in own {
            let mut gained = Inventory::new();
            let cells = std::iter::once(cell).chain(self.map.board.neighbors(cell));
            let cells: Vec<CellId> = cells.collect();
            for c in cells {
                for res in self.available_deposits(c) {
                    if self.map.category(res) >= 2 {
                        let amount = self.harvest(c, res, yield_n);
                        gained.add(res, amount);
                    }
                }
            }
            if !gained.is_empty() {
                let mut copy = gained.clone();
                self.state
                    .factories
                    .get_mut(&fid)
                    .expect("Fabrik existiert")
                    .storage
                    .absorb(&mut copy);
                events.push(Event::FactoryYield {
                    factory: fid,
                    gained,
                });
            }
        }
    }

    /// Naechster lebender Spieler; nach dem letzten beginnt eine neue Runde.
    fn advance_turn(&mut self, events: &mut Vec<Event>) {
        let n = self.config.players as usize;
        let mut i = self.state.current.idx();
        loop {
            i = (i + 1) % n;
            if i <= self.state.current.idx() && self.state.alive[i] {
                self.state.round += 1;
            }
            if self.state.alive[i] {
                break;
            }
        }
        self.state.current = PlayerId(i as u8);
        if self.state.round > self.config.max_rounds && self.state.outcome.is_none() {
            self.state.outcome = Some(Outcome::Draw);
            events.push(Event::GameOver {
                outcome: Outcome::Draw,
            });
        }
    }

    // ----- Aufbau von Szenarien (Tests, spaeter Raetsel/Tutorial) ----------------------------

    /// Stellt eine Figur direkt aufs Brett. Fuer Tests und Szenarien, nicht fuer normale Zuege.
    pub fn setup_unit(&mut self, owner: PlayerId, cell: CellId, level: u8, range: u8) -> PieceId {
        let colors = (0..level).map(|i| ResourceId(i as u16)).collect();
        self.state.insert_piece(
            owner,
            cell,
            PieceKind::Unit {
                level,
                range,
                colors,
            },
        )
    }

    /// Stellt eine Fabrik direkt aufs Brett (ohne Kosten). Fuer Tests und Szenarien.
    pub fn setup_factory(&mut self, owner: PlayerId, cell: CellId) -> FactoryId {
        self.state.insert_factory(owner, cell)
    }

    /// Legt Rohstoffe in das Gepaeck des Koenigs und bucht sie als erzeugt. Fuer Tests.
    pub fn setup_carry(&mut self, player: PlayerId, res: ResourceId, amount: u32) {
        self.state.carried[player.idx()].add(res, amount);
        self.state.ledger.created += amount as u64;
    }

    /// Legt Rohstoffe in das Lager einer Fabrik und bucht sie als erzeugt. Fuer Tests.
    pub fn setup_storage(&mut self, factory: FactoryId, res: ResourceId, amount: u32) {
        self.state
            .factories
            .get_mut(&factory)
            .expect("Fabrik existiert")
            .storage
            .add(res, amount);
        self.state.ledger.created += amount as u64;
    }

    /// Setzt, wer am Zug ist. Fuer Tests.
    pub fn setup_current(&mut self, player: PlayerId) {
        self.state.current = player;
    }

    /// Setzt eine Figur ohne Zug auf ein freies Feld. Fuer Tests und Szenarien.
    pub fn setup_teleport(&mut self, piece: PieceId, cell: CellId) {
        assert!(
            self.state.occupant[cell.idx()].is_none(),
            "Zielfeld ist besetzt"
        );
        let from = self.state.pieces[&piece].cell;
        self.state.occupant[from.idx()] = None;
        self.state.occupant[cell.idx()] = Some(piece);
        self.state
            .pieces
            .get_mut(&piece)
            .expect("Figur existiert")
            .cell = cell;
    }

    // ----- Selbstpruefung -------------------------------------------------------------------

    /// Prueft alle Invarianten des Zustands. `Err` beschreibt die erste Verletzung.
    pub fn check_invariants(&self) -> Result<(), String> {
        let board_len = self.map.board.len();
        let st = &self.state;

        if st.occupant.len() != board_len || st.factory_at.len() != board_len {
            return Err("Indexlisten haben falsche Laenge".into());
        }
        let occupied = st.occupant.iter().flatten().count();
        if occupied != st.pieces.len() {
            return Err(format!(
                "{occupied} belegte Felder, aber {} Figuren",
                st.pieces.len()
            ));
        }
        for piece in st.pieces.values() {
            if piece.cell.idx() >= board_len {
                return Err(format!("{:?} steht ausserhalb der Karte", piece.id));
            }
            if st.occupant[piece.cell.idx()] != Some(piece.id) {
                return Err(format!("{:?} fehlt im Feldindex", piece.id));
            }
            if let PieceKind::Unit {
                level,
                range,
                colors,
            } = &piece.kind
            {
                if *level == 0 || *level as usize > self.config.corner_count() {
                    return Err(format!("{:?} hat ungueltige Stufe {level}", piece.id));
                }
                if *range == 0 || *range as usize > self.config.corner_count() {
                    return Err(format!("{:?} hat ungueltige Reichweite {range}", piece.id));
                }
                if colors.len() != *level as usize {
                    return Err(format!("{:?}: Farben passen nicht zur Stufe", piece.id));
                }
            }
        }

        let fcount = st.factory_at.iter().flatten().count();
        if fcount != st.factories.len() {
            return Err(format!(
                "{fcount} Fabrikfelder, aber {} Fabriken",
                st.factories.len()
            ));
        }
        for f in st.factories.values() {
            if st.factory_at[f.cell.idx()] != Some(f.id) {
                return Err(format!("{:?} fehlt im Fabrikindex", f.id));
            }
            if !st.alive[f.owner.idx()] {
                return Err(format!("{:?} gehoert einem ausgeschiedenen Spieler", f.id));
            }
        }
        let list: Vec<&Factory> = st.factories.values().collect();
        for (i, a) in list.iter().enumerate() {
            let dist = self.map.board.distances_from(a.cell);
            for b in &list[i + 1..] {
                if dist[b.cell.idx()] < self.config.min_factory_distance {
                    return Err(format!(
                        "{:?} und {:?} stehen zu nah beieinander",
                        a.id, b.id
                    ));
                }
            }
        }

        for p in 0..self.config.players {
            let player = PlayerId(p);
            let kings = self.pieces_of(player).filter(|x| x.is_king()).count();
            if st.alive[p as usize] {
                if kings != 1 {
                    return Err(format!("{player:?} lebt, hat aber {kings} Koenige"));
                }
            } else {
                if self.pieces_of(player).next().is_some()
                    || self.factories_of(player).next().is_some()
                {
                    return Err(format!(
                        "{player:?} ist ausgeschieden, hat aber noch Besitz"
                    ));
                }
                if !st.carried[p as usize].is_empty() {
                    return Err(format!(
                        "{player:?} ist ausgeschieden, traegt aber noch Rohstoffe"
                    ));
                }
            }
        }

        for &(cell, res) in &st.depleted {
            if cell.idx() >= board_len || !self.map.deposits_at(cell).contains(&res) {
                return Err(format!(
                    "verbrauchtes Vorkommen {res:?} auf {cell:?} gibt es nicht"
                ));
            }
        }
        if self.config.deposit_stock > 0
            && st
                .harvested
                .values()
                .any(|&n| n > self.config.deposit_stock)
        {
            return Err("mehr geerntet als ein Vorkommen hergibt".into());
        }

        if st.loot.values().any(|pile| pile.is_empty()) {
            return Err("leere Beute-Haufen".into());
        }

        if st.outcome.is_none() && !st.alive[st.current.idx()] {
            return Err("am Zug ist ein ausgeschiedener Spieler".into());
        }
        if st.outcome.is_none() && self.alive_players().len() < 2 {
            return Err("weniger als zwei lebende Spieler, aber kein Ergebnis".into());
        }

        let in_play: u64 = st.carried.iter().map(Inventory::total).sum::<u64>()
            + st.factories
                .values()
                .map(|f| f.storage.total())
                .sum::<u64>()
            + st.loot.values().map(Inventory::total).sum::<u64>();
        let l = st.ledger;
        let booked = in_play + l.consumed_by_spawns + l.vanished;
        if booked != l.created {
            return Err(format!(
                "Rohstoffe stimmen nicht: erzeugt {} aber verbucht {booked} ({l:?}, im Spiel {in_play})",
                l.created
            ));
        }
        Ok(())
    }
}
