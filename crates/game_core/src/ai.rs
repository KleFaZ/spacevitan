//! Starker Computergegner ("stark"): rechnet Zuege voraus.
//!
//! **Suche.** Best-Reply-Search (Schadd und Winands, 2011) mit Alpha-Beta: Zwischen zwei eigenen
//! Zuegen zieht nur der Gegner, dessen Antwort fuer uns am gefaehrlichsten ist; die anderen
//! setzen in der Vorausschau aus. Bei zwei Spielern ist das genau das klassische Alpha-Beta der
//! Schachprogramme, bei mehr Spielern bleibt die Suche so tief genug, um Taktik zu sehen. Dazu
//! kommen iterative Vertiefung, eine Ruhesuche ueber Schlagzuege (damit ein Abtausch nicht
//! mitten drin bewertet wird), eine Verlaengerung bei angegriffenem Koenig und Killerzuege.
//! Gerechnet wird mit einem festen Knotenbudget statt einer Zeitgrenze: dieselbe Partie
//! verlaeuft auf jedem Rechner gleich (Wiedergaben, Tests, Fehlersuche).
//!
//! **Bewertung.** Material (Figuren nach Stufe, Fabriken als kommende Figuren), Druck auf
//! gegnerische Koenige, Sicherheit des eigenen Koenigs und die Wirtschaft: Bauplaetze besetzen,
//! fremde Farben holen und in die Fabrik bringen, rechtzeitig aufloesen. Die Wirtschaft steckt
//! in der Bewertung, weil sie ueber viele Zuege geht und keine Suche so weit reicht. Bewertet
//! wird immer aus Sicht des Spielers am Zug: eigener Wert minus Wert der Gegner (Mittel aus
//! dem staerksten und dem Durchschnitt).

use crate::action::Action;
use crate::board::Board;
use crate::bot::Bot;
use crate::game::Game;
use crate::ids::{CellId, PlayerId, ResourceId};
use crate::rng::Rng;
use crate::state::{Factory, Outcome, PieceKind};

const WIN: i32 = 1_000_000;
const INF: i32 = 2_000_000;
/// Hoechstens so viele Verlaengerungen pro Suchpfad, wenn ein Koenig angegriffen ist.
const MAX_EXTENSIONS: u32 = 2;
/// Ab so vielen Zeilen speichert der Abstandsspeicher nichts mehr (riesige Karten).
const DIST_ROW_LIMIT: usize = 2048;

/// Wert einer Figur nach Stufe. Stufe 1 zieht ein Feld geradeaus, ab Stufe 2 frei und weiter.
const UNIT_VALUE: [i32; 9] = [0, 100, 260, 400, 520, 620, 700, 760, 800];

/// In Debug-Builds (Tests) prueft die Suche jede erzeugte Stellung auf die Invarianten der
/// Regel-Engine; im Release kostet das nichts.
fn debug_check(g: &Game) {
    if cfg!(debug_assertions) {
        if let Err(e) = g.check_invariants() {
            panic!("Suche erzeugte einen ungueltigen Zustand: {e}");
        }
    }
}

fn unit_value(level: usize) -> i32 {
    UNIT_VALUE[level.min(UNIT_VALUE.len() - 1)]
}

/// Wie viel der Bot rechnet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchParams {
    /// Hoechstzahl untersuchter Stellungen pro Zug.
    pub node_budget: u64,
    /// Hoechste Suchtiefe in Halbzuegen (eigener Zug, beste Antwort, eigener Zug, ...).
    pub max_depth: u32,
    /// Wie viele Schlagzuege die Ruhesuche hoechstens anhaengt.
    pub quiescence_depth: u32,
}

impl SearchParams {
    pub const DEFAULT: SearchParams = SearchParams {
        node_budget: 20_000,
        max_depth: 12,
        quiescence_depth: 4,
    };

    /// Standard mit anderem Knotenbudget.
    pub fn with_budget(node_budget: u64) -> SearchParams {
        SearchParams {
            node_budget,
            ..SearchParams::DEFAULT
        }
    }
}

impl Default for SearchParams {
    fn default() -> Self {
        SearchParams::DEFAULT
    }
}

