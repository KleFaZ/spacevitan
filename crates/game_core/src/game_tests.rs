//! Szenario-Tests fuer die Regel-Engine. Jeder Test pruft eine Regel aus dem Regelwerk
//! und danach die Invarianten des Zustands.
//!
//! Rohstoff-IDs auf der 4-Ecken-Karte: Ecke `c` hat die Typen `6c` (Kat 1), `6c+1`/`6c+2`
//! (Kat 2) und `6c+3..=6c+5` (Kat 3). Die Karten dieser Tests sind leer; Vorkommen werden
//! gezielt gesetzt.

use crate::action::{Action, Event, IllegalReason, RuleError};
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, PieceId, PlayerId, ResourceId};
use crate::map::Map;
use crate::state::{Inventory, Outcome, PieceKind};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);
const P3: PlayerId = PlayerId(3);

fn cat1(corner: u16) -> ResourceId {
    ResourceId(corner * 6)
}
fn cat2(corner: u16, k: u16) -> ResourceId {
    ResourceId(corner * 6 + 1 + k)
}
fn cat3(corner: u16, k: u16) -> ResourceId {
    ResourceId(corner * 6 + 3 + k)
}

fn blank_with(config: Config) -> Game {
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    Game::with_map(config, map)
}

fn blank(players: u8) -> Game {
    blank_with(Config::new(players, 1))
}

fn c(g: &Game, x: i32, y: i32) -> CellId {
    g.map.board.cell_at(x, y).expect("Feld auf der Karte")
}

fn put(g: &mut Game, x: i32, y: i32, res: ResourceId) {
    let cell = c(g, x, y);
    g.map.deposits[cell.idx()].push(res);
}

fn king_id(g: &Game, p: PlayerId) -> PieceId {
    g.king(p).expect("Koenig lebt").id
}

fn king_to(g: &Game, p: PlayerId, x: i32, y: i32) -> Action {
    Action::Move {
        piece: king_id(g, p),
        to: c(g, x, y),
    }
}

fn apply_checked(g: &mut Game, a: Action) -> Vec<Event> {
    let events = g
        .apply(a)
        .unwrap_or_else(|e| panic!("{a:?} sollte erlaubt sein: {e}"));
    g.check_invariants()
        .unwrap_or_else(|e| panic!("Invariante verletzt nach {a:?}: {e}"));
    events
}

/// Wertet die Aktion zuerst aus (liest `g`) und fuehrt sie dann aus (aendert `g`).
macro_rules! act {
    ($g:ident, $action:expr) => {{
        let action = $action;
        apply_checked(&mut $g, action)
    }};
}

fn carry(g: &Game, p: PlayerId, res: ResourceId) -> u32 {
    g.state.carried[p.idx()].get(res)
}

fn inv(items: &[(ResourceId, u32)]) -> Inventory {
    let mut i = Inventory::new();
    for &(r, n) in items {
        i.add(r, n);
    }
    i
}

// ----- Start und Zugfolge --------------------------------------------------------------------

#[test]
fn every_player_starts_with_only_a_king_in_their_corner() {
    for players in [2u8, 4] {
        let g = blank(players);
        assert_eq!(g.state.pieces.len(), players as usize);
        for p in 0..players {
            let p = PlayerId(p);
            let king = g.king(p).expect("Koenig");
            assert_eq!(king.cell, g.map.start_cell(p));
            assert!(g.factories_of(p).next().is_none());
            assert!(g.state.carried[p.idx()].is_empty());
        }
        assert_eq!(g.current_player(), P0);
        assert_eq!(g.state.round, 1);
        assert_eq!(g.outcome(), None);
        g.check_invariants().unwrap();
    }
}

#[test]
fn two_players_start_in_opposite_corners() {
    let g = blank(2);
    assert_eq!(g.king(P0).unwrap().cell, c(&g, 0, 0));
    assert_eq!(g.king(P1).unwrap().cell, c(&g, 9, 9));
}

#[test]
fn a_king_in_the_corner_has_exactly_two_moves_and_nothing_else() {
    let g = blank(2);
    let actions = g.legal_actions();
    assert_eq!(actions, vec![king_to(&g, P0, 1, 0), king_to(&g, P0, 0, 1)]);
}

#[test]
fn the_king_moves_one_step_and_never_diagonally() {
    let mut g = blank(2);
    let king = g.king(P0).unwrap().id;
    g.setup_teleport(king, c(&g, 4, 4));
    let mut targets: Vec<(i32, i32)> = g
        .piece_destinations(king)
        .iter()
        .map(|&cell| g.map.board.coord(cell))
        .collect();
    targets.sort();
    assert_eq!(targets, vec![(3, 4), (4, 3), (4, 5), (5, 4)]);
}

#[test]
fn turns_alternate_and_a_round_ends_after_the_last_player() {
    let mut g = blank(4);
    assert_eq!((g.current_player(), g.state.round), (P0, 1));
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!((g.current_player(), g.state.round), (P1, 1));
    act!(g, king_to(&g, P1, 8, 0));
    act!(g, king_to(&g, P2, 8, 9));
    assert_eq!((g.current_player(), g.state.round), (P3, 1));
    act!(g, king_to(&g, P3, 1, 9));
    assert_eq!((g.current_player(), g.state.round), (P0, 2));
    assert_eq!(g.state.turns_played, 4);
}

#[test]
fn a_player_cannot_move_someone_elses_piece() {
    let mut g = blank(2);
    let enemy_king = king_id(&g, P1);
    let to = c(&g, 8, 9);
    assert_eq!(
        g.apply(Action::Move {
            piece: enemy_king,
            to
        }),
        Err(RuleError::Illegal(IllegalReason::NotYourPiece))
    );
}

