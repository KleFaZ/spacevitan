//! Tests fuer den Bot "stark": Taktik in kleinen Stellungen, Wirtschaft, Determinismus,
//! Budget und ein kleines Turnier gegen "gierig".
//!
//! Die Karten sind leer (2 Spieler, Quadrat 10x10, A startet auf (0,0), B auf (9,9)); Figuren
//! und Vorkommen werden gezielt gesetzt. Rohstoff-IDs: Ecke `c` hat den Kat-1-Typ `6c`.

use crate::action::Action;
use crate::ai::{SearchBot, SearchParams};
use crate::bot::{play_out, Bot, GreedyBot};
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, PieceId, PlayerId, ResourceId};
use crate::map::Map;
use crate::state::Outcome;
use std::sync::Arc;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn cat1(corner: u16) -> ResourceId {
    ResourceId(corner * 6)
}

fn blank(players: u8) -> Game {
    let config = Config::new(players, 1);
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    Game::with_map(config, map)
}

fn c(g: &Game, x: i32, y: i32) -> CellId {
    g.map.board.cell_at(x, y).expect("Feld auf der Karte")
}

fn put(g: &mut Game, x: i32, y: i32, res: ResourceId) {
    let cell = c(g, x, y);
    Arc::make_mut(&mut g.map).deposits[cell.idx()].push(res);
}

fn king_id(g: &Game, p: PlayerId) -> PieceId {
    g.king(p).expect("Koenig lebt").id
}

fn mv(piece: PieceId, to: CellId) -> Action {
    Action::Move { piece, to }
}

fn bot() -> SearchBot {
    SearchBot::with_params(7, SearchParams::with_budget(3_000))
}

fn choose(g: &Game) -> Action {
    let a = bot().choose(g);
    assert!(g.legal_actions().contains(&a), "{a:?} ist nicht erlaubt");
    a
}

#[test]
fn takes_the_enemy_king_when_it_can() {
    let mut g = blank(2);
    let hunter = g.setup_unit(P0, c(&g, 9, 8), 1, 1);
    assert_eq!(choose(&g), mv(hunter, c(&g, 9, 9)));
}

#[test]
fn takes_the_enemy_king_with_a_far_reaching_piece() {
    let mut g = blank(2);
    // Drei Schritte bis (9,9); der Bauer daneben kommt nicht ran und lenkt nur ab.
    let hunter = g.setup_unit(P0, c(&g, 7, 8), 3, 3);
    g.setup_unit(P0, c(&g, 9, 6), 1, 1);
    assert_eq!(choose(&g), mv(hunter, c(&g, 9, 9)));
}

#[test]
fn the_king_steps_away_instead_of_taking_a_protected_piece() {
    let mut g = blank(2);
    // Bauer von B neben dem Koenig von A, gedeckt durch einen zweiten Bauern dahinter.
    g.setup_unit(P1, c(&g, 1, 0), 1, 1);
    g.setup_unit(P1, c(&g, 2, 0), 1, 1);
    let king = king_id(&g, P0);
    assert_eq!(choose(&g), mv(king, c(&g, 0, 1)));
}

#[test]
fn takes_a_free_piece() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    g.setup_unit(P1, c(&g, 5, 6), 1, 1);
    assert_eq!(choose(&g), mv(u, c(&g, 5, 6)));
}

#[test]
fn does_not_trade_a_strong_piece_for_a_protected_pawn() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 3, 3);
    g.setup_unit(P1, c(&g, 5, 7), 1, 1);
    // Stufe-2-Figur von B deckt den Bauern (Abstand 2, frei ziehend).
    g.setup_unit(P1, c(&g, 6, 8), 2, 2);
    assert_ne!(choose(&g), mv(u, c(&g, 5, 7)));
}

#[test]
fn moves_a_threatened_piece_to_safety() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 4, 4), 2, 2);
    // Stufe-3-Figur von B greift (4,4) an.
    g.setup_unit(P1, c(&g, 4, 7), 3, 3);
    let a = choose(&g);
    let mut after = g.clone();
    after.apply(a).expect("erlaubt");
    assert!(
        after.state.pieces.contains_key(&u),
        "die Figur muss noch leben"
    );
    let cell = after.state.pieces[&u].cell;
    assert!(
        !after.threatened_cells(P0).contains(&cell),
        "{a:?}: die Figur steht weiter im Angriff"
    );
}

#[test]
fn builds_a_factory_on_its_own_category_1_deposit() {
    let mut g = blank(2);
    put(&mut g, 0, 0, cat1(0));
    assert_eq!(choose(&g), Action::BuildFactory { pay: cat1(0) });
}

