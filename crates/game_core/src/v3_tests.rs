//! Tests fuer die Regeln von Version 3 (`Config::set_v3`): Umzug und Neubau der Fabrik durch den
//! Koenig, Wehrlosigkeit waehrend des Baus, Erntezaehler je Vorkommen und Kat 3 gegen Flaeche.
//!
//! Leere Karte, 10x10 Quadrat; A startet auf (0,0), B auf (9,9).

use crate::action::{Action, Event, IllegalReason, RuleError};
use crate::area_bot::AreaBot;
use crate::bot::{Bot, GreedyBot, RandomBot};
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, PieceId, PlayerId, ResourceId};
use crate::map::Map;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn v3_config(players: u8) -> Config {
    let mut c = Config::new(players, 1);
    c.territory = true;
    c.set_v3(true);
    c
}

/// Karte ohne Rohstoffe, dazu die gewuenschten Vorkommen.
fn with_deposits(deps: &[((i32, i32), ResourceId)]) -> Game {
    let config = v3_config(2);
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    for &((x, y), res) in deps {
        let cell = map.board.cell_at(x, y).expect("Feld");
        map.deposits[cell.idx()].push(res);
    }
    Game::with_map(config, map)
}

fn blank() -> Game {
    with_deposits(&[])
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

/// Ein Streifen eigenen Landes von (0,0) bis (x,0), damit alles mit der Fabrik zusammenhaengt.
fn strip(g: &mut Game, p: PlayerId, to_x: i32) {
    for x in 0..=to_x {
        paint(g, p, &[(x, 0)]);
    }
}

fn king_id(g: &Game, p: PlayerId) -> PieceId {
    g.king(p).expect("Koenig lebt").id
}

/// Ressourcentyp der Kategorie `cat` der Ecke `corner` (V3: drei Kategorien je Ecke).
fn typ(g: &Game, corner: usize, cat: u8) -> ResourceId {
    g.map
        .resource_types
        .iter()
        .find(|t| t.home_corner == Some(corner) && t.category == cat)
        .expect("Typ")
        .id
}

fn stock(g: &mut Game, p: PlayerId, res: ResourceId, n: u32) {
    let fid = g.home_factory(p).unwrap().id;
    g.setup_storage(fid, res, n);
}

/// Zugende fuer `p` ohne Aktion, wie nach einem Zug (nur der Ertrag).
fn income(g: &mut Game, p: PlayerId) -> Vec<Event> {
    let mut events = Vec::new();
    g.territory_income(p, &mut events);
    events
}

fn storage_of(g: &Game, p: PlayerId, res: ResourceId) -> u32 {
    g.home_factory(p).map_or(0, |f| f.storage.get(res))
}

#[test]
fn the_v3_switch_builds_on_v2_and_can_be_undone() {
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1").unwrap();
    assert!(cfg.terr_exact && cfg.terr_cards == 1 && cfg.terr_cycle && cfg.terr_trade);
    assert_eq!(cfg.terr_grow, 2);
    assert_eq!(cfg.terr_ripen, [3, 10, 10, 0]);
    assert_eq!((cfg.terr_move, cfg.terr_rebuild), (1, 3));
    assert_eq!(cfg.category_count(), 3);
    cfg.set_all("terr_v3=0").unwrap();
    assert_eq!(cfg, {
        let mut v1 = Config::new(2, 1);
        v1.territory = true;
        v1
    });
    // Version 2 bleibt ohne die neuen Regeln.
    cfg.set_all("terr_v2=1").unwrap();
    assert!(!cfg.terr_cycle && cfg.terr_move == 0 && !cfg.terr_trade);
    // Einzelne Schalter und Rohstoffmenge lassen sich aendern.
    cfg.set_all("terr_v3=1,terr_rebuild=2,terr_move_k1=7,cells3=4,veins3=2")
        .unwrap();
    assert_eq!(cfg.terr_rebuild, 2);
    assert_eq!(cfg.terr_move_k1, 7);
    assert_eq!((cfg.cells_per_type[2], cfg.veins_per_type[2]), (4, 2));
    let mut bad = cfg.clone();
    bad.terr_rebuild = 0;
    assert!(bad.validate().is_err(), "Umzug ohne Bauzeit ist sinnlos");
}

#[test]
fn v3_maps_have_fewer_resources_and_some_of_category_three() {
    let count = |cfg: &Config| {
        let map = Map::generate(cfg).expect("Karte");
        let mut n = [0usize; 4];
        for d in &map.deposits {
            for &r in d {
                n[usize::from(map.category(r))] += 1;
            }
        }
        n
    };
    let mut v2 = Config::new(2, 1);
    v2.territory = true;
    v2.set_v2(true);
    let n2 = count(&v2);
    let n3 = count(&v3_config(2));
    assert!(n3[1..].iter().sum::<usize>() < n2[1..].iter().sum::<usize>());
    assert_eq!(n2[3], 0);
    assert!(n3[3] >= 4, "es gibt einige Kat 3: {n3:?}");
    for players in 2..=8u8 {
        for seed in 0..4u64 {
            let mut cfg = Config::new(players, seed);
            cfg.territory = true;
            cfg.set_v3(true);
            let map = Map::generate(&cfg).unwrap_or_else(|e| panic!("{players}p seed {seed}: {e}"));
            assert_eq!(map.category_count(), 3);
        }
    }
}

// ----- Umzug ------------------------------------------------------------------------------------

/// A hat Land bis (4,0), der Koenig steht auf (3,0), das Lager hat genug fuer den Umzug.
fn ready_to_move() -> Game {
    let mut g = blank();
    strip(&mut g, P0, 4);
    let king = king_id(&g, P0);
    let at = c(&g, 3, 0);
    g.setup_teleport(king, at);
    let (k1, k2) = (typ(&g, 0, 1), typ(&g, 0, 2));
    stock(&mut g, P0, k1, 5);
    stock(&mut g, P0, k2, 3);
    g
}

#[test]
fn moving_the_factory_costs_resources_and_takes_as_many_rounds_as_the_distance() {
    let mut g = ready_to_move();
    assert!(g.legal_actions().contains(&Action::RebuildFactory));
    let old = g.home_factory(P0).unwrap().cell;
    let events = g.apply(Action::RebuildFactory).unwrap();
    g.check_invariants().unwrap();

    let f = g.home_factory(P0).unwrap();
    assert_eq!(
        f.cell,
        c(&g, 3, 0),
        "die Fabrik steht am neuen Platz im Bau"
    );
    assert_ne!(f.cell, old);
    assert_eq!(
        f.building, 3,
        "Entfernung 3, nach dem Zugende noch drei Runden"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::FactoryRebuilding { rounds: 3, from: Some(from), .. } if *from == old
    )));
    // Preis: 4 Kat 1 und 2 Kat 2 aus dem Lager; der Rest zieht mit um.
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 1)), 1);
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 2)), 1);
    assert_eq!(g.state.ledger.spent_on_moves, 6);
    assert_eq!(g.current_player(), P1);
}

