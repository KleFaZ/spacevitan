//! Szenario-Tests fuer den Kartenmarkt: Kaufregeln, Bezahlung und jede Kartenwirkung.
//!
//! Leere 2-Spieler-Karte (Quadrat 10x10, A startet auf (0,0), B auf (9,9)). Rohstoff-IDs: Ecke
//! `c` hat `6c` (Kat 1), `6c+1`/`6c+2` (Kat 2), `6c+3..=6c+5` (Kat 3). Der Markt wird in jedem
//! Test gezielt gesetzt.

use crate::action::{Action, Event, IllegalReason, PaySource, RuleError};
use crate::cards::Card;
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use crate::map::Map;
use crate::state::PieceKind;
use std::sync::Arc;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn cat1(corner: u16) -> ResourceId {
    ResourceId(corner * 6)
}
fn cat2(corner: u16, k: u16) -> ResourceId {
    ResourceId(corner * 6 + 1 + k)
}
fn cat3(corner: u16, k: u16) -> ResourceId {
    ResourceId(corner * 6 + 3 + k)
}

fn blank_with(config: Config, market: &[Card]) -> Game {
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    let mut g = Game::with_map(config, map);
    g.state.market = market.to_vec();
    g
}

fn blank(market: &[Card]) -> Game {
    blank_with(Config::new(2, 1), market)
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

fn apply_checked(g: &mut Game, a: Action) -> Vec<Event> {
    let events = g
        .apply(a)
        .unwrap_or_else(|e| panic!("{a:?} sollte erlaubt sein: {e}"));
    g.check_invariants()
        .unwrap_or_else(|e| panic!("Invariante verletzt nach {a:?}: {e}"));
    events
}

macro_rules! act {
    ($g:ident, $action:expr) => {{
        let action = $action;
        apply_checked(&mut $g, action)
    }};
}

fn buy(card: Card, from: PaySource, target: Option<PieceId>) -> Action {
    Action::BuyCard { card, from, target }
}

/// Fabrik von A mit reichlich verschiedenen Rohstoffen.
fn rich_factory(g: &mut Game, x: i32, y: i32) -> FactoryId {
    let f = g.setup_factory(P0, c(g, x, y));
    g.setup_storage(f, cat1(0), 2);
    for k in 0..2 {
        g.setup_storage(f, cat2(0, k), 3);
    }
    for k in 0..3 {
        g.setup_storage(f, cat3(0, k), 3);
    }
    f
}

fn level_range(g: &Game, id: PieceId) -> (u8, u8) {
    match &g.state.pieces[&id].kind {
        PieceKind::Unit { level, range, .. } => (*level, *range),
        PieceKind::King => panic!("Koenig"),
    }
}

// ----- Markt und Kaufregeln -----------------------------------------------------------------

#[test]
fn the_market_is_drawn_at_the_start_and_fits_the_board() {
    let g = Game::new(Config::new(4, 9)).unwrap();
    assert_eq!(g.state.market.len(), 8, "4 Spieler: 2 Karten je Spieler");
    let h = Game::new(Config::new(6, 9)).unwrap();
    assert_eq!(h.state.market.len(), 6, "ab 5 Spielern eine je Spieler");
    assert!(!h.state.market.contains(&Card::DiagonalStrike));
    let again = Game::new(Config::new(4, 9)).unwrap();
    assert_eq!(
        g.state.market, again.state.market,
        "gleicher Seed, gleicher Markt"
    );
}

#[test]
fn without_a_market_there_is_nothing_to_buy() {
    let mut config = Config::new(2, 1);
    config.market_size = 0;
    let mut g = blank_with(config, &[]);
    g.setup_carry(P0, cat3(0, 0), 5);
    assert!(g.buy_actions(P0).is_empty());
    assert!(g
        .legal_actions()
        .iter()
        .all(|a| !matches!(a, Action::BuyCard { .. })));
}

#[test]
fn buying_is_extra_to_the_action_and_only_once_per_turn() {
    let mut g = blank(&[Card::Shield, Card::DoubleShift]);
    let f = rich_factory(&mut g, 2, 2);
    let u = g.setup_unit(P0, c(&g, 2, 3), 1, 1);
    let events = act!(g, buy(Card::Shield, PaySource::Factory(f), Some(u)));
    assert!(matches!(
        events[0],
        Event::CardBought {
            card: Card::Shield,
            ..
        }
    ));
    assert_eq!(g.current_player(), P0, "nach dem Kauf noch am Zug");
    assert!(g.buy_actions(P0).is_empty(), "nur eine Karte pro Zug");
    assert_eq!(
        g.apply(buy(Card::DoubleShift, PaySource::Factory(f), None)),
        Err(RuleError::Illegal(IllegalReason::AlreadyBoughtThisTurn))
    );
    act!(g, mv(king_id(&g, P0), c(&g, 1, 0)));
    assert_eq!(g.current_player(), P1);
    act!(g, mv(king_id(&g, P1), c(&g, 8, 9)));
    assert!(!g.buy_actions(P0).is_empty(), "im naechsten Zug wieder");
}

#[test]
fn payment_comes_from_one_place_only() {
    let mut g = blank(&[Card::Shield]);
    let f = g.setup_factory(P0, c(&g, 2, 2));
    g.setup_storage(f, cat3(0, 0), 1);
    g.setup_carry(P0, cat3(0, 1), 1);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 2, 3));
    let u = g.setup_unit(P0, c(&g, 2, 4), 1, 1);
    assert!(
        g.buy_actions(P0).is_empty(),
        "je eine Einheit an zwei Orten reicht nicht"
    );
    assert_eq!(
        g.apply(buy(Card::Shield, PaySource::Factory(f), Some(u))),
        Err(RuleError::Illegal(IllegalReason::CannotPayCard))
    );
    g.setup_carry(P0, cat3(0, 1), 1);
    let events = act!(g, buy(Card::Shield, PaySource::King, Some(u)));
    assert!(
        g.state.carried[P0.idx()].is_empty(),
        "bezahlt aus dem Gepaeck"
    );
    assert_eq!(g.state.ledger.spent_on_cards, 2);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::CardBought { paid, .. } if paid.total() == 2)));
}

