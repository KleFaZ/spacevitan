//! Tests fuer den Gebietsmodus (`Config::territory`, siehe `territory.rs`).
//!
//! Leere Karte, 10x10 Quadrat; A startet auf (0,0), B auf (9,9). Rohstoff-IDs: Ecke `c` hat
//! `3c` (Kat 1), `3c+1` (Kat 2), `3c+2` (Kat 3).

use crate::action::{Action, Event, IllegalReason, PaySource, RuleError};
use crate::cards::Card;
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, PieceId, PlayerId, ResourceId};
use crate::map::Map;
use crate::rng::Rng;
use crate::state::{Outcome, PieceKind};

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn cat1(corner: u16) -> ResourceId {
    ResourceId(corner * 3)
}
fn cat2(corner: u16) -> ResourceId {
    ResourceId(corner * 3 + 1)
}
fn cat3(corner: u16) -> ResourceId {
    ResourceId(corner * 3 + 2)
}

fn config(players: u8) -> Config {
    let mut c = Config::new(players, 1);
    c.territory = true;
    c
}

/// Die urspruenglichen runden Preise (Stufe 2 bis 4 je 100 Flaeche, Karten mal 3), damit die
/// Ablauf-Tests nicht an der Feinabstimmung der Standardpreise haengen.
fn round_prices() -> Config {
    let mut c = config(2);
    c.terr_area = 100;
    c.terr_area_step = 0;
    c.terr_card_mult = 3;
    c
}

fn blank_with(config: Config) -> Game {
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    Game::with_map(config, map)
}

fn blank() -> Game {
    blank_with(config(2))
}

fn c(g: &Game, x: i32, y: i32) -> CellId {
    g.map.board.cell_at(x, y).expect("Feld auf der Karte")
}

fn king_id(g: &Game, p: PlayerId) -> PieceId {
    g.king(p).expect("Koenig lebt").id
}

fn mv(g: &Game, piece: PieceId, x: i32, y: i32) -> Action {
    Action::Move {
        piece,
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
    ($g:ident, $action:expr $(,)?) => {{
        let action = $action;
        apply_checked(&mut $g, action)
    }};
}

/// Faerbt Felder direkt (Szenario-Aufbau).
fn paint(g: &mut Game, p: PlayerId, cells: &[(i32, i32)]) {
    for &(x, y) in cells {
        let cell = c(g, x, y);
        g.state.land[cell.idx()] = Some(p);
    }
}

fn owner(g: &Game, x: i32, y: i32) -> Option<PlayerId> {
    g.state.land[c(g, x, y).idx()]
}

fn land_of(g: &Game, p: PlayerId) -> u32 {
    g.land_count(p)
}

/// Der Koenig von A geht auf (1,0), das A gehoert: die Fabrik ist frei, alles bleibt verbunden.
fn king_off_factory(g: &mut Game) -> PieceId {
    let king = king_id(g, P0);
    paint(g, P0, &[(1, 0)]);
    let cell = c(g, 1, 0);
    g.setup_teleport(king, cell);
    king
}

/// Spieler B hat keine Fabrik mehr: seine Figuren sind frei beweglich (kein Zusammenhang).
fn without_factory(g: &mut Game, p: PlayerId) {
    let fid = g.home_factory(p).expect("Fabrik").id;
    g.state.remove_factory(fid);
}

// ----- Start ---------------------------------------------------------------------------------

#[test]
fn only_the_start_corner_is_coloured_and_holds_the_only_factory() {
    for players in [2u8, 3, 4] {
        let g = blank_with(config(players));
        for p in 0..players {
            let p = PlayerId(p);
            let start = g.map.start_cell(p);
            assert_eq!(g.state.land[start.idx()], Some(p));
            assert_eq!(land_of(&g, p), 1);
            assert_eq!(g.factories_of(p).count(), 1);
            assert_eq!(g.home_factory(p).unwrap().cell, start);
            assert_eq!(g.state.area[p.idx()], 0);
        }
        assert_eq!(g.state.factories.len(), players as usize);
        g.check_invariants().unwrap();
    }
}

#[test]
fn no_factories_can_be_built_and_nothing_is_offered_but_moves() {
    let g = blank();
    for a in g.legal_actions() {
        assert!(matches!(a, Action::Move { .. }), "{a:?}");
    }
    // Der Koenig steht auf der Fabrik: Bauen geht nicht, solange sie besetzt ist.
    assert!(g
        .legal_actions()
        .iter()
        .all(|a| !matches!(a, Action::BuildUnit { .. })));
}

#[test]
fn the_market_holds_only_unit_abilities() {
    let g = blank();
    assert!(!g.state.market.is_empty());
    for card in &g.state.market {
        assert!(crate::cards::TERRITORY_POOL.contains(card), "{card:?}");
    }
}

// ----- Faerben -------------------------------------------------------------------------------

#[test]
fn a_king_paints_the_cell_he_enters_and_the_factory_collects_the_land() {
    let mut g = blank();
    let king = king_id(&g, P0);
    let events = act!(g, mv(&g, king, 1, 0));
    assert_eq!(owner(&g, 1, 0), Some(P0));
    assert_eq!(owner(&g, 0, 0), Some(P0));
    assert!(events.iter().any(
        |e| matches!(e, Event::Painted { player, cells } if *player == P0 && cells.len() == 1)
    ));
    // Zugende: zwei verbundene Felder, also 2 Flaeche.
    assert_eq!(g.state.area[P0.idx()], 2);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Income { player, area: 2, .. } if *player == P0)));
}