#[test]
fn illegal_moves_are_explained() {
    let mut g = blank(2);
    let king = king_id(&g, P0);
    let own = g.setup_unit(P0, c(&g, 1, 0), 1, 1);
    let _ = own;
    let far = c(&g, 5, 5);
    assert_eq!(
        g.apply(Action::Move {
            piece: king,
            to: far
        }),
        Err(RuleError::Illegal(IllegalReason::CannotReach))
    );
    assert_eq!(
        g.apply(Action::Move {
            piece: king,
            to: c(&g, 1, 0)
        }),
        Err(RuleError::Illegal(IllegalReason::OccupiedByOwnPiece))
    );
    assert_eq!(
        g.apply(Action::Move {
            piece: king,
            to: CellId(10_000)
        }),
        Err(RuleError::Illegal(IllegalReason::CellOutsideBoard))
    );
    assert_eq!(
        g.apply(Action::Move {
            piece: PieceId(999),
            to: far
        }),
        Err(RuleError::Illegal(IllegalReason::NoSuchPiece))
    );
}

#[test]
fn an_illegal_action_changes_nothing() {
    let mut g = blank(2);
    let before = g.state.clone();
    let _ = g.apply(Action::Pass);
    let _ = g.apply(Action::BuildFactory { pay: cat1(0) });
    let _ = g.apply(Action::SpawnPiece {
        factory: crate::ids::FactoryId(0),
    });
    assert_eq!(g.state, before);
}

#[test]
fn every_legal_action_is_accepted_and_keeps_the_invariants() {
    let mut g = blank(4);
    put(&mut g, 1, 0, cat1(0));
    g.setup_unit(P0, c(&g, 4, 4), 2, 2);
    g.setup_factory(P0, c(&g, 7, 7));
    for _ in 0..3 {
        for action in g.legal_actions() {
            let mut copy = g.clone();
            copy.apply(action)
                .unwrap_or_else(|e| panic!("{action:?}: {e}"));
            copy.check_invariants()
                .unwrap_or_else(|e| panic!("{action:?}: {e}"));
        }
        let first = g.legal_actions()[0];
        act!(g, first);
    }
}

#[test]
fn pass_is_rejected_while_other_actions_exist() {
    let mut g = blank(2);
    assert_eq!(
        g.apply(Action::Pass),
        Err(RuleError::Illegal(IllegalReason::PassNotAllowed))
    );
}

#[test]
fn pass_is_the_only_action_for_a_player_without_any_piece() {
    // Kuenstlicher Zustand: im echten Spiel scheidet ein Spieler ohne Koenig aus.
    let mut g = blank(2);
    let king = king_id(&g, P0);
    g.state.remove_piece(king);
    assert_eq!(g.legal_actions(), vec![Action::Pass]);
    let events = g.apply(Action::Pass).unwrap();
    assert_eq!(events[0], Event::Passed { player: P0 });
    assert_eq!(g.current_player(), P1);
}

// ----- Sammeln -------------------------------------------------------------------------------

#[test]
fn the_king_collects_everything_on_his_cell_at_the_end_of_his_turn() {
    let mut g = blank(2);
    put(&mut g, 1, 0, cat1(0));
    put(&mut g, 1, 0, cat2(0, 0));
    put(&mut g, 1, 0, cat3(1, 2));
    put(&mut g, 0, 1, cat3(0, 0));
    let events = act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat1(0)), 1);
    assert_eq!(carry(&g, P0, cat2(0, 0)), 1);
    assert_eq!(carry(&g, P0, cat3(1, 2)), 1);
    assert_eq!(
        carry(&g, P0, cat3(0, 0)),
        0,
        "Nachbarfeld wird nicht gesammelt"
    );
    assert!(events.contains(&Event::Collected {
        player: P0,
        cell: c(&g, 1, 0),
        gained: inv(&[(cat1(0), 1), (cat2(0, 0), 1), (cat3(1, 2), 1)]),
    }));
    assert_eq!(g.state.ledger.created, 3);
}

#[test]
fn the_yield_per_round_scales_the_amount() {
    let mut config = Config::new(2, 1);
    config.yield_per_round = 3;
    let mut g = blank_with(config);
    put(&mut g, 1, 0, cat1(0));
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat1(0)), 3);
}

#[test]
fn the_king_keeps_collecting_every_turn_he_stays_on_a_deposit() {
    let mut g = blank(2);
    put(&mut g, 1, 0, cat1(0));
    g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    let unit = g.pieces_of(P0).find(|p| !p.is_king()).unwrap().id;
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat1(0)), 1);
    act!(g, king_to(&g, P1, 8, 9));
    act!(
        g,
        Action::Move {
            piece: unit,
            to: c(&g, 6, 5)
        }
    );
    assert_eq!(
        carry(&g, P0, cat1(0)),
        2,
        "Koenig steht noch dort und sammelt wieder"
    );
}

#[test]
fn only_the_king_collects_from_fields_other_pieces_do_not() {
    let mut g = blank(2);
    put(&mut g, 6, 5, cat2(0, 0));
    g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    let unit = g.pieces_of(P0).find(|p| !p.is_king()).unwrap().id;
    act!(
        g,
        Action::Move {
            piece: unit,
            to: c(&g, 6, 5)
        }
    );
    assert!(g.state.carried[P0.idx()].is_empty());
    assert_eq!(g.state.ledger.created, 0);
}

// ----- Fabriken ------------------------------------------------------------------------------