#[test]
fn nothing_is_collected_or_built_while_the_factory_is_being_built() {
    let mut g = ready_to_move();
    g.state.area[P0.idx()] = 500;
    g.apply(Action::RebuildFactory).unwrap();
    assert!(g.rebuilding(P0));
    assert!(g.standing_factory(P0).is_none());
    // Kein Ertrag: kein Land verbunden, die Flaeche waechst nicht.
    assert_eq!(g.connected_count(P0), 0);
    let area = g.state.area[P0.idx()];
    let mut seen = Vec::new();
    for _ in 0..3 {
        // B zieht, dann A.
        let a = g.legal_actions()[0];
        g.apply(a).unwrap();
        g.check_invariants().unwrap();
        let legal = g.legal_actions();
        assert!(
            !legal
                .iter()
                .any(|a| matches!(a, Action::BuildUnit { .. } | Action::BuyCard { .. })),
            "im Bau gibt es nichts zu bauen oder zu kaufen: {legal:?}"
        );
        let a = legal[0];
        seen.push(a);
        g.apply(a).unwrap();
        assert_eq!(g.state.area[P0.idx()], area, "kein Flaechenertrag im Bau");
    }
    assert!(
        !g.rebuilding(P0),
        "nach drei Runden steht die Fabrik wieder"
    );
    assert!(g.standing_factory(P0).is_some());
}