#[test]
fn the_king_walks_towards_a_free_building_site() {
    let mut g = blank(2);
    put(&mut g, 0, 3, cat1(0));
    let king = king_id(&g, P0);
    assert_eq!(choose(&g), mv(king, c(&g, 0, 1)));
}

#[test]
fn brings_a_new_colour_home_before_spawning() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 2, 2));
    g.setup_storage(f, cat1(0), 1);
    g.setup_carry(P0, cat1(1), 1);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 2, 3));
    assert_eq!(
        choose(&g),
        mv(king, c(&g, 2, 2)),
        "erst abladen (Stufe 2), nicht sofort einen Bauern machen"
    );
}

#[test]
fn spawns_when_the_factory_is_threatened() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    g.setup_storage(f, cat1(0), 1);
    g.setup_storage(f, cat1(1), 1);
    // Figur von B kann die Fabrik im naechsten Zug einnehmen.
    g.setup_unit(P1, c(&g, 5, 7), 2, 2);
    assert_eq!(choose(&g), Action::SpawnPiece { factory: f });
}

#[test]
fn the_same_seed_gives_the_same_move() {
    let mut g = blank(2);
    g.setup_unit(P0, c(&g, 3, 3), 2, 2);
    g.setup_unit(P1, c(&g, 6, 6), 2, 2);
    let mut a = bot();
    let mut b = bot();
    assert_eq!(a.choose(&g), b.choose(&g));
    assert_eq!(a.last_stats(), b.last_stats());
}

#[test]
fn stays_within_the_node_budget() {
    let mut g = blank(2);
    g.setup_unit(P0, c(&g, 3, 3), 2, 2);
    g.setup_unit(P1, c(&g, 6, 6), 2, 2);
    for budget in [50, 500, 5_000] {
        let mut b = SearchBot::with_params(1, SearchParams::with_budget(budget));
        let a = b.choose(&g);
        assert!(g.legal_actions().contains(&a));
        assert!(
            b.last_stats().nodes <= budget + 1,
            "{} Knoten bei Budget {budget}",
            b.last_stats().nodes
        );
    }
}

#[test]
fn a_bigger_budget_searches_deeper() {
    let mut g = blank(2);
    g.setup_unit(P0, c(&g, 3, 3), 2, 2);
    g.setup_unit(P1, c(&g, 6, 6), 2, 2);
    let mut small = SearchBot::with_params(1, SearchParams::with_budget(200));
    let mut big = SearchBot::with_params(1, SearchParams::with_budget(20_000));
    small.choose(&g);
    big.choose(&g);
    assert!(big.last_stats().depth > small.last_stats().depth);
}

#[test]
fn a_whole_game_is_reproducible() {
    let run = || {
        let mut config = Config::new(2, 5);
        config.max_rounds = 25;
        let mut game = Game::new(config).expect("Karte");
        let mut bots: Vec<Box<dyn Bot>> = vec![
            Box::new(SearchBot::with_params(11, SearchParams::with_budget(400))),
            Box::new(GreedyBot::new(12)),
        ];
        let mut moves = Vec::new();
        play_out(&mut game, &mut bots, |_, _, a, _| moves.push(*a));
        moves
    };
    assert_eq!(run(), run());
}

#[test]
fn beats_the_greedy_bot_from_both_seats() {
    let mut wins = 0;
    let games = 4;
    for seed in 0..games {
        let mut config = Config::new(2, 40 + seed);
        config.max_rounds = 120;
        let mut game = Game::new(config).expect("Karte");
        let strong_seat = (seed % 2) as usize;
        let mut bots: Vec<Box<dyn Bot>> = Vec::new();
        for seat in 0..2 {
            if seat == strong_seat {
                bots.push(Box::new(SearchBot::with_params(
                    seed,
                    SearchParams::with_budget(1_500),
                )));
            } else {
                bots.push(Box::new(GreedyBot::new(seed)));
            }
        }
        let outcome = play_out(&mut game, &mut bots, |g, _, _, _| {
            g.check_invariants().expect("Invarianten");
        });
        if outcome == Outcome::Winner(PlayerId(strong_seat as u8)) {
            wins += 1;
        }
    }
    assert!(wins >= 3, "nur {wins} von {games} Partien gewonnen");
}

#[test]
fn evaluation_prefers_more_material() {
    let mut g = blank(2);
    let mut b = bot();
    let even = b.evaluate_for(&g, P0);
    g.setup_unit(P0, c(&g, 2, 2), 2, 2);
    let ahead = b.evaluate_for(&g, P0);
    assert!(ahead > even + 150, "{even} -> {ahead}");
    assert!(
        b.evaluate_for(&g, P1) < -150,
        "aus Sicht von B ist es schlechter"
    );
}