#[test]
fn a_straight_line_paints_the_whole_path() {
    let mut g = blank();
    let unit = g.setup_unit(P0, c(&g, 0, 1), 3, 3);
    paint(&mut g, P0, &[(0, 1)]);
    // Stufe 3 zieht frei bis 3 Schritte: von (0,1) nach (3,1) ueber (1,1), (2,1).
    act!(g, mv(&g, unit, 3, 1));
    for x in 0..=3 {
        assert_eq!(owner(&g, x, 1), Some(P0), "x={x}");
    }
    assert_eq!(land_of(&g, P0), 5);
    assert_eq!(g.connected_count(P0), 5);
}

#[test]
fn a_move_flips_enemy_land_and_cuts_off_what_is_no_longer_connected() {
    let mut g = blank();
    // A: Linie (0,0)..(5,0), B ohne Fabrik (frei) mit einem Bauern auf (3,1).
    paint(&mut g, P0, &[(1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]);
    without_factory(&mut g, P1);
    let unit = g.setup_unit(P1, c(&g, 3, 1), 1, 1);
    g.setup_current(P1);
    let events = act!(g, mv(&g, unit, 3, 0));
    assert_eq!(owner(&g, 3, 0), Some(P1));
    // (4,0) und (5,0) hingen nur ueber (3,0) an der Fabrik: sie gehen an B.
    assert_eq!(owner(&g, 4, 0), Some(P1));
    assert_eq!(owner(&g, 5, 0), Some(P1));
    assert_eq!(owner(&g, 2, 0), Some(P0));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::CutOff { from, to: Some(to), cells } if *from == P0 && *to == P1 && cells.len() == 2
    )));
}

#[test]
fn cut_off_land_becomes_neutral_when_flipping_is_switched_off() {
    let mut cfg = config(2);
    cfg.terr_cut_flips = false;
    let mut g = blank_with(cfg);
    paint(&mut g, P0, &[(1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]);
    without_factory(&mut g, P1);
    let unit = g.setup_unit(P1, c(&g, 3, 1), 1, 1);
    g.setup_current(P1);
    act!(g, mv(&g, unit, 3, 0));
    assert_eq!(owner(&g, 3, 0), Some(P1));
    assert_eq!(owner(&g, 4, 0), None);
    assert_eq!(owner(&g, 5, 0), None);
}

#[test]
fn land_that_is_cut_off_does_not_count_for_the_factory() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0), (3, 0)]);
    // Ein Stueck, das nicht an der Fabrik haengt.
    paint(&mut g, P0, &[(6, 6), (6, 7)]);
    assert_eq!(land_of(&g, P0), 6);
    assert_eq!(g.connected_count(P0), 4);
    let king = king_id(&g, P0);
    act!(g, mv(&g, king, 0, 1));
    // Verbunden: (0,0), (1,0), (2,0), (3,0), (0,1).
    assert_eq!(g.state.area[P0.idx()], 5);
}

// ----- Wehrlose Figuren ----------------------------------------------------------------------

#[test]
fn a_piece_off_the_connected_land_paints_nothing_and_cannot_attack() {
    let mut g = blank();
    // B hat eine Fabrik, A's Bauer steht weit weg auf nicht verbundenem Land.
    let a_unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    paint(&mut g, P0, &[(5, 5)]);
    let b_unit = g.setup_unit(P1, c(&g, 5, 6), 1, 1);
    assert!(!g.piece_connected(&g.state.pieces[&a_unit]));
    let dests = g.piece_destinations(a_unit);
    assert!(
        !dests.contains(&c(&g, 5, 6)),
        "eine wehrlose Figur darf nicht schlagen"
    );
    assert!(dests.contains(&c(&g, 5, 4)), "ziehen darf sie");
    // Sie faerbt dabei nichts.
    let before = land_of(&g, P0);
    act!(g, mv(&g, a_unit, 5, 4));
    assert_eq!(owner(&g, 5, 4), None);
    assert_eq!(land_of(&g, P0), before);
    // B (verbunden ist B's Bauer nicht, aber ohne Land-Zusammenhang kann er auch nicht schlagen).
    assert!(!g.piece_connected(&g.state.pieces[&b_unit]));
}