#[test]
fn building_needs_a_category_1_deposit_under_the_king() {
    let mut g = blank(2);
    g.setup_carry(P0, cat1(0), 3); // Gepaeck allein reicht nicht
    assert!(!g
        .legal_actions()
        .iter()
        .any(|a| matches!(a, Action::BuildFactory { .. })));
    assert_eq!(
        g.apply(Action::BuildFactory { pay: cat1(0) }),
        Err(RuleError::Illegal(IllegalReason::NoCategory1DepositHere))
    );
    put(&mut g, 0, 0, cat2(0, 0));
    assert_eq!(
        g.apply(Action::BuildFactory { pay: cat2(0, 0) }),
        Err(RuleError::Illegal(IllegalReason::PayTypeIsNotCategory1))
    );
    assert_eq!(
        g.apply(Action::BuildFactory {
            pay: ResourceId(999)
        }),
        Err(RuleError::Illegal(IllegalReason::UnknownResource))
    );
    put(&mut g, 0, 0, cat1(1));
    assert_eq!(
        g.legal_actions()
            .into_iter()
            .filter(|a| matches!(a, Action::BuildFactory { .. }))
            .collect::<Vec<_>>(),
        vec![Action::BuildFactory { pay: cat1(1) }],
        "nur das Kat-1-Vorkommen auf dem Feld zaehlt, nicht das Gepaeck"
    );
}

#[test]
fn building_consumes_the_category_1_deposit_and_puts_it_into_the_storage() {
    let mut g = blank(2);
    put(&mut g, 0, 0, cat1(1));
    put(&mut g, 0, 0, cat2(0, 0));
    let events = act!(g, Action::BuildFactory { pay: cat1(1) });
    let factory = g.factories_of(P0).next().expect("Fabrik").clone();
    assert_eq!(factory.cell, c(&g, 0, 0));
    assert!(events.contains(&Event::FactoryBuilt {
        factory: factory.id,
        owner: P0,
        cell: c(&g, 0, 0),
    }));
    assert!(g.state.depleted.contains(&(c(&g, 0, 0), cat1(1))));
    assert_eq!(
        g.available_deposits(c(&g, 0, 0)),
        vec![cat2(0, 0)],
        "nur Kat 1 ist weg"
    );
    // 1x Kat 1 aus dem Vorkommen; Kat 2 sammelt der Koenig am Zugende und laedt es ab,
    // ausserdem erntet die Fabrik ihr eigenes Feld.
    assert_eq!(factory.storage.get(cat1(1)), 1);
    assert_eq!(factory.storage.get(cat2(0, 0)), 2);
    assert_eq!(g.deposit_remaining(c(&g, 0, 0), cat1(1)), Some(0));
}

#[test]
fn a_used_category_1_deposit_never_comes_back() {
    let mut g = blank(2);
    put(&mut g, 1, 0, cat1(0));
    act!(g, king_to(&g, P0, 1, 0));
    act!(g, king_to(&g, P1, 8, 9));
    act!(g, Action::BuildFactory { pay: cat1(0) });
    let f = g.factories_of(P0).next().unwrap().id;
    act!(g, king_to(&g, P1, 9, 9));
    act!(g, king_to(&g, P0, 2, 0));
    act!(g, king_to(&g, P1, 8, 9));
    act!(g, Action::SpawnPiece { factory: f });
    // Fabrik ist aufgeloest; das Feld hat kein Kat 1 mehr, hier kann nie wieder gebaut werden.
    let pawn = g.state.piece_at(c(&g, 1, 0)).unwrap().id;
    act!(g, king_to(&g, P1, 9, 9));
    act!(
        g,
        Action::Move {
            piece: pawn,
            to: c(&g, 1, 1)
        }
    );
    act!(g, king_to(&g, P1, 8, 9));
    let before = g.state.ledger.created;
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(g.state.ledger.created, before, "nichts mehr zu sammeln");
    assert!(g.available_deposits(c(&g, 1, 0)).is_empty());
    act!(g, king_to(&g, P1, 9, 9));
    assert!(!g
        .legal_actions()
        .iter()
        .any(|a| matches!(a, Action::BuildFactory { .. })));
}

#[test]
fn spawning_right_after_building_gives_only_a_pawn() {
    let mut g = blank(2);
    put(&mut g, 1, 0, cat1(0));
    put(&mut g, 2, 0, cat3(0, 0));
    act!(g, king_to(&g, P0, 1, 0));
    act!(g, king_to(&g, P1, 8, 9));
    act!(g, Action::BuildFactory { pay: cat1(0) });
    act!(g, king_to(&g, P1, 9, 9));
    act!(g, king_to(&g, P0, 1, 1));
    act!(g, king_to(&g, P1, 8, 9));
    let f = g.factories_of(P0).next().unwrap().id;
    act!(g, Action::SpawnPiece { factory: f });
    let piece = g.state.piece_at(c(&g, 1, 0)).unwrap();
    assert!(
        matches!(
            piece.kind,
            PieceKind::Unit {
                level: 1,
                range: 1,
                ..
            }
        ),
        "Kat 3 im Lager macht keine groessere Reichweite mehr: {:?}",
        piece.kind
    );
}

#[test]
fn the_king_unloads_everything_into_the_factory_he_stands_on_at_turn_end() {
    let mut g = blank(2);
    put(&mut g, 1, 0, cat1(0));
    act!(g, king_to(&g, P0, 1, 0)); // sammelt 1 x Kat 1
    act!(g, king_to(&g, P1, 8, 9));
    let events = act!(g, Action::BuildFactory { pay: cat1(0) });
    // 1 Einheit aus dem verbauten Vorkommen + 1 Einheit, die der Koenig trug und ablaedt.
    let factory = g.factories_of(P0).next().unwrap();
    assert_eq!(factory.storage.get(cat1(0)), 2);
    assert!(g.state.carried[P0.idx()].is_empty());
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Unloaded { player, .. } if *player == P0)));
}

