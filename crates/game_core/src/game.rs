//! Regel-Engine: erlaubte Aktionen erzeugen, Aktionen ausfuehren, Zugende abwickeln.
//!
//! Quelle der Wahrheit fuer Zulaessigkeit ist `legal_actions`; `apply` akzeptiert genau diese
//! Aktionen. Die Regeln stammen aus dem Regelwerk (Abschnitte 4 bis 8).

use crate::action::{Action, Event, IllegalReason, PaySource, RuleError};
use crate::board::GridKind;
use crate::cards::{self, Card, CardTarget, MAX_EXTRA_RANGE, SET_SIZE};
use crate::config::Config;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use crate::map::{Map, MapError};
use crate::state::{Factory, Inventory, Outcome, Piece, PieceKind, State};
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;

/// Ein laufendes Spiel. Die Karte aendert sich waehrend der Partie nicht und wird beim Kopieren
/// geteilt; eine Kopie kostet damit nur den Zustand (wichtig fuer die Suche der KI).
#[derive(Clone, Debug)]
pub struct Game {
    pub config: Config,
    pub map: Arc<Map>,
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
        state.market =
            cards::draw_market(config.seed, config.market_size as usize, map.board.kind());
        Game {
            config,
            map: Arc::new(map),
            state,
        }
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
        // Stufe = Farben + Ausbaustufe, hoechstens so viele wie Ecken.
        // Reichweite = Stufe: ein Bauer (Stufe 1) zieht ein Feld weit.
        let cap = self.config.corner_count() as u8;
        let level = (colors.len() as u8 + self.upgrade_level(&factory.storage)).min(cap);
        Some((level, level, colors))
    }

    /// Ausbaustufe eines Lagers: Stufe k braucht `k * upgrade_step` Einheiten ab Kat 2 aus
    /// mindestens `k + 1` verschiedenen Typen (Vielfalt zaehlt). Die Fabrik ernten dafuer
    /// selbst; der Koenig kann zusaetzlich abladen und beschleunigt so.
    pub fn upgrade_level(&self, storage: &Inventory) -> u8 {
        let (units, kinds) = self.upgrade_stock(storage);
        let step = self.config.upgrade_step;
        if step == 0 {
            return 0;
        }
        let mut k = 0u8;
        while k < self.config.max_upgrades
            && units >= (k as u64 + 1) * step as u64
            && kinds >= k as usize + 2
        {
            k += 1;
        }
        k
    }

    /// Einheiten und Typen ab Kat 2 in einem Lager (das, was fuer den Ausbau zaehlt).
    pub fn upgrade_stock(&self, storage: &Inventory) -> (u64, usize) {
        storage
            .iter()
            .filter(|&(r, _)| self.map.category(r) >= 2)
            .fold((0, 0), |(u, k), (_, n)| (u + n as u64, k + 1))
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
                let range = *range + self.charge_bonus(owner);
                for dir in 0..board.dir_count() {
                    for cell in board.line(piece.cell, dir, range as usize) {
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
                let range = &(*range + self.charge_bonus(owner));
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
        let enemy_at = |cell: CellId| self.state.piece_at(cell).is_some_and(|o| o.owner != owner);
        if !piece.is_king() {
            let pk = piece.perks;
            for dir in 0..board.dir_count() {
                let line = board.line(piece.cell, dir, 2);
                if line.len() < 2 {
                    continue;
                }
                // Lanze: schlaegt zwei Felder weit geradeaus, wenn das erste frei ist.
                if pk.lance && self.state.piece_at(line[0]).is_none() && enemy_at(line[1]) {
                    out.push(line[1]);
                }
                // Sprungfeder: ueber eine Nachbarfigur auf das Feld dahinter.
                if pk.spring
                    && self.state.piece_at(line[0]).is_some()
                    && !self.own_piece_at(line[1], owner)
                {
                    out.push(line[1]);
                }
            }
            // Pferd: jedes Feld im Abstand 2, ueber alles hinweg.
            if pk.horse {
                let near: Vec<CellId> = board.neighbors(piece.cell).collect();
                for &n in &near {
                    for m in board.neighbors(n) {
                        if m != piece.cell && !near.contains(&m) && !self.own_piece_at(m, owner) {
                            out.push(m);
                        }
                    }
                }
            }
        }
        // Laeuferschlag: eine Figur schlaegt zusaetzlich diagonal (nur im Quadratgitter).
        if piece.perks.diagonal && !piece.is_king() && board.kind() == GridKind::Square {
            let (x, y) = board.coord(piece.cell);
            for (dx, dy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                if let Some(cell) = board.cell_at(x + dx, y + dy) {
                    if enemy_at(cell) {
                        out.push(cell);
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Sturmangriff: Reichweite +1 fuer die Figuren des Spielers am Zug, nur in diesem Zug.
    fn charge_bonus(&self, owner: PlayerId) -> u8 {
        (self.state.charge && owner == self.state.current) as u8
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
        let extra = self.state.extra_deposits.get(&cell).map(|v| v.as_slice());
        self.map
            .deposits_at(cell)
            .iter()
            .chain(extra.unwrap_or(&[]))
            .copied()
            .filter(|&r| !self.state.depleted.contains(&(cell, r)))
            .collect()
    }

    /// Alle Vorkommen eines Feldes, auch verbrauchte (Karte plus Sternsplitter).
    pub fn all_deposits(&self, cell: CellId) -> Vec<ResourceId> {
        let mut out = self.map.deposits_at(cell).to_vec();
        if let Some(extra) = self.state.extra_deposits.get(&cell) {
            out.extend(extra.iter().copied());
        }
        out
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

    fn can_spawn(&self, factory: &Factory, me: PlayerId) -> bool {
        factory.owner == me
            && self.spawn_preview(factory).is_some()
            && self.state.piece_at(factory.cell).is_none()
    }

    // ----- Erlaubte Aktionen ----------------------------------------------------------------

    pub fn legal_actions(&self) -> Vec<Action> {
        self.legal_actions_for(self.state.current)
    }

    /// Was `player` tun duerfte, wenn er am Zug waere. Die KI fragt damit die Antworten der
    /// Gegner ab, ohne den Zustand umzustellen.
    pub fn legal_actions_for(&self, me: PlayerId) -> Vec<Action> {
        if self.state.outcome.is_some() || !self.state.alive[me.idx()] {
            return Vec::new();
        }
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
            let cat1: Vec<ResourceId> = self
                .available_deposits(king.cell)
                .into_iter()
                .filter(|&r| self.map.category(r) == 1)
                .collect();
            // Abstandspruefung (Breitensuche) nur, wenn hier ueberhaupt gebaut werden koennte.
            if !cat1.is_empty()
                && self.state.factory_at(king.cell).is_none()
                && self.factory_distance_ok(king.cell)
            {
                for res in cat1 {
                    actions.push(Action::BuildFactory { pay: res });
                }
            }
        }

        for factory in self.factories_of(me) {
            if self.can_spawn(factory, me) {
                actions.push(Action::SpawnPiece {
                    factory: factory.id,
                });
            }
        }

        if actions.is_empty() {
            actions.push(Action::Pass);
        }
        actions.extend(self.buy_actions(me));
        actions
    }

    /// Alle erlaubten Kartenkaeufe von `me` (leer, wenn in diesem Zug schon gekauft wurde).
    pub fn buy_actions(&self, me: PlayerId) -> Vec<Action> {
        let mut out = Vec::new();
        if self.state.bought_this_turn || self.state.market.is_empty() {
            return out;
        }
        let Some(king) = self.king(me) else {
            return out;
        };
        let mut sources: Vec<(PaySource, CellId)> = vec![(PaySource::King, king.cell)];
        sources.extend(
            self.factories_of(me)
                .map(|f| (PaySource::Factory(f.id), f.cell)),
        );
        for &card in &self.state.market {
            for &(from, at) in &sources {
                if card.needs_factory() && from == PaySource::King {
                    continue;
                }
                if self.payment(me, from, card).is_none() {
                    continue;
                }
                for target in self.card_targets(me, card, from, at) {
                    out.push(Action::BuyCard { card, from, target });
                }
            }
        }
        out
    }

    /// Was `me` fuer `card` aus `from` bezahlen wuerde, oder `None`.
    pub fn payment(&self, me: PlayerId, from: PaySource, card: Card) -> Option<Inventory> {
        let stock = match from {
            PaySource::King => &self.state.carried[me.idx()],
            PaySource::Factory(fid) => {
                let f = self.state.factories.get(&fid)?;
                if f.owner != me {
                    return None;
                }
                &f.storage
            }
        };
        cards::plan_payment(&self.map, self.map.own_color(me), stock, &card.cost())
    }

    /// Moegliche Ziele einer Karte, bezahlt an Feld `at`. `None` im Ergebnis = kein Ziel noetig.
    /// Leer = die Karte waere hier wirkungslos.
    fn card_targets(
        &self,
        me: PlayerId,
        card: Card,
        from: PaySource,
        at: CellId,
    ) -> Vec<Option<PieceId>> {
        let board = &self.map.board;
        let only_near = self.config.card_target_near;
        let near =
            |cell: CellId| !only_near || cell == at || board.neighbors(at).any(|n| n == cell);
        let cap = self.config.corner_count() as u8;
        let perks = self.state.player_perks[me.idx()];
        let units = || {
            self.pieces_of(me)
                .filter(|p| !p.is_king() && near(p.cell))
                .map(|p| (p.id, self.unit_level(p), p.perks))
        };
        match card.target() {
            CardTarget::Unit => {
                let fits = |level: u8, pk: crate::state::Perks| match card {
                    Card::Training => level < cap,
                    Card::LongLegs => pk.extra_range < MAX_EXTRA_RANGE,
                    Card::DiagonalStrike => !pk.diagonal,
                    Card::Lance => !pk.lance,
                    Card::Spring => !pk.spring,
                    Card::Horse => !pk.horse,
                    Card::Runestone => level < cap,
                    _ => false,
                };
                // Die ersten Runensteine sind nur Zaehler; erst der letzte braucht eine Figur.
                if card == Card::Runestone && perks.runes + 1 < SET_SIZE {
                    return vec![None];
                }
                units()
                    .filter(|&(_, level, pk)| fits(level, pk))
                    .map(|(id, _, _)| Some(id))
                    .collect()
            }
            CardTarget::Player => {
                let ok = match card {
                    Card::Mining => !perks.mining,
                    Card::Farming => !perks.farming,
                    Card::DoubleShift => self.factories_of(me).next().is_some(),
                    Card::DoubleMove => true,
                    Card::Charge => !self.state.charge && self.pieces_of(me).any(|p| !p.is_king()),
                    Card::Recruit => match from {
                        PaySource::Factory(fid) => self.recruit_cell(fid).is_some(),
                        PaySource::King => false,
                    },
                    Card::Starshard => {
                        perks.shards + 1 < SET_SIZE
                            || self.king(me).is_some_and(|k| self.shard_cell_ok(k.cell))
                    }
                    _ => false,
                };
                if ok {
                    vec![None]
                } else {
                    Vec::new()
                }
            }
        }
    }

    fn unit_level(&self, piece: &Piece) -> u8 {
        match &piece.kind {
            PieceKind::Unit { level, .. } => *level,
            PieceKind::King => 0,
        }
    }

    /// Wo der Rekrut einer Fabrik erscheint: das freie Nachbarfeld mit der kleinsten Nummer.
    pub fn recruit_cell(&self, factory: FactoryId) -> Option<CellId> {
        let f = self.state.factories.get(&factory)?;
        let mut free: Vec<CellId> = self
            .map
            .board
            .neighbors(f.cell)
            .filter(|&n| self.state.piece_at(n).is_none())
            .collect();
        free.sort();
        free.first().copied()
    }

    /// Darf ein Sternsplitter-Set hier ein Vorkommen anlegen? Nur ohne Kat 1 und ohne Fabrik.
    fn shard_cell_ok(&self, cell: CellId) -> bool {
        self.state.factory_at(cell).is_none()
            && !self
                .all_deposits(cell)
                .iter()
                .any(|&r| self.map.category(r) == 1)
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
            Action::BuyCard { card, from, .. } => {
                if !self.state.market.contains(&card) {
                    return IllegalReason::CardNotInMarket;
                }
                if self.state.bought_this_turn {
                    return IllegalReason::AlreadyBoughtThisTurn;
                }
                if self.payment(me, from, card).is_none() {
                    return IllegalReason::CannotPayCard;
                }
                return IllegalReason::InvalidCardTarget;
            }
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
        Ok(self.apply_unchecked(action))
    }

    /// Wie `apply`, aber ohne Pruefung. Nur fuer Aktionen aus `legal_actions_for` des
    /// Spielers, der gerade am Zug ist (die Suche der KI spart sich so die doppelte Arbeit).
    pub(crate) fn apply_unchecked(&mut self, action: Action) -> Vec<Event> {
        debug_assert!(self.state.outcome.is_none(), "Spiel ist vorbei");
        let actor = self.state.current;
        let mut events = Vec::new();

        match action {
            Action::Move { piece, to } => self.do_move(actor, piece, to, &mut events),
            Action::BuildFactory { pay } => self.do_build(actor, pay, &mut events),
            Action::SpawnPiece { factory } => self.do_spawn(actor, factory, &mut events),
            Action::Pass => events.push(Event::Passed { player: actor }),
            Action::BuyCard { card, from, target } => {
                // Ein Kauf ist keine Aktion: der Spieler bleibt am Zug.
                self.do_buy(actor, card, from, target, &mut events);
                self.state.turns_played += 1;
                return events;
            }
        }

        // Doppelzug: noch eine Aktion, das Zugende kommt erst danach.
        if self.state.outcome.is_none()
            && self.state.extra_actions > 0
            && self.state.alive[actor.idx()]
        {
            self.state.extra_actions -= 1;
            self.state.turns_played += 1;
            return events;
        }

        if self.state.outcome.is_none() {
            self.end_of_turn(actor, &mut events);
        }
        self.state.turns_played += 1;
        if self.state.outcome.is_none() {
            self.advance_turn(&mut events);
        }
        events
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
        // Auf fremder Farbe gebaut: die eigene Farbe kommt dazu, die Fabrik hat gleich Stufe 2.
        let own = self.map.own_color(actor);
        let bonus = self.config.foreign_build_bonus && pay != own;
        let storage = &mut self
            .state
            .factories
            .get_mut(&factory)
            .expect("Fabrik existiert")
            .storage;
        storage.add(pay, 1);
        if bonus {
            storage.add(own, 1);
            self.state.ledger.created += 1;
        }
        events.push(Event::FactoryBuilt {
            factory,
            owner: actor,
            cell,
        });
    }

    fn do_buy(
        &mut self,
        actor: PlayerId,
        card: Card,
        from: PaySource,
        target: Option<PieceId>,
        events: &mut Vec<Event>,
    ) {
        let paid = self.payment(actor, from, card).expect("bezahlbar");
        let stock = match from {
            PaySource::King => &mut self.state.carried[actor.idx()],
            PaySource::Factory(fid) => {
                &mut self
                    .state
                    .factories
                    .get_mut(&fid)
                    .expect("Fabrik existiert")
                    .storage
            }
        };
        for (res, n) in paid.iter() {
            assert!(
                stock.take(res, n),
                "Bezahlung geplant, aber nicht vorhanden"
            );
        }
        self.state.ledger.spent_on_cards += paid.total();
        self.state.bought_this_turn = true;
        self.state.player_perks[actor.idx()].cards_bought += 1;
        events.push(Event::CardBought {
            player: actor,
            card,
            from,
            target,
            paid: paid.clone(),
        });
        let color = paid.kinds().find(|&r| self.map.category(r) == 1);
        self.apply_card(actor, card, from, target, color, events);
    }

    /// Schenkt `player` die Wirkung einer Karte ohne Bezahlung (fuer die Kartenwert-Messung
    /// und Tests). Sets wirken sofort ganz (drei Runensteine bzw. Sternsplitter). Rekrut braucht
    /// eine eigene Fabrik mit freiem Nachbarfeld, Figurenkarten eine Figur. Gibt `false`
    /// zurueck, wenn die Karte gerade nicht wirken kann.
    pub fn grant_card(&mut self, player: PlayerId, card: Card, target: Option<PieceId>) -> bool {
        let mut events = Vec::new();
        let from = match card {
            Card::Recruit => {
                let Some(f) = self
                    .factories_of(player)
                    .map(|f| f.id)
                    .find(|&f| self.recruit_cell(f).is_some())
                else {
                    return false;
                };
                PaySource::Factory(f)
            }
            _ => PaySource::King,
        };
        if card.target() == CardTarget::Unit && target.is_none() {
            return false;
        }
        if card == Card::Starshard
            && !self
                .king(player)
                .is_some_and(|k| self.shard_cell_ok(k.cell))
        {
            return false;
        }
        // Sets: die vorherigen Teile gelten als schon gekauft.
        if card.is_set_piece() {
            let perks = &mut self.state.player_perks[player.idx()];
            match card {
                Card::Runestone => perks.runes = SET_SIZE - 1,
                _ => perks.shards = SET_SIZE - 1,
            }
        }
        let color = Some(self.map.own_color(player));
        self.apply_card(player, card, from, target, color, &mut events);
        true
    }

    /// Schenkt `player` `units` Rohstoffeinheiten ab Kat 2 ins Gepaeck, abwechselnd aus den
    /// Kat-2- und Kat-3-Typen seiner Ecke (fuer die Kartenwert-Messung).
    pub fn grant_resources(&mut self, player: PlayerId, units: u32) {
        let corner = self.map.player_corners[player.idx()];
        let types: Vec<ResourceId> = self
            .map
            .types_of_corner(corner)
            .filter(|t| t.category >= 2)
            .map(|t| t.id)
            .collect();
        for i in 0..units as usize {
            self.state.carried[player.idx()].add(types[i % types.len()], 1);
        }
        self.state.ledger.created += units as u64;
    }

    /// Wirkung einer Karte (nach der Bezahlung). `color`: die bezahlte Farbe (Rekrut).
    fn apply_card(
        &mut self,
        actor: PlayerId,
        card: Card,
        from: PaySource,
        target: Option<PieceId>,
        color: Option<ResourceId>,
        events: &mut Vec<Event>,
    ) {
        let cap = self.config.corner_count() as u8;
        let raise = |piece: &mut Piece, by: u8| {
            if let PieceKind::Unit { level, range, .. } = &mut piece.kind {
                let add = by.min(cap - *level);
                *level += add;
                *range += add;
                piece.perks.card_levels += add;
            }
        };
        match card {
            Card::Training => raise(self.target_mut(target), 1),
            Card::LongLegs => {
                let piece = self.target_mut(target);
                if let PieceKind::Unit { range, .. } = &mut piece.kind {
                    *range += 1;
                }
                piece.perks.extra_range += 1;
            }
            Card::DiagonalStrike => self.target_mut(target).perks.diagonal = true,
            Card::Lance => self.target_mut(target).perks.lance = true,
            Card::Spring => self.target_mut(target).perks.spring = true,
            Card::Horse => self.target_mut(target).perks.horse = true,
            Card::Charge => self.state.charge = true,
            Card::Mining => self.state.player_perks[actor.idx()].mining = true,
            Card::Farming => self.state.player_perks[actor.idx()].farming = true,
            Card::DoubleMove => self.state.extra_actions += 1,
            Card::DoubleShift => self.harvest_factories(actor, events),
            Card::Recruit => {
                let PaySource::Factory(fid) = from else {
                    unreachable!("Rekrut wird aus einer Fabrik bezahlt")
                };
                let cell = self.recruit_cell(fid).expect("freies Nachbarfeld");
                let color = color.expect("Rekrut kostet eine Farbe");
                let piece = self.state.insert_piece(
                    actor,
                    cell,
                    PieceKind::Unit {
                        level: 1,
                        range: 1,
                        colors: vec![color],
                    },
                );
                events.push(Event::Recruited {
                    piece,
                    owner: actor,
                    factory: fid,
                    cell,
                });
            }
            Card::Runestone => {
                let perks = &mut self.state.player_perks[actor.idx()];
                perks.runes += 1;
                if perks.runes >= SET_SIZE {
                    perks.runes = 0;
                    raise(self.target_mut(target), 2);
                }
            }
            Card::Starshard => {
                let perks = &mut self.state.player_perks[actor.idx()];
                perks.shards += 1;
                if perks.shards >= SET_SIZE {
                    perks.shards = 0;
                    let cell = self.king(actor).expect("Koenig lebt").cell;
                    let resource = self.map.own_color(actor);
                    self.state
                        .extra_deposits
                        .entry(cell)
                        .or_default()
                        .push(resource);
                    events.push(Event::DepositCreated { cell, resource });
                }
            }
        }
    }

    fn target_mut(&mut self, target: Option<PieceId>) -> &mut Piece {
        let id = target.expect("Karte braucht ein Ziel");
        self.state.pieces.get_mut(&id).expect("Ziel existiert")
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

        self.harvest_factories(actor, events);
    }

    /// Alle Fabriken von `actor` ernten ihr Feld und die Nachbarfelder (ab Kat 2).
    /// Bergbau und Ackerbau verdoppeln Kat 2 bzw. Kat 3.
    fn harvest_factories(&mut self, actor: PlayerId, events: &mut Vec<Event>) {
        let yield_n = self.config.yield_per_round;
        let perks = self.state.player_perks[actor.idx()];
        let own: Vec<(FactoryId, CellId)> =
            self.factories_of(actor).map(|f| (f.id, f.cell)).collect();
        for (fid, cell) in own {
            let mut gained = Inventory::new();
            let cells = std::iter::once(cell).chain(self.map.board.neighbors(cell));
            let cells: Vec<CellId> = cells.collect();
            for c in cells {
                for res in self.available_deposits(c) {
                    let cat = self.map.category(res);
                    if cat >= 2 {
                        let double = (cat == 2 && perks.mining) || (cat == 3 && perks.farming);
                        let want = if double { yield_n * 2 } else { yield_n };
                        let amount = self.harvest(c, res, want);
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
        self.state.bought_this_turn = false;
        self.state.extra_actions = 0;
        self.state.charge = false;
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
    /// Reichweite ueber der Stufe wird als "Lange Beine" verbucht (Reichweite = Stufe + Extra).
    pub fn setup_unit(&mut self, owner: PlayerId, cell: CellId, level: u8, range: u8) -> PieceId {
        assert!(range >= level, "Reichweite ist mindestens die Stufe");
        let colors = (0..level).map(|i| ResourceId(i as u16)).collect();
        let id = self.state.insert_piece(
            owner,
            cell,
            PieceKind::Unit {
                level,
                range,
                colors,
            },
        );
        self.state
            .pieces
            .get_mut(&id)
            .expect("eben gesetzt")
            .perks
            .extra_range = range - level;
        id
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
                // Reichweite = Stufe + "Lange Beine".
                if *range != *level + piece.perks.extra_range
                    || piece.perks.extra_range > MAX_EXTRA_RANGE
                {
                    return Err(format!("{:?} hat ungueltige Reichweite {range}", piece.id));
                }
                // Stufe = Farben + Ausbau + Karten (gedeckelt): nie weniger als die Farben
                // (ausser am Deckel), nie mehr als Farben plus hoechster Ausbau plus Karten.
                let colors_n = colors.len();
                let cap = self.config.corner_count();
                if colors_n == 0
                    || (*level as usize) < colors_n.min(cap)
                    || (*level as usize)
                        > colors_n
                            + self.config.max_upgrades as usize
                            + piece.perks.card_levels as usize
                {
                    return Err(format!("{:?}: Farben passen nicht zur Stufe", piece.id));
                }
            } else if piece.perks.any() {
                return Err(format!("{:?}: Koenig mit Figurenkarte", piece.id));
            }
        }

        if st.player_perks.len() != self.config.players as usize {
            return Err("Kartenwirkungen fehlen fuer manche Spieler".into());
        }
        for (i, pp) in st.player_perks.iter().enumerate() {
            if pp.runes >= SET_SIZE || pp.shards >= SET_SIZE {
                return Err(format!("Spieler {i}: Set-Zaehler ueber dem Set"));
            }
        }
        for c in &st.market {
            if !c.available_on(self.map.board.kind()) {
                return Err(format!("{c:?} liegt aus, passt aber nicht zur Karte"));
            }
        }
        for (cell, list) in &st.extra_deposits {
            if list.iter().any(|&r| self.map.category(r) != 1) {
                return Err(format!("neues Vorkommen auf {cell:?} ist kein Kat 1"));
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
            if cell.idx() >= board_len || !self.all_deposits(cell).contains(&res) {
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
        let booked = in_play + l.consumed_by_spawns + l.vanished + l.spent_on_cards;
        if booked != l.created {
            return Err(format!(
                "Rohstoffe stimmen nicht: erzeugt {} aber verbucht {booked} ({l:?}, im Spiel {in_play})",
                l.created
            ));
        }
        Ok(())
    }
}