/// Gewichte der Bewertung. Alle Werte in Hundertsteln eines Bauern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weights {
    /// Je Stufe und je Feld, das eine Figur naeher am naechsten gegnerischen Koenig steht.
    pub pressure: i32,
    /// Ab diesem Abstand zaehlt kein Druck mehr.
    pub pressure_radius: u32,
    /// Fuer bis zu zwei eigene Figuren direkt beim eigenen Koenig.
    pub guard: i32,
    /// Gegnerische Figur in Reichweite des eigenen Koenigs (je Stufe und Naehe).
    pub danger: i32,
    /// Fabrik in Prozent des Werts der Figur, die sie jetzt ergaebe.
    pub factory_share: i32,
    pub factory_base: i32,
    /// Getragene neue Farbe in Prozent dessen, was sie der Fabrik an Stufe bringt.
    pub carry_share: i32,
    /// Je Feld Rueckweg zur Fabrik, solange der Koenig eine neue Farbe traegt.
    pub return_cost: i32,
    /// Je Feld Weg zu einer fremden Farbe, die die Fabrik noch nicht hat.
    pub source_step: i32,
    /// Freier Bauplatz (Kat-1-Vorkommen) in Reichweite des Koenigs.
    pub site_value: i32,
    pub site_step: i32,
    /// Getragene Farbe ohne eigene Fabrik.
    pub loose_color: i32,
    /// Abzug, wenn der eigene Koenig am Ende der Ruhesuche angegriffen ist.
    pub hanging_king: i32,
    /// Je Nachbarfeld eines gegnerischen Koenigs, das eigene Figuren abdecken oder das es am
    /// Rand gar nicht gibt. Ein Koenig ohne freies Fluchtfeld muss in einen Angriff ziehen.
    pub trap: i32,
    /// Eigener Koenig naeher an einem gegnerischen Koenig, der keine Figuren mehr hat.
    pub king_hunt: i32,
    /// 0: jede Figur jagt den naechsten gegnerischen Koenig, eingekesselt wird jeder (Standard,
    /// in 60 Viererpartien 49 statt 27 Siege); 1: alle jagen denselben Zielkoenig.
    pub focus: i32,
}

impl Weights {
    /// Setzt Gewichte aus einer Liste wie `trap=30,king_hunt=4` (zum Abstimmen).
    pub fn parse_overrides(mut self, list: &str) -> Result<Weights, String> {
        for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| format!("'{item}': erwartet name=wert"))?;
            let value: i32 = value
                .trim()
                .parse()
                .map_err(|_| format!("'{item}': keine Zahl"))?;
            let slot = match key.trim() {
                "pressure" => &mut self.pressure,
                "guard" => &mut self.guard,
                "danger" => &mut self.danger,
                "factory_share" => &mut self.factory_share,
                "factory_base" => &mut self.factory_base,
                "carry_share" => &mut self.carry_share,
                "return_cost" => &mut self.return_cost,
                "source_step" => &mut self.source_step,
                "site_value" => &mut self.site_value,
                "site_step" => &mut self.site_step,
                "loose_color" => &mut self.loose_color,
                "hanging_king" => &mut self.hanging_king,
                "trap" => &mut self.trap,
                "king_hunt" => &mut self.king_hunt,
                "focus" => &mut self.focus,
                "pressure_radius" => {
                    self.pressure_radius = value.max(1) as u32;
                    continue;
                }
                other => return Err(format!("unbekanntes Gewicht '{other}'")),
            };
            *slot = value;
        }
        Ok(self)
    }
}

impl Weights {
    pub const DEFAULT: Weights = Weights {
        pressure: 3,
        pressure_radius: 12,
        guard: 12,
        danger: 15,
        factory_share: 70,
        factory_base: 25,
        carry_share: 60,
        return_cost: 2,
        source_step: 2,
        site_value: 60,
        site_step: 3,
        loose_color: 20,
        hanging_king: 400,
        trap: 25,
        king_hunt: 6,
        focus: 0,
    };
}

impl Default for Weights {
    fn default() -> Self {
        Weights::DEFAULT
    }
}

/// Was die letzte Suche ergab (Anzeige, Tests, Abstimmen der Staerke).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SearchStats {
    pub nodes: u64,
    /// Tiefe der letzten vollstaendig oder teilweise durchsuchten Iteration.
    pub depth: u32,
    /// Bewertung des gewaehlten Zugs aus Sicht des Bots.
    pub score: i32,
}