#[test]
fn a_piece_becomes_dangerous_again_when_it_is_connected() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    let a_unit = g.setup_unit(P0, c(&g, 2, 0), 1, 1);
    let _b_unit = g.setup_unit(P1, c(&g, 2, 1), 1, 1);
    assert!(g.piece_connected(&g.state.pieces[&a_unit]));
    assert!(g.piece_destinations(a_unit).contains(&c(&g, 2, 1)));
}

// ----- Wieder anbinden -----------------------------------------------------------------------

fn reconnected(events: &[Event]) -> Vec<(PieceId, CellId)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Reconnected { piece, cell, .. } => Some((*piece, *cell)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_cut_off_piece_wakes_up_next_to_its_land_and_paints_its_cell() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    // Ein Bauer weit weg vom Land: wehrlos.
    let unit = g.setup_unit(P0, c(&g, 2, 2), 2, 2);
    assert!(!g.piece_connected(&g.state.pieces[&unit]));
    // Er zieht ein Feld neben das verbundene Land: sofort wieder aktiv, und das Feld ist seins.
    let events = act!(g, mv(&g, unit, 2, 1));
    assert_eq!(reconnected(&events), vec![(unit, c(&g, 2, 1))]);
    assert_eq!(owner(&g, 2, 1), Some(P0));
    assert!(g.piece_connected(&g.state.pieces[&unit]));
    // Auf dem Weg dorthin wurde nichts gefaerbt (er war ja noch wehrlos).
    assert_eq!(owner(&g, 2, 2), None);
    // Und er kann jetzt angreifen.
    let _enemy = g.setup_unit(P1, c(&g, 3, 1), 1, 1);
    assert!(g.piece_destinations(unit).contains(&c(&g, 3, 1)));
}

#[test]
fn a_piece_that_is_not_next_to_its_land_stays_helpless() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    let unit = g.setup_unit(P0, c(&g, 4, 4), 2, 2);
    let events = act!(g, mv(&g, unit, 4, 3));
    assert!(reconnected(&events).is_empty());
    assert_eq!(owner(&g, 4, 3), None);
    assert!(!g.piece_connected(&g.state.pieces[&unit]));
}

#[test]
fn only_edges_count_for_waking_up_not_corners() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    // (3,1) liegt nur schraeg neben (2,0).
    let unit = g.setup_unit(P0, c(&g, 3, 3), 2, 2);
    let events = act!(g, mv(&g, unit, 3, 2));
    assert!(reconnected(&events).is_empty());
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    let unit = g.setup_unit(P0, c(&g, 4, 2), 2, 2);
    g.setup_current(P0);
    let events = act!(g, mv(&g, unit, 3, 1));
    assert!(reconnected(&events).is_empty(), "nur schraeg neben (2,0)");
    assert_eq!(owner(&g, 3, 1), None);
}

#[test]
fn a_piece_of_a_player_without_a_factory_is_not_affected() {
    let mut g = blank();
    without_factory(&mut g, P0);
    paint(&mut g, P0, &[(1, 0)]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    let events = act!(g, mv(&g, unit, 5, 4));
    // Ohne Fabrik gibt es keinen Zusammenhang: die Figur faerbt wie immer, ohne "wieder anbinden".
    assert!(reconnected(&events).is_empty());
    assert_eq!(owner(&g, 5, 4), Some(P0));
}

#[test]
fn a_waiting_piece_wakes_up_when_its_land_grows_next_to_it() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0), (1, 1)]);
    let waiting = g.setup_unit(P0, c(&g, 2, 2), 2, 2);
    assert!(!g.piece_connected(&g.state.pieces[&waiting]));
    // Der Koenig faerbt (2,1): damit grenzt das wartende Feld (2,2) ans verbundene Land.
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 1, 1));
    let events = act!(g, mv(&g, king, 2, 1));
    assert_eq!(reconnected(&events), vec![(waiting, c(&g, 2, 2))]);
    assert_eq!(owner(&g, 2, 2), Some(P0));
    assert!(g.piece_connected(&g.state.pieces[&waiting]));
}