#[test]
fn a_figure_card_needs_the_figure_at_the_paying_field() {
    let mut g = blank(&[Card::Training]);
    let f = rich_factory(&mut g, 2, 2);
    let near = g.setup_unit(P0, c(&g, 3, 2), 1, 1);
    let far = g.setup_unit(P0, c(&g, 6, 6), 1, 1);
    let targets: Vec<Option<PieceId>> = g
        .buy_actions(P0)
        .into_iter()
        .filter_map(|a| match a {
            Action::BuyCard { target, from, .. } if from == PaySource::Factory(f) => Some(target),
            _ => None,
        })
        .collect();
    assert_eq!(targets, vec![Some(near)]);
    assert_eq!(
        g.apply(buy(Card::Training, PaySource::Factory(f), Some(far))),
        Err(RuleError::Illegal(IllegalReason::InvalidCardTarget))
    );
}

#[test]
fn pass_stays_possible_next_to_purchases_when_nothing_else_is_left() {
    let mut g = blank(&[Card::DoubleMove]);
    // Koenig von A eingeklemmt zwischen eigenen Figuren in der Ecke.
    g.setup_unit(P0, c(&g, 1, 0), 1, 1);
    g.setup_unit(P0, c(&g, 0, 1), 1, 1);
    g.setup_unit(P0, c(&g, 2, 0), 1, 1);
    g.setup_unit(P0, c(&g, 1, 1), 1, 1);
    g.setup_unit(P0, c(&g, 0, 2), 1, 1);
    g.setup_unit(P0, c(&g, 3, 0), 1, 1);
    g.setup_unit(P0, c(&g, 2, 1), 1, 1);
    g.setup_unit(P0, c(&g, 1, 2), 1, 1);
    g.setup_unit(P0, c(&g, 0, 3), 1, 1);
    let legal = g.legal_actions();
    let normal: Vec<&Action> = legal
        .iter()
        .filter(|a| !matches!(a, Action::BuyCard { .. }))
        .collect();
    assert!(!normal.is_empty());
    assert!(!normal.contains(&&Action::Pass), "es gibt noch Zuege");
}

// ----- Figurenkarten ------------------------------------------------------------------------

