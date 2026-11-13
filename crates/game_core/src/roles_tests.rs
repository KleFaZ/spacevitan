//! Tests fuer die Regel `kat_roles`: Kat 1 baut und gibt Stufen, Kat 2 kauft Bauern (Rekrut, die
//! Fabrik bleibt), Kat 3 kauft Spezialzuege, auch fuer den Koenig.
//!
//! Leere 2-Spieler-Karte (Quadrat 10x10, A startet auf (0,0), B auf (9,9)). Rohstoff-IDs: Ecke
//! `c` hat `3c` (Kat 1), `3c+1` (Kat 2), `3c+2` (Kat 3) in der Reihenfolge der Typen.

use crate::action::{Action, Event, PaySource};
use crate::ai::{SearchBot, SearchParams};
use crate::bot::{play_out, Bot};
use crate::cards::Card;
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use crate::map::Map;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn cat1(corner: u16) -> ResourceId {
    ResourceId(corner * 3)
}
fn cat2(corner: u16, k: u16) -> ResourceId {
    ResourceId((corner + k) % 4 * 3 + 1)
}
fn cat3(corner: u16, k: u16) -> ResourceId {
    ResourceId((corner + k) % 4 * 3 + 2)
}

fn game_with(roles: bool, market: &[Card]) -> Game {
    let mut config = Config::new(2, 1);
    config.regrow_rounds = [1, 1, 1, 1];
    config.kat_roles = roles;
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    let mut g = Game::with_map(config, map);
    g.state.market = market.to_vec();
    g
}

fn c(g: &Game, x: i32, y: i32) -> CellId {
    g.map.board.cell_at(x, y).expect("Feld auf der Karte")
}

fn king_id(g: &Game, p: PlayerId) -> PieceId {
    g.king(p).expect("Koenig lebt").id
}

fn buy(card: Card, from: PaySource, target: Option<PieceId>) -> Action {
    Action::BuyCard { card, from, target }
}

fn do_it(g: &mut Game, a: Action) -> Vec<Event> {
    let events = g
        .apply(a)
        .unwrap_or_else(|e| panic!("{a:?} sollte erlaubt sein: {e}"));
    g.check_invariants()
        .unwrap_or_else(|e| panic!("Invariante verletzt nach {a:?}: {e}"));
    events
}

/// Fabrik von A, darin nur die angegebenen Mengen.
fn factory_with(g: &mut Game, k1: u32, k2: u32, k3: u32) -> FactoryId {
    let f = g.setup_factory(P0, c(g, 2, 2));
    if k1 > 0 {
        g.setup_storage(f, cat1(0), k1);
    }
    if k2 > 0 {
        g.setup_storage(f, cat2(0, 0), k2);
    }
    if k3 > 0 {
        g.setup_storage(f, cat3(0, 0), k3);
    }
    f
}

fn can_buy(g: &Game, card: Card) -> bool {
    g.buy_actions(P0)
        .iter()
        .any(|a| matches!(a, Action::BuyCard { card: c, .. } if *c == card))
}

fn king_targets(g: &Game, card: Card) -> bool {
    let king = king_id(g, P0);
    g.buy_actions(P0).iter().any(|a| {
        matches!(a, Action::BuyCard { card: c, target: Some(t), .. } if *c == card && *t == king)
    })
}

// ----- Markt --------------------------------------------------------------------------------

#[test]
fn the_rule_is_off_by_default_and_can_be_set() {
    let mut config = Config::new(2, 1);
    assert!(!config.kat_roles);
    assert_eq!(config.pawn_cost, 4);
    config.set("kat_roles", 1).unwrap();
    config.set("pawn_cost", 6).unwrap();
    assert!(config.kat_roles);
    assert_eq!(config.pawn_cost, 6);
    assert_eq!(config.validate(), Ok(()));
    config.pawn_cost = 0;
    assert!(config.validate().is_err());
    config.pawn_cost = 31;
    assert!(config.validate().is_err());
}

#[test]
fn recruit_is_always_in_the_market() {
    for seed in 0..12 {
        let mut config = Config::new(2, seed);
        config.kat_roles = true;
        config.market_size = 0;
        let g = Game::new(config.clone()).unwrap();
        assert_eq!(g.state.market, vec![Card::Recruit], "ohne gezogene Karten");
        config.market_size = 3;
        let g = Game::new(config).unwrap();
        assert_eq!(g.state.market.len(), 4, "drei gezogene plus Rekrut");
        assert!(g.state.market.contains(&Card::Recruit));
    }
    let mut hex = Config::new(6, 3);
    hex.kat_roles = true;
    let g = Game::new(hex).unwrap();
    assert!(g.state.market.contains(&Card::Recruit));
    assert!(!g.state.market.contains(&Card::DiagonalStrike));
    g.check_invariants().unwrap();
}