#[test]
fn waking_up_takes_an_enemy_cell_and_can_cut_the_enemy_line() {
    let mut g = blank();
    // A's Fabrik steht bei (6,7), das Land reicht bis (6,8); A's Koenig ist weit weg.
    without_factory(&mut g, P0);
    paint(&mut g, P0, &[(6, 7), (6, 8)]);
    g.setup_factory(P0, c(&g, 6, 7));
    // B's Linie von der Fabrik (9,9) nach links.
    paint(&mut g, P1, &[(8, 9), (7, 9), (6, 9), (5, 9)]);
    let unit = g.setup_unit(P0, c(&g, 4, 9), 2, 2);
    assert!(!g.piece_connected(&g.state.pieces[&unit]));
    // A's Bauer zieht auf B's Feld (6,9) neben A's Land: er nimmt es und durchtrennt B's Linie.
    let events = act!(g, mv(&g, unit, 6, 9));
    assert_eq!(reconnected(&events), vec![(unit, c(&g, 6, 9))]);
    assert_eq!(owner(&g, 6, 9), Some(P0));
    assert_eq!(owner(&g, 7, 9), Some(P1));
    // (5,9) hing nur ueber (6,9) an B's Fabrik und geht an A.
    assert_eq!(owner(&g, 5, 9), Some(P0));
    assert!(events.iter().any(|e| matches!(
        e,
        Event::CutOff { from, to: Some(to), cells } if *from == P1 && *to == P0 && cells.len() == 1
    )));
}

#[test]
fn waking_up_can_be_switched_off() {
    let mut cfg = config(2);
    cfg.terr_reconnect = false;
    let mut g = blank_with(cfg);
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    let unit = g.setup_unit(P0, c(&g, 2, 2), 2, 2);
    let events = act!(g, mv(&g, unit, 2, 1));
    assert!(reconnected(&events).is_empty());
    assert_eq!(owner(&g, 2, 1), None);
    assert!(!g.piece_connected(&g.state.pieces[&unit]));
}

// ----- Umschliessen --------------------------------------------------------------------------

fn enclosed(events: &[Event]) -> Vec<(PlayerId, Vec<CellId>)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Enclosed { player, cells } => Some((*player, cells.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_neutral_pocket_closed_by_one_colour_falls_to_it() {
    let mut g = blank();
    // Ein Ring um (0,1) und (1,1); es fehlt nur (0,2).
    paint(&mut g, P0, &[(1, 0), (2, 0), (2, 1), (2, 2), (1, 2)]);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 1, 2));
    let events = act!(g, mv(&g, king, 0, 2));
    let got = enclosed(&events);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, P0);
    let mut cells = got[0].1.clone();
    cells.sort();
    let mut want = vec![c(&g, 0, 1), c(&g, 1, 1)];
    want.sort();
    assert_eq!(cells, want);
    assert_eq!(owner(&g, 0, 1), Some(P0));
    assert_eq!(owner(&g, 1, 1), Some(P0));
    // Alles haengt an der Fabrik und zaehlt als Flaeche: 9 Felder.
    assert_eq!(g.connected_count(P0), 9);
    assert_eq!(g.state.area[P0.idx()], 9);
}

#[test]
fn the_board_edge_closes_a_pocket_too() {
    let mut g = blank();
    without_factory(&mut g, P0);
    paint(&mut g, P0, &[(0, 4), (1, 5)]);
    let unit = g.setup_unit(P0, c(&g, 0, 7), 1, 1);
    let events = act!(g, mv(&g, unit, 0, 6));
    assert_eq!(enclosed(&events), vec![(P0, vec![c(&g, 0, 5)])]);
    assert_eq!(owner(&g, 0, 5), Some(P0));
}

#[test]
fn a_pocket_that_touches_two_colours_stays_neutral() {
    let mut g = blank();
    without_factory(&mut g, P0);
    without_factory(&mut g, P1);
    // (4,4) grenzt an zwei Farben.
    paint(&mut g, P0, &[(3, 4), (4, 3)]);
    paint(&mut g, P1, &[(5, 4), (4, 5)]);
    let unit = g.setup_unit(P0, c(&g, 8, 0), 1, 1);
    let events = act!(g, mv(&g, unit, 8, 1));
    assert!(enclosed(&events).is_empty());
    assert_eq!(owner(&g, 4, 4), None);
}