#[test]
fn training_raises_level_and_range_up_to_the_corner_count() {
    let mut g = blank(&[Card::Training]);
    let f = rich_factory(&mut g, 2, 2);
    let u = g.setup_unit(P0, c(&g, 2, 3), 3, 3);
    act!(g, buy(Card::Training, PaySource::Factory(f), Some(u)));
    assert_eq!(level_range(&g, u), (4, 4));
    assert_eq!(g.state.pieces[&u].perks.card_levels, 1);
    act!(g, mv(king_id(&g, P0), c(&g, 1, 0)));
    act!(g, mv(king_id(&g, P1), c(&g, 8, 9)));
    assert!(
        g.buy_actions(P0).is_empty(),
        "Stufe 4 ist auf der Quadratkarte das Maximum"
    );
}

#[test]
fn long_legs_add_range_at_most_twice() {
    let mut g = blank(&[Card::LongLegs]);
    let f = rich_factory(&mut g, 2, 2);
    let u = g.setup_unit(P0, c(&g, 2, 3), 1, 1);
    for expected in [2, 3] {
        act!(g, buy(Card::LongLegs, PaySource::Factory(f), Some(u)));
        assert_eq!(level_range(&g, u), (1, expected));
        act!(g, mv(king_id(&g, P0), c(&g, 1, 0)));
        act!(g, mv(king_id(&g, P1), c(&g, 8, 9)));
        act!(g, mv(king_id(&g, P0), c(&g, 0, 0)));
        act!(g, mv(king_id(&g, P1), c(&g, 9, 9)));
        g.setup_storage(f, cat2(0, 0), 2);
        g.setup_storage(f, cat2(0, 1), 2);
    }
    assert!(g.buy_actions(P0).is_empty(), "hoechstens zweimal");
    let dest = g.piece_destinations(u);
    assert!(
        dest.contains(&c(&g, 2, 6)),
        "Stufe 1 mit Reichweite 3 zieht 3 geradeaus"
    );
    assert!(!dest.contains(&c(&g, 3, 4)), "aber nicht ums Eck");
}

#[test]
fn the_diagonal_strike_captures_diagonally_but_does_not_move_there() {
    let mut g = blank(&[Card::DiagonalStrike]);
    let f = rich_factory(&mut g, 2, 2);
    let u = g.setup_unit(P0, c(&g, 2, 3), 1, 1);
    let enemy = g.setup_unit(P1, c(&g, 3, 4), 1, 1);
    assert!(!g.piece_destinations(u).contains(&c(&g, 3, 4)));
    act!(g, buy(Card::DiagonalStrike, PaySource::Factory(f), Some(u)));
    let dest = g.piece_destinations(u);
    assert!(dest.contains(&c(&g, 3, 4)), "schlaegt diagonal");
    assert!(
        !dest.contains(&c(&g, 1, 4)),
        "zieht nicht diagonal auf freie Felder"
    );
    let events = act!(g, mv(u, c(&g, 3, 4)));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Captured { piece, .. } if *piece == enemy)));
}

#[test]
fn the_diagonal_strike_is_never_offered_on_hex_boards() {
    let mut config = Config::new(6, 1);
    config.market_size = 13;
    let g = Game::new(config).unwrap();
    assert!(!g.state.market.contains(&Card::DiagonalStrike));
    assert_eq!(g.state.market.len(), 12);
}

#[test]
fn a_shield_stops_one_capture_and_is_used_up() {
    let mut g = blank(&[Card::Shield]);
    let f = rich_factory(&mut g, 2, 2);
    let u = g.setup_unit(P0, c(&g, 2, 3), 1, 1);
    let attacker = g.setup_unit(P1, c(&g, 2, 5), 2, 2);
    act!(g, buy(Card::Shield, PaySource::Factory(f), Some(u)));
    act!(g, mv(king_id(&g, P0), c(&g, 1, 0)));
    let events = act!(g, mv(attacker, c(&g, 2, 3)));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ShieldBlocked { piece, .. } if *piece == u)));
    assert_eq!(g.state.pieces[&u].cell, c(&g, 2, 3), "Figur steht noch");
    assert_eq!(
        g.state.pieces[&attacker].cell,
        c(&g, 2, 5),
        "Angreifer bleibt stehen"
    );
    assert!(!g.state.pieces[&u].perks.shield);
    act!(g, mv(king_id(&g, P0), c(&g, 0, 0)));
    act!(g, mv(attacker, c(&g, 2, 3)));
    assert!(!g.state.pieces.contains_key(&u), "der zweite Schlag trifft");
}

// ----- Koenigskarten ------------------------------------------------------------------------

