//! Tests fuer die Regeln von Version 2 (`Config::set_v2`): Bewegung genau n Felder, Flaechenpreise,
//! Reifezeit der Vorkommen, die neuen Karten und der Koenigs-Boost.
//!
//! Leere Karte, 10x10 Quadrat; A startet auf (0,0), B auf (9,9).

use crate::action::{Action, Event, IllegalReason, PaySource, RuleError};
use crate::area_bot::AreaBot;
use crate::bot::{Bot, GreedyBot, RandomBot};
use crate::cards::Card;
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, PieceId, PlayerId, ResourceId};
use crate::map::Map;
use crate::state::MAX_KING_BOOST;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn v2_config(players: u8) -> Config {
    let mut c = Config::new(players, 1);
    c.territory = true;
    c.set_v2(true);
    c
}

fn blank_with(config: Config) -> Game {
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    Game::with_map(config, map)
}

fn blank() -> Game {
    blank_with(v2_config(2))
}

fn c(g: &Game, x: i32, y: i32) -> CellId {
    g.map.board.cell_at(x, y).expect("Feld auf der Karte")
}

fn paint(g: &mut Game, p: PlayerId, cells: &[(i32, i32)]) {
    for &(x, y) in cells {
        let cell = c(g, x, y);
        g.set_land(cell, Some(p));
    }
}

fn king_id(g: &Game, p: PlayerId) -> PieceId {
    g.king(p).expect("Koenig lebt").id
}

/// Alle Felder (x, y), auf die die Figur ziehen darf, sortiert.
fn dests(g: &Game, id: PieceId) -> Vec<(i32, i32)> {
    let mut v: Vec<(i32, i32)> = g
        .piece_destinations(id)
        .into_iter()
        .map(|cell| g.map.board.coord(cell))
        .collect();
    v.sort();
    v
}

/// Ein Land, das ganz dem Spieler gehoert, damit die Figur verbunden ist: der ganze Streifen
/// zwischen (0,0) und (x,y) der Fabrik-Seite waere aufwendig, also faerbt der Test alles.
fn own_everything(g: &mut Game, p: PlayerId) {
    for cell in g.map.board.cells().collect::<Vec<_>>() {
        g.set_land(cell, Some(p));
    }
}

/// Setzt eine Figur der Stufe `level` auf (x, y); A besitzt dann das ganze Brett (verbunden).
fn unit_at(g: &mut Game, level: u8, x: i32, y: i32) -> PieceId {
    own_everything(g, P0);
    let cell = c(g, x, y);
    g.setup_unit(P0, cell, level, level)
}

#[test]
fn the_v2_switch_sets_all_rules_and_can_be_undone() {
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v2=1").unwrap();
    assert!(cfg.terr_exact && cfg.terr_cards == 1);
    assert_eq!(cfg.terr_grow, 2);
    assert_eq!(cfg.terr_ripen, [3, 10, 0, 0]);
    assert_eq!(cfg.category_count(), 2);
    cfg.set_all("terr_v2=0").unwrap();
    assert_eq!(cfg, {
        let mut v1 = Config::new(2, 1);
        v1.territory = true;
        v1
    });
    // Einzelne Schalter lassen sich danach aendern.
    cfg.set_all("terr_v2=1,terr_ripen2=7,terr_grow=3").unwrap();
    assert_eq!(cfg.terr_ripen, [3, 7, 0, 0]);
    assert_eq!(cfg.terr_grow, 3);
    assert_eq!(Config::new(2, 1).terr_cats, 0);
    let mut bad = Config::new(2, 1);
    bad.terr_grow = 1;
    assert!(bad.validate().is_err());
    bad.terr_grow = 0;
    bad.terr_cats = 1;
    assert!(bad.validate().is_err());
}