#[test]
fn the_king_does_not_unload_into_a_foreign_factory() {
    let mut g = blank(2);
    g.setup_factory(P1, c(&g, 1, 0));
    g.setup_carry(P0, cat2(0, 0), 1);
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat2(0, 0)), 1);
    assert!(
        g.state.factory_at(c(&g, 1, 0)).is_none(),
        "die Fabrik wurde eingenommen"
    );
}

#[test]
fn factories_keep_a_minimum_distance_to_each_other() {
    // Standard: Abstand mindestens 3.
    for (fx, fy, allowed) in [
        (1, 0, false),
        (2, 0, false),
        (3, 0, true),
        (2, 1, true),
        (1, 1, false),
    ] {
        let mut g = blank(2);
        g.setup_factory(P1, c(&g, fx, fy));
        put(&mut g, 0, 0, cat1(0));
        let offered = g
            .legal_actions()
            .iter()
            .any(|a| matches!(a, Action::BuildFactory { .. }));
        // (2,1) hat Abstand 3 zu (0,0), (1,1) hat Abstand 2.
        assert_eq!(offered, allowed, "Fabrik bei ({fx},{fy})");
    }
}

#[test]
fn the_minimum_distance_is_configurable() {
    let mut config = Config::new(2, 1);
    config.min_factory_distance = 2;
    let mut g = blank_with(config);
    g.setup_factory(P1, c(&g, 2, 0));
    put(&mut g, 0, 0, cat1(0));
    assert!(g
        .legal_actions()
        .iter()
        .any(|a| matches!(a, Action::BuildFactory { .. })));
}

#[test]
fn the_distance_rule_also_applies_to_the_own_factories_and_explains_itself() {
    let mut g = blank(2);
    g.setup_factory(P0, c(&g, 0, 2));
    put(&mut g, 0, 0, cat1(0));
    assert_eq!(
        g.apply(Action::BuildFactory { pay: cat1(0) }),
        Err(RuleError::Illegal(IllegalReason::TooCloseToAnotherFactory))
    );
}

#[test]
fn no_second_factory_on_the_same_cell() {
    let mut g = blank(2);
    g.setup_factory(P0, c(&g, 0, 0));
    put(&mut g, 0, 0, cat1(0));
    assert_eq!(
        g.apply(Action::BuildFactory { pay: cat1(0) }),
        Err(RuleError::Illegal(IllegalReason::FactoryAlreadyHere))
    );
}

#[test]
fn a_factory_collects_categories_two_and_three_from_its_cell_and_neighbors_only() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    put(&mut g, 5, 5, cat2(0, 0)); // eigenes Feld
    put(&mut g, 6, 5, cat3(0, 0)); // Nachbar
    put(&mut g, 4, 5, cat1(0)); // Kat 1 sammelt eine Fabrik nie
    put(&mut g, 7, 5, cat2(0, 1)); // zwei Felder weit
    put(&mut g, 6, 6, cat2(0, 1)); // diagonal, kein Nachbar
    let events = act!(g, king_to(&g, P0, 1, 0));
    let storage = &g.state.factories[&f].storage;
    assert_eq!(*storage, inv(&[(cat2(0, 0), 1), (cat3(0, 0), 1)]));
    assert!(events.contains(&Event::FactoryYield {
        factory: f,
        gained: inv(&[(cat2(0, 0), 1), (cat3(0, 0), 1)]),
    }));
}

#[test]
fn factories_produce_only_on_their_owners_turn() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    put(&mut g, 5, 5, cat2(0, 0));
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(g.state.factories[&f].storage.get(cat2(0, 0)), 1);
    act!(g, king_to(&g, P1, 8, 9));
    assert_eq!(
        g.state.factories[&f].storage.get(cat2(0, 0)),
        1,
        "nicht im Zug des Gegners"
    );
    act!(g, king_to(&g, P0, 2, 0));
    assert_eq!(g.state.factories[&f].storage.get(cat2(0, 0)), 2);
}

#[test]
fn multiple_deposits_on_a_cell_each_yield_and_the_yield_is_scaled() {
    let mut config = Config::new(2, 1);
    config.yield_per_round = 2;
    let mut g = blank_with(config);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    put(&mut g, 5, 5, cat2(0, 0));
    put(&mut g, 5, 5, cat3(0, 1));
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(
        g.state.factories[&f].storage,
        inv(&[(cat2(0, 0), 2), (cat3(0, 1), 2)])
    );
}

// ----- Figuren erstellen ---------------------------------------------------------------------

#[test]
fn spawning_needs_a_category_1_resource_in_the_storage() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    g.setup_storage(f, cat3(0, 0), 4);
    assert_eq!(
        g.apply(Action::SpawnPiece { factory: f }),
        Err(RuleError::Illegal(IllegalReason::FactoryHasNoCategory1))
    );
}

