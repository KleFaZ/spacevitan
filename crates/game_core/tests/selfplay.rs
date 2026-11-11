//! Selbstspiel: viele komplette Partien mit Bots. Nach jedem Zug werden alle Invarianten
//! geprueft. Ausserdem werden `legal_actions` und `apply` gegeneinander gefuzzt.

use game_core::action::Action;
use game_core::bot::{play_out, Bot, GreedyBot, RandomBot};
use game_core::config::Config;
use game_core::game::Game;
use game_core::ids::{CellId, FactoryId, PieceId, ResourceId};
use game_core::rng::Rng;
use game_core::state::Outcome;

fn config(players: u8, seed: u64, max_rounds: u32) -> Config {
    let mut c = Config::new(players, seed);
    c.max_rounds = max_rounds;
    c
}

fn bots(kinds: &[&str], seed: u64) -> Vec<Box<dyn Bot>> {
    kinds
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let s = seed * 31 + i as u64;
            match *k {
                "random" => Box::new(RandomBot::new(s)) as Box<dyn Bot>,
                _ => Box::new(GreedyBot::new(s)) as Box<dyn Bot>,
            }
        })
        .collect()
}

/// Spielt eine Partie und prueft nach jedem Zug die Invarianten.
fn checked(players: u8, seed: u64, max_rounds: u32, kinds: &[&str]) -> (Game, Outcome) {
    let mut game = Game::new(config(players, seed, max_rounds)).expect("Karte");
    let mut b = bots(kinds, seed);
    let outcome = play_out(&mut game, &mut b, |g, _, action, _| {
        if let Err(e) = g.check_invariants() {
            panic!(
                "Seed {seed}, nach {action:?} in Runde {}: {e}",
                g.state.round
            );
        }
    });
    (game, outcome)
}

#[test]
fn random_two_player_games_never_break_an_invariant() {
    for seed in 0..40 {
        let (game, _) = checked(2, seed, 150, &["random", "random"]);
        assert!(game.outcome().is_some());
    }
}

#[test]
fn random_four_player_games_never_break_an_invariant() {
    for seed in 0..15 {
        let (game, _) = checked(4, seed, 120, &["random"; 4]);
        assert!(game.outcome().is_some());
    }
}

#[test]
fn greedy_games_never_break_an_invariant_and_usually_produce_a_winner() {
    let mut winners = 0;
    for seed in 0..30 {
        let (_, outcome) = checked(2, seed, 300, &["greedy", "greedy"]);
        if matches!(outcome, Outcome::Winner(_)) {
            winners += 1;
        }
    }
    assert!(winners >= 15, "nur {winners} von 30 Partien mit Sieger");
}

#[test]
fn greedy_four_player_games_never_break_an_invariant() {
    for seed in 0..8 {
        checked(4, seed, 200, &["greedy"; 4]);
    }
}

#[test]
fn the_greedy_bot_beats_the_random_bot() {
    let mut greedy_wins = 0;
    for seed in 0..20 {
        let (_, a) = checked(2, seed, 300, &["greedy", "random"]);
        let (_, b) = checked(2, seed, 300, &["random", "greedy"]);
        greedy_wins += (a == Outcome::Winner(game_core::ids::PlayerId(0))) as u32;
        greedy_wins += (b == Outcome::Winner(game_core::ids::PlayerId(1))) as u32;
    }
    assert!(greedy_wins >= 36, "gierig gewann nur {greedy_wins} von 40");
}

#[test]
fn a_game_is_fully_reproducible_from_its_seed() {
    let (a, oa) = checked(2, 17, 300, &["greedy", "random"]);
    let (b, ob) = checked(2, 17, 300, &["greedy", "random"]);
    assert_eq!(oa, ob);
    assert_eq!(a.state, b.state);
    assert_eq!(a.map, b.map);
}

/// Zufaellige, meist unsinnige Aktion, um `apply` mit Muell zu fuettern.
fn junk_action(rng: &mut Rng, game: &Game) -> Action {
    let cells = game.map.board.len() as u32;
    match rng.below(4) {
        0 => Action::Move {
            piece: PieceId(rng.below(40) as u32),
            to: CellId(rng.below(cells as usize + 5) as u32),
        },
        1 => Action::BuildFactory {
            pay: ResourceId(rng.below(30) as u16),
        },
        2 => Action::SpawnPiece {
            factory: FactoryId(rng.below(40) as u32),
        },
        _ => Action::Pass,
    }
}

#[test]
fn legal_actions_and_apply_agree_on_every_action_including_junk() {
    let mut rng = Rng::new(2024);
    let mut checked_states = 0;
    for seed in 0..10 {
        for players in [2u8, 4] {
            let mut game = Game::new(config(players, seed, 80)).expect("Karte");
            let mut driver = GreedyBot::new(seed);
            let mut turn = 0;
            while game.outcome().is_none() {
                if turn % 4 == 0 {
                    checked_states += 1;
                    let legal = game.legal_actions();
                    for &action in &legal {
                        let mut copy = game.clone();
                        copy.apply(action).unwrap_or_else(|e| {
                            panic!("erlaubte Aktion {action:?} abgelehnt: {e}")
                        });
                        copy.check_invariants()
                            .expect("Invarianten nach erlaubter Aktion");
                    }
                    for _ in 0..30 {
                        let action = junk_action(&mut rng, &game);
                        let mut copy = game.clone();
                        let result = copy.apply(action);
                        assert_eq!(
                            result.is_ok(),
                            legal.contains(&action),
                            "{action:?}: {result:?}"
                        );
                        if result.is_err() {
                            assert_eq!(copy.state, game.state, "Ablehnung darf nichts aendern");
                        }
                    }
                }
                // Abwechselnd gierig und zufaellig ziehen, damit viele Situationen entstehen.
                let action = if turn % 3 == 0 {
                    let legal = game.legal_actions();
                    *rng.pick(&legal).unwrap()
                } else {
                    driver.choose(&game)
                };
                game.apply(action).expect("Bot waehlt nur Erlaubtes");
                turn += 1;
            }
        }
    }
    assert!(
        checked_states > 100,
        "zu wenige Zustaende geprueft: {checked_states}"
    );
}