#[test]
fn levels_cost_only_area_and_double_each_time() {
    let g = blank();
    assert_eq!(
        [1, 2, 3, 4].map(|l| g.unit_price(l).area),
        [50, 100, 200, 400]
    );
    for l in 1..=4 {
        let p = g.unit_price(l);
        assert_eq!((p.k1, p.k2, p.k3), (0, 0, 0), "Stufe {l}: keine Rohstoffe");
    }
    let mut cfg = v2_config(2);
    cfg.terr_grow = 3;
    cfg.terr_pawn = 40;
    let g = blank_with(cfg);
    assert_eq!(
        [1, 2, 3, 4].map(|l| g.unit_price(l).area),
        [40, 120, 360, 1080]
    );
}

#[test]
fn a_level_can_be_built_with_area_alone() {
    let mut g = blank();
    let king = king_id(&g, P0);
    paint(&mut g, P0, &[(1, 0)]);
    let to = c(&g, 1, 0);
    g.setup_teleport(king, to);
    let fid = g.home_factory(P0).unwrap().id;
    g.state.area[P0.idx()] = 100;
    let lvl = |level| Action::BuildUnit {
        factory: fid,
        level,
    };
    assert!(g.legal_actions().contains(&lvl(2)));
    assert!(!g.legal_actions().contains(&lvl(3)));
    g.state.area[P0.idx()] = 400;
    assert!(g.legal_actions().contains(&lvl(4)));
}

// ----- Bewegung genau n Felder ------------------------------------------------------------------

#[test]
fn a_piece_moves_exactly_as_far_in_a_line_as_its_level() {
    let mut g = blank();
    let p1 = unit_at(&mut g, 1, 4, 4);
    assert_eq!(dests(&g, p1), vec![(3, 4), (4, 3), (4, 5), (5, 4)]);
    g.state.remove_piece(p1);
    let p2 = unit_at(&mut g, 2, 4, 4);
    assert_eq!(dests(&g, p2), vec![(2, 4), (4, 2), (4, 6), (6, 4)]);
    g.state.remove_piece(p2);
    let p3 = unit_at(&mut g, 3, 4, 4);
    assert_eq!(dests(&g, p3), vec![(1, 4), (4, 1), (4, 7), (7, 4)]);
}

#[test]
fn the_board_edge_cuts_moves_off() {
    let mut g = blank();
    let p = unit_at(&mut g, 3, 1, 1);
    // Nach links und oben fehlen die drei Felder.
    assert_eq!(dests(&g, p), vec![(1, 4), (4, 1)]);
}

#[test]
fn pieces_in_the_way_block_and_the_target_may_hold_an_enemy_but_not_a_friend() {
    let mut g = blank();
    let p = unit_at(&mut g, 2, 4, 4);
    // Eigene Figur im Weg (4,5): (4,6) ist nicht erreichbar.
    let friend = c(&g, 4, 5);
    g.setup_unit(P0, friend, 1, 1);
    assert!(!dests(&g, p).contains(&(4, 6)));
    // Eigene Figur auf dem Ziel: nicht erlaubt.
    let friend2 = c(&g, 6, 4);
    g.setup_unit(P0, friend2, 1, 1);
    assert!(!dests(&g, p).contains(&(6, 4)));
    // Gegner auf dem Ziel (2,4): wird geschlagen.
    let enemy = c(&g, 2, 4);
    g.setup_unit(P1, enemy, 1, 1);
    assert!(dests(&g, p).contains(&(2, 4)));
    // Gegner im Weg (4,3): dahinter geht es nicht weiter.
    let enemy2 = c(&g, 4, 3);
    g.setup_unit(P1, enemy2, 1, 1);
    assert!(!dests(&g, p).contains(&(4, 2)));
}

#[test]
fn a_move_paints_the_whole_straight_path() {
    let mut g = blank();
    let p = unit_at(&mut g, 3, 0, 4);
    // Alles gehoert A; fuer den Test gehoert die Strecke nach rechts B.
    for x in 1..=3 {
        let cell = c(&g, x, 4);
        g.set_land(cell, Some(P1));
    }
    let path = g.move_path(&g.state.pieces[&p].clone(), c(&g, 3, 4));
    assert_eq!(path, vec![c(&g, 1, 4), c(&g, 2, 4), c(&g, 3, 4)]);
}