#[test]
fn spawning_dissolves_the_factory_into_a_piece_with_level_and_range_from_the_storage() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    g.setup_storage(f, cat1(0), 3); // mehrere Einheiten derselben Farbe zaehlen einfach
    g.setup_storage(f, cat1(1), 1);
    g.setup_storage(f, cat3(0, 0), 2);
    g.setup_storage(f, cat2(1, 1), 1);
    let events = act!(g, Action::SpawnPiece { factory: f });
    assert!(g.state.factories.is_empty());
    let piece = g
        .state
        .piece_at(c(&g, 5, 5))
        .expect("Figur auf dem Fabrikfeld");
    assert_eq!(piece.owner, P0);
    assert_eq!(
        piece.kind,
        PieceKind::Unit {
            level: 2,
            range: 2,
            colors: vec![cat1(0), cat1(1)]
        }
    );
    assert!(events.contains(&Event::Spawned {
        piece: piece.id,
        owner: P0,
        factory: f,
        cell: c(&g, 5, 5),
        level: 2,
        range: 2,
    }));
    assert_eq!(g.state.ledger.consumed_by_spawns, 7);
}

#[test]
fn range_equals_the_level_whatever_else_is_in_the_storage() {
    for (colors, expected) in [(1u16, 1u8), (2, 2), (3, 3)] {
        let mut g = blank(4);
        let f = g.setup_factory(P0, c(&g, 5, 5));
        for corner in 0..colors {
            g.setup_storage(f, cat1(corner), 1);
        }
        g.setup_storage(f, cat3(0, 0), 5);
        act!(g, Action::SpawnPiece { factory: f });
        let piece = g.state.piece_at(c(&g, 5, 5)).unwrap();
        assert!(
            matches!(piece.kind, PieceKind::Unit { level, range, .. } if level == expected && range == expected),
            "{:?}",
            piece.kind
        );
    }
}

#[test]
fn level_is_capped_by_the_number_of_category_1_types_on_the_map() {
    let mut g = blank(4);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    for corner in 0..4 {
        g.setup_storage(f, cat1(corner), 1);
    }
    act!(g, Action::SpawnPiece { factory: f });
    let piece = g.state.piece_at(c(&g, 5, 5)).unwrap();
    assert!(matches!(piece.kind, PieceKind::Unit { level: 4, .. }));
}

#[test]
fn spawning_is_blocked_while_a_piece_stands_on_the_factory_cell() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 0, 0)); // der Koenig steht darauf
    g.setup_storage(f, cat1(0), 1);
    assert_eq!(
        g.apply(Action::SpawnPiece { factory: f }),
        Err(RuleError::Illegal(IllegalReason::SpawnCellOccupied))
    );
    act!(g, king_to(&g, P0, 1, 0));
    act!(g, king_to(&g, P1, 8, 9));
    act!(g, Action::SpawnPiece { factory: f });
    assert!(g.state.piece_at(c(&g, 0, 0)).is_some());
}

#[test]
fn only_the_owner_may_spawn_from_a_factory() {
    let mut g = blank(2);
    let f = g.setup_factory(P1, c(&g, 5, 5));
    g.setup_storage(f, cat1(1), 1);
    assert_eq!(
        g.apply(Action::SpawnPiece { factory: f }),
        Err(RuleError::Illegal(IllegalReason::NotYourFactory))
    );
    assert_eq!(
        g.apply(Action::SpawnPiece {
            factory: crate::ids::FactoryId(77)
        }),
        Err(RuleError::Illegal(IllegalReason::NoSuchFactory))
    );
}

// ----- Bewegung der Figuren -----------------------------------------------------------------

fn dests(g: &Game, piece: PieceId) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = g
        .piece_destinations(piece)
        .iter()
        .map(|&x| g.map.board.coord(x))
        .collect();
    v.sort();
    v
}

#[test]
fn level_1_units_move_straight_up_to_their_range_and_never_diagonally() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 1, 3);
    let d = dests(&g, u);
    assert_eq!(d.len(), 12);
    assert!(
        d.contains(&(8, 5)) && d.contains(&(2, 5)) && d.contains(&(5, 2)) && d.contains(&(5, 8))
    );
    assert!(!d.contains(&(6, 6)), "keine Diagonale");
    let u1 = g.setup_unit(P0, c(&g, 1, 8), 1, 1);
    assert_eq!(dests(&g, u1).len(), 4);
}

#[test]
fn level_1_units_stop_at_the_edge_of_the_board() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 8, 4), 1, 3);
    let east: Vec<_> = dests(&g, u)
        .into_iter()
        .filter(|&(x, y)| y == 4 && x > 8)
        .collect();
    assert_eq!(east, vec![(9, 4)]);
}

#[test]
fn straight_movers_are_blocked_by_own_pieces_and_stop_at_enemies() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 1, 3);
    g.setup_unit(P0, c(&g, 7, 5), 1, 1); // eigene Figur: blockiert ab hier
    g.setup_unit(P1, c(&g, 5, 7), 1, 1); // Gegner: schlagbar, aber nichts dahinter
    let d = dests(&g, u);
    assert!(d.contains(&(6, 5)));
    assert!(!d.contains(&(7, 5)) && !d.contains(&(8, 5)));
    assert!(d.contains(&(5, 6)) && d.contains(&(5, 7)) && !d.contains(&(5, 8)));
}

#[test]
fn level_2_units_walk_any_path_within_their_range() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let d = dests(&g, u);
    assert_eq!(d.len(), 12, "alle Felder mit Abstand 1 oder 2");
    assert!(d.contains(&(6, 6)) && d.contains(&(4, 6)) && d.contains(&(7, 5)));
    assert!(!d.contains(&(5, 5)));
    assert!(!d.contains(&(8, 5)), "zu weit");
}