/// Abstaende auf der Karte (ohne Figuren), zeilenweise bei Bedarf berechnet.
struct DistCache {
    rows: Vec<Option<Box<[u16]>>>,
    filled: usize,
}

impl DistCache {
    fn new() -> DistCache {
        DistCache {
            rows: Vec::new(),
            filled: 0,
        }
    }

    fn get(&mut self, board: &Board, from: CellId, to: CellId) -> u32 {
        if self.rows.len() != board.len() {
            self.rows = vec![None; board.len()];
            self.filled = 0;
        }
        if let Some(row) = &self.rows[from.idx()] {
            return row[to.idx()] as u32;
        }
        let row: Box<[u16]> = board
            .distances_from(from)
            .into_iter()
            .map(|d| d.min(u16::MAX as u32) as u16)
            .collect();
        let d = row[to.idx()] as u32;
        if self.filled < DIST_ROW_LIMIT {
            self.rows[from.idx()] = Some(row);
            self.filled += 1;
        }
        d
    }
}

pub struct SearchBot {
    params: SearchParams,
    weights: Weights,
    rng: Rng,
    root: PlayerId,
    nodes: u64,
    aborted: bool,
    killers: Vec<[Option<Action>; 2]>,
    dist: DistCache,
    /// Alle Kat-1-Vorkommen der Karte (Feld, Typ); pro Zug neu bestimmt.
    cat1: Vec<(CellId, ResourceId)>,
    /// Groesste Nachbarzahl der Karte (0 = noch nicht bestimmt).
    max_degree: u32,
    stats: SearchStats,
}

impl SearchBot {
    pub fn new(seed: u64) -> SearchBot {
        SearchBot::with_params(seed, SearchParams::DEFAULT)
    }

    pub fn with_params(seed: u64, params: SearchParams) -> SearchBot {
        SearchBot {
            params,
            weights: Weights::DEFAULT,
            rng: Rng::new(seed),
            root: PlayerId(0),
            nodes: 0,
            aborted: false,
            killers: Vec::new(),
            dist: DistCache::new(),
            cat1: Vec::new(),
            max_degree: 0,
            stats: SearchStats::default(),
        }
    }

    pub fn with_weights(mut self, weights: Weights) -> SearchBot {
        self.weights = weights;
        self
    }

    pub fn last_stats(&self) -> SearchStats {
        self.stats
    }

    /// Bewertung der Stellung aus Sicht von `player` (fuer Anzeige und Tests).
    pub fn evaluate_for(&mut self, game: &Game, player: PlayerId) -> i32 {
        self.root = player;
        self.prepare(game);
        self.evaluate(game)
    }

    fn prepare(&mut self, game: &Game) {
        self.max_degree = 0;
        self.cat1.clear();
        for cell in game.map.board.cells() {
            for &res in game.map.deposits_at(cell) {
                if game.map.category(res) == 1 {
                    self.cat1.push((cell, res));
                }
            }
        }
    }

    // ----- Suche ----------------------------------------------------------------------------

    fn search(&mut self, game: &Game) -> Action {
        self.root = game.current_player();
        self.prepare(game);
        self.nodes = 0;
        self.aborted = false;
        self.killers.clear();

        let mut moves = game.legal_actions();
        // Gleich gute Zuege in zufaelliger, aber durch den Seed festgelegter Reihenfolge.
        self.rng.shuffle(&mut moves);
        let mut moves = self.ordered(game, self.root, moves, 0);
        let mut best = moves[0];
        let mut best_score = -INF;
        let mut depth_done = 0;

        for depth in 1..=self.params.max_depth {
            let mut alpha = -INF;
            let mut iter_best: Option<(Action, i32)> = None;
            for &a in &moves {
                let mut child = game.clone();
                child.apply_unchecked(a);
                debug_check(&child);
                let v = self.min_node(&child, depth - 1, alpha, INF, 1, MAX_EXTENSIONS);
                if self.aborted {
                    break;
                }
                if iter_best.is_none_or(|(_, s)| v > s) {
                    iter_best = Some((a, v));
                }
                alpha = alpha.max(v);
            }
            // Auch eine abgebrochene Iteration zaehlt: der bisher beste Zug wurde zuerst
            // durchsucht, ein gefundener besserer Zug ist also sicher besser.
            if let Some((a, s)) = iter_best {
                best = a;
                best_score = s;
                depth_done = depth;
                if let Some(pos) = moves.iter().position(|&m| m == a) {
                    let m = moves.remove(pos);
                    moves.insert(0, m);
                }
            }
            if self.aborted || best_score.abs() >= WIN - 1000 {
                break;
            }
        }

        self.stats = SearchStats {
            nodes: self.nodes,
            depth: depth_done,
            score: best_score,
        };
        best
    }