#[test]
fn the_open_board_is_not_enclosed_by_anyone() {
    // Auch in Spielen zu dritt und viert faellt am Anfang nichts zu.
    for players in [2u8, 3, 4] {
        let mut g = blank_with(config(players));
        let king = king_id(&g, P0);
        let step = g
            .legal_actions()
            .into_iter()
            .find(|a| matches!(a, Action::Move { piece, .. } if *piece == king))
            .expect("der Koenig kann ziehen");
        let events = act!(g, step);
        assert!(enclosed(&events).is_empty(), "{players} Spieler");
        assert_eq!(land_of(&g, P0), 2);
    }
}

#[test]
fn enclosing_can_be_switched_off() {
    let mut cfg = config(2);
    cfg.terr_enclose = false;
    let mut g = blank_with(cfg);
    paint(&mut g, P0, &[(1, 0), (2, 0), (2, 1), (2, 2), (1, 2)]);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 1, 2));
    let events = act!(g, mv(&g, king, 0, 2));
    assert!(enclosed(&events).is_empty());
    assert_eq!(owner(&g, 0, 1), None);
    assert_eq!(owner(&g, 1, 1), None);
}

#[test]
fn cutting_and_enclosing_work_together() {
    let mut g = blank();
    without_factory(&mut g, P1);
    // A: Linie (1,0)..(3,0) an der Fabrik. B: Land rechts und unten um das Loch (4,0).
    paint(&mut g, P0, &[(1, 0), (2, 0), (3, 0)]);
    paint(&mut g, P1, &[(3, 1), (4, 1), (5, 0)]);
    let unit = g.setup_unit(P1, c(&g, 2, 1), 1, 1);
    g.setup_current(P1);
    assert_eq!(owner(&g, 4, 0), None);
    // B schneidet bei (2,0): (3,0) faellt an B, und damit ist (4,0) von B umschlossen.
    let events = act!(g, mv(&g, unit, 2, 0));
    assert_eq!(owner(&g, 3, 0), Some(P1));
    assert_eq!(owner(&g, 4, 0), Some(P1));
    assert_eq!(enclosed(&events), vec![(P1, vec![c(&g, 4, 0)])]);
}

// ----- Fabrik --------------------------------------------------------------------------------

#[test]
fn entering_the_enemy_factory_destroys_it_but_does_not_end_the_game() {
    let mut g = blank();
    // B's Land reicht bis neben A's Fabrik; A's Koenig ist weg von der Fabrik.
    paint(&mut g, P1, &[(0, 1), (0, 2)]);
    let a_king = king_id(&g, P0);
    g.setup_teleport(a_king, c(&g, 3, 3));
    // B's Fabrik steht nah (auf (0,2)), das Land ist verbunden.
    without_factory(&mut g, P1);
    g.setup_factory(P1, c(&g, 0, 2));
    let b_unit = g.setup_unit(P1, c(&g, 0, 1), 1, 1);
    g.setup_current(P1);
    assert!(g.piece_connected(&g.state.pieces[&b_unit]));
    let events = act!(g, mv(&g, b_unit, 0, 0));
    assert!(g.home_factory(P0).is_none(), "Fabrik zerstoert");
    assert!(events.iter().any(
        |e| matches!(e, Event::FactoryDestroyed { owner, by, .. } if *owner == P0 && *by == P1)
    ));
    assert_eq!(g.outcome(), None, "nur der Koenig entscheidet");
    assert_eq!(owner(&g, 0, 0), Some(P1));
    // A kann nichts mehr bauen und keine Karten kaufen.
    g.setup_current(P0);
    for a in g.legal_actions() {
        assert!(matches!(a, Action::Move { .. } | Action::Pass), "{a:?}");
    }
}

#[test]
fn a_destroyed_factory_loses_its_storage_and_stored_card() {
    let mut g = blank();
    let fid = g.home_factory(P0).unwrap().id;
    g.setup_storage(fid, cat1(0), 7);
    g.state.area[P0.idx()] = 30;
    g.state.stored_card[P0.idx()] = Some(Card::Lance);
    without_factory(&mut g, P1);
    let king = king_id(&g, P0);
    g.setup_teleport(king, c(&g, 3, 3));
    let b_unit = g.setup_unit(P1, c(&g, 1, 0), 1, 1);
    g.setup_current(P1);
    act!(g, mv(&g, b_unit, 0, 0));
    assert!(g.home_factory(P0).is_none());
    assert_eq!(g.state.area[P0.idx()], 0);
    assert_eq!(g.state.stored_card[P0.idx()], None);
    assert_eq!(g.state.ledger.vanished, 7);
}

// ----- Bauen ---------------------------------------------------------------------------------