#[test]
fn level_2_units_cannot_pass_through_pieces_but_can_capture_the_first_enemy() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    g.setup_unit(P1, c(&g, 6, 5), 1, 1);
    let d = dests(&g, u);
    assert!(d.contains(&(6, 5)), "Gegner ist schlagbar");
    assert!(
        !d.contains(&(7, 5)),
        "dahinter nur durch die Figur hindurch"
    );

    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 2, 3);
    for (x, y) in [(6, 5), (4, 5), (5, 6), (5, 4)] {
        g.setup_unit(P0, c(&g, x, y), 1, 1);
    }
    assert!(dests(&g, u).is_empty(), "von eigenen Figuren eingemauert");
}

#[test]
fn moving_onto_an_enemy_unit_captures_it() {
    let mut g = blank(2);
    let u = g.setup_unit(P0, c(&g, 5, 5), 1, 2);
    let victim = g.setup_unit(P1, c(&g, 7, 5), 1, 1);
    let events = act!(
        g,
        Action::Move {
            piece: u,
            to: c(&g, 7, 5)
        }
    );
    assert!(!g.state.pieces.contains_key(&victim));
    assert_eq!(g.state.piece_at(c(&g, 7, 5)).unwrap().id, u);
    assert!(events.contains(&Event::Captured {
        piece: victim,
        owner: P1,
        at: c(&g, 7, 5),
        by: P0
    }));
    assert_eq!(
        g.outcome(),
        None,
        "nur ein gefallener Koenig beendet das Spiel"
    );
}

#[test]
fn a_king_can_capture_too() {
    let mut g = blank(2);
    let victim = g.setup_unit(P1, c(&g, 1, 0), 1, 1);
    act!(g, king_to(&g, P0, 1, 0));
    assert!(!g.state.pieces.contains_key(&victim));
}

#[test]
fn threatened_cells_cover_what_enemy_pieces_could_reach() {
    let mut g = blank(2);
    g.setup_unit(P1, c(&g, 5, 5), 1, 2);
    let threatened = g.threatened_cells(P0);
    assert!(threatened.contains(&c(&g, 7, 5)));
    assert!(!threatened.contains(&c(&g, 6, 6)));
    assert!(
        threatened.contains(&c(&g, 8, 9)),
        "Feld neben dem gegnerischen Koenig"
    );
    assert!(!threatened.contains(&c(&g, 0, 0)));
}

// ----- Fabriken erobern, Beute ---------------------------------------------------------------

#[test]
fn walking_onto_an_enemy_factory_destroys_it_and_leaves_its_storage_as_loot() {
    let mut g = blank(2);
    let f = g.setup_factory(P1, c(&g, 5, 5));
    g.setup_storage(f, cat3(1, 0), 2);
    g.setup_storage(f, cat2(1, 1), 1);
    let u = g.setup_unit(P0, c(&g, 3, 5), 1, 2);
    let events = act!(
        g,
        Action::Move {
            piece: u,
            to: c(&g, 5, 5)
        }
    );
    assert!(g.state.factories.is_empty());
    assert_eq!(
        g.state.loot[&c(&g, 5, 5)],
        inv(&[(cat3(1, 0), 2), (cat2(1, 1), 1)])
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::FactoryDestroyed { by, .. } if *by == P0)));
    assert_eq!(
        g.state.ledger.consumed_by_spawns, 0,
        "Beute ist nicht verbraucht"
    );
}

#[test]
fn walking_onto_your_own_factory_does_nothing_special() {
    let mut g = blank(2);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    g.setup_storage(f, cat3(0, 0), 2);
    let u = g.setup_unit(P0, c(&g, 3, 5), 1, 2);
    act!(
        g,
        Action::Move {
            piece: u,
            to: c(&g, 5, 5)
        }
    );
    assert_eq!(g.state.factories.len(), 1);
    assert!(g.state.loot.is_empty());
}

#[test]
fn capturing_a_piece_that_stands_on_an_enemy_factory_takes_both() {
    let mut g = blank(2);
    let f = g.setup_factory(P1, c(&g, 5, 5));
    g.setup_storage(f, cat3(1, 0), 1);
    g.setup_unit(P1, c(&g, 5, 5), 1, 1);
    let u = g.setup_unit(P0, c(&g, 3, 5), 1, 2);
    act!(
        g,
        Action::Move {
            piece: u,
            to: c(&g, 5, 5)
        }
    );
    assert!(g.state.factories.is_empty());
    assert_eq!(g.state.loot[&c(&g, 5, 5)], inv(&[(cat3(1, 0), 1)]));
}

#[test]
fn only_a_king_picks_up_loot_and_any_king_may() {
    let mut g = blank(2);
    let f = g.setup_factory(P1, c(&g, 5, 5));
    g.setup_storage(f, cat3(1, 0), 2);
    let u = g.setup_unit(P0, c(&g, 3, 5), 1, 2);
    act!(
        g,
        Action::Move {
            piece: u,
            to: c(&g, 5, 5)
        }
    );

    // Die Figur steht auf der Beute, nimmt sie aber nicht auf.
    act!(g, king_to(&g, P1, 8, 9));
    let king0 = king_id(&g, P0);
    act!(
        g,
        Action::Move {
            piece: king0,
            to: c(&g, 1, 0)
        }
    );
    assert!(g.state.loot.contains_key(&c(&g, 5, 5)));
    assert!(g.state.carried[P0.idx()].is_empty());

    // Der Koenig des frueheren Besitzers schlaegt die Figur und holt die Beute zurueck.
    let king1 = king_id(&g, P1);
    g.setup_teleport(king1, c(&g, 5, 6));
    act!(
        g,
        Action::Move {
            piece: king1,
            to: c(&g, 5, 5)
        }
    );
    assert!(g.state.loot.is_empty());
    assert_eq!(carry(&g, P1, cat3(1, 0)), 2);
}

