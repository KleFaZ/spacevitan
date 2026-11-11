//! Computergegner. Sie benutzen dieselbe Schnittstelle wie ein menschlicher Spieler:
//! `legal_actions` lesen, eine Aktion waehlen.
//!
//! - `RandomBot`: waehlt zufaellig. Dient als Fuzzer fuer die Regeln.
//! - `GreedyBot`: einfache Heuristik (sammeln, bauen, Figuren erzeugen, auf den Koenig zu).
//!   Keine Vorausschau; ein Platzhalter, bis es eine echte KI gibt.

use crate::action::{Action, Event};
use crate::game::Game;
use crate::ids::{CellId, PlayerId};
use crate::rng::Rng;
use crate::state::Outcome;

pub trait Bot {
    fn name(&self) -> &'static str;
    fn choose(&mut self, game: &Game) -> Action;
}

pub struct RandomBot {
    rng: Rng,
}

impl RandomBot {
    pub fn new(seed: u64) -> Self {
        RandomBot {
            rng: Rng::new(seed),
        }
    }
}

impl Bot for RandomBot {
    fn name(&self) -> &'static str {
        "zufall"
    }

    fn choose(&mut self, game: &Game) -> Action {
        let legal = game.legal_actions();
        *self
            .rng
            .pick(&legal)
            .expect("es gibt immer mindestens eine Aktion")
    }
}

pub struct GreedyBot {
    rng: Rng,
}

impl GreedyBot {
    pub fn new(seed: u64) -> Self {
        GreedyBot {
            rng: Rng::new(seed),
        }
    }

    /// Unter mehreren gleich guten Aktionen zufaellig waehlen.
    fn best_by<F: Fn(&Action) -> i64>(&mut self, actions: &[Action], score: F) -> Option<Action> {
        let best = actions.iter().map(&score).max()?;
        let top: Vec<Action> = actions
            .iter()
            .copied()
            .filter(|a| score(a) == best)
            .collect();
        self.rng.pick(&top).copied()
    }
}

fn nearest(dist: &[u32], targets: &[CellId]) -> u32 {
    targets
        .iter()
        .map(|t| dist[t.idx()])
        .min()
        .unwrap_or(u32::MAX)
}