#[test]
fn a_pawn_costs_area_and_appears_on_the_factory() {
    let mut g = blank();
    king_off_factory(&mut g);
    g.state.area[P0.idx()] = 49;
    assert!(g
        .legal_actions()
        .iter()
        .all(|a| !matches!(a, Action::BuildUnit { .. })));
    g.state.area[P0.idx()] = 50;
    let fid = g.home_factory(P0).unwrap().id;
    let build = Action::BuildUnit {
        factory: fid,
        level: 1,
    };
    assert!(g.legal_actions().contains(&build));
    // Nur Stufe 1: Stufe 2 braucht 100 Flaeche und Kat 1.
    assert!(!g.legal_actions().contains(&Action::BuildUnit {
        factory: fid,
        level: 2
    }));
    let events = act!(g, build);
    // 50 bezahlt; am Zugende sammelt die Fabrik die zwei verbundenen Felder neu ein.
    assert_eq!(g.state.area[P0.idx()], 2);
    let piece = g
        .state
        .piece_at(g.map.start_cell(P0))
        .expect("Figur auf der Fabrik");
    assert!(matches!(
        piece.kind,
        PieceKind::Unit {
            level: 1,
            range: 1,
            ..
        }
    ));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Spawned { level: 1, .. })));
    // Die Fabrik bleibt stehen.
    assert!(g.home_factory(P0).is_some());
}

#[test]
fn the_factory_cell_must_be_free_to_build() {
    let mut g = blank();
    g.state.area[P0.idx()] = 500;
    // Der Koenig steht noch auf der Fabrik.
    assert!(g
        .legal_actions()
        .iter()
        .all(|a| !matches!(a, Action::BuildUnit { .. })));
    let fid = g.home_factory(P0).unwrap().id;
    assert_eq!(
        g.apply(Action::BuildUnit {
            factory: fid,
            level: 1
        }),
        Err(RuleError::Illegal(IllegalReason::SpawnCellOccupied))
    );
}

#[test]
fn the_area_price_grows_with_each_level() {
    // Standard: 150 / 200 / 250 Flaeche fuer Stufe 2 / 3 / 4, der Bauer bleibt bei 50.
    let g = blank();
    assert_eq!(g.unit_price(1).area, 50);
    assert_eq!([2, 3, 4].map(|l| g.unit_price(l).area), [150, 200, 250]);
    assert_eq!(g.unit_price(4).k3, 60);
    // Ohne Aufschlag kosten alle hoeheren Stufen gleich viel Flaeche.
    let g = blank_with(round_prices());
    assert_eq!([2, 3, 4].map(|l| g.unit_price(l).area), [100, 100, 100]);
    // Ein anderer Aufschlag laesst sich einstellen.
    let mut c = config(2);
    c.set("terr_area_step", 75).unwrap();
    let g = blank_with(c);
    assert_eq!([2, 3, 4].map(|l| g.unit_price(l).area), [150, 225, 300]);
}

#[test]
fn cards_cost_sixteen_times_the_old_category_prices_by_default() {
    let g = blank();
    // Pferd: alt 2 Kat 2 und 2 Kat 3; Lange Beine: alt 1 Kat 2 und 2 Kat 3.
    assert_eq!(g.territory_card_price(Card::Horse), [0, 32, 32]);
    assert_eq!(g.territory_card_price(Card::LongLegs), [0, 16, 32]);
    let g = blank_with(round_prices());
    assert_eq!(g.territory_card_price(Card::Horse), [0, 6, 6]);
}

#[test]
fn higher_levels_need_area_and_resources_of_each_category_and_consume_them() {
    let mut g = blank_with(round_prices());
    king_off_factory(&mut g);
    let fid = g.home_factory(P0).unwrap().id;
    let lvl = |level| Action::BuildUnit {
        factory: fid,
        level,
    };
    g.state.area[P0.idx()] = 100;
    assert!(!g.legal_actions().contains(&lvl(2)), "ohne Kat 1 nicht");
    g.setup_storage(fid, cat1(0), 20);
    assert!(g.legal_actions().contains(&lvl(2)));
    assert!(
        !g.legal_actions().contains(&lvl(3)),
        "Stufe 3 braucht Kat 2"
    );
    g.setup_storage(fid, cat2(0), 40);
    assert!(g.legal_actions().contains(&lvl(3)));
    assert!(!g.legal_actions().contains(&lvl(4)));
    g.setup_storage(fid, cat3(1), 60);
    assert!(
        g.legal_actions().contains(&lvl(4)),
        "Kat 3 darf eine fremde Farbe sein"
    );
    act!(g, lvl(4));
    assert_eq!(g.state.area[P0.idx()], 2, "nur der Ertrag dieses Zuges");
    assert!(g.home_factory(P0).unwrap().storage.is_empty());
    assert_eq!(g.state.ledger.consumed_by_spawns, 120);
    let piece = g.state.piece_at(g.map.start_cell(P0)).unwrap();
    assert!(matches!(
        piece.kind,
        PieceKind::Unit {
            level: 4,
            range: 4,
            ..
        }
    ));
}