#[test]
fn the_factory_stands_at_the_end_of_the_nth_turn_after_the_order() {
    let mut g = ready_to_move();
    g.apply(Action::RebuildFactory).unwrap();
    let mut stands = None;
    let mut turns = 0;
    for _ in 0..6 {
        // B zieht.
        let a = g.legal_actions()[0];
        g.apply(a).unwrap();
        // A zieht (nur der gesperrte Koenig und ggf. nichts anderes: Passen).
        let a = g.legal_actions()[0];
        turns += 1;
        let events = g.apply(a).unwrap();
        if events
            .iter()
            .any(|e| matches!(e, Event::FactoryStands { .. }))
        {
            stands = Some(turns);
            break;
        }
    }
    assert_eq!(stands, Some(3), "Bauzeit 3 = drei eigene Folgezuege");
    assert!(g.standing_factory(P0).is_some());
}

#[test]
fn the_king_stays_put_while_building_but_other_figures_move() {
    let mut g = ready_to_move();
    own_cells_for_unit(&mut g);
    let unit = g.setup_unit(P0, c(&g, 1, 0), 2, 2);
    g.apply(Action::RebuildFactory).unwrap();
    // B zieht; dann ist A dran.
    let a = g.legal_actions()[0];
    g.apply(a).unwrap();
    let legal = g.legal_actions();
    let king = king_id(&g, P0);
    assert!(
        !legal
            .iter()
            .any(|a| matches!(a, Action::Move { piece, .. } if *piece == king)),
        "der Koenig baut und darf sich nicht bewegen"
    );
    assert!(legal
        .iter()
        .any(|a| matches!(a, Action::Move { piece, .. } if *piece == unit)));
    assert!(!legal.contains(&Action::RebuildFactory), "nicht doppelt");
}

fn own_cells_for_unit(g: &mut Game) {
    strip(g, P0, 4);
}

#[test]
fn figures_are_helpless_during_the_build_and_paint_nothing() {
    let mut g = ready_to_move();
    // A hat eine Figur auf (4,0), daneben steht auf B-Land ein Gegner.
    let a_unit = g.setup_unit(P0, c(&g, 4, 0), 1, 1);
    paint(&mut g, P1, &[(5, 0)]);
    let b_unit = g.setup_unit(P1, c(&g, 5, 0), 1, 1);
    let enemy = c(&g, 5, 0);
    assert!(
        g.piece_destinations(a_unit).contains(&enemy),
        "verbunden darf A schlagen"
    );
    g.apply(Action::RebuildFactory).unwrap();
    assert!(
        !g.piece_connected(&g.state.pieces[&a_unit]),
        "wehrlos wie abgetrennt"
    );
    assert!(
        !g.piece_destinations(a_unit).contains(&enemy),
        "eine wehrlose Figur schlaegt nicht"
    );

    // Das Land von A wird bei einem Zug von B nicht abgetrennt und verteilt.
    let before = g.land_count(P0);
    let mut events = Vec::new();
    g.cut_off(P1, &mut events);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(g.land_count(P0), before);

    // B ist dran; danach zieht A: die Figur faerbt nichts.
    let mv = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { piece, .. } if *piece != b_unit))
        .unwrap();
    g.apply(mv).unwrap();
    assert_eq!(g.current_player(), P0);
    let free = c(&g, 4, 1);
    assert!(g.piece_destinations(a_unit).contains(&free));
    let events = g
        .apply(Action::Move {
            piece: a_unit,
            to: free,
        })
        .unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Painted { .. })));
    assert_eq!(g.state.land[free.idx()], None, "nichts gefaerbt");
    g.check_invariants().unwrap();
}