    fn tick(&mut self) -> bool {
        self.nodes += 1;
        if self.nodes > self.params.node_budget {
            self.aborted = true;
        }
        self.aborted
    }

    /// Ergebnis einer beendeten Partie (oder eines ausgeschiedenen Bots) aus Sicht des Bots.
    fn terminal(&self, g: &Game, ply: usize) -> Option<i32> {
        let ply = ply as i32;
        if !g.state.alive[self.root.idx()] {
            return Some(-WIN + ply);
        }
        match g.state.outcome {
            Some(Outcome::Winner(p)) if p == self.root => Some(WIN - ply),
            Some(Outcome::Winner(_)) => Some(-WIN + ply),
            Some(Outcome::Draw) => Some(0),
            None => None,
        }
    }

    /// Der Bot ist am Zug.
    fn max_node(
        &mut self,
        g: &Game,
        depth: u32,
        mut alpha: i32,
        beta: i32,
        ply: usize,
        ext: u32,
    ) -> i32 {
        if let Some(t) = self.terminal(g, ply) {
            return t;
        }
        if self.tick() {
            return 0;
        }
        let (mut depth, mut ext) = (depth, ext);
        if depth == 0 {
            if ext > 0 && self.king_attacked(g, self.root) {
                depth = 1;
                ext -= 1;
            } else {
                return self.quiesce(g, alpha, beta, true, self.params.quiescence_depth, ply);
            }
        }
        let moves = g.legal_actions_for(self.root);
        let moves = self.ordered(g, self.root, moves, ply);
        let mut best = -INF;
        for a in moves {
            let mut child = g.clone();
            child.state.current = self.root;
            child.apply_unchecked(a);
            debug_check(&child);
            let v = self.min_node(&child, depth - 1, alpha, beta, ply + 1, ext);
            if self.aborted {
                return 0;
            }
            best = best.max(v);
            alpha = alpha.max(v);
            if alpha >= beta {
                self.store_killer(ply, a);
                break;
            }
        }
        best
    }

    /// Die Gegner sind dran; es zaehlt nur die fuer den Bot schlechteste Antwort.
    fn min_node(
        &mut self,
        g: &Game,
        depth: u32,
        alpha: i32,
        mut beta: i32,
        ply: usize,
        ext: u32,
    ) -> i32 {
        if let Some(t) = self.terminal(g, ply) {
            return t;
        }
        if self.tick() {
            return 0;
        }
        let (mut depth, mut ext) = (depth, ext);
        if depth == 0 {
            if ext > 0 && self.some_opponent_king_attacked(g) {
                depth = 1;
                ext -= 1;
            } else {
                return self.quiesce(g, alpha, beta, false, self.params.quiescence_depth, ply);
            }
        }
        let replies = self.opponent_moves(g, ply, false);
        if replies.is_empty() {
            // Kein Gegner kann etwas tun: weiter mit dem eigenen Zug.
            let mut next = g.clone();
            next.state.current = self.root;
            return self.max_node(&next, depth - 1, alpha, beta, ply + 1, ext);
        }
        let mut best = INF;
        for (who, a) in replies {
            let child = self.play_reply(g, who, a);
            let v = self.max_node(&child, depth - 1, alpha, beta, ply + 1, ext);
            if self.aborted {
                return 0;
            }
            best = best.min(v);
            beta = beta.min(v);
            if alpha >= beta {
                self.store_killer(ply, a);
                break;
            }
        }
        best
    }