#[test]
fn shorten_allows_one_step_less() {
    let mut g = blank();
    let p = unit_at(&mut g, 3, 4, 4);
    g.state.pieces.get_mut(&p).unwrap().perks.shorten = true;
    let d = dests(&g, p);
    for x in [
        (1, 4),
        (7, 4),
        (4, 1),
        (4, 7),
        (2, 4),
        (6, 4),
        (4, 2),
        (4, 6),
    ] {
        assert!(d.contains(&x), "{x:?} fehlt");
    }
    assert_eq!(d.len(), 8);
    // Ein Bauer kann nicht auf null verkuerzen.
    g.state.remove_piece(p);
    let pawn = unit_at(&mut g, 1, 2, 2);
    g.state.pieces.get_mut(&pawn).unwrap().perks.shorten = true;
    assert_eq!(dests(&g, pawn).len(), 4);
}

#[test]
fn bend_allows_one_turn_on_the_way() {
    let mut g = blank();
    let p = unit_at(&mut g, 2, 4, 4);
    assert!(!dests(&g, p).contains(&(5, 5)));
    g.state.pieces.get_mut(&p).unwrap().perks.bend = true;
    let d = dests(&g, p);
    // Zwei Schritte mit einer Ecke: ein Schritt und dann einer zur Seite.
    for x in [(5, 5), (3, 3), (5, 3), (3, 5)] {
        assert!(d.contains(&x), "{x:?} fehlt");
    }
    // Weiter weg als zwei Schritte bleibt unerreichbar.
    assert!(!d.contains(&(6, 5)));
    // Die geraden Ziele bleiben.
    assert!(d.contains(&(6, 4)) && d.contains(&(2, 4)));
}

#[test]
fn bend_with_a_blocked_corner_uses_the_other_way_round() {
    let mut g = blank();
    let p = unit_at(&mut g, 2, 4, 4);
    g.state.pieces.get_mut(&p).unwrap().perks.bend = true;
    // (5,4) ist besetzt: nach (5,5) geht es noch ueber (4,5).
    let block = c(&g, 5, 4);
    g.setup_unit(P0, block, 1, 1);
    assert!(dests(&g, p).contains(&(5, 5)));
    let block2 = c(&g, 4, 5);
    g.setup_unit(P0, block2, 1, 1);
    assert!(!dests(&g, p).contains(&(5, 5)));
}

#[test]
fn hop_allows_passing_over_one_figure_but_not_two() {
    let mut g = blank();
    let p = unit_at(&mut g, 3, 1, 4);
    let a = c(&g, 2, 4);
    g.setup_unit(P0, a, 1, 1);
    assert!(!dests(&g, p).contains(&(4, 4)));
    g.state.pieces.get_mut(&p).unwrap().perks.hop = true;
    assert!(dests(&g, p).contains(&(4, 4)), "ueber eine Figur");
    let b = c(&g, 3, 4);
    g.setup_unit(P1, b, 1, 1);
    assert!(!dests(&g, p).contains(&(4, 4)), "zwei Figuren im Weg");
}

#[test]
fn hop_and_bend_work_together() {
    let mut g = blank();
    let p = unit_at(&mut g, 3, 4, 4);
    {
        let perks = &mut g.state.pieces.get_mut(&p).unwrap().perks;
        perks.hop = true;
        perks.bend = true;
    }
    let block = c(&g, 5, 4);
    g.setup_unit(P1, block, 1, 1);
    // (5,4) Gegner springen, bei (6,4) um die Ecke, nach (6,5): drei Schritte.
    assert!(dests(&g, p).contains(&(6, 5)));
}