#[test]
fn without_the_build_disconnected_land_still_flips_to_the_opponent() {
    // Gegenprobe zur Wehrlosigkeit: mit stehender Fabrik geht abgetrenntes Land an den Gegner.
    let mut g = blank();
    strip(&mut g, P0, 2);
    paint(&mut g, P0, &[(8, 8)]);
    let mut events = Vec::new();
    g.cut_off(P1, &mut events);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::CutOff { from, .. } if *from == P0)));
    assert_eq!(g.state.land[c(&g, 8, 8).idx()], Some(P1));
}

#[test]
fn the_move_needs_connected_land_the_price_and_another_cell() {
    // Nicht auf dem Fabrikfeld.
    let mut g = ready_to_move();
    let king = king_id(&g, P0);
    let start = g.home_factory(P0).unwrap().cell;
    g.setup_teleport(king, start);
    assert!(!g.legal_actions().contains(&Action::RebuildFactory));

    // Nicht auf Land, das nicht mit der Fabrik zusammenhaengt.
    let mut g = ready_to_move();
    paint(&mut g, P0, &[(6, 6)]);
    let king = king_id(&g, P0);
    let far = c(&g, 6, 6);
    g.setup_teleport(king, far);
    assert!(!g.legal_actions().contains(&Action::RebuildFactory));

    // Ohne genug Rohstoffe im Lager.
    let mut g = blank();
    strip(&mut g, P0, 4);
    let king = king_id(&g, P0);
    let at = c(&g, 3, 0);
    g.setup_teleport(king, at);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 3);
    assert!(!g.legal_actions().contains(&Action::RebuildFactory));
    let err = g.apply(Action::RebuildFactory).unwrap_err();
    assert_eq!(
        err,
        RuleError::Illegal(IllegalReason::CannotRebuildHere),
        "{err}"
    );
}

#[test]
fn area_and_the_card_move_along_with_the_factory() {
    let mut g = ready_to_move();
    g.state.area[P0.idx()] = 77;
    g.state.stored_card[P0.idx()] = Some(crate::cards::Card::Hop);
    g.apply(Action::RebuildFactory).unwrap();
    assert_eq!(g.state.area[P0.idx()], 77);
    assert_eq!(g.state.stored_card[P0.idx()], Some(crate::cards::Card::Hop));
    g.check_invariants().unwrap();
}

// ----- Zerstoerte Fabrik ------------------------------------------------------------------------

/// B zerstoert die Fabrik von A: A besitzt (0,0)..(2,0) und (0,1), der Koenig steht auf (0,1);
/// B hat eine Figur der Stufe 3 auf (3,0), die genau drei Felder bis (0,0) zieht.
fn factory_destroyed() -> Game {
    let mut g = blank();
    for cell in g.map.board.cells().collect::<Vec<_>>() {
        g.set_land(cell, Some(P1));
    }
    paint(&mut g, P0, &[(0, 0), (1, 0), (2, 0), (0, 1)]);
    let king = king_id(&g, P0);
    let at = c(&g, 0, 1);
    g.setup_teleport(king, at);
    let unit = g.setup_unit(P1, c(&g, 3, 0), 3, 3);
    g.setup_current(P1);
    let fac = g.home_factory(P0).unwrap().cell;
    assert_eq!(fac, c(&g, 0, 0));
    g.apply(Action::Move {
        piece: unit,
        to: fac,
    })
    .expect("B zerstoert die Fabrik");
    g
}

#[test]
fn an_enemy_destroys_the_factory_and_the_king_rebuilds_it_for_free() {
    let mut g = factory_destroyed();
    assert!(g.home_factory(P0).is_none());
    assert_eq!(g.state.area[P0.idx()], 0);
    assert!(g.state.alive[P0.idx()]);
    g.check_invariants().unwrap();
    // Ohne Fabrik ist der Koenig frei und kann am Platz bauen (eigenes Land unter ihm).
    assert!(g.legal_actions().contains(&Action::RebuildFactory));
    g.apply(Action::RebuildFactory).unwrap();
    let f = g.home_factory(P0).unwrap();
    assert_eq!((f.cell, f.building), (c(&g, 0, 1), 3));
    g.check_invariants().unwrap();
}