#[test]
fn a_bought_card_waits_next_to_the_factory_and_works_on_the_next_unit() {
    let mut g = blank_with(round_prices());
    g.state.market = vec![Card::Horse, Card::Lance];
    let king = king_off_factory(&mut g);
    let fid = g.home_factory(P0).unwrap().id;
    let buys = |g: &Game| {
        g.legal_actions()
            .iter()
            .any(|a| matches!(a, Action::BuyCard { .. }))
    };
    assert!(!buys(&g), "ohne Rohstoffe nichts zu kaufen");
    // Lanze: alter Preis 2 Kat 2 und 2 Kat 3, mal 3.
    g.setup_storage(fid, cat2(0), 6);
    g.setup_storage(fid, cat3(0), 6);
    let buy = Action::BuyCard {
        card: Card::Lance,
        from: PaySource::Factory(fid),
        target: None,
    };
    assert!(g.legal_actions().contains(&buy));
    act!(g, buy);
    assert_eq!(g.state.stored_card[P0.idx()], Some(Card::Lance));
    assert!(g.home_factory(P0).unwrap().storage.is_empty());
    // Ein Kauf ist keine Aktion: weiter im selben Zug. Ziehen, B zieht.
    act!(g, mv(&g, king, 2, 0));
    let bk = king_id(&g, P1);
    act!(g, mv(&g, bk, 8, 9));
    // Es liegt schon eine Karte neben der Fabrik: keine zweite.
    g.setup_storage(fid, cat2(0), 6);
    g.setup_storage(fid, cat3(0), 6);
    assert!(!buys(&g));
    assert_eq!(
        g.apply(Action::BuyCard {
            card: Card::Horse,
            from: PaySource::Factory(fid),
            target: None
        }),
        Err(RuleError::Illegal(IllegalReason::CardSlotFull))
    );
    // Figur bauen: die Karte wirkt automatisch.
    g.state.area[P0.idx()] = 50;
    let events = act!(
        g,
        Action::BuildUnit {
            factory: fid,
            level: 1,
        },
    );
    assert!(events.iter().any(|e| matches!(e, Event::Spawned { .. })));
    let piece = g.state.piece_at(g.map.start_cell(P0)).unwrap();
    assert!(piece.perks.lance);
    assert_eq!(g.state.stored_card[P0.idx()], None);
}

// ----- Einkommen -----------------------------------------------------------------------------

#[test]
fn the_factory_collects_deposits_on_connected_land_every_round_without_exhausting_them() {
    let mut g = blank();
    let cell = c(&g, 1, 0);
    std::sync::Arc::make_mut(&mut g.map).deposits[cell.idx()].push(cat1(0));
    let far = c(&g, 6, 6);
    std::sync::Arc::make_mut(&mut g.map).deposits[far.idx()].push(cat1(0));
    paint(&mut g, P0, &[(1, 0), (6, 6)]);
    let king = king_id(&g, P0);
    let bk = king_id(&g, P1);
    act!(g, mv(&g, king, 0, 1));
    assert_eq!(g.home_factory(P0).unwrap().storage.get(cat1(0)), 1);
    act!(g, mv(&g, bk, 8, 9));
    act!(g, mv(&g, king, 0, 0));
    act!(g, mv(&g, bk, 9, 9));
    assert_eq!(
        g.home_factory(P0).unwrap().storage.get(cat1(0)),
        2,
        "jede Runde wieder, das Vorkommen bleibt"
    );
    assert!(g.state.depleted.is_empty());
}

#[test]
fn income_stops_when_the_enemy_cuts_the_land_off() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0), (3, 0)]);
    without_factory(&mut g, P1);
    let unit = g.setup_unit(P1, c(&g, 1, 1), 1, 1);
    g.setup_current(P1);
    act!(g, mv(&g, unit, 1, 0));
    // (2,0), (3,0) sind B's; A hat nur noch (0,0).
    assert_eq!(g.connected_count(P0), 1);
}

// ----- Spielende -----------------------------------------------------------------------------