#[test]
fn a_disconnected_piece_cannot_capture_in_v2_either() {
    let mut g = blank();
    // A besitzt nur die Startecke: eine Figur weit weg ist abgetrennt.
    let cell = c(&g, 4, 4);
    let p = g.setup_unit(P0, cell, 2, 2);
    let enemy = c(&g, 4, 6);
    g.setup_unit(P1, enemy, 1, 1);
    assert!(!dests(&g, p).contains(&(4, 6)));
    assert!(dests(&g, p).contains(&(4, 2)));
}

// ----- Reifezeit --------------------------------------------------------------------------------

fn cat1(corner: u16) -> ResourceId {
    ResourceId(corner * 2)
}
fn cat2(corner: u16) -> ResourceId {
    ResourceId(corner * 2 + 1)
}

fn with_deposits(deps: &[((i32, i32), ResourceId)]) -> Game {
    let config = v2_config(2);
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    for &((x, y), res) in deps {
        let cell = map.board.cell_at(x, y).unwrap();
        map.deposits[cell.idx()].push(res);
    }
    Game::with_map(config, map)
}

#[test]
fn two_categories_have_their_own_ripening_time() {
    let g = with_deposits(&[((1, 0), cat1(0)), ((2, 0), cat2(0))]);
    assert_eq!(g.map.category(cat1(0)), 1);
    assert_eq!(g.map.category(cat2(0)), 2);
    let mut g = g;
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    let (a, b) = (c(&g, 1, 0), c(&g, 2, 0));
    g.state.round = 1;
    assert_eq!(g.ripen_wait(a, cat1(0)), 3);
    assert_eq!(g.ripen_wait(b, cat2(0)), 10);
    g.state.round = 4;
    assert_eq!(g.ripen_wait(a, cat1(0)), 0);
    assert_eq!(g.ripen_wait(b, cat2(0)), 7);
    g.state.round = 11;
    assert_eq!(g.ripen_wait(b, cat2(0)), 0);
}

#[test]
fn the_first_yield_comes_after_the_ripening_rounds_and_flipping_starts_over() {
    let mut g = with_deposits(&[((1, 0), cat1(0))]);
    // A faerbt (1,0) in Runde 1 mit dem Koenig.
    let king = king_id(&g, P0);
    let to = c(&g, 1, 0);
    let mut first_yield = None;
    g.apply(Action::Move { piece: king, to }).unwrap();
    assert_eq!(g.state.owned_since[to.idx()], 1);
    // B zieht (Koenig hin und her), A zieht hin und her zwischen den eigenen Feldern.
    let kb = king_id(&g, P1);
    let b_cells = [c(&g, 9, 8), c(&g, 9, 9)];
    let a_cells = [c(&g, 0, 0), c(&g, 1, 0)];
    let (mut ai, mut bi) = (0usize, 0usize);
    for _ in 0..12 {
        let who = g.current_player();
        let events = if who == P1 {
            let e = g
                .apply(Action::Move {
                    piece: kb,
                    to: b_cells[bi % 2],
                })
                .unwrap();
            bi += 1;
            e
        } else {
            let e = g
                .apply(Action::Move {
                    piece: king,
                    to: a_cells[ai % 2],
                })
                .unwrap();
            ai += 1;
            e
        };
        if who == P0 && first_yield.is_none() {
            for ev in &events {
                if let Event::Income { gained, .. } = ev {
                    if gained.total() > 0 {
                        first_yield = Some(g.state.round);
                    }
                }
            }
        }
    }
    // Gefaerbt in Runde 1: der erste Ertrag kommt am Ende des Zuges von A in Runde 4.
    assert_eq!(first_yield, Some(4));
}