impl Bot for GreedyBot {
    fn name(&self) -> &'static str {
        "gierig"
    }

    fn choose(&mut self, game: &Game) -> Action {
        let legal = game.legal_actions();
        if legal.len() == 1 {
            return legal[0];
        }
        let me = game.current_player();
        let board = &game.map.board;
        let state = &game.state;
        let king = game
            .king(me)
            .expect("lebender Spieler hat einen Koenig")
            .clone();
        let threatened = game.threatened_cells(me);

        let enemy_kings: Vec<CellId> = game
            .alive_players()
            .into_iter()
            .filter(|&p| p != me)
            .filter_map(|p| game.king(p).map(|k| k.cell))
            .collect();
        let enemy_at = |cell: CellId| state.piece_at(cell).is_some_and(|p| p.owner != me);
        let enemy_factory_at = |cell: CellId| state.factory_at(cell).is_some_and(|f| f.owner != me);

        let moves_of = |piece_is_king: bool| -> Vec<Action> {
            legal
                .iter()
                .copied()
                .filter(|a| match a {
                    Action::Move { piece, .. } => state.pieces[piece].is_king() == piece_is_king,
                    _ => false,
                })
                .collect()
        };
        let target = |a: &Action| match a {
            Action::Move { to, .. } => Some(*to),
            _ => None,
        };
        let king_moves = moves_of(true);
        let unit_moves = moves_of(false);
        let safe_king_moves: Vec<Action> = king_moves
            .iter()
            .copied()
            .filter(|a| !threatened.contains(&target(a).unwrap()))
            .collect();

        // 1. Gegnerischen Koenig schlagen, wenn moeglich.
        if let Some(a) = legal
            .iter()
            .find(|a| target(a).is_some_and(|t| enemy_kings.contains(&t)))
        {
            return *a;
        }

        // 2. Eigener Koenig bedroht: auf ein sicheres Feld, schlagen bevorzugt.
        if threatened.contains(&king.cell) && !safe_king_moves.is_empty() {
            return self
                .best_by(&safe_king_moves, |a| enemy_at(target(a).unwrap()) as i64)
                .unwrap();
        }

        // 3. Figur erzeugen (hoechste Stufe zuerst).
        let spawns: Vec<Action> = legal
            .iter()
            .copied()
            .filter(|a| matches!(a, Action::SpawnPiece { .. }))
            .collect();
        if let Some(a) = self.best_by(&spawns, |a| match a {
            Action::SpawnPiece { factory } => game
                .spawn_preview(&state.factories[factory])
                .map_or(0, |(level, range, _)| level as i64 * 10 + range as i64),
            _ => 0,
        }) {
            return a;
        }

        // 4. Fabrik bauen, solange es keine gibt.
        if game.factories_of(me).next().is_none() {
            if let Some(a) = legal
                .iter()
                .find(|a| matches!(a, Action::BuildFactory { .. }))
            {
                return *a;
            }
        }

        // 5. Mit einer Figur schlagen oder eine feindliche Fabrik einnehmen.
        let captures: Vec<Action> = unit_moves
            .iter()
            .copied()
            .filter(|a| {
                let t = target(a).unwrap();
                enemy_at(t) || enemy_factory_at(t)
            })
            .collect();
        if let Some(a) = self.best_by(&captures, |a| {
            let t = target(a).unwrap();
            let loot = state.factory_at(t).map_or(0, |f| f.storage.total() as i64);
            loot + if enemy_at(t) { 5 } else { 0 }
        }) {
            return a;
        }

        // 6. Koenig: sammeln, abladen, Fabrikfeld fuer das Erzeugen freimachen.
        let own_factories: Vec<CellId> = game.factories_of(me).map(|f| f.cell).collect();
        let carried = &state.carried[me.idx()];
        let cat1_cells: Vec<CellId> = board
            .cells()
            .filter(|&c| {
                game.available_deposits(c)
                    .iter()
                    .any(|&r| game.map.category(r) == 1)
            })
            .filter(|&c| !state.factory_at(c).is_some_and(|f| f.owner != me))
            .collect();
        let blocking_spawn = game
            .factories_of(me)
            .any(|f| f.cell == king.cell && game.spawn_preview(f).is_some());
        let king_goal: Option<Vec<CellId>> = if blocking_spawn {
            // Irgendein Nachbarfeld: nur weg vom Fabrikfeld.
            Some(board.neighbors(king.cell).collect())
        } else if !carried.is_empty() && !own_factories.is_empty() {
            Some(own_factories.clone())
        } else if !cat1_cells.contains(&king.cell) {
            Some(cat1_cells.clone())
        } else {
            None // steht auf Kat 1 und sammelt
        };
        if let Some(goals) = king_goal.filter(|g| !g.is_empty()) {
            let here = nearest(&board.distances_from(king.cell), &goals);
            let candidates = if safe_king_moves.is_empty() {
                &king_moves
            } else {
                &safe_king_moves
            };
            let improving: Vec<Action> = candidates
                .iter()
                .copied()
                .filter(|a| {
                    let t = target(a).unwrap();
                    nearest(&board.distances_from(t), &goals) < here || goals.contains(&t)
                })
                .collect();
            if let Some(a) = self.best_by(&improving, |a| {
                -(nearest(&board.distances_from(target(a).unwrap()), &goals) as i64)
            }) {
                return a;
            }
        }

        // 7. Figuren auf den naechsten gegnerischen Koenig zu.
        if let Some(a) = self.best_by(&unit_moves, |a| {
            let t = target(a).unwrap();
            let d = nearest(&board.distances_from(t), &enemy_kings) as i64;
            -d * 2 - threatened.contains(&t) as i64
        }) {
            return a;
        }

        // 8. Sonst: sicherer Koenigszug oder irgendetwas Erlaubtes.
        if let Some(a) = self.rng.pick(&safe_king_moves) {
            return *a;
        }
        *self.rng.pick(&legal).expect("mindestens eine Aktion")
    }
}

/// Spielt eine Partie zu Ende. `bots[i]` steuert Spieler `i`.
/// `on_turn` wird nach jedem Zug aufgerufen (z. B. fuer Invariantenpruefung, Statistik, Anzeige).
pub fn play_out(
    game: &mut Game,
    bots: &mut [Box<dyn Bot>],
    mut on_turn: impl FnMut(&Game, PlayerId, &Action, &[Event]),
) -> Outcome {
    assert_eq!(
        bots.len(),
        game.config.players as usize,
        "ein Bot pro Spieler"
    );
    while game.outcome().is_none() {
        let player = game.current_player();
        let action = bots[player.idx()].choose(game);
        let events = game.apply(action).unwrap_or_else(|e| {
            panic!("Bot {} waehlte {action:?}: {e}", bots[player.idx()].name())
        });
        on_turn(game, player, &action, &events);
    }
    game.outcome().expect("Schleife endet nur mit Ergebnis")
}