#[test]
fn only_the_king_ends_the_game_and_the_loser_s_land_turns_white() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0), (2, 0)]);
    let a_unit = g.setup_unit(P0, c(&g, 2, 0), 1, 1);
    let bk = king_id(&g, P1);
    g.setup_teleport(bk, c(&g, 2, 1));
    let events = act!(g, mv(&g, a_unit, 2, 1));
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::PlayerEliminated { player } if *player == P1)));
    assert_eq!(g.outcome(), Some(Outcome::Winner(P0)));
    assert_eq!(land_of(&g, P1), 0);
}

// ----- Spruenge ------------------------------------------------------------------------------

#[test]
fn a_horse_jump_paints_a_bridge_so_the_piece_stays_connected() {
    let mut g = blank();
    paint(&mut g, P0, &[(1, 0)]);
    let unit = g.setup_unit(P0, c(&g, 1, 0), 1, 1);
    g.state.pieces.get_mut(&unit).unwrap().perks.horse = true;
    let target = c(&g, 3, 0);
    assert!(g.piece_destinations(unit).contains(&target));
    act!(g, mv(&g, unit, 3, 0));
    assert_eq!(owner(&g, 3, 0), Some(P0));
    assert_eq!(owner(&g, 2, 0), Some(P0), "die Bruecke");
    assert!(
        g.piece_connected(&g.state.pieces[&unit]),
        "Bruecke verbindet"
    );
}

// ----- Zufallspartien ------------------------------------------------------------------------

#[derive(Default, Debug)]
struct Seen {
    spawned: u32,
    cut_off: u32,
    factory_destroyed: u32,
    bought: u32,
    eliminated: u32,
    captured: u32,
    enclosed: u32,
    reconnected: u32,
}

fn random_playout(players: u8, seed: u64, turns: usize, seen: &mut Seen) {
    let mut cfg = Config::new(players, seed);
    cfg.territory = true;
    cfg.terr_pawn = 6;
    cfg.terr_area = 12;
    // Kleine, gleiche Preise, damit Zufallsspieler Bauen und Kaufen auch wirklich erreichen.
    cfg.terr_area_step = 0;
    cfg.terr_card_mult = 1;
    cfg.terr_k1 = 2;
    cfg.terr_k2 = 3;
    cfg.terr_k3 = 4;
    let mut g = Game::new(cfg).expect("Karte");
    let mut rng = Rng::new(seed ^ 0xabcdef);
    for turn in 0..turns {
        if g.outcome().is_some() {
            break;
        }
        let actions = g.legal_actions();
        assert!(!actions.is_empty(), "Zug {turn}: keine Aktion");
        // Bauen und Kaufen bevorzugen, damit auch diese Pfade laufen.
        let builds: Vec<&Action> = actions
            .iter()
            .filter(|a| matches!(a, Action::BuildUnit { .. } | Action::BuyCard { .. }))
            .collect();
        let a = if !builds.is_empty() && rng.chance(0.6) {
            **rng.pick(&builds).unwrap()
        } else {
            *rng.pick(&actions).unwrap()
        };
        let events = g
            .apply(a)
            .unwrap_or_else(|e| panic!("Seed {seed}, Zug {turn}: {a:?}: {e}"));
        for e in &events {
            match e {
                Event::Spawned { .. } => seen.spawned += 1,
                Event::CutOff { .. } => seen.cut_off += 1,
                Event::FactoryDestroyed { .. } => seen.factory_destroyed += 1,
                Event::CardBought { .. } => seen.bought += 1,
                Event::PlayerEliminated { .. } => seen.eliminated += 1,
                Event::Captured { .. } => seen.captured += 1,
                Event::Enclosed { .. } => seen.enclosed += 1,
                Event::Reconnected { .. } => seen.reconnected += 1,
                _ => {}
            }
        }
        g.check_invariants()
            .unwrap_or_else(|e| panic!("Seed {seed}, Zug {turn} nach {a:?}: {e}"));
    }
}

#[test]
fn random_games_keep_all_invariants() {
    let mut seen = Seen::default();
    for players in [2u8, 3, 4] {
        for seed in 0..12 {
            random_playout(players, seed, 400, &mut seen);
        }
    }
    // Die Zufallspartien muessen die seltenen Pfade auch wirklich beruehren.
    assert!(seen.spawned > 0, "{seen:?}");
    assert!(seen.cut_off > 0, "{seen:?}");
    assert!(seen.factory_destroyed > 0, "{seen:?}");
    assert!(seen.bought > 0, "{seen:?}");
    assert!(seen.captured > 0, "{seen:?}");
    assert!(seen.enclosed > 0, "{seen:?}");
    assert!(seen.reconnected > 0, "{seen:?}");
    eprintln!("{seen:?}");
}