#[test]
fn a_flipped_cell_starts_ripening_again() {
    let mut g = with_deposits(&[((1, 0), cat1(0))]);
    paint(&mut g, P0, &[(1, 0)]);
    let cell = c(&g, 1, 0);
    g.state.round = 5;
    assert_eq!(g.ripen_wait(cell, cat1(0)), 0);
    g.set_land(cell, Some(P1));
    assert_eq!(g.state.owned_since[cell.idx()], 5);
    g.state.round = 6;
    g.set_land(cell, Some(P0));
    assert_eq!(g.state.owned_since[cell.idx()], 6);
    assert_eq!(g.ripen_wait(cell, cat1(0)), 3);
    // Wer sein eigenes Feld nur ueberfaerbt, verliert nichts.
    g.state.round = 8;
    g.set_land(cell, Some(P0));
    assert_eq!(g.state.owned_since[cell.idx()], 6);
}

#[test]
fn without_ripening_rules_everything_yields_at_once() {
    let mut cfg = Config::new(2, 1);
    cfg.territory = true;
    let mut map = Map::generate(&cfg).unwrap();
    map.deposits = vec![Vec::new(); map.board.len()];
    let cell = map.board.cell_at(1, 0).unwrap();
    map.deposits[cell.idx()].push(ResourceId(0));
    let mut g = Game::with_map(cfg, map);
    paint(&mut g, P0, &[(1, 0)]);
    assert_eq!(g.ripen_wait(cell, ResourceId(0)), 0);
}

// ----- Fabrik ruecken ---------------------------------------------------------------------------

fn give_card(g: &mut Game, p: PlayerId, card: Card) {
    g.state.stored_card[p.idx()] = Some(card);
}

#[test]
fn the_factory_shifts_diagonally_onto_connected_own_land() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (0, 1), (1, 1)]);
    // Der Koenig steht auf der Fabrik, (1,1) ist frei.
    give_card(&mut g, P0, Card::FactoryShift);
    let fid = g.home_factory(P0).unwrap().id;
    let to = c(&g, 1, 1);
    let shift = Action::ShiftFactory { factory: fid, to };
    assert!(g.legal_actions().contains(&shift));
    // Ein neutrales Diagonalfeld ist nicht erlaubt (nur (1,1) ist ein Ziel).
    assert_eq!(g.factory_shift_targets(P0), vec![to]);
    let events = g.apply(shift).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::FactoryShifted { to: t, .. } if *t == to)));
    assert_eq!(g.home_factory(P0).unwrap().cell, to);
    assert!(g.state.factory_at(to).is_some());
    assert!(g.state.factory_at(c(&g, 0, 0)).is_none());
    assert_eq!(
        g.state.stored_card[P0.idx()],
        None,
        "die Karte ist verbraucht"
    );
    g.check_invariants().unwrap();
    // Das Land haengt weiter zusammen.
    assert_eq!(g.connected_count(P0), 4);
}

#[test]
fn shifting_needs_the_card_free_connected_cells_and_the_square_board() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (0, 1), (1, 1)]);
    let fid = g.home_factory(P0).unwrap().id;
    let to = c(&g, 1, 1);
    assert_eq!(
        g.apply(Action::ShiftFactory { factory: fid, to }),
        Err(RuleError::Illegal(IllegalReason::NoFactoryShiftCard))
    );
    give_card(&mut g, P0, Card::FactoryShift);
    // Besetztes Zielfeld.
    let unit = g.setup_unit(P0, to, 1, 1);
    assert!(g.factory_shift_targets(P0).is_empty());
    assert_eq!(
        g.apply(Action::ShiftFactory { factory: fid, to }),
        Err(RuleError::Illegal(IllegalReason::CannotShiftThere))
    );
    g.state.remove_piece(unit);
    // Land, das nicht zusammenhaengt, taugt nicht: (2,2) liegt nur diagonal an (1,1).
    paint(&mut g, P0, &[(3, 3)]);
    g.state.factories.get_mut(&fid).unwrap();
    assert_eq!(g.factory_shift_targets(P0), vec![to]);
}