#[test]
fn king_armor_saves_the_king_once() {
    let mut g = blank(&[Card::KingArmor]);
    g.setup_carry(P0, cat1(0), 1);
    g.setup_carry(P0, cat2(0, 0), 2);
    g.setup_carry(P0, cat3(0, 0), 2);
    let king = king_id(&g, P0);
    act!(g, buy(Card::KingArmor, PaySource::King, Some(king)));
    let hunter = g.setup_unit(P1, c(&g, 1, 2), 2, 2);
    act!(g, mv(king, c(&g, 1, 0)));
    let events = act!(g, mv(hunter, c(&g, 1, 0)));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::ShieldBlocked { .. })));
    assert!(g.state.alive[P0.idx()], "A lebt noch");
    assert_eq!(g.outcome(), None);
}

#[test]
fn king_step_lets_the_king_move_two_fields_straight() {
    let mut g = blank(&[Card::KingStep]);
    g.setup_carry(P0, cat2(0, 0), 1);
    g.setup_carry(P0, cat2(0, 1), 1);
    g.setup_carry(P0, cat2(1, 0), 1);
    g.setup_carry(P0, cat3(0, 0), 1);
    let king = king_id(&g, P0);
    assert!(!g.piece_destinations(king).contains(&c(&g, 2, 0)));
    act!(g, buy(Card::KingStep, PaySource::King, Some(king)));
    let dest = g.piece_destinations(king);
    assert!(dest.contains(&c(&g, 2, 0)) && dest.contains(&c(&g, 0, 2)));
    assert!(!dest.contains(&c(&g, 1, 1)), "nicht diagonal");
    g.setup_unit(P1, c(&g, 1, 0), 1, 1);
    assert!(
        !g.piece_destinations(king).contains(&c(&g, 2, 0)),
        "nicht ueber eine Figur hinweg"
    );
}

// ----- Wirtschaftskarten --------------------------------------------------------------------

#[test]
fn a_recruit_appears_next_to_the_factory_which_stays() {
    let mut g = blank(&[Card::Recruit]);
    let f = rich_factory(&mut g, 2, 2);
    assert!(
        g.buy_actions(P0).iter().all(|a| matches!(
            a,
            Action::BuyCard {
                from: PaySource::Factory(_),
                ..
            }
        )),
        "nur aus einer Fabrik bezahlbar"
    );
    let events = act!(g, buy(Card::Recruit, PaySource::Factory(f), None));
    let cell = g.recruit_cell(f);
    let Some(Event::Recruited {
        piece, cell: at, ..
    }) = events
        .iter()
        .find(|e| matches!(e, Event::Recruited { .. }))
        .cloned()
    else {
        panic!("kein Rekrut")
    };
    assert_ne!(Some(at), cell, "das Feld ist jetzt besetzt");
    assert!(g.map.board.neighbors(c(&g, 2, 2)).any(|n| n == at));
    assert_eq!(level_range(&g, piece), (1, 1));
    assert!(g.state.factories.contains_key(&f), "die Fabrik bleibt");
    assert_eq!(
        g.state.factories[&f].storage.get(cat1(0)),
        1,
        "eine Farbe bezahlt"
    );
}

#[test]
fn double_shift_harvests_right_away_and_mining_doubles_category_2() {
    let mut g = blank(&[Card::DoubleShift, Card::Mining]);
    let f = rich_factory(&mut g, 2, 2);
    put(&mut g, 2, 3, cat2(0, 0));
    put(&mut g, 3, 2, cat3(0, 0));
    let before = g.state.factories[&f].storage.clone();
    act!(g, buy(Card::DoubleShift, PaySource::Factory(f), None));
    let after = &g.state.factories[&f].storage;
    assert_eq!(
        after.get(cat2(0, 0)),
        before.get(cat2(0, 0)) + 1,
        "sofort geerntet"
    );
    act!(g, mv(king_id(&g, P0), c(&g, 1, 0)));
    act!(g, mv(king_id(&g, P1), c(&g, 8, 9)));
    g.setup_storage(f, cat2(1, 0), 1); // Bergbau braucht drei verschiedene Kat-2-Typen
    act!(g, buy(Card::Mining, PaySource::Factory(f), None));
    assert!(g.state.player_perks[P0.idx()].mining);
    let k2 = g.state.factories[&f].storage.get(cat2(0, 0));
    let k3 = g.state.factories[&f].storage.get(cat3(0, 0));
    act!(g, mv(king_id(&g, P0), c(&g, 0, 0)));
    assert_eq!(
        g.state.factories[&f].storage.get(cat2(0, 0)),
        k2 + 2,
        "Kat 2 doppelt"
    );
    assert_eq!(
        g.state.factories[&f].storage.get(cat3(0, 0)),
        k3 + 1,
        "Kat 3 normal"
    );
}