#[test]
fn loot_and_deposits_are_collected_together() {
    let mut g = blank(2);
    put(&mut g, 1, 0, cat2(0, 0));
    g.state.loot.insert(c(&g, 1, 0), inv(&[(cat3(1, 0), 3)]));
    g.state.ledger.created += 3;
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat3(1, 0)), 3);
    assert_eq!(carry(&g, P0, cat2(0, 0)), 1);
}

// ----- Koenig faellt, Spielende -------------------------------------------------------------

#[test]
fn capturing_the_king_wins_a_two_player_game_and_removes_all_of_his_belongings() {
    let mut g = blank(2);
    let f = g.setup_factory(P1, c(&g, 5, 5));
    g.setup_storage(f, cat3(1, 0), 4);
    g.setup_unit(P1, c(&g, 6, 6), 1, 1);
    g.setup_carry(P1, cat1(1), 2);
    let hunter = g.setup_unit(P0, c(&g, 7, 9), 1, 2);
    let events = act!(
        g,
        Action::Move {
            piece: hunter,
            to: c(&g, 9, 9)
        }
    );

    assert_eq!(g.outcome(), Some(Outcome::Winner(P0)));
    assert!(events.contains(&Event::PlayerEliminated { player: P1 }));
    assert!(events.contains(&Event::GameOver {
        outcome: Outcome::Winner(P0)
    }));
    assert!(g.pieces_of(P1).next().is_none());
    assert!(g.factories_of(P1).next().is_none());
    assert!(g.state.carried[P1.idx()].is_empty());
    assert_eq!(g.state.ledger.vanished, 6, "4 im Lager und 2 im Gepaeck");
    assert!(g.legal_actions().is_empty());
    assert_eq!(g.apply(Action::Pass), Err(RuleError::GameOver));
}

#[test]
fn nothing_is_collected_after_the_game_is_over() {
    let mut g = blank(2);
    put(&mut g, 9, 9, cat2(0, 0));
    let hunter = g.setup_unit(P0, c(&g, 7, 9), 1, 2);
    act!(
        g,
        Action::Move {
            piece: hunter,
            to: c(&g, 9, 9)
        }
    );
    assert_eq!(g.state.ledger.created, 0);
}

#[test]
fn in_a_four_player_game_play_continues_and_the_eliminated_player_is_skipped() {
    let mut g = blank(4);
    let f = g.setup_factory(P1, c(&g, 5, 2));
    g.setup_storage(f, cat1(1), 2);
    g.setup_unit(P1, c(&g, 4, 4), 1, 1);
    g.setup_carry(P1, cat3(1, 1), 3);
    let hunter = g.setup_unit(P0, c(&g, 7, 0), 1, 2);
    let events = act!(
        g,
        Action::Move {
            piece: hunter,
            to: c(&g, 9, 0)
        }
    );

    assert_eq!(g.outcome(), None);
    assert!(events.contains(&Event::PlayerEliminated { player: P1 }));
    assert_eq!(g.alive_players(), vec![P0, P2, P3]);
    assert!(g.pieces_of(P1).next().is_none() && g.factories_of(P1).next().is_none());
    assert_eq!(g.state.ledger.vanished, 5);
    assert_eq!(g.current_player(), P2, "P1 wird uebersprungen");
    assert_eq!(g.state.round, 1);

    act!(g, king_to(&g, P2, 8, 9));
    act!(g, king_to(&g, P3, 1, 9));
    assert_eq!((g.current_player(), g.state.round), (P0, 2));
}

#[test]
fn the_round_counter_survives_the_first_player_dropping_out() {
    let mut g = blank(4);
    g.setup_current(P1);
    let hunter = g.setup_unit(P1, c(&g, 2, 0), 1, 2);
    act!(
        g,
        Action::Move {
            piece: hunter,
            to: c(&g, 0, 0)
        }
    );
    assert_eq!(g.alive_players(), vec![P1, P2, P3]);
    assert_eq!(g.current_player(), P2);
    act!(g, king_to(&g, P2, 8, 9));
    act!(g, king_to(&g, P3, 1, 9));
    assert_eq!(
        (g.current_player(), g.state.round),
        (P1, 2),
        "neue Runde beginnt bei P1"
    );
}

#[test]
fn the_last_survivor_of_a_four_player_game_wins() {
    let mut g = blank(4);
    // P1 und P2 sind schon raus (kuenstlich), P0 schlaegt zuletzt P3.
    for out in [P1, P2] {
        let king = king_id(&g, out);
        g.state.remove_piece(king);
        g.state.alive[out.idx()] = false;
    }
    let hunter = g.setup_unit(P0, c(&g, 2, 9), 1, 2);
    act!(
        g,
        Action::Move {
            piece: hunter,
            to: c(&g, 0, 9)
        }
    );
    assert_eq!(g.outcome(), Some(Outcome::Winner(P0)));
}

#[test]
fn the_game_is_a_draw_after_the_maximum_number_of_rounds() {
    let mut config = Config::new(2, 1);
    config.max_rounds = 2;
    let mut g = blank_with(config);
    act!(g, king_to(&g, P0, 1, 0));
    act!(g, king_to(&g, P1, 8, 9));
    assert_eq!(g.state.round, 2);
    act!(g, king_to(&g, P0, 0, 0));
    assert_eq!(g.outcome(), None);
    let events = act!(g, king_to(&g, P1, 9, 9));
    assert_eq!(g.outcome(), Some(Outcome::Draw));
    assert!(events.contains(&Event::GameOver {
        outcome: Outcome::Draw
    }));
    assert_eq!(g.state.turns_played, 4);
}

// ----- Selbstpruefung ------------------------------------------------------------------------