#[test]
fn the_shift_card_stays_beside_the_factory_when_a_unit_is_built() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (0, 1), (1, 1)]);
    let king = king_id(&g, P0);
    let off = c(&g, 1, 0);
    g.setup_teleport(king, off);
    give_card(&mut g, P0, Card::FactoryShift);
    g.state.area[P0.idx()] = 50;
    let fid = g.home_factory(P0).unwrap().id;
    g.apply(Action::BuildUnit {
        factory: fid,
        level: 1,
    })
    .unwrap();
    assert_eq!(g.state.stored_card[P0.idx()], Some(Card::FactoryShift));
}

// ----- Koenigs-Boost ----------------------------------------------------------------------------

fn stock(g: &mut Game, p: PlayerId, res: ResourceId, n: u32) {
    let fid = g.home_factory(p).unwrap().id;
    g.setup_storage(fid, res, n);
}

fn market_with(g: &mut Game, card: Card) {
    g.state.market = vec![card];
}

#[test]
fn the_king_boost_works_at_once_costs_more_each_level_and_stops_at_three() {
    let mut g = blank();
    market_with(&mut g, Card::KingBoost);
    assert_eq!(g.territory_card_price(Card::KingBoost), [0, 3, 0]);
    // Kein Rohstoff: nicht kaufbar.
    assert!(g
        .legal_actions()
        .iter()
        .all(|a| !matches!(a, Action::BuyCard { .. })));
    stock(&mut g, P0, cat2(0), 3);
    let buy = Action::BuyCard {
        card: Card::KingBoost,
        from: PaySource::Factory(g.home_factory(P0).unwrap().id),
        target: None,
    };
    assert!(g.legal_actions().contains(&buy));
    g.apply(buy).unwrap();
    assert_eq!(g.state.player_perks[P0.idx()].king_boost, 1);
    assert_eq!(
        g.state.stored_card[P0.idx()],
        None,
        "belegt keinen Kartenplatz"
    );
    assert_eq!(g.territory_card_price_for(P0, Card::KingBoost), [0, 6, 0]);
    // Ein Kauf pro Zug.
    assert!(g
        .legal_actions()
        .iter()
        .all(|a| !matches!(a, Action::BuyCard { .. })));
    g.check_invariants().unwrap();
    // Stufe 2 und 3 per Zuruecksetzen des Zuges.
    for level in 2..=MAX_KING_BOOST {
        g.state.bought_this_turn = false;
        stock(&mut g, P0, cat2(0), 3 * level as u32);
        assert!(g.legal_actions().contains(&buy), "Stufe {level} kaufbar");
        g.apply(buy).unwrap();
        assert_eq!(g.state.player_perks[P0.idx()].king_boost, level);
        g.setup_current(P0);
    }
    g.state.bought_this_turn = false;
    stock(&mut g, P0, cat2(0), 99);
    assert!(
        g.legal_actions()
            .iter()
            .all(|a| !matches!(a, Action::BuyCard { .. })),
        "nach Stufe 3 nicht mehr"
    );
}

#[test]
fn the_boost_gives_the_king_more_moves_level_by_level() {
    let mut g = blank();
    own_everything(&mut g, P0);
    let king = king_id(&g, P0);
    let mid = c(&g, 4, 4);
    g.setup_teleport(king, mid);
    assert_eq!(dests(&g, king).len(), 4);
    g.state.player_perks[P0.idx()].king_boost = 1;
    let d1 = dests(&g, king);
    assert_eq!(d1.len(), 8);
    for x in [(5, 5), (3, 3), (5, 3), (3, 5)] {
        assert!(d1.contains(&x), "diagonal {x:?}");
    }
    g.state.player_perks[P0.idx()].king_boost = 2;
    let d2 = dests(&g, king);
    assert_eq!(d2.len(), 12);
    for x in [(6, 4), (2, 4), (4, 6), (4, 2)] {
        assert!(d2.contains(&x), "gerade zwei {x:?}");
    }
    g.state.player_perks[P0.idx()].king_boost = 3;
    let d3 = dests(&g, king);
    assert_eq!(d3.len(), 20);
    for x in [
        (6, 5),
        (6, 3),
        (2, 5),
        (2, 3),
        (5, 6),
        (3, 6),
        (5, 2),
        (3, 2),
    ] {
        assert!(d3.contains(&x), "zwei vor, eins zur Seite {x:?}");
    }
}