#[test]
fn without_the_rule_the_market_is_unchanged() {
    let a = Game::new(Config::new(2, 5)).unwrap();
    let mut config = Config::new(2, 5);
    config.kat_roles = false;
    let b = Game::new(config).unwrap();
    assert_eq!(a.state.market, b.state.market);
    assert_eq!(
        crate::cards::draw_market(5, 4, crate::board::GridKind::Square),
        crate::cards::draw_market_with(5, 4, crate::board::GridKind::Square, &[])
    );
}

// ----- Bauern aus Kat 2 -----------------------------------------------------------------------

#[test]
fn a_pawn_costs_only_category_2_and_the_factory_stays() {
    let mut g = game_with(true, &[Card::Recruit]);
    let f = factory_with(&mut g, 2, 6, 6);
    let events = do_it(&mut g, buy(Card::Recruit, PaySource::Factory(f), None));
    let st = &g.state.factories[&f].storage;
    assert_eq!(st.get(cat2(0, 0)), 2, "vier Kat-2-Einheiten bezahlt");
    assert_eq!(st.get(cat3(0, 0)), 6, "Kat 3 unberuehrt");
    assert_eq!(st.get(cat1(0)), 2, "Kat 1 unberuehrt");
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Recruited { factory, .. } if *factory == f)));
    assert!(g.state.factories.contains_key(&f), "die Fabrik bleibt");
    assert_eq!(g.state.ledger.spent_on_cards, 4);
}

#[test]
fn a_pawn_cannot_be_paid_with_category_1_or_3() {
    let mut g = game_with(true, &[Card::Recruit]);
    factory_with(&mut g, 5, 3, 9);
    assert!(
        !can_buy(&g, Card::Recruit),
        "3 Kat-2-Einheiten reichen nicht"
    );
    let mut h = game_with(true, &[Card::Recruit]);
    factory_with(&mut h, 5, 0, 9);
    assert!(!can_buy(&h, Card::Recruit), "ohne Kat 2 kein Bauer");
    // Ohne die Regel kostet derselbe Rekrut eine Farbe plus Kat 2 und Kat 3.
    let mut old = game_with(false, &[Card::Recruit]);
    factory_with(&mut old, 2, 3, 3);
    assert!(can_buy(&old, Card::Recruit));
}

#[test]
fn the_pawn_price_is_a_setting() {
    let mut g = game_with(true, &[Card::Recruit]);
    g.config.pawn_cost = 2;
    let f = factory_with(&mut g, 0, 2, 0);
    do_it(&mut g, buy(Card::Recruit, PaySource::Factory(f), None));
    assert_eq!(g.state.factories[&f].storage.get(cat2(0, 0)), 0);
}

// ----- Spezialzuege aus Kat 3 -----------------------------------------------------------------

#[test]
fn special_moves_cost_only_category_3() {
    let mut g = game_with(true, &[Card::Horse]);
    let f = factory_with(&mut g, 2, 9, 3);
    let u = g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    assert!(!can_buy(&g, Card::Horse), "Pferd braucht 4 Kat-3-Einheiten");
    g.setup_storage(f, cat3(0, 0), 1);
    do_it(&mut g, buy(Card::Horse, PaySource::Factory(f), Some(u)));
    let st = &g.state.factories[&f].storage;
    assert_eq!(st.get(cat3(0, 0)), 0);
    assert_eq!(st.get(cat2(0, 0)), 9, "Kat 2 bleibt");
    assert!(g.state.pieces[&u].perks.horse);
}