#[test]
fn after_a_destruction_the_king_rebuilds_on_his_own_land_in_three_fixed_rounds() {
    let mut g = blank();
    strip(&mut g, P0, 4);
    // Zerstoerung "von Hand": Fabrik weg, Flaeche weg, wie bei destroy_factory.
    let fid = g.home_factory(P0).unwrap().id;
    g.state.remove_factory(fid);
    g.state.area[P0.idx()] = 0;
    let king = king_id(&g, P0);
    let at = c(&g, 2, 0);
    g.setup_teleport(king, at);

    assert!(g.legal_actions().contains(&Action::RebuildFactory));
    let events = g.apply(Action::RebuildFactory).unwrap();
    g.check_invariants().unwrap();
    let f = g.home_factory(P0).unwrap();
    assert_eq!(f.cell, at);
    assert_eq!(f.building, 3, "fest drei Runden, egal wie weit weg");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::FactoryRebuilding {
            rounds: 3,
            from: None,
            ..
        }
    )));
    assert_eq!(g.state.ledger.spent_on_moves, 0, "kostenlos");

    // Waehrend des Baus ist A wehrlos, danach ist das Land wieder verbunden.
    for turn in 1..=3 {
        let a = g.legal_actions()[0];
        g.apply(a).unwrap();
        let a = g.legal_actions()[0];
        g.apply(a).unwrap();
        g.check_invariants().unwrap();
        assert_eq!(g.rebuilding(P0), turn < 3, "Runde {turn}");
    }
    assert!(g.standing_factory(P0).is_some());
    assert_eq!(
        g.connected_count(P0),
        5,
        "Land (0,0)..(4,0) haengt wieder zusammen"
    );
}

#[test]
fn rebuilding_needs_own_land_under_the_king() {
    let mut g = blank();
    strip(&mut g, P0, 2);
    let fid = g.home_factory(P0).unwrap().id;
    g.state.remove_factory(fid);
    let king = king_id(&g, P0);
    let off = c(&g, 5, 5);
    g.setup_teleport(king, off);
    assert!(!g.legal_actions().contains(&Action::RebuildFactory));
}

// ----- Erntezaehler -----------------------------------------------------------------------------

#[test]
fn a_resource_is_harvested_once_per_countdown_and_then_starts_over() {
    let g0 = blank();
    let k1 = typ(&g0, 0, 1);
    let mut g = with_deposits(&[((2, 0), k1)]);
    strip(&mut g, P0, 3);
    assert_eq!(g.harvest_period(k1), 3);
    assert_eq!(g.harvest_in(c(&g, 2, 0), k1), 3);
    let mut gained = Vec::new();
    for tick in 1..=7 {
        income(&mut g, P0);
        gained.push(storage_of(&g, P0, k1));
        assert_eq!(
            g.harvest_in(c(&g, 2, 0), k1),
            3 - tick % 3,
            "Zaehler nach {tick} Ernten"
        );
    }
    assert_eq!(gained, vec![0, 0, 1, 1, 1, 2, 2], "alle drei Runden einmal");
    g.check_invariants().unwrap();
}

#[test]
fn the_countdown_stands_still_without_a_standing_factory() {
    let g0 = blank();
    let k1 = typ(&g0, 0, 1);
    let mut g = with_deposits(&[((2, 0), k1)]);
    strip(&mut g, P0, 3);
    income(&mut g, P0);
    income(&mut g, P0);
    assert_eq!(g.harvest_in(c(&g, 2, 0), k1), 1);
    // Fabrik im Bau: nichts laeuft weiter.
    g.home_factory(P0).unwrap();
    let fid = g.home_factory(P0).unwrap().id;
    g.state.factories.get_mut(&fid).unwrap().building = 2;
    // Der Koenig muesste auf dem Bauplatz stehen; hier zaehlt nur der Ertrag.
    income(&mut g, P0);
    assert_eq!(g.harvest_in(c(&g, 2, 0), k1), 1, "kein Zaehler im Bau");
    assert_eq!(storage_of(&g, P0, k1), 0);
    assert_eq!(g.state.age[c(&g, 2, 0).idx()], 2);
}