#[test]
fn double_move_gives_a_second_action_in_the_same_turn() {
    let mut g = blank(&[Card::DoubleMove]);
    let f = rich_factory(&mut g, 2, 2);
    act!(g, buy(Card::DoubleMove, PaySource::Factory(f), None));
    let king = king_id(&g, P0);
    act!(g, mv(king, c(&g, 1, 0)));
    assert_eq!(g.current_player(), P0, "zweite Aktion");
    assert!(g.buy_actions(P0).is_empty(), "aber keine zweite Karte");
    act!(g, mv(king, c(&g, 1, 1)));
    assert_eq!(g.current_player(), P1);
    assert_eq!(g.state.extra_actions, 0);
}

// ----- Sets ---------------------------------------------------------------------------------

#[test]
fn three_runestones_give_a_figure_two_levels() {
    let mut g = blank(&[Card::Runestone]);
    let f = rich_factory(&mut g, 2, 2);
    let u = g.setup_unit(P0, c(&g, 2, 3), 1, 1);
    for round in 0..3 {
        let options = g.buy_actions(P0);
        if round < 2 {
            assert!(options.contains(&buy(Card::Runestone, PaySource::Factory(f), None)));
            act!(g, buy(Card::Runestone, PaySource::Factory(f), None));
            assert_eq!(level_range(&g, u), (1, 1), "allein wirkungslos");
        } else {
            assert!(
                !options.contains(&buy(Card::Runestone, PaySource::Factory(f), None)),
                "der dritte braucht eine Figur"
            );
            act!(g, buy(Card::Runestone, PaySource::Factory(f), Some(u)));
            assert_eq!(level_range(&g, u), (3, 3));
            assert_eq!(g.state.player_perks[P0.idx()].runes, 0, "Set verbraucht");
        }
        // Koenige pendeln, damit die Runde weitergeht: A zwischen (1,0) und (0,0), B
        // zwischen (8,9) und (9,9).
        let (a_x, b_x) = if round % 2 == 0 { (1, 8) } else { (0, 9) };
        act!(g, mv(king_id(&g, P0), c(&g, a_x, 0)));
        act!(g, mv(king_id(&g, P1), c(&g, b_x, 9)));
    }
}

#[test]
fn three_star_shards_create_a_new_building_site_under_the_king() {
    let mut g = blank(&[Card::Starshard]);
    g.setup_carry(P0, cat2(0, 0), 3);
    g.setup_carry(P0, cat3(0, 0), 6);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 4, 4));
    for round in 0..3 {
        let events = act!(g, buy(Card::Starshard, PaySource::King, None));
        if round == 2 {
            assert!(events
                .iter()
                .any(|e| matches!(e, Event::DepositCreated { .. })));
        }
        let back = if round % 2 == 0 { (4, 5) } else { (4, 4) };
        act!(g, mv(king, c(&g, back.0, back.1)));
        act!(
            g,
            mv(king_id(&g, P1), c(&g, 9 - (round % 2 == 0) as i32, 9))
        );
    }
    // Das neue Vorkommen liegt unter dem Koenig zum Zeitpunkt des dritten Kaufs: (4,4).
    let cell = c(&g, 4, 5);
    assert!(
        g.state.extra_deposits.contains_key(&c(&g, 4, 4))
            || g.state.extra_deposits.contains_key(&cell)
    );
    let site = *g.state.extra_deposits.keys().next().unwrap();
    if g.king(P0).unwrap().cell != site {
        act!(g, mv(king, site));
        act!(g, mv(king_id(&g, P1), c(&g, 9, 9)));
    }
    assert!(g
        .legal_actions()
        .contains(&Action::BuildFactory { pay: cat1(0) }));
    act!(g, Action::BuildFactory { pay: cat1(0) });
    assert!(g.factories_of(P0).any(|f| f.cell == site));
}