    /// Schlagzuege bis zur Ruhe. `max_side`: der Bot ist am Zug.
    fn quiesce(
        &mut self,
        g: &Game,
        mut alpha: i32,
        mut beta: i32,
        max_side: bool,
        qdepth: u32,
        ply: usize,
    ) -> i32 {
        if let Some(t) = self.terminal(g, ply) {
            return t;
        }
        if self.tick() {
            return 0;
        }
        let mut stand = self.evaluate(g);
        if max_side {
            if self.king_attacked(g, self.root) {
                stand -= self.weights.hanging_king;
            }
            if stand >= beta || qdepth == 0 {
                return stand;
            }
            alpha = alpha.max(stand);
            let caps: Vec<Action> = g
                .legal_actions_for(self.root)
                .into_iter()
                .filter(|a| self.is_capture(g, self.root, a))
                .collect();
            let caps = self.ordered(g, self.root, caps, ply);
            let mut best = stand;
            for a in caps {
                let mut child = g.clone();
                child.state.current = self.root;
                child.apply_unchecked(a);
                debug_check(&child);
                let v = self.quiesce(&child, alpha, beta, false, qdepth - 1, ply + 1);
                if self.aborted {
                    return 0;
                }
                best = best.max(v);
                alpha = alpha.max(v);
                if alpha >= beta {
                    break;
                }
            }
            best
        } else {
            if stand <= alpha || qdepth == 0 {
                return stand;
            }
            beta = beta.min(stand);
            let mut best = stand;
            for (who, a) in self.opponent_moves(g, ply, true) {
                let child = self.play_reply(g, who, a);
                let v = self.quiesce(&child, alpha, beta, true, qdepth - 1, ply + 1);
                if self.aborted {
                    return 0;
                }
                best = best.min(v);
                beta = beta.min(v);
                if alpha >= beta {
                    break;
                }
            }
            best
        }
    }

    /// Zuege aller lebenden Gegner, sortiert. `captures_only`: nur Schlagzuege gegen den Bot.
    fn opponent_moves(
        &mut self,
        g: &Game,
        ply: usize,
        captures_only: bool,
    ) -> Vec<(PlayerId, Action)> {
        let mut keyed: Vec<(i32, PlayerId, Action)> = Vec::new();
        for p in 0..g.config.players {
            let who = PlayerId(p);
            if who == self.root || !g.state.alive[who.idx()] {
                continue;
            }
            for a in g.legal_actions_for(who) {
                if a == Action::Pass {
                    continue;
                }
                if captures_only && !self.captures_from(g, &a, self.root) {
                    continue;
                }
                let key = self.order_key(g, who, &a, ply);
                keyed.push((key, who, a));
            }
        }
        keyed.sort_by_key(|k| std::cmp::Reverse(k.0));
        keyed.into_iter().map(|(_, w, a)| (w, a)).collect()
    }

    /// Fuehrt die Antwort `a` von `who` aus; danach ist wieder der Bot am Zug.
    fn play_reply(&self, g: &Game, who: PlayerId, a: Action) -> Game {
        let mut child = g.clone();
        child.state.current = who;
        child.apply_unchecked(a);
        if child.state.outcome.is_none() && child.state.alive[self.root.idx()] {
            child.state.current = self.root;
        }
        debug_check(&child);
        child
    }

    fn store_killer(&mut self, ply: usize, a: Action) {
        if self.is_quiet(&a) {
            if self.killers.len() <= ply {
                self.killers.resize(ply + 1, [None, None]);
            }
            let slot = &mut self.killers[ply];
            if slot[0] != Some(a) {
                slot[1] = slot[0];
                slot[0] = Some(a);
            }
        }
    }

    fn is_quiet(&self, a: &Action) -> bool {
        matches!(a, Action::Move { .. })
    }

    /// Schlaegt `a` eine fremde Figur oder nimmt eine fremde Fabrik ein?
    fn is_capture(&self, g: &Game, mover: PlayerId, a: &Action) -> bool {
        let Action::Move { to, .. } = *a else {
            return false;
        };
        g.state.piece_at(to).is_some_and(|p| p.owner != mover)
            || g.state.factory_at(to).is_some_and(|f| f.owner != mover)
    }

    /// Schlaegt `a` eine Figur oder Fabrik von `victim`?
    fn captures_from(&self, g: &Game, a: &Action, victim: PlayerId) -> bool {
        let Action::Move { to, .. } = *a else {
            return false;
        };
        g.state.piece_at(to).is_some_and(|p| p.owner == victim)
            || g.state.factory_at(to).is_some_and(|f| f.owner == victim)
    }