#[test]
fn a_flipped_field_starts_its_countdown_over() {
    let g0 = blank();
    let k1 = typ(&g0, 0, 1);
    let mut g = with_deposits(&[((2, 0), k1)]);
    strip(&mut g, P0, 3);
    income(&mut g, P0);
    income(&mut g, P0);
    assert_eq!(g.harvest_in(c(&g, 2, 0), k1), 1);
    let cell = c(&g, 2, 0);
    g.set_land(cell, Some(P1));
    g.set_land(cell, Some(P0));
    assert_eq!(g.harvest_in(cell, k1), 3, "der Zaehler fing von vorn an");
    income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k1), 0);
}

#[test]
fn disconnected_fields_do_not_count_down() {
    let g0 = blank();
    let k1 = typ(&g0, 0, 1);
    let mut g = with_deposits(&[((8, 8), k1)]);
    paint(&mut g, P0, &[(8, 8)]);
    strip(&mut g, P0, 2);
    for _ in 0..4 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 0);
    assert_eq!(g.state.age[c(&g, 8, 8).idx()], 0);
}

// ----- Kat 3 gegen Flaeche ----------------------------------------------------------------------

#[test]
fn kat_three_is_traded_for_two_to_the_k_area() {
    assert_eq!(
        [0, 1, 2, 3, 5, 10].map(Game::trade_area),
        [1, 2, 4, 8, 32, 1024]
    );
    assert_eq!(Game::trade_area(40), 1 << 20, "gedeckelt");

    let mut g = blank();
    assert!(!g
        .legal_actions()
        .iter()
        .any(|a| matches!(a, Action::TradeKat3 { .. })));
    let k3 = typ(&g, 0, 3);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k3, 4);
    stock(&mut g, P0, k1, 2);
    g.state.area[P0.idx()] = 10;
    let fid = g.home_factory(P0).unwrap().id;
    let trade = Action::TradeKat3 { factory: fid };
    assert!(g.legal_actions().contains(&trade));
    let events = g.apply(trade).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        Event::Traded {
            count: 4,
            area: 16,
            ..
        }
    )));
    assert_eq!(g.state.area[P0.idx()], 26);
    assert_eq!(storage_of(&g, P0, k3), 0);
    assert_eq!(storage_of(&g, P0, k1), 2, "anderes bleibt");
    assert_eq!(g.current_player(), P0, "Tauschen ist keine Aktion");
    assert_eq!(g.state.ledger.traded, 4);
    assert!(
        !g.legal_actions().contains(&trade),
        "nichts mehr zu tauschen"
    );
    g.check_invariants().unwrap();
}

#[test]
fn kat_three_deposits_are_harvested_into_the_store_after_their_countdown() {
    let g0 = blank();
    let k3 = typ(&g0, 0, 3);
    let mut g = with_deposits(&[((1, 0), k3)]);
    strip(&mut g, P0, 2);
    for _ in 0..9 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k3), 0);
    income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k3), 1);
}

#[test]
fn trading_is_off_without_the_rule() {
    let mut cfg = v3_config(2);
    cfg.terr_trade = false;
    let mut map = Map::generate(&cfg).unwrap();
    map.deposits = vec![Vec::new(); map.board.len()];
    let mut g = Game::with_map(cfg, map);
    let k3 = typ(&g, 0, 3);
    stock(&mut g, P0, k3, 2);
    assert!(!g
        .legal_actions()
        .iter()
        .any(|a| matches!(a, Action::TradeKat3 { .. })));
}

// ----- Bots -------------------------------------------------------------------------------------