#[test]
fn the_boosted_king_paints_the_bridge_and_the_l_path() {
    let mut g = blank();
    own_everything(&mut g, P1);
    paint(&mut g, P0, &[(4, 4)]);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 4, 4));
    g.state.player_perks[P0.idx()].king_boost = 3;
    let piece = g.state.pieces[&king].clone();
    let diag = g.move_path(&piece, c(&g, 5, 5));
    assert_eq!(diag.len(), 2);
    assert_eq!(*diag.last().unwrap(), c(&g, 5, 5));
    let knight = g.move_path(&piece, c(&g, 6, 5));
    assert_eq!(knight, vec![c(&g, 5, 4), c(&g, 6, 4), c(&g, 6, 5)]);
}

// ----- Ganze Partien ----------------------------------------------------------------------------

fn play_random(players: u8, seed: u64, rounds: u32) {
    let mut cfg = Config::new(players, seed);
    cfg.territory = true;
    cfg.set_v2(true);
    cfg.max_rounds = rounds;
    cfg.score_after = rounds;
    let mut g = Game::new(cfg).expect("Karte");
    let mut bots: Vec<Box<dyn Bot>> = (0..players)
        .map(|i| {
            if i % 2 == 0 {
                Box::new(RandomBot::new(seed + i as u64)) as Box<dyn Bot>
            } else {
                Box::new(GreedyBot::new(seed + i as u64)) as Box<dyn Bot>
            }
        })
        .collect();
    let mut steps = 0;
    while g.outcome().is_none() {
        let me = g.current_player().idx();
        let action = bots[me].choose(&g);
        assert!(g.legal_actions().contains(&action), "Bot zieht unzulaessig");
        g.apply(action).unwrap();
        g.check_invariants()
            .unwrap_or_else(|e| panic!("{players}p seed {seed} Zug {steps}: {e}"));
        steps += 1;
        assert!(steps < 20_000);
    }
}

#[test]
fn whole_games_in_v2_keep_every_invariant_on_every_board() {
    for (players, seed) in [(2u8, 1u64), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)] {
        play_random(players, seed, 40);
    }
}

#[test]
fn maps_with_two_categories_can_be_generated_for_every_player_count() {
    for players in 2..=8u8 {
        for seed in 0..6u64 {
            let mut cfg = Config::new(players, seed);
            cfg.territory = true;
            cfg.set_v2(true);
            let map = Map::generate(&cfg).unwrap_or_else(|e| panic!("{players}p seed {seed}: {e}"));
            assert_eq!(map.category_count(), 2);
            assert!(map.resource_types.iter().all(|t| t.category <= 2));
        }
    }
}

#[test]
fn the_v2_market_holds_only_v2_cards_that_fit_the_board() {
    for (players, square) in [(2u8, true), (3, false), (4, true), (6, false)] {
        let g = Game::new(v2_config(players)).unwrap();
        assert!(!g.state.market.is_empty());
        for card in &g.state.market {
            assert!(Card::V2.contains(card), "{card:?}");
            if !square {
                assert!(
                    !matches!(card, Card::FactoryShift | Card::KingBoost),
                    "{players} Spieler: {card:?} braucht das Quadratgitter"
                );
            }
        }
    }
}