    /// Reihenfolge: Koenig schlagen, wertvolle Figur schlagen, Fabrik einnehmen, bauen,
    /// aufloesen, Killerzuege, dann ruhige Zuege Richtung Gegner.
    fn order_key(&mut self, g: &Game, mover: PlayerId, a: &Action, ply: usize) -> i32 {
        match *a {
            Action::Move { piece, to } => {
                if let Some(victim) = g.state.piece_at(to).filter(|p| p.owner != mover) {
                    let attacker = match &g.state.pieces[&piece].kind {
                        PieceKind::King => 1,
                        PieceKind::Unit { level, .. } => *level as i32,
                    };
                    return match &victim.kind {
                        PieceKind::King => 1_000_000,
                        PieceKind::Unit { level, .. } => {
                            100_000 + unit_value(*level as usize) * 10 - attacker
                        }
                    };
                }
                if let Some(f) = g.state.factory_at(to).filter(|f| f.owner != mover) {
                    return 80_000 + f.storage.total().min(1000) as i32;
                }
                if let Some(slot) = self.killers.get(ply) {
                    if slot[0] == Some(*a) {
                        return 40_000;
                    }
                    if slot[1] == Some(*a) {
                        return 39_000;
                    }
                }
                // Ruhiger Zug: naeher an den naechsten gegnerischen Koenig ist meist besser.
                let board = &g.map.board;
                let mut best = u32::MAX;
                for p in g.state.pieces.values() {
                    if p.is_king() && p.owner != mover {
                        best = best.min(self.dist.get(board, p.cell, to));
                    }
                }
                -(best.min(1000) as i32)
            }
            Action::BuildFactory { .. } => 60_000,
            Action::SpawnPiece { .. } => 50_000,
            Action::Pass => -100_000,
        }
    }

    fn ordered(
        &mut self,
        g: &Game,
        mover: PlayerId,
        moves: Vec<Action>,
        ply: usize,
    ) -> Vec<Action> {
        let mut keyed: Vec<(i32, Action)> = moves
            .into_iter()
            .map(|a| (self.order_key(g, mover, &a, ply), a))
            .collect();
        keyed.sort_by_key(|k| std::cmp::Reverse(k.0));
        keyed.into_iter().map(|(_, a)| a).collect()
    }

    /// Kann ein Gegner von `owner` dessen Koenig im naechsten Zug schlagen?
    fn king_attacked(&mut self, g: &Game, owner: PlayerId) -> bool {
        let Some(kc) = g.king(owner).map(|k| k.cell) else {
            return false;
        };
        self.cell_attacked(g, kc, |p| p != owner)
    }

    fn some_opponent_king_attacked(&mut self, g: &Game) -> bool {
        let root = self.root;
        let kings: Vec<CellId> = g
            .state
            .pieces
            .values()
            .filter(|p| p.is_king() && p.owner != root)
            .map(|p| p.cell)
            .collect();
        kings
            .into_iter()
            .any(|kc| self.cell_attacked(g, kc, |p| p == root))
    }

    /// Erreicht eine Figur eines Spielers mit `by(spieler)` das Feld? Erst grob ueber den
    /// Abstand, nur bei Bedarf genau ueber die erlaubten Ziele.
    fn cell_attacked(&mut self, g: &Game, cell: CellId, by: impl Fn(PlayerId) -> bool) -> bool {
        let board = &g.map.board;
        let candidates: Vec<_> = g
            .state
            .pieces
            .values()
            .filter(|p| by(p.owner) && p.cell != cell)
            .map(|p| {
                let reach = match &p.kind {
                    PieceKind::King => 1,
                    PieceKind::Unit { range, .. } => *range as u32,
                };
                (p.id, p.cell, reach)
            })
            .collect();
        for (id, at, reach) in candidates {
            if self.dist.get(board, cell, at) <= reach && g.piece_destinations(id).contains(&cell) {
                return true;
            }
        }
        false
    }

    // ----- Bewertung ------------------------------------------------------------------------