#[test]
fn the_invariant_check_detects_broken_states() {
    let base = blank(2);
    assert!(base.check_invariants().is_ok());

    let mut g = base.clone();
    g.state.carried[0].add(cat1(0), 1); // ohne Buchung
    assert!(g.check_invariants().unwrap_err().contains("Rohstoffe"));

    let mut g = base.clone();
    let corner = c(&g, 0, 0);
    g.state.occupant[corner.idx()] = None;
    assert!(g.check_invariants().is_err());

    let mut g = base.clone();
    g.setup_unit(P0, c(&g, 3, 3), 1, 1);
    let king = g
        .state
        .pieces
        .values()
        .find(|p| p.owner == P0 && !p.is_king())
        .unwrap()
        .id;
    g.state.pieces.get_mut(&king).unwrap().kind = PieceKind::King;
    assert!(g.check_invariants().unwrap_err().contains("Koenige"));

    let mut g = base.clone();
    g.setup_factory(P0, c(&g, 4, 4));
    g.setup_factory(P1, c(&g, 4, 5));
    assert!(g.check_invariants().unwrap_err().contains("zu nah"));

    let mut g = base.clone();
    g.state.loot.insert(c(&g, 3, 3), Inventory::new());
    assert!(g.check_invariants().unwrap_err().contains("Beute"));

    let mut g = base;
    g.setup_unit(P0, c(&g, 3, 3), 1, 9);
    assert!(g.check_invariants().unwrap_err().contains("Reichweite"));
}

#[test]
fn identical_action_sequences_give_identical_states() {
    let play = || {
        let mut g = Game::new(Config::new(4, 31)).unwrap();
        for _ in 0..12 {
            let first = g.legal_actions()[0];
            g.apply(first).unwrap();
        }
        g.state
    };
    assert_eq!(play(), play());
}

// ----- Begrenzter Vorrat je Vorkommen -------------------------------------------------------

fn with_stock(stock: u32, yield_n: u32) -> Game {
    let mut config = Config::new(2, 1);
    config.deposit_stock = stock;
    config.yield_per_round = yield_n;
    blank_with(config)
}

#[test]
fn unlimited_deposits_never_run_out() {
    let mut g = with_stock(0, 1);
    put(&mut g, 1, 0, cat2(0, 0));
    assert_eq!(g.deposit_remaining(c(&g, 1, 0), cat2(0, 0)), None);
    act!(g, king_to(&g, P0, 1, 0));
    for _ in 0..5 {
        act!(g, king_to(&g, P1, 8, 9));
        act!(g, king_to(&g, P0, 1, 1));
        act!(g, king_to(&g, P1, 9, 9));
        act!(g, king_to(&g, P0, 1, 0));
    }
    assert_eq!(carry(&g, P0, cat2(0, 0)), 6);
    assert!(
        g.state.harvested.is_empty(),
        "ohne Grenze wird nichts mitgezaehlt"
    );
}

#[test]
fn a_limited_deposit_is_used_up_by_the_king() {
    let mut g = with_stock(3, 1);
    put(&mut g, 1, 0, cat2(0, 0));
    let unit = g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(g.deposit_remaining(c(&g, 1, 0), cat2(0, 0)), Some(2));
    // Der Koenig bleibt stehen und sammelt jede Runde; eine Figur pendelt, P1 auch.
    let mut unit_at = (5, 5);
    let mut p1_at = (9, 9);
    for left in [1, 0, 0] {
        p1_at = if p1_at == (9, 9) { (8, 9) } else { (9, 9) };
        act!(g, king_to(&g, P1, p1_at.0, p1_at.1));
        unit_at = if unit_at == (5, 5) { (6, 5) } else { (5, 5) };
        act!(
            g,
            Action::Move {
                piece: unit,
                to: c(&g, unit_at.0, unit_at.1)
            }
        );
        assert_eq!(g.deposit_remaining(c(&g, 1, 0), cat2(0, 0)), Some(left));
    }
    assert_eq!(
        carry(&g, P0, cat2(0, 0)),
        3,
        "mehr als der Vorrat gibt es nicht"
    );
    assert!(g.available_deposits(c(&g, 1, 0)).is_empty());
}

#[test]
fn a_large_yield_takes_only_what_is_left() {
    let mut g = with_stock(3, 2);
    put(&mut g, 1, 0, cat3(0, 0));
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat3(0, 0)), 2);
    act!(g, king_to(&g, P1, 8, 9));
    act!(g, king_to(&g, P0, 1, 1));
    act!(g, king_to(&g, P1, 9, 9));
    act!(g, king_to(&g, P0, 1, 0));
    assert_eq!(carry(&g, P0, cat3(0, 0)), 3, "2 + der Rest von 1");
    assert_eq!(g.deposit_remaining(c(&g, 1, 0), cat3(0, 0)), Some(0));
}

#[test]
fn factories_drain_their_surroundings_with_a_limited_stock() {
    let mut g = with_stock(1, 1);
    let f = g.setup_factory(P0, c(&g, 5, 5));
    put(&mut g, 5, 5, cat2(0, 0));
    put(&mut g, 6, 5, cat3(0, 0));
    act!(g, king_to(&g, P0, 1, 0));
    act!(g, king_to(&g, P1, 8, 9));
    act!(g, king_to(&g, P0, 2, 0));
    assert_eq!(
        g.state.factories[&f].storage,
        inv(&[(cat2(0, 0), 1), (cat3(0, 0), 1)]),
        "Vorrat 1: jedes Vorkommen gibt genau einmal etwas her"
    );
    assert_eq!(g.state.depleted.len(), 2);
}