#[test]
fn the_area_bot_plays_whole_games_in_both_rule_sets_and_beats_the_greedy_bot_mostly() {
    let mut wins = 0;
    let games = 8;
    for seed in 0..games {
        for v2 in [false, true] {
            let mut cfg = Config::new(2, 500 + seed);
            cfg.territory = true;
            cfg.set_v2(v2);
            let mut g = Game::new(cfg).unwrap();
            let mut area = AreaBot::new(seed);
            let mut greedy = GreedyBot::new(seed);
            let mut steps = 0;
            while g.outcome().is_none() {
                let action = if g.current_player() == P0 {
                    area.choose(&g)
                } else {
                    greedy.choose(&g)
                };
                assert!(g.legal_actions().contains(&action));
                g.apply(action).unwrap();
                g.check_invariants().unwrap();
                steps += 1;
                assert!(steps < 20_000);
            }
            if matches!(
                g.outcome(),
                Some(crate::state::Outcome::Winner(p) | crate::state::Outcome::Points(p)) if p == P0
            ) {
                wins += 1;
            }
        }
    }
    assert!(
        wins >= games,
        "der Flaechen-Bot gewann nur {wins} von {}",
        2 * games
    );
}

#[test]
fn the_area_bot_buys_a_king_boost_and_a_unit_card_in_v2() {
    let mut g = blank();
    let king = king_id(&g, P0);
    paint(&mut g, P0, &[(1, 0)]);
    let to = c(&g, 1, 0);
    g.setup_teleport(king, to);
    stock(&mut g, P0, cat2(0), 3);
    stock(&mut g, P0, cat1(0), 10);
    g.state.market = vec![Card::KingBoost, Card::Bend];
    g.state.area[P0.idx()] = 100;
    let mut bot = AreaBot::new(1);
    let first = bot.choose(&g);
    assert!(
        matches!(
            first,
            Action::BuyCard {
                card: Card::KingBoost,
                ..
            }
        ),
        "{first:?}"
    );
    g.apply(first).unwrap();
    // Pro Zug nur ein Kauf: jetzt wird gebaut.
    let second = bot.choose(&g);
    assert!(
        matches!(second, Action::BuildUnit { level: 2, .. }),
        "{second:?}"
    );

    // Im naechsten Zug (ohne Boost im Markt): erst die Ecke, dann die Figur.
    let mut g = blank();
    let king = king_id(&g, P0);
    paint(&mut g, P0, &[(1, 0)]);
    let to = c(&g, 1, 0);
    g.setup_teleport(king, to);
    stock(&mut g, P0, cat1(0), 10);
    g.state.market = vec![Card::Hop, Card::Bend];
    g.state.area[P0.idx()] = 100;
    let first = bot.choose(&g);
    assert!(
        matches!(
            first,
            Action::BuyCard {
                card: Card::Bend,
                ..
            }
        ),
        "{first:?}"
    );
    g.apply(first).unwrap();
    let second = bot.choose(&g);
    assert!(
        matches!(second, Action::BuildUnit { level: 2, .. }),
        "{second:?}"
    );
    g.apply(second).unwrap();
    let unit = g.state.piece_at(g.map.start_cell(P0)).unwrap();
    assert!(
        unit.perks.bend,
        "die gekaufte Ecke wirkt auf die neue Figur"
    );
}

#[test]
fn the_area_bot_clears_a_figure_off_the_factory_when_a_better_one_is_affordable() {
    let mut g = blank();
    own_everything(&mut g, P0);
    let king = king_id(&g, P0);
    let away = c(&g, 2, 0);
    g.setup_teleport(king, away);
    let start = g.map.start_cell(P0);
    let blocker = g.setup_unit(P0, start, 1, 1);
    g.state.area[P0.idx()] = 100;
    assert!(
        !g.legal_actions()
            .iter()
            .any(|a| matches!(a, Action::BuildUnit { .. })),
        "mit besetzter Fabrik gibt es nichts zu bauen"
    );
    let mut bot = AreaBot::new(3);
    for _ in 0..5 {
        let a = bot.choose(&g);
        assert!(
            matches!(a, Action::Move { piece, .. } if piece == blocker),
            "die Figur muss von der Fabrik, damit Stufe 2 gebaut werden kann: {a:?}"
        );
    }
}