    fn evaluate(&mut self, g: &Game) -> i32 {
        let n = g.config.players as usize;
        let mut kings: Vec<Option<CellId>> = vec![None; n];
        for p in g.state.pieces.values() {
            if p.is_king() {
                kings[p.owner.idx()] = Some(p.cell);
            }
        }
        if kings[self.root.idx()].is_none() {
            return -WIN;
        }
        let mine = self.player_value(g, self.root, &kings);
        let mut others = Vec::new();
        for p in 0..n {
            if p != self.root.idx() && g.state.alive[p] {
                others.push(self.player_value(g, PlayerId(p as u8), &kings));
            }
        }
        if others.is_empty() {
            return WIN;
        }
        let max = *others.iter().max().expect("nicht leer");
        let avg = others.iter().sum::<i32>() / others.len() as i32;
        mine - (max + avg) / 2
    }

    /// Groesste Zahl an Nachbarn eines Feldes (4 im Quadrat-, 6 im Hexgitter).
    fn max_degree(&mut self, board: &Board) -> u32 {
        if self.max_degree == 0 {
            self.max_degree = board
                .cells()
                .map(|c| board.neighbors(c).count() as u32)
                .max()
                .unwrap_or(0);
        }
        self.max_degree
    }

    fn player_value(&mut self, g: &Game, p: PlayerId, kings: &[Option<CellId>]) -> i32 {
        let w = self.weights;
        let board = &g.map.board;
        let st = &g.state;
        let Some(kc) = kings[p.idx()] else {
            return 0;
        };
        let enemy_kings: Vec<CellId> = kings
            .iter()
            .enumerate()
            .filter(|&(i, k)| i != p.idx() && k.is_some())
            .map(|(_, k)| k.expect("gefiltert"))
            .collect();
        let radius = w.pressure_radius;
        let mut v = 0;
        let mut guards = 0;

        let mine: Vec<(CellId, u32, i32)> = st
            .pieces
            .values()
            .filter(|x| x.owner == p)
            .map(|x| match &x.kind {
                PieceKind::King => (x.cell, 1, 0),
                PieceKind::Unit { level, range, .. } => (x.cell, *range as u32, *level as i32),
            })
            .collect();
        let has_units = mine.iter().any(|&(_, _, level)| level > 0);

        // Zielkoenig: der gegnerische Koenig, der den eigenen Figuren am naechsten ist. Alle
        // Figuren jagen denselben, sonst verteilt sich der Druck und keiner wird gefangen.
        let mut target: Option<CellId> = None;
        let mut target_cost = u32::MAX;
        for &ek in &enemy_kings {
            let mut cost = 0;
            for &(c, _, level) in &mine {
                if level > 0 {
                    cost += self.dist.get(board, ek, c);
                }
            }
            if !has_units {
                cost = self.dist.get(board, ek, kc);
            }
            if cost < target_cost {
                target_cost = cost;
                target = Some(ek);
            }
        }

        for piece in st.pieces.values() {
            if piece.owner == p {
                if let PieceKind::Unit { level, .. } = piece.kind {
                    let level = level as i32;
                    v += unit_value(level as usize);
                    let mut d = radius;
                    if w.focus != 0 {
                        if let Some(t) = target {
                            d = self.dist.get(board, t, piece.cell);
                        }
                    } else {
                        for &k in &enemy_kings {
                            d = d.min(self.dist.get(board, k, piece.cell));
                        }
                    }
                    v += w.pressure * level * (radius - d.min(radius)) as i32;
                    if guards < 2 && self.dist.get(board, kc, piece.cell) <= 2 {
                        guards += 1;
                        v += w.guard;
                    }
                }
            } else {
                let (level, reach) = match &piece.kind {
                    PieceKind::King => (1, 1),
                    PieceKind::Unit { level, range, .. } => (*level as i32, *range as u32),
                };
                let d = self.dist.get(board, kc, piece.cell);
                if d <= reach + 1 {
                    v -= w.danger * level * (reach + 2 - d) as i32;
                }
            }
        }

        // Zielkoenig einkesseln: Fluchtfelder abdecken, an den Rand draengen. Ein Koenig ohne
        // freies Fluchtfeld muss in einen Angriff ziehen (es gibt kein Patt).
        let hunted: Vec<CellId> = if w.focus != 0 {
            target.into_iter().collect()
        } else {
            enemy_kings.clone()
        };
        for ek in hunted {
            let max_degree = self.max_degree(board);
            let owner = st.piece_at(ek).map(|x| x.owner);
            let mut free = 0;
            for n in board.neighbors(ek) {
                if st.piece_at(n).is_some_and(|x| Some(x.owner) == owner) {
                    continue;
                }
                let covered = mine
                    .iter()
                    .any(|&(c, reach, _)| c != n && self.dist.get(board, n, c) <= reach);
                if !covered {
                    free += 1;
                }
            }
            v += w.trap * (max_degree - free.min(max_degree)) as i32;
            // Hat der Gegner keine Figuren mehr, jagt der eigene Koenig mit.
            let lone =
                owner.is_some_and(|o| !st.pieces.values().any(|x| x.owner == o && !x.is_king()));
            if lone && has_units {
                let d = self.dist.get(board, ek, kc).min(radius);
                v += w.king_hunt * (radius - d) as i32;
            }
        }

        // Fabriken: kommende Figuren. Die mit den meisten Farben (bei Gleichstand die naechste)
        // ist das Ziel fuer neue Farben.
        let is_cat1 = |r: ResourceId| g.map.category(r) == 1;
        let colors_of = |f: &Factory| f.storage.kinds().filter(|&r| is_cat1(r)).count();
        let mut target: Option<(&Factory, usize, u32)> = None;
        for f in st.factories.values().filter(|f| f.owner == p) {
            let colors = colors_of(f);
            v += unit_value(colors) * w.factory_share / 100 + w.factory_base;
            let d = self.dist.get(board, kc, f.cell);
            let better = match target {
                None => true,
                Some((_, c, dd)) => colors > c || (colors == c && d < dd),
            };
            if better {
                target = Some((f, colors, d));
            }
        }

        let carried: Vec<ResourceId> = st.carried[p.idx()]
            .kinds()
            .filter(|&r| is_cat1(r))
            .collect();
        match target {
            Some((f, colors, d_home)) => {
                let new = carried.iter().filter(|&&r| f.storage.get(r) == 0).count();
                if new > 0 {
                    let gain = unit_value(colors + new) - unit_value(colors);
                    v += gain * w.carry_share / 100 - w.return_cost * d_home as i32;
                } else {
                    // Naechste fremde Farbe: Vorkommen oder Beute mit einem Typ, den die
                    // Fabrik noch nicht hat. Der Wert entspricht dem, was der Koenig nach dem
                    // Einsammeln haette, abzueglich Hinweg.
                    let gain = unit_value(colors + 1) - unit_value(colors);
                    let base = gain * w.carry_share / 100 - 10;
                    let mut best = 0;
                    let home = f.cell;
                    for i in 0..self.cat1.len() {
                        let (cell, res) = self.cat1[i];
                        if f.storage.get(res) > 0 || st.depleted.contains(&(cell, res)) {
                            continue;
                        }
                        let val = base
                            - w.return_cost * self.dist.get(board, cell, home) as i32
                            - w.source_step * self.dist.get(board, cell, kc) as i32;
                        best = best.max(val);
                    }
                    for (&cell, pile) in &st.loot {
                        if pile.kinds().any(|r| is_cat1(r) && f.storage.get(r) == 0) {
                            let val = base
                                - w.return_cost * self.dist.get(board, cell, home) as i32
                                - w.source_step * self.dist.get(board, cell, kc) as i32;
                            best = best.max(val);
                        }
                    }
                    v += best;
                }
            }
            None => v += w.loose_color * carried.len() as i32,
        }

        // Freie Bauplaetze in Reichweite des Koenigs.
        let mut best_site = 0;
        for i in 0..self.cat1.len() {
            let (cell, res) = self.cat1[i];
            if st.depleted.contains(&(cell, res)) || st.factory_at(cell).is_some() {
                continue;
            }
            let min_dist = g.config.min_factory_distance;
            let mut free = true;
            for f in st.factories.values() {
                if self.dist.get(board, f.cell, cell) < min_dist {
                    free = false;
                    break;
                }
            }
            if free {
                let val = w.site_value - w.site_step * self.dist.get(board, cell, kc) as i32;
                best_site = best_site.max(val);
            }
        }
        v + best_site
    }
}

impl Bot for SearchBot {
    fn name(&self) -> &'static str {
        "stark"
    }

    fn choose(&mut self, game: &Game) -> Action {
        let legal = game.legal_actions();
        if legal.len() == 1 {
            self.stats = SearchStats::default();
            return legal[0];
        }
        self.search(game)
    }
}