#[test]
fn without_the_rule_special_moves_keep_their_old_price() {
    let mut g = game_with(false, &[Card::Spring]);
    let f = factory_with(&mut g, 0, 1, 2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    do_it(&mut g, buy(Card::Spring, PaySource::Factory(f), Some(u)));
    assert_eq!(
        g.state.factories[&f].storage.total(),
        0,
        "1 Kat 2 + 2 Kat 3"
    );
}

// ----- Der Koenig -----------------------------------------------------------------------------

#[test]
fn only_with_the_rule_can_the_king_get_cards() {
    for roles in [false, true] {
        let mut g = game_with(
            roles,
            &[Card::Horse, Card::Spring, Card::Lance, Card::DiagonalStrike],
        );
        factory_with(&mut g, 0, 9, 9);
        for card in [Card::Horse, Card::Spring, Card::Lance, Card::DiagonalStrike] {
            assert_eq!(king_targets(&g, card), roles, "{card:?}, Regel {roles}");
        }
    }
}

#[test]
fn the_king_cannot_be_trained_or_get_long_legs() {
    let mut g = game_with(true, &[Card::Training, Card::LongLegs]);
    factory_with(&mut g, 2, 12, 12);
    g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    assert!(!king_targets(&g, Card::Training));
    assert!(!king_targets(&g, Card::LongLegs));
    assert!(can_buy(&g, Card::Training), "Figuren schon");
}

#[test]
fn a_king_with_a_horse_jumps_two_fields() {
    let mut g = game_with(true, &[Card::Horse]);
    factory_with(&mut g, 0, 0, 9);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 5, 5));
    assert!(!g.piece_destinations(king).contains(&c(&g, 5, 7)));
    do_it(
        &mut g,
        buy(Card::Horse, PaySource::Factory(FactoryId(0)), Some(king)),
    );
    let dest = g.piece_destinations(king);
    for (x, y) in [(5, 7), (7, 5), (3, 5), (5, 3), (4, 4), (6, 6)] {
        assert!(dest.contains(&c(&g, x, y)), "({x},{y})");
    }
    assert!(dest.contains(&c(&g, 5, 6)), "die Nachbarfelder bleiben");
    assert!(!dest.contains(&c(&g, 8, 8)));
    assert!(!king_targets(&g, Card::Horse), "nur einmal je Karte");
}

#[test]
fn a_king_with_the_bishop_strike_steps_over_the_corner() {
    let mut g = game_with(true, &[Card::DiagonalStrike]);
    let f = factory_with(&mut g, 0, 0, 9);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 5, 5));
    g.setup_unit(P0, c(&g, 6, 6), 1, 1);
    g.setup_unit(P1, c(&g, 4, 4), 1, 1);
    do_it(
        &mut g,
        buy(Card::DiagonalStrike, PaySource::Factory(f), Some(king)),
    );
    let dest = g.piece_destinations(king);
    assert!(dest.contains(&c(&g, 6, 4)), "freies Diagonalfeld");
    assert!(dest.contains(&c(&g, 4, 6)));
    assert!(
        dest.contains(&c(&g, 4, 4)),
        "Gegner auf der Diagonale schlagen"
    );
    assert!(!dest.contains(&c(&g, 6, 6)), "eigene Figur blockiert");
}

#[test]
fn a_unit_with_the_bishop_strike_still_only_captures_diagonally() {
    let mut g = game_with(true, &[Card::DiagonalStrike]);
    let f = factory_with(&mut g, 0, 0, 9);
    let u = g.setup_unit(P0, c(&g, 3, 3), 1, 1);
    g.setup_unit(P1, c(&g, 2, 2), 1, 1);
    do_it(
        &mut g,
        buy(Card::DiagonalStrike, PaySource::Factory(f), Some(u)),
    );
    let dest = g.piece_destinations(u);
    assert!(dest.contains(&c(&g, 2, 2)), "Gegner diagonal");
    assert!(!dest.contains(&c(&g, 4, 4)), "freies Feld diagonal nicht");
}

#[test]
fn an_enemy_king_with_a_horse_threatens_more_cells() {
    let mut g = game_with(true, &[Card::Horse]);
    let f = g.setup_factory(P1, c(&g, 7, 7));
    g.setup_storage(f, cat3(1, 0), 9);
    g.setup_current(P1);
    let king = king_id(&g, P1);
    g.setup_teleport(king, c(&g, 5, 5));
    let before = g.threatened_cells(P0).len();
    do_it(&mut g, buy(Card::Horse, PaySource::Factory(f), Some(king)));
    assert!(g.threatened_cells(P0).len() > before);
}

// ----- Ganze Partien --------------------------------------------------------------------------

#[test]
fn strong_bots_play_whole_games_with_the_rule() {
    for seed in 0..3u64 {
        let mut config = Config::new(2, 60 + seed);
        config.kat_roles = true;
        config.max_rounds = 80;
        let mut game = Game::new(config).expect("Karte");
        let mut bots: Vec<Box<dyn Bot>> = (0..2)
            .map(|i| {
                Box::new(SearchBot::with_params(
                    seed + i,
                    SearchParams::with_budget(1_200),
                )) as Box<dyn Bot>
            })
            .collect();
        let mut bought = 0;
        play_out(&mut game, &mut bots, |g, _, a, _| {
            g.check_invariants().expect("Invarianten");
            if matches!(a, Action::BuyCard { .. }) {
                bought += 1;
            }
        });
        assert!(bought > 0, "Seed {seed}: es wurde nie gekauft");
        let _ = P1;
    }
}