#[test]
fn the_bots_rebuild_a_destroyed_factory_at_once_and_trade_kat_three_by_a_rule() {
    for which in 0..2 {
        let mut g = blank();
        strip(&mut g, P0, 4);
        let fid = g.home_factory(P0).unwrap().id;
        g.state.remove_factory(fid);
        let king = king_id(&g, P0);
        let at = c(&g, 2, 0);
        g.setup_teleport(king, at);
        let a = if which == 0 {
            GreedyBot::new(1).choose(&g)
        } else {
            AreaBot::new(1).choose(&g)
        };
        assert_eq!(a, Action::RebuildFactory, "Bot {which}");
    }

    // Fuenf Kat 3 werden getauscht, ein einzelnes nicht (es wartet auf mehr).
    let mut g = blank();
    let k3 = typ(&g, 0, 3);
    stock(&mut g, P0, k3, 1);
    assert!(!g.bot_should_trade(P0));
    assert_ne!(
        AreaBot::new(1).choose(&g),
        g.legal_actions()
            .into_iter()
            .find(|a| matches!(a, Action::TradeKat3 { .. }))
            .unwrap()
    );
    stock(&mut g, P0, k3, 4);
    assert!(g.bot_should_trade(P0));
    assert!(matches!(
        AreaBot::new(1).choose(&g),
        Action::TradeKat3 { .. }
    ));
    assert!(matches!(
        GreedyBot::new(1).choose(&g),
        Action::TradeKat3 { .. }
    ));

    // Wird dadurch eine hoehere Stufe bezahlbar, tauschen sie auch ein einzelnes.
    let mut g = blank();
    stock(&mut g, P0, k3, 1);
    g.state.area[P0.idx()] = 49;
    assert!(g.bot_should_trade(P0), "49 + 2 reicht fuer Stufe 1 (50)");
}

#[test]
fn bots_never_relocate_the_factory_on_their_own() {
    let mut g = ready_to_move();
    assert!(g.legal_actions().contains(&Action::RebuildFactory));
    for seed in 0..20 {
        assert_ne!(AreaBot::new(seed).choose(&g), Action::RebuildFactory);
        assert_ne!(GreedyBot::new(seed).choose(&g), Action::RebuildFactory);
    }
    g.setup_current(P0);
}

// ----- Ganze Partien ----------------------------------------------------------------------------

fn play(players: u8, seed: u64, rounds: u32, bots: Vec<Box<dyn Bot>>) -> Game {
    let mut cfg = Config::new(players, seed);
    cfg.territory = true;
    cfg.set_v3(true);
    cfg.max_rounds = rounds;
    cfg.score_after = rounds;
    let mut g = Game::new(cfg).expect("Karte");
    let mut bots = bots;
    let mut steps = 0;
    while g.outcome().is_none() {
        let me = g.current_player().idx();
        let action = bots[me].choose(&g);
        assert!(g.legal_actions().contains(&action), "Bot zieht unzulaessig");
        g.apply(action).unwrap();
        g.check_invariants()
            .unwrap_or_else(|e| panic!("{players}p seed {seed} Zug {steps}: {e}"));
        steps += 1;
        assert!(steps < 30_000);
    }
    g
}

#[test]
fn whole_games_in_v3_keep_every_invariant_on_every_board() {
    for (players, seed) in [(2u8, 1u64), (2, 2), (3, 3), (4, 4), (5, 5), (6, 6)] {
        let bots: Vec<Box<dyn Bot>> = (0..players)
            .map(|i| match i % 3 {
                0 => Box::new(RandomBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                1 => Box::new(GreedyBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                _ => Box::new(AreaBot::new(seed + u64::from(i))) as Box<dyn Bot>,
            })
            .collect();
        play(players, seed, 60, bots);
    }
}

#[test]
fn random_players_that_rebuild_and_trade_never_break_the_rules() {
    // Zufallsspieler waehlen auch Umzug und Tausch, wann immer es geht.
    for seed in 0..6u64 {
        let bots: Vec<Box<dyn Bot>> = (0..2)
            .map(|i| Box::new(RandomBot::new(seed * 7 + i)) as Box<dyn Bot>)
            .collect();
        let g = play(2, seed, 120, bots);
        assert!(g.state.round > 1);
    }
}
