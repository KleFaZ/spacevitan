//! Tests fuer die Regeln von Version 3 (`Config::set_v3`): Umzug und Neubau der Fabrik durch den
//! Koenig, Wehrlosigkeit waehrend des Baus, Erntezaehler je Vorkommen und Kat 3 als Flaechenbonus
//! (`2^k` je Runde) oder, in der alten Fassung, zum Tauschen gegen Flaeche.
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

/// Version 3 ohne Besetzungsregel und mit dem alten Kat-3-Flaechenbonus `2^k` (`terr_bread=0`);
/// seit Version 3.3 ist Brot standardmaessig ein Lager mit Erntezaehler.
fn bonus_config() -> Config {
    let mut c = open_config();
    c.terr_bread = 0;
    c
}

/// Alte Fassung: Kat 3 wird geerntet und gegen `2^k` Flaeche getauscht (statt Rundenbonus).
fn v3_trade_config() -> Config {
    let mut c = open_config();
    c.terr_bread = 0;
    c.terr_bonus = false;
    c.terr_trade = true;
    c
}

/// Version 3 ohne die Besetzungsregel (`terr_occupy`): die Zaehlertests gelten fuer jedes verbundene
/// Feld. Die Besetzung hat eigene Tests weiter unten.
fn open_config() -> Config {
    let mut c = v3_config(2);
    c.terr_occupy = false;
    c
}

/// Karte ohne Rohstoffe, dazu die gewuenschten Vorkommen (ohne Besetzungsregel).
fn with_deposits(deps: &[((i32, i32), ResourceId)]) -> Game {
    with_deposits_cfg(open_config(), deps)
}

fn with_deposits_cfg(config: Config, deps: &[((i32, i32), ResourceId)]) -> Game {
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

fn blank_trade() -> Game {
    with_deposits_cfg(v3_trade_config(), &[])
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

/// Gold (Kategorie 2) in die Heimatfabrik legen, die neuen Karten (Ecke u. a.) brauchen es.
fn stock_gold(g: &mut Game, p: PlayerId, n: u32) {
    let k2 = typ(g, 0, 2);
    stock(g, p, k2, n);
}

/// Brot (Kategorie 3) in die Heimatfabrik legen (ab Version 3.3 bezahlen die Tempokarten damit).
fn stock_bread(g: &mut Game, p: PlayerId, n: u32) {
    let k3 = typ(g, 0, 3);
    stock(g, p, k3, n);
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
    assert!(cfg.terr_exact && cfg.terr_cards == 1 && cfg.terr_cycle);
    assert!(cfg.terr_bonus && !cfg.terr_trade, "Kat 3 ist kein Tausch");
    assert_eq!(cfg.terr_bread, 3, "Brot ist ein Lager, Takt wie Eisen");
    assert!(!cfg.kat3_bonus_on(), "und kein 2^k-Flaechenbonus mehr");
    assert_eq!(cfg.terr_bread, cfg.terr_ripen[0], "Takt von Eisen");
    assert_eq!(cfg.terr_grow, 2);
    assert_eq!(
        cfg.terr_ripen,
        [3, 5, 10, 0],
        "Kat 2 wird alle 5 Runden geerntet"
    );
    assert_eq!(
        (
            cfg.terr_up_mult,
            cfg.terr_cardlock,
            cfg.terr_control,
            cfg.kat3_spread
        ),
        (2, 1, false, true),
        "Preise ab Stufe 2 doppelt, Karte sperrt die Figur, Kontrollfelder aus, Kat 3 gestreut"
    );
    assert_eq!((cfg.veins_per_type[2], cfg.cells_per_type[2]), (2, 1));
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
    assert!(!cfg.terr_cycle && cfg.terr_move == 0 && !cfg.terr_trade && !cfg.terr_bonus);
    // Einzelne Schalter und Rohstoffmenge lassen sich aendern.
    cfg.set_all("terr_v3=1,terr_rebuild=2,terr_move_k1=7,cells3=4,veins3=2")
        .unwrap();
    assert_eq!(cfg.terr_rebuild, 2);
    assert_eq!(cfg.terr_move_k1, 7);
    assert_eq!((cfg.cells_per_type[2], cfg.veins_per_type[2]), (4, 2));
    let mut bad = cfg.clone();
    bad.terr_rebuild = 0;
    assert!(bad.validate().is_err(), "Umzug ohne Bauzeit ist sinnlos");
    let mut both = cfg.clone();
    both.terr_trade = true;
    assert!(
        both.validate().is_err(),
        "Bonus und Tausch schliessen sich aus"
    );
    let mut with_bread = cfg.clone();
    with_bread.set_all("terr_bonus=0,terr_trade=1").unwrap();
    assert!(
        with_bread.validate().is_err(),
        "Brot-Lager und Tausch schliessen sich aus"
    );
    cfg.set_all("terr_bread=0,terr_bonus=0,terr_trade=1")
        .unwrap();
    assert!(cfg.validate().is_ok());
    // Der Weg zurueck zum alten Flaechenbonus ist `terr_bread=0`.
    let mut old = Config::new(2, 1);
    old.set_all("territory=1,terr_v3=1,terr_bread=0").unwrap();
    assert!(old.kat3_bonus_on() && old.validate().is_ok());
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
fn moving_the_factory_is_free_and_takes_half_the_distance_rounded_up() {
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
        f.building, 2,
        "Entfernung 3, halbiert und aufgerundet 2 Runden"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::FactoryRebuilding { rounds: 2, from: Some(from), .. } if *from == old
    )));
    // Der Umzug kostet nichts: das ganze Lager zieht mit um.
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 1)), 5);
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 2)), 3);
    assert_eq!(g.state.ledger.spent_on_moves, 0);
    assert_eq!(g.current_player(), P1);
}

#[test]
fn the_duration_is_half_the_distance_rounded_up_and_at_least_one() {
    let g = blank();
    let from = c(&g, 0, 0);
    let rounds: Vec<u32> = (0..=9)
        .map(|x| g.relocation_rounds(from, c(&g, x, 0)))
        .collect();
    assert_eq!(rounds, vec![1, 1, 1, 2, 2, 3, 3, 4, 4, 5]);
    // Quer ueber das Brett (Entfernung 18) sind es 9 Runden.
    assert_eq!(g.relocation_rounds(from, c(&g, 9, 9)), 9);
    // Der Plan des Koenigs nennt dieselbe Zahl schon vor dem Zug.
    let mut g = ready_to_move();
    let (rounds, price) = g.rebuild_plan(P0).expect("Umzug moeglich");
    assert_eq!((rounds, price), (2, [0, 0]));
    g.apply(Action::RebuildFactory).unwrap();
    assert_eq!(g.home_factory(P0).unwrap().building, rounds);
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
    for _ in 0..2 {
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
        "nach zwei Runden steht die Fabrik wieder"
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
    assert_eq!(stands, Some(2), "Bauzeit 2 = zwei eigene Folgezuege");
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

    // Mit einem Preis (Schalter terr_move_k1 / terr_move_k2): ohne genug Rohstoffe im Lager nicht.
    let mut g = blank();
    g.config.terr_move_k1 = 4;
    g.config.terr_move_k2 = 2;
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
    g.state.stored_card[P0.idx()] = Some(crate::cards::Card::FactoryShift);
    g.apply(Action::RebuildFactory).unwrap();
    assert_eq!(g.state.area[P0.idx()], 77);
    assert_eq!(
        g.state.stored_card[P0.idx()],
        Some(crate::cards::Card::FactoryShift)
    );
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
    assert_eq!(
        g.state.lost_factory[P0.idx()],
        Some(c(&g, 0, 0)),
        "der alte Platz ist gemerkt"
    );
    g.apply(Action::RebuildFactory).unwrap();
    let f = g.home_factory(P0).unwrap();
    // Die alte Fabrik stand auf (0,0), der Koenig steht auf (0,1): ein Feld, also eine Runde.
    assert_eq!((f.cell, f.building), (c(&g, 0, 1), 1));
    assert_eq!(
        g.state.lost_factory[P0.idx()],
        None,
        "nach dem Neubau vergessen"
    );
    g.check_invariants().unwrap();
}

/// Zerstoerte Fabrik von A, die auf (0,0) stand; A besitzt (0,0)..(6,0), der Koenig steht auf `at`.
fn destroyed_with_king_at(at: (i32, i32), dist_rule: bool) -> Game {
    let mut cfg = open_config();
    cfg.terr_rebuild_dist = dist_rule;
    let mut g = with_deposits_cfg(cfg, &[]);
    strip(&mut g, P0, 6);
    let fid = g.home_factory(P0).unwrap().id;
    let old = g.home_factory(P0).unwrap().cell;
    g.state.remove_factory(fid);
    g.state.area[P0.idx()] = 0;
    g.state.lost_factory[P0.idx()] = Some(old);
    let king = king_id(&g, P0);
    let to = c(&g, at.0, at.1);
    g.setup_teleport(king, to);
    g
}

#[test]
fn after_a_destruction_the_rebuild_time_follows_the_distance_to_the_old_place() {
    // Abstand 1 und 2: eine Runde; 3 und 4: zwei; 6: drei (wie beim Umzug, halbe Strecke aufgerundet).
    for (at, rounds) in [
        ((1, 0), 1),
        ((2, 0), 1),
        ((3, 0), 2),
        ((4, 0), 2),
        ((6, 0), 3),
    ] {
        let mut g = destroyed_with_king_at(at, true);
        assert_eq!(
            g.rebuild_plan(P0),
            Some((rounds, [0, 0])),
            "Koenig auf {at:?}"
        );
        let events = g.apply(Action::RebuildFactory).unwrap();
        assert!(events.iter().any(|e| matches!(
            e,
            Event::FactoryRebuilding { rounds: r, from: None, .. } if *r == rounds
        )));
        assert_eq!(g.home_factory(P0).unwrap().building, rounds);
        assert_eq!(g.state.ledger.spent_on_moves, 0, "kostenlos");
        g.check_invariants().unwrap();
    }
}

#[test]
fn without_the_distance_rule_the_rebuild_after_a_destruction_stays_fixed() {
    for at in [(1, 0), (6, 0)] {
        let g = destroyed_with_king_at(at, false);
        assert_eq!(
            g.rebuild_plan(P0),
            Some((3, [0, 0])),
            "fest, egal wo: {at:?}"
        );
    }
}

#[test]
fn a_second_destruction_remembers_the_newest_place() {
    let mut g = destroyed_with_king_at((4, 0), true);
    g.apply(Action::RebuildFactory).unwrap();
    assert_eq!(g.state.lost_factory[P0.idx()], None);
    // Die neue Fabrik steht auf (4,0). Zerstoert man sie wieder, merkt sich das Spiel diesen Platz.
    let fid = g.home_factory(P0).unwrap().id;
    let cell = g.home_factory(P0).unwrap().cell;
    assert_eq!(cell, c(&g, 4, 0));
    g.state.remove_factory(fid);
    g.state.lost_factory[P0.idx()] = Some(cell);
    let king = king_id(&g, P0);
    let near = c(&g, 5, 0);
    g.setup_teleport(king, near);
    assert_eq!(g.rebuild_plan(P0), Some((1, [0, 0])));
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

// ----- Kat 3 als Rundenbonus --------------------------------------------------------------------

#[test]
fn the_bonus_is_two_to_the_k_and_nothing_without_kat_three() {
    assert_eq!(
        [0, 1, 2, 3, 5, 10].map(Game::kat3_bonus),
        [0, 2, 4, 8, 32, 1024]
    );
    assert_eq!(Game::kat3_bonus(40), 1 << 20, "gedeckelt");
}

#[test]
fn connected_kat_three_fields_add_two_to_the_k_area_every_round() {
    let g0 = blank();
    let k3 = typ(&g0, 0, 3);
    let k1 = typ(&g0, 0, 1);
    let mut g = with_deposits_cfg(
        bonus_config(),
        &[((1, 0), k3), ((2, 0), k3), ((3, 0), k3), ((2, 0), k1)],
    );
    strip(&mut g, P0, 3);
    assert_eq!(g.connected_kat3(P0), 3);
    let before = g.state.area[P0.idx()];
    let mut seen = Vec::new();
    for round in 1..=4u32 {
        let events = income(&mut g, P0);
        // Vier verbundene Felder (0..=3), dazu 2^3 = 8 Bonus, jede Runde, ohne Zaehler.
        assert!(events.iter().any(|e| matches!(
            e,
            Event::Income {
                area: 4,
                kat3: 3,
                bonus: 8,
                ..
            }
        )));
        assert_eq!(g.state.area[P0.idx()], before + 12 * round);
        seen.push(storage_of(&g, P0, k3));
    }
    assert_eq!(seen, vec![0; 4], "Kat 3 wird nicht geerntet");
    g.check_invariants().unwrap();
}

#[test]
fn the_bonus_needs_connected_land_a_standing_factory_and_ownership() {
    let g0 = blank();
    let k3 = typ(&g0, 0, 3);
    let mut g = with_deposits_cfg(bonus_config(), &[((1, 0), k3), ((8, 8), k3), ((5, 5), k3)]);
    strip(&mut g, P0, 2);
    // Abgetrennt: gehoert A, haengt aber nicht an der Fabrik.
    paint(&mut g, P0, &[(8, 8)]);
    // Gehoert B.
    paint(&mut g, P1, &[(5, 5)]);
    assert_eq!(g.connected_kat3(P0), 1, "nur das verbundene Feld zaehlt");
    let before = g.state.area[P0.idx()];
    income(&mut g, P0);
    assert_eq!(g.state.area[P0.idx()], before + 3 + 2, "3 Felder + 2^1");
    // Fabrik im Bau: kein Bonus, kein Ertrag.
    let fid = g.home_factory(P0).unwrap().id;
    g.state.factories.get_mut(&fid).unwrap().building = 2;
    assert_eq!(g.connected_kat3(P0), 0);
    let before = g.state.area[P0.idx()];
    income(&mut g, P0);
    assert_eq!(g.state.area[P0.idx()], before);
    // Faellt das Feld an den Gegner, zaehlt es fuer ihn.
    g.state.factories.get_mut(&fid).unwrap().building = 0;
    let cell = c(&g, 1, 0);
    g.set_land(cell, Some(P1));
    assert_eq!(g.connected_kat3(P0), 0);
}

#[test]
fn bonus_and_trade_are_alternatives() {
    // Mit Bonus kommt kein Kat 3 ins Lager, also gibt es nichts zu tauschen.
    let g0 = blank();
    let k3 = typ(&g0, 0, 3);
    let mut g = with_deposits_cfg(bonus_config(), &[((1, 0), k3)]);
    strip(&mut g, P0, 2);
    for _ in 0..12 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k3), 0);
    assert!(!g
        .legal_actions()
        .iter()
        .any(|a| matches!(a, Action::TradeKat3 { .. })));
}

// ----- Kat 3 gegen Flaeche (alte Fassung, `terr_trade=1`) ---------------------------------------

#[test]
fn kat_three_is_traded_for_two_to_the_k_area() {
    assert_eq!(
        [0, 1, 2, 3, 5, 10].map(Game::trade_area),
        [1, 2, 4, 8, 32, 1024]
    );
    assert_eq!(Game::trade_area(40), 1 << 20, "gedeckelt");

    let mut g = blank_trade();
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
    let g0 = blank_trade();
    let k3 = typ(&g0, 0, 3);
    let mut g = with_deposits_cfg(v3_trade_config(), &[((1, 0), k3)]);
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
    let mut g = blank_trade();
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
    let mut g = blank_trade();
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

// ----- Brett frei waehlbar, faire Ecken, Auffuell-Bots ------------------------------------------

#[test]
fn fair_corner_sets_follow_the_symmetry_of_the_board() {
    use crate::config::{corners_are_fair, fair_corners, fair_player_count};
    // Quadrat: zwei gegenueber, drei gehen nicht (vierter Platz mit Bot), vier alle.
    assert_eq!(fair_corners(4, 2), vec![0, 2]);
    assert_eq!(fair_player_count(4, 2), 2);
    assert_eq!(fair_player_count(4, 3), 4);
    assert_eq!(fair_player_count(4, 4), 4);
    assert!(!corners_are_fair(4, &[0, 1, 2]));
    // Sechseck: 2, 3 und 4 gehen, 5 nicht (sechster Platz mit Bot).
    assert_eq!(fair_corners(6, 2), vec![0, 3]);
    assert_eq!(fair_corners(6, 3), vec![0, 2, 4]);
    assert_eq!(fair_corners(6, 4), vec![0, 1, 3, 4]);
    for (k, want) in [(2, 2), (3, 3), (4, 4), (5, 6), (6, 6)] {
        assert_eq!(fair_player_count(6, k), want, "Sechseck, {k} Spieler");
    }
    // Dreieck: zwei und drei gehen immer.
    assert_eq!(fair_player_count(3, 2), 2);
    assert_eq!(fair_player_count(3, 3), 3);
    // Fuenfeck und Siebeneck: ausser zwei Spielern nur alle Ecken.
    for n in [5usize, 7] {
        assert_eq!(fair_player_count(n, 2), 2, "{n}-Eck");
        for k in 3..=n {
            assert_eq!(fair_player_count(n, k), n, "{n}-Eck, {k} Spieler");
        }
    }
    // Achteck: 4 und 8 gehen, 3 wird 4, 5 bis 7 werden 8.
    for (k, want) in [(2, 2), (3, 4), (4, 4), (5, 8), (6, 8), (7, 8), (8, 8)] {
        assert_eq!(fair_player_count(8, k), want, "Achteck, {k} Spieler");
    }
    // Jede gewaehlte faire Menge ist wirklich fair und hat die verlangte Groesse.
    for n in 3..=8usize {
        for k in 2..=n {
            let m = fair_player_count(n, k);
            let c = fair_corners(n, m);
            assert_eq!(c.len(), m);
            assert!(corners_are_fair(n, &c), "{n} Ecken, {m} Spieler: {c:?}");
        }
    }
}

#[test]
fn the_board_shape_is_free_and_corners_follow_the_fair_choice() {
    let mut c = Config::new(3, 1);
    c.board_corners = 6;
    assert_eq!(c.grid_kind(), crate::board::GridKind::Hex);
    assert_eq!(c.corner_count(), 6);
    assert_eq!(c.player_corners(), vec![0, 2, 4]);
    assert!(c.validate().is_ok());
    c.board_corners = 4;
    assert_eq!(c.grid_kind(), crate::board::GridKind::Square);
    c.players = 5;
    assert!(c.validate().is_err(), "mehr Spieler als Ecken");
    c.players = 3;
    c.board_corners = 3;
    assert_eq!(c.grid_kind(), crate::board::GridKind::Triangle);
    assert_eq!(
        Config::new(2, 1).player_corners(),
        vec![0, 2],
        "Standard unveraendert"
    );
    c.board_corners = 9;
    assert!(c.validate().is_err());
}

#[test]
fn every_player_count_plays_on_every_board() {
    for n in 3..=8usize {
        for k in 2..=n {
            for seed in [1u64, 2, 3] {
                let mut cfg = Config::new(k as u8, seed);
                cfg.territory = true;
                cfg.set_v3(true);
                cfg.board_corners = n as u8;
                cfg.max_rounds = 30;
                cfg.score_after = 30;
                let mut g = Game::new(cfg)
                    .unwrap_or_else(|e| panic!("{n} Ecken, {k} Spieler, Seed {seed}: {e}"));
                let mut bots: Vec<Box<dyn Bot>> = (0..k)
                    .map(|i| Box::new(RandomBot::new(seed + i as u64)) as Box<dyn Bot>)
                    .collect();
                let mut steps = 0;
                while g.outcome().is_none() && steps < 400 {
                    let me = g.current_player().idx();
                    let a = bots[me].choose(&g);
                    g.apply(a).unwrap();
                    g.check_invariants()
                        .unwrap_or_else(|e| panic!("{n} Ecken, {k} Spieler, Zug {steps}: {e}"));
                    steps += 1;
                }
            }
        }
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

// ----- Messsonde: lohnt sich ein frueher Umzug zur Mitte? -----------------------------------------

/// Kein Regeltest, sondern eine Messsonde: ein Bot, der den Koenig zur Mitte schickt und die Fabrik dort
/// neu baut, sobald der Umzug bezahlbar ist (nur einmal). Alles andere macht der Bot darunter.
struct Rusher {
    inner: Box<dyn Bot>,
    radius: u32,
    done: bool,
}

impl Bot for Rusher {
    fn name(&self) -> &'static str {
        "rusher"
    }

    fn choose(&mut self, g: &Game) -> Action {
        let me = g.current_player();
        let target = g.map.board.cell_at(5, 5).expect("Mitte");
        if !self.done {
            if let (Some(king), Some(f)) = (g.king(me), g.standing_factory(me)) {
                let afford = g.category_units(&f.storage, 1) >= g.config.terr_move_k1
                    && g.category_units(&f.storage, 2) >= g.config.terr_move_k2;
                let dist = g.map.board.distances_from(target);
                let d = dist[king.cell.idx()];
                let legal = g.legal_actions();
                if afford && d <= self.radius && legal.contains(&Action::RebuildFactory) {
                    self.done = true;
                    return Action::RebuildFactory;
                }
                if afford && d > self.radius {
                    let step = legal
                        .iter()
                        .filter_map(|a| match *a {
                            Action::Move { piece, to } if piece == king.id => {
                                Some((dist[to.idx()], *a))
                            }
                            _ => None,
                        })
                        .min_by_key(|&(nd, _)| nd);
                    if let Some((nd, a)) = step {
                        if nd < d {
                            return a;
                        }
                    }
                }
            }
        }
        self.inner.choose(g)
    }
}

#[derive(Default, Debug)]
struct RushStats {
    games: u32,
    rusher_wins: u32,
    draws: u32,
    rounds: u64,
    relocations: u32,
    stood: u32,
    rusher_factories_lost: u32,
    first_sum: u64,
    first_n: u32,
}

fn rush_series(
    opponent: &str,
    nodes: u64,
    seeds: std::ops::Range<u64>,
    rush: bool,
    free: bool,
) -> RushStats {
    use crate::ai::{SearchBot, SearchParams};
    let mut st = RushStats::default();
    for seed in seeds {
        for seat in 0..2usize {
            let mut cfg = Config::new(2, seed);
            cfg.territory = true;
            cfg.set_v3(true);
            if free {
                // Ohne Preis: zeigt, ob die Kosten oder die Wehrlosigkeit der Bremse sind.
                cfg.terr_move_k1 = 0;
                cfg.terr_move_k2 = 0;
            }
            let mut g = Game::new(cfg).expect("Karte");
            let make = |kind: &str, s: u64| -> Box<dyn Bot> {
                match kind {
                    "stark" => {
                        Box::new(SearchBot::with_params(s, SearchParams::with_budget(nodes)))
                    }
                    _ => Box::new(AreaBot::new(s)),
                }
            };
            let mut bots: Vec<Box<dyn Bot>> = Vec::new();
            for i in 0..2usize {
                let s = seed * 1000 + i as u64;
                if i == seat {
                    let inner: Box<dyn Bot> = Box::new(AreaBot::new(s));
                    bots.push(if rush {
                        Box::new(Rusher {
                            inner,
                            radius: 2,
                            done: false,
                        })
                    } else {
                        inner
                    });
                } else {
                    bots.push(make(opponent, s));
                }
            }
            let me = PlayerId(seat as u8);
            let mut first = None;
            let mut steps = 0;
            while g.outcome().is_none() {
                let who = g.current_player().idx();
                let action = bots[who].choose(&g);
                let events = g.apply(action).expect("legal");
                for e in &events {
                    match e {
                        Event::FactoryRebuilding {
                            owner,
                            from: Some(_),
                            ..
                        } if *owner == me => {
                            st.relocations += 1;
                            first.get_or_insert(u64::from(g.state.round));
                        }
                        Event::FactoryStands { owner, .. } if *owner == me => st.stood += 1,
                        Event::FactoryDestroyed { owner, .. } if *owner == me => {
                            st.rusher_factories_lost += 1
                        }
                        _ => {}
                    }
                }
                steps += 1;
                assert!(steps < 30_000);
            }
            st.games += 1;
            st.rounds += u64::from(g.state.round);
            if let Some(r) = first {
                st.first_sum += r;
                st.first_n += 1;
            }
            match g.outcome() {
                Some(crate::state::Outcome::Winner(w)) | Some(crate::state::Outcome::Points(w)) => {
                    if w == me {
                        st.rusher_wins += 1;
                    }
                }
                _ => st.draws += 1,
            }
        }
    }
    st
}

/// `cargo test --release -p game_core rush_probe -- --ignored --nocapture`
#[test]
#[ignore = "Messsonde, dauert eine Weile"]
fn rush_probe_center_relocation() {
    for (opp, rush, free) in [
        ("flaeche", false, false),
        ("flaeche", true, false),
        ("flaeche", true, true),
        ("stark", false, false),
        ("stark", true, false),
        ("stark", true, true),
    ] {
        {
            let st = rush_series(opp, 2000, 100..120, rush, free);
            println!(
                "{}{} gegen {opp}: {} von {} Partien gewonnen ({:.0} %), {:.0} Runden im Schnitt; Umzuege {} (Fabrik stand {} Mal wieder), verlorene Fabriken {}, erster Umzug im Schnitt Runde {:.0}",
                if rush { "Rusher" } else { "Bot ohne Umzug" },
                if free { " (Umzug gratis)" } else { "" },
                st.rusher_wins,
                st.games,
                100.0 * f64::from(st.rusher_wins) / f64::from(st.games),
                st.rounds as f64 / f64::from(st.games),
                st.relocations,
                st.stood,
                st.rusher_factories_lost,
                st.first_sum as f64 / f64::from(st.first_n.max(1)),
            );
        }
    }
}

// ----- Besetzung: der Erntezaehler laeuft nur mit einer eigenen Figur auf dem Feld ---------------

fn v3_game(deps: &[((i32, i32), ResourceId)]) -> Game {
    with_deposits_cfg(v3_config(2), deps)
}

#[test]
fn the_occupy_and_card_switches_belong_to_version_3_and_can_be_changed() {
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1").unwrap();
    assert!(cfg.terr_occupy && cfg.terr_cardany);
    assert_eq!(cfg.terr_card_mult, 8, "Karten kosten die Haelfte");
    cfg.set_all("terr_occupy=0,terr_cardany=0,terr_card_mult=16")
        .unwrap();
    assert!(!cfg.terr_occupy && !cfg.terr_cardany);
    cfg.set_all("terr_v2=1").unwrap();
    assert!(!cfg.terr_occupy && !cfg.terr_cardany && cfg.terr_card_mult == 16);
    let mut bad = Config::new(2, 1);
    bad.territory = true;
    bad.terr_occupy = true;
    assert!(
        bad.validate().is_err(),
        "Besetzung braucht den Erntezaehler"
    );
    let mut bad = Config::new(2, 1);
    bad.territory = true;
    bad.terr_cardany = true;
    assert!(
        bad.validate().is_err(),
        "Karten auf Figuren brauchen die neuen Karten"
    );
}

#[test]
fn a_field_counts_down_only_while_an_own_piece_stands_on_it() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let mut g = v3_game(&[((2, 0), k1)]);
    strip(&mut g, P0, 3);
    let field = c(&g, 2, 0);
    // Ohne Figur: der Zaehler steht, es gibt nichts.
    for _ in 0..7 {
        income(&mut g, P0);
    }
    assert_eq!(g.harvest_in(field, k1), 3);
    assert_eq!(storage_of(&g, P0, k1), 0);
    // Eine gegnerische Figur zaehlt nicht fuer den Besitzer.
    let enemy = g.setup_unit(P1, field, 1, 1);
    for _ in 0..4 {
        income(&mut g, P0);
    }
    assert_eq!(
        g.harvest_in(field, k1),
        3,
        "der Gegner erntet fuer uns nicht"
    );
    g.state.remove_piece(enemy);
    // Mit eigener Figur laeuft er, nach drei Zugenden gibt es die Ernte und er beginnt von vorn.
    let mine = g.setup_unit(P0, field, 1, 1);
    income(&mut g, P0);
    income(&mut g, P0);
    assert_eq!((g.harvest_in(field, k1), storage_of(&g, P0, k1)), (1, 0));
    income(&mut g, P0);
    assert_eq!((g.harvest_in(field, k1), storage_of(&g, P0, k1)), (3, 1));
    // Die Figur geht weg: der Stand bleibt stehen (er faengt nicht von vorn an) und laeuft weiter.
    income(&mut g, P0);
    g.state.remove_piece(mine);
    for _ in 0..5 {
        income(&mut g, P0);
    }
    assert_eq!(g.harvest_in(field, k1), 2, "Zaehler pausiert");
    assert_eq!(storage_of(&g, P0, k1), 1);
    g.setup_unit(P0, field, 1, 1);
    income(&mut g, P0);
    income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k1), 2, "mit Figur geht es dort weiter");
    g.check_invariants().unwrap();
}

#[test]
fn the_king_counts_as_a_figure_and_the_area_still_comes_from_every_field() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let mut g = v3_game(&[((1, 0), k1)]);
    strip(&mut g, P0, 2);
    let field = c(&g, 1, 0);
    let king = king_id(&g, P0);
    g.setup_teleport(king, field);
    let before = g.state.area[P0.idx()];
    for _ in 0..3 {
        income(&mut g, P0);
    }
    assert_eq!(
        storage_of(&g, P0, k1),
        1,
        "auch der Koenig bearbeitet ein Feld"
    );
    assert_eq!(
        g.state.area[P0.idx()] - before,
        9,
        "die Flaeche kommt von jedem verbundenen Feld, besetzt oder nicht (3 Felder, 3 Zuege)"
    );
}

#[test]
fn a_cut_off_field_is_not_worked_even_with_a_figure_on_it() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let mut g = v3_game(&[((5, 5), k1)]);
    paint(&mut g, P0, &[(5, 5)]);
    let field = c(&g, 5, 5);
    g.setup_unit(P0, field, 1, 1);
    for _ in 0..6 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 0, "nicht mit der Fabrik verbunden");
    assert_eq!(g.harvest_in(field, k1), 3);
}

#[test]
fn recolouring_still_starts_the_count_over_with_the_rule_on() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let mut g = v3_game(&[((2, 0), k1)]);
    strip(&mut g, P0, 3);
    let field = c(&g, 2, 0);
    g.setup_unit(P0, field, 1, 1);
    income(&mut g, P0);
    income(&mut g, P0);
    assert_eq!(g.harvest_in(field, k1), 1);
    g.state.age[field.idx()] = 0;
    assert_eq!(g.harvest_in(field, k1), 3);
}

// ----- Karten auf jede eigene Einheit, auch auf schon gebaute -----------------------------------

/// Wie `card_game`, aber ohne Brot im Lager: die Tests legen alles selbst hinein.
fn card_game_no_bread(cards: &[crate::cards::Card]) -> Game {
    let mut g = v3_game(&[]);
    g.state.market = cards.to_vec();
    // Viel Flaeche, damit der Unterhalt der Testfiguren keine aufloest.
    g.state.area = vec![10_000; 2];
    g
}

fn card_game(cards: &[crate::cards::Card]) -> Game {
    let mut g = card_game_no_bread(cards);
    // Brot gibt es reichlich; die Preise in Eisen und Gold legen die Tests selbst fest.
    stock_bread(&mut g, P0, 40);
    stock_bread(&mut g, P1, 40);
    g
}

fn buys(g: &Game) -> Vec<Action> {
    g.legal_actions()
        .into_iter()
        .filter(|a| matches!(a, Action::BuyCard { .. }))
        .collect()
}

#[test]
fn a_unit_card_goes_on_a_chosen_existing_unit_at_once() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Bend, Card::Hop, Card::Shorten]);
    let pawn = g.setup_unit(P0, c(&g, 3, 0), 1, 1);
    let two = g.setup_unit(P0, c(&g, 4, 0), 2, 2);
    let three = g.setup_unit(P0, c(&g, 5, 0), 3, 3);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 12);
    stock_gold(&mut g, P0, 2); // reicht fuer die Ecke (1), nicht fuer die Koenigskarte (3)
    let fid = g.home_factory(P0).unwrap().id;
    let all = buys(&g);
    // Je Karte nur Einheiten ab Reichweite 2, nie der Koenig, nie der Bauer.
    assert_eq!(
        all.len(),
        3 * 2,
        "drei Karten, zwei passende Figuren: {all:?}"
    );
    for a in &all {
        let Action::BuyCard { target, .. } = a else {
            unreachable!()
        };
        let t = target.expect("jede Karte braucht eine Figur");
        assert!(t == two || t == three, "{t:?} (Bauer {pawn:?})");
    }
    let bend = Action::BuyCard {
        card: Card::Bend,
        from: crate::action::PaySource::Factory(fid),
        target: Some(three),
    };
    assert!(all.contains(&bend));
    let events = g.apply(bend).unwrap();
    assert!(g.state.pieces[&three].perks.bend > 0, "wirkt sofort");
    assert_eq!(g.state.pieces[&two].perks.bend, 0);
    assert_eq!(
        g.state.stored_card[P0.idx()],
        None,
        "keine Karte neben der Fabrik"
    );
    assert_eq!(g.current_player(), P0, "Kaufen ist keine Aktion");
    assert_eq!(storage_of(&g, P0, k1), 12, "Ecke kostet kein Eisen");
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 2)), 1, "sondern 1 Kat 2");
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 3)), 38, "und 2 Brot");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::CardBought { card: Card::Bend, target: Some(t), .. } if *t == three
    )));
    g.check_invariants().unwrap();
    // Ein Kauf pro Zug.
    assert!(buys(&g).is_empty());
    g.state.bought_this_turn = false;
    // Stufe 3 darf bis zu zwei Ecken tragen; ohne Stapeln nur eine, aber eine andere Karte geht.
    let again = buys(&g);
    assert!(
        again.contains(&bend),
        "zweite Ecke: Stufe 3 hat zwei Schritte zum Knicken"
    );
    assert!(again.iter().any(|a| matches!(
        a,
        Action::BuyCard { card: Card::Hop, target: Some(t), .. } if *t == three
    )));
    g.config.terr_stack = false;
    let again = buys(&g);
    assert!(!again.contains(&bend), "ohne Stapeln nur einmal");
    assert!(again.iter().any(|a| matches!(
        a,
        Action::BuyCard { card: Card::Hop, target: Some(t), .. } if *t == three
    )));
}

#[test]
fn the_unit_card_changes_the_moves_of_that_unit() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Bend]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 5);
    stock_gold(&mut g, P0, 8);
    let corner = c(&g, 6, 6);
    assert!(!g.piece_destinations(unit).contains(&corner));
    let fid = g.home_factory(P0).unwrap().id;
    g.apply(Action::BuyCard {
        card: Card::Bend,
        from: crate::action::PaySource::Factory(fid),
        target: Some(unit),
    })
    .unwrap();
    // Die Figur bekam die Karte und ruht in diesem Zug; im naechsten zieht sie um die Ecke.
    assert!(g.piece_destinations(unit).is_empty(), "gesperrt");
    let king = king_id(&g, P0);
    let step = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { piece, .. } if *piece == king))
        .expect("der Koenig kann ziehen");
    g.apply(step).unwrap();
    assert!(
        g.piece_destinations(unit).contains(&corner),
        "jetzt um die Ecke"
    );
}

fn buy(g: &mut Game, card: crate::cards::Card, target: Option<PieceId>) {
    let fid = g.home_factory(P0).unwrap().id;
    g.apply(Action::BuyCard {
        card,
        from: crate::action::PaySource::Factory(fid),
        target,
    })
    .unwrap();
}

fn moves_of(g: &Game, piece: PieceId) -> usize {
    g.legal_actions()
        .iter()
        .filter(|a| matches!(a, Action::Move { piece: p, .. } if *p == piece))
        .count()
}

#[test]
fn a_card_locks_the_upgraded_figure_for_the_turn_but_not_the_others() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Bend]);
    let a = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let b = g.setup_unit(P0, c(&g, 7, 7), 2, 2);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 20);
    stock_gold(&mut g, P0, 8);
    assert!(moves_of(&g, a) > 0 && moves_of(&g, b) > 0);
    buy(&mut g, Card::Bend, Some(a));
    assert_eq!(g.state.card_lock, Some(a));
    assert_eq!(moves_of(&g, a), 0, "die verbesserte Figur ruht");
    assert!(moves_of(&g, b) > 0, "die anderen ziehen");
    assert!(moves_of(&g, king_id(&g, P0)) > 0);
    // Ein anderer Zug beendet den Zug und hebt die Sperre auf.
    let to = g.piece_destinations(b)[0];
    g.apply(Action::Move { piece: b, to }).unwrap();
    assert_eq!(g.state.card_lock, None);
    assert_eq!(g.current_player(), P1);
    assert!(!g.piece_destinations(a).is_empty());
    g.check_invariants().unwrap();
}

#[test]
fn the_king_boost_locks_the_king() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::KingBoost]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let k2 = typ(&g, 0, 2);
    stock(&mut g, P0, k2, 4);
    let king = king_id(&g, P0);
    assert!(moves_of(&g, king) > 0);
    buy(&mut g, Card::KingBoost, None);
    assert_eq!(g.state.card_lock, Some(king));
    assert_eq!(moves_of(&g, king), 0, "der Koenig ruht in diesem Zug");
    assert!(moves_of(&g, unit) > 0);
}

#[test]
fn the_card_lock_can_be_switched_off_or_cost_the_whole_turn() {
    use crate::cards::Card;
    // 0: keine Sperre.
    let mut g = card_game(&[Card::Bend]);
    g.config.terr_cardlock = 0;
    let a = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 20);
    stock_gold(&mut g, P0, 8);
    buy(&mut g, Card::Bend, Some(a));
    assert_eq!(g.state.card_lock, None);
    assert!(moves_of(&g, a) > 0, "ohne Sperre darf sie sofort ziehen");
    // 2: der Kauf ist der ganze Zug.
    let mut g = card_game(&[Card::Bend]);
    g.config.terr_cardlock = 2;
    let a = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    stock(&mut g, P0, k1, 20);
    stock_gold(&mut g, P0, 8);
    buy(&mut g, Card::Bend, Some(a));
    assert_eq!(g.current_player(), P1, "der Kauf beendet den Zug");
    assert_eq!(g.state.card_lock, None);
    assert!(g.state.pieces[&a].perks.bend > 0);
    // Fabrik ruecken ist keine Verbesserung und kostet den Kauf keinen Zug (das Rutschen ist der Zug).
    let mut g = card_game(&[Card::FactoryShift]);
    g.config.terr_cardlock = 2;
    paint(&mut g, P0, &[(1, 0), (0, 1), (1, 1)]);
    stock(&mut g, P0, k1, 20);
    stock_gold(&mut g, P0, 8);
    buy(&mut g, Card::FactoryShift, None);
    assert_eq!(g.current_player(), P0);
    g.check_invariants().unwrap();
}

#[test]
fn units_from_level_two_cost_twice_as_much_in_version_3() {
    let g = v3_game(&[]);
    let prices: Vec<u32> = (1..=4).map(|l| g.unit_price(l).area).collect();
    assert_eq!(prices, [50, 200, 400, 800]);
    let mut old = v3_config(2);
    old.terr_up_mult = 1;
    let g = with_deposits_cfg(old, &[]);
    let prices: Vec<u32> = (1..=4).map(|l| g.unit_price(l).area).collect();
    assert_eq!(prices, [50, 100, 200, 400], "ohne den Faktor wie Version 2");
}

// ----- Karten mehrfach auf dieselbe Figur (`terr_stack`) -----------------------------------------

/// Die Figuren, auf die der Spieler jetzt eine Karte legen koennte.
fn card_targets(g: &Game) -> Vec<PieceId> {
    buys(g)
        .into_iter()
        .filter_map(|a| match a {
            Action::BuyCard { target, .. } => target,
            _ => None,
        })
        .collect()
}

/// Naechster Kauf im selben Zug (ein Kauf je Zug ist hier nicht das Thema): Markt und Zaehler
/// zuruecksetzen.
fn stock_up_for(g: &mut Game, card: crate::cards::Card) {
    g.state.bought_this_turn = false;
    g.state.market = vec![card];
}

#[test]
fn shorten_stacks_up_to_the_level_minus_one() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Shorten]);
    g.config.terr_cardlock = 0;
    let two = g.setup_unit(P0, c(&g, 2, 6), 2, 2);
    let three = g.setup_unit(P0, c(&g, 5, 5), 3, 3);
    let four = g.setup_unit(P0, c(&g, 1, 8), 4, 4);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 60);
    // Stufe 3 zieht genau 3 Felder: 4 Ziele. Jede Karte gibt eine Laenge dazu.
    assert_eq!(g.piece_destinations(three).len(), 4);
    buy(&mut g, Card::Shorten, Some(three));
    assert_eq!(g.state.pieces[&three].perks.shorten, 1);
    assert_eq!(g.piece_destinations(three).len(), 8, "3 oder 2 Felder");
    stock_up_for(&mut g, Card::Shorten);
    assert!(card_targets(&g).contains(&three), "ein zweites Mal");
    buy(&mut g, Card::Shorten, Some(three));
    assert_eq!(g.state.pieces[&three].perks.shorten, 2);
    assert_eq!(g.piece_destinations(three).len(), 12, "3, 2 oder 1 Feld");
    stock_up_for(&mut g, Card::Shorten);
    assert!(
        !card_targets(&g).contains(&three),
        "kuerzer als 1 Feld gibt es nicht"
    );
    // Stufe 2: nur einmal, Stufe 4: dreimal.
    buy(&mut g, Card::Shorten, Some(two));
    stock_up_for(&mut g, Card::Shorten);
    assert!(!card_targets(&g).contains(&two));
    for n in 0..3 {
        assert!(card_targets(&g).contains(&four), "Stufe 4, Karte {}", n + 1);
        buy(&mut g, Card::Shorten, Some(four));
        stock_up_for(&mut g, Card::Shorten);
    }
    assert_eq!(g.state.pieces[&four].perks.shorten, 3);
    assert!(!card_targets(&g).contains(&four));
    g.check_invariants().unwrap();
}

#[test]
fn without_the_stack_switch_every_card_goes_on_a_figure_only_once() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Shorten]);
    g.config.terr_cardlock = 0;
    g.config.terr_stack = false;
    let four = g.setup_unit(P0, c(&g, 4, 4), 4, 4);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 60);
    buy(&mut g, Card::Shorten, Some(four));
    stock_up_for(&mut g, Card::Shorten);
    assert!(!card_targets(&g).contains(&four), "wie in Version 2");
    let mut cfg = v3_config(2);
    cfg.terr_cardany = false;
    assert!(cfg.validate().is_err(), "terr_stack braucht terr_cardany");
    assert!(v3_config(2).terr_stack && !Config::new(2, 1).terr_stack);
}

#[test]
fn a_second_corner_card_allows_two_turns_on_the_way() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Bend]);
    g.config.terr_cardlock = 0;
    let unit = g.setup_unit(P0, c(&g, 5, 5), 3, 3);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 60);
    stock_gold(&mut g, P0, 8);
    // Rechts, hoch, links endet neben dem Start: dafuer braucht der Weg zwei Ecken.
    let hook = c(&g, 5, 6);
    buy(&mut g, Card::Bend, Some(unit));
    assert!(
        !g.piece_destinations(unit).contains(&hook),
        "eine Ecke reicht nicht"
    );
    stock_up_for(&mut g, Card::Bend);
    assert!(card_targets(&g).contains(&unit));
    buy(&mut g, Card::Bend, Some(unit));
    assert_eq!(g.state.pieces[&unit].perks.bend, 2);
    assert!(g.piece_destinations(unit).contains(&hook), "zwei Ecken");
    stock_up_for(&mut g, Card::Bend);
    assert!(
        !card_targets(&g).contains(&unit),
        "bei Stufe 3 nicht mehr als zwei Ecken (der letzte Schritt ist das Ziel)"
    );
}

#[test]
fn a_second_hop_card_jumps_over_two_figures() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Hop]);
    g.config.terr_cardlock = 0;
    let unit = g.setup_unit(P0, c(&g, 5, 5), 3, 3);
    g.setup_unit(P0, c(&g, 6, 5), 1, 1);
    g.setup_unit(P1, c(&g, 7, 5), 1, 1);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 60);
    let far = c(&g, 8, 5);
    assert!(!g.piece_destinations(unit).contains(&far));
    buy(&mut g, Card::Hop, Some(unit));
    assert!(
        !g.piece_destinations(unit).contains(&far),
        "ein Sprung bringt nur ueber eine Figur"
    );
    stock_up_for(&mut g, Card::Hop);
    buy(&mut g, Card::Hop, Some(unit));
    assert_eq!(g.state.pieces[&unit].perks.hop, 2);
    assert!(g.piece_destinations(unit).contains(&far), "zwei Spruenge");
    g.check_invariants().unwrap();
}

#[test]
fn the_control_area_follows_the_second_hop_card() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let mut g = control_game(&[((2, 0), k1)]);
    strip(&mut g, P0, 5);
    let id = g.setup_unit(P0, c(&g, 5, 0), 3, 3);
    g.setup_unit(P1, c(&g, 4, 0), 1, 1);
    g.setup_unit(P1, c(&g, 3, 0), 1, 1);
    g.state.pieces.get_mut(&id).unwrap().perks.hop = 1;
    assert!(!g.worked_cells(P0)[c(&g, 2, 0).idx()]);
    g.state.pieces.get_mut(&id).unwrap().perks.hop = 2;
    assert!(g.worked_cells(P0)[c(&g, 2, 0).idx()]);
}

// ----- Kontrollfelder (`terr_control`) ----------------------------------------------------------

fn control_game(deps: &[((i32, i32), ResourceId)]) -> Game {
    let mut cfg = v3_config(2);
    cfg.terr_control = true;
    with_deposits_cfg(cfg, deps)
}

#[test]
fn control_needs_the_occupy_rule() {
    let mut cfg = v3_config(2);
    cfg.terr_control = true;
    assert!(cfg.validate().is_ok());
    cfg.terr_occupy = false;
    assert!(cfg.validate().is_err());
    assert!(!v3_config(2).terr_control, "Standard: aus");
}

#[test]
fn a_field_in_reach_of_a_figure_counts_as_worked_with_the_control_rule() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let deps = [((3, 0), k1)];
    // Mit Kontrolle: Einheit der Stufe 2 auf (5,0) erreicht (4,0) und (3,0).
    let mut g = control_game(&deps);
    strip(&mut g, P0, 5);
    g.setup_unit(P0, c(&g, 5, 0), 2, 2);
    for _ in 0..3 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 1, "im Einflussbereich geerntet");
    // Ohne die Regel (nur Besetzung): nichts.
    let mut g = v3_game(&deps);
    strip(&mut g, P0, 5);
    g.setup_unit(P0, c(&g, 5, 0), 2, 2);
    for _ in 0..3 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 0);
    // Zu weit weg (Reichweite 1 von (5,0) erreicht (3,0) nicht).
    let mut g = control_game(&deps);
    strip(&mut g, P0, 5);
    g.setup_unit(P0, c(&g, 5, 0), 1, 1);
    for _ in 0..3 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 0);
}

#[test]
fn control_stops_at_the_first_figure_and_a_helpless_figure_controls_nothing() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let deps = [((3, 0), k1)];
    // Eine Figur auf (4,0) verdeckt (3,0).
    let mut g = control_game(&deps);
    strip(&mut g, P0, 5);
    g.setup_unit(P0, c(&g, 5, 0), 2, 2);
    g.setup_unit(P1, c(&g, 4, 0), 1, 1);
    let worked = g.worked_cells(P0);
    assert!(
        worked[c(&g, 4, 0).idx()],
        "das Feld mit dem Gegner ist im Bereich"
    );
    assert!(!worked[c(&g, 3, 0).idx()], "dahinter ist nichts mehr");
    // Mit Sprungfeder sieht sie eine Figur weiter.
    let id = g.state.piece_at(c(&g, 5, 0)).unwrap().id;
    g.state.pieces.get_mut(&id).unwrap().perks.hop = 1;
    assert!(g.worked_cells(P0)[c(&g, 3, 0).idx()]);
    // Eine Figur ausserhalb des verbundenen Landes beherrscht nichts.
    let mut g = control_game(&deps);
    strip(&mut g, P0, 5);
    g.setup_unit(P0, c(&g, 8, 0), 5, 5);
    assert!(!g.worked_cells(P0)[c(&g, 3, 0).idx()]);
}

// ----- Kat 3 gleichmaessig verteilt -------------------------------------------------------------

#[test]
fn kat_three_fields_are_single_cells_spread_far_apart() {
    for players in 2..=4u8 {
        for seed in 0..60u64 {
            let mut cfg = Config::new(players, seed);
            cfg.territory = true;
            cfg.set_v3(true);
            let map = Map::generate(&cfg).expect("Karte");
            let cells3: Vec<CellId> = map
                .board
                .cells()
                .filter(|&cell| map.deposits_at(cell).iter().any(|&r| map.category(r) == 3))
                .collect();
            assert_eq!(
                cells3.len(),
                2 * cfg.corner_count(),
                "zwei Einzelfelder je Ecke, {players}p seed {seed}"
            );
            let nearest = cells3
                .iter()
                .map(|&a| {
                    let d = map.board.distances_from(a);
                    cells3
                        .iter()
                        .filter(|&&b| b != a)
                        .map(|b| d[b.idx()])
                        .min()
                        .unwrap()
                })
                .min()
                .unwrap();
            assert!(
                nearest >= 3,
                "Kat-3-Felder liegen mindestens 3 Felder auseinander ({players}p seed {seed}: {nearest})"
            );
        }
    }
}

#[test]
fn a_card_needs_a_fitting_figure_to_be_bought() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Bend]);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 12);
    assert!(buys(&g).is_empty(), "nur der Koenig, kein Ziel");
    g.setup_unit(P0, c(&g, 3, 0), 1, 1);
    assert!(buys(&g).is_empty(), "ein Bauer hat keine Ecke zu gehen");
    let fid = g.home_factory(P0).unwrap().id;
    let no_target = Action::BuyCard {
        card: Card::Bend,
        from: crate::action::PaySource::Factory(fid),
        target: None,
    };
    let err = g.apply(no_target).unwrap_err();
    assert_eq!(
        err,
        RuleError::Illegal(IllegalReason::InvalidCardTarget),
        "{err}"
    );
}

#[test]
fn the_factory_shift_card_acts_at_once_and_forces_the_shift() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::FactoryShift]);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 12);
    let fid = g.home_factory(P0).unwrap().id;
    let buy = Action::BuyCard {
        card: Card::FactoryShift,
        from: crate::action::PaySource::Factory(fid),
        target: None,
    };
    // Ohne ein freies, verbundenes Diagonalfeld waere der Kauf wirkungslos: er ist nicht erlaubt.
    assert!(g.factory_shift_cells(P0).is_empty());
    assert!(!buys(&g).contains(&buy));
    let from = g.home_factory(P0).unwrap().cell;
    paint(&mut g, P0, &[(1, 0), (0, 1), (1, 1)]);
    assert!(buys(&g).contains(&buy));
    g.apply(buy).unwrap();
    // Jetzt ist nur noch das Verschieben der Fabrik erlaubt (kein Zug, kein Bau, kein Kauf).
    let legal = g.legal_actions();
    assert!(!legal.is_empty());
    assert!(
        legal
            .iter()
            .all(|a| matches!(a, Action::ShiftFactory { .. })),
        "{legal:?}"
    );
    assert_eq!(g.current_player(), P0);
    g.apply(legal[0]).unwrap();
    assert_ne!(g.home_factory(P0).unwrap().cell, from);
    assert_eq!(
        g.state.stored_card[P0.idx()],
        None,
        "die Karte ist verbraucht"
    );
    g.check_invariants().unwrap();
}

#[test]
fn version_3_cards_have_fixed_prices_in_iron_gold_and_bread() {
    use crate::cards::Card;
    let g = card_game(&[Card::Bend]);
    // Brot ist Standard: Tempo und Nachschub kosten Brot, Kampf und Bauten Eisen, Dauerhaftes Gold.
    assert_eq!(g.territory_card_price(Card::Shorten), [0, 0, 3]);
    assert_eq!(g.territory_card_price(Card::Bend), [0, 1, 2]);
    assert_eq!(g.territory_card_price(Card::Hop), [1, 0, 3]);
    assert_eq!(g.territory_card_price(Card::Pass), [1, 0, 1]);
    assert_eq!(g.territory_card_price(Card::PawnDiagonal), [3, 1, 0]);
    assert_eq!(g.territory_card_price(Card::Settle), [1, 2, 2]);
    assert_eq!(g.territory_card_price(Card::FactoryShift), [3, 0, 0]);
    assert_eq!(g.territory_card_price(Card::Charge), [3, 1, 0]);
    assert_eq!(g.territory_card_price(Card::Mining), [2, 3, 0]);
    assert_eq!(g.territory_card_price(Card::Smelter), [4, 1, 0]);
    assert_eq!(g.territory_card_price(Card::DoubleMove), [1, 2, 4]);
    assert_eq!(g.territory_card_price(Card::KingBoost), [0, 3, 0]);
    // Mit `terr_bread=0` gibt es kein Brot im Lager: die alten Preise in Eisen und Gold gelten.
    let mut old = g.config.clone();
    old.set_all("terr_bread=0").unwrap();
    let h = with_deposits_cfg(old, &[]);
    assert_eq!(h.territory_card_price(Card::Shorten), [3, 0, 0]);
    assert_eq!(h.territory_card_price(Card::Hop), [4, 0, 0]);
    assert_eq!(h.territory_card_price(Card::Bend), [2, 1, 0]);
    assert_eq!(h.territory_card_price(Card::FactoryShift), [3, 0, 0]);
    assert_eq!(h.territory_card_price(Card::KingBoost), [0, 3, 0]);
}

#[test]
fn the_bread_prices_spread_over_all_three_resources() {
    use crate::cards::Card;
    let cards = [
        Card::Shorten,
        Card::Bend,
        Card::Hop,
        Card::Pass,
        Card::PawnDiagonal,
        Card::Settle,
        Card::FactoryShift,
        Card::Charge,
        Card::Mining,
        Card::Smelter,
        Card::DoubleMove,
        Card::KingBoost,
    ];
    let uses = |i: usize| cards.iter().filter(|c| c.v3_price_bread()[i] > 0).count();
    // Jeder Rohstoff wird von mehreren Karten gebraucht; keiner ist ueberfluessig.
    assert!(uses(0) >= 6, "Eisen: {}", uses(0));
    assert!(uses(1) >= 6, "Gold: {}", uses(1));
    assert!(uses(2) >= 5, "Brot: {}", uses(2));
    // Es gibt Karten, die vor allem Brot brauchen, und welche, die vor allem Eisen brauchen.
    assert!(cards
        .iter()
        .any(|c| c.v3_price_bread()[2] > c.v3_price_bread()[0]));
    assert!(cards
        .iter()
        .any(|c| c.v3_price_bread()[0] > c.v3_price_bread()[2]));
    // Jede Karte hat einen Preis, und er blieb ungefaehr so hoch wie vor der Verteilung.
    for c in cards {
        let total: u32 = c.v3_price_bread().iter().sum();
        let old: u32 = c.v3_price().iter().sum();
        assert!(
            total > 0 && total.abs_diff(old) <= 1,
            "{c:?}: {total} vs {old}"
        );
    }
}

#[test]
fn the_area_bot_puts_a_card_on_its_best_existing_unit() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Bend, Card::Hop]);
    g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let best = g.setup_unit(P0, c(&g, 4, 0), 3, 3);
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 12);
    stock_gold(&mut g, P0, 2); // reicht fuer die Ecke (1), nicht fuer die Koenigskarte (3)
    let mut bot = AreaBot::new(1);
    let a = bot.choose(&g);
    assert!(
        matches!(a, Action::BuyCard { card: Card::Bend, target: Some(t), .. } if t == best),
        "{a:?}"
    );
}

// ----- Fuenfte Aenderung: Unterhalt der Figuren und Koenigs-Upgrade mit Kat 2 -------------------

fn give_area(g: &mut Game, p: PlayerId, n: u32) {
    g.state.area[p.idx()] = n;
}

/// Flaeche und Bonus, die der Ertrag von `p` gerade bringt (ohne Unterhalt gerechnet).
fn earned(g: &Game, p: PlayerId) -> u32 {
    let mut probe = g.clone();
    probe.config.terr_upkeep = false;
    income(&mut probe, p)
        .iter()
        .find_map(|e| match e {
            Event::Income { area, bonus, .. } => Some(area + bonus),
            _ => None,
        })
        .expect("Ertrag")
}

#[test]
fn every_figure_costs_its_level_in_area_each_round() {
    let mut g = v3_game(&[]);
    g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    g.setup_unit(P0, c(&g, 4, 0), 3, 3);
    give_area(&mut g, P0, 100);
    assert_eq!(g.territory_upkeep(P0), Some(5), "der Koenig zahlt nichts");
    let e = earned(&g, P0);
    let ev = income(&mut g, P0);
    assert!(
        ev.contains(&Event::Upkeep {
            player: P0,
            paid: 5
        }),
        "{ev:?}"
    );
    assert_eq!(g.state.area[0], 100 + e - 5);
    assert!(!ev.iter().any(|x| matches!(x, Event::Dissolved { .. })));
}

#[test]
fn upkeep_can_be_switched_off_and_is_not_charged_without_income() {
    let mut g = v3_game(&[]);
    g.setup_unit(P0, c(&g, 3, 0), 4, 4);
    give_area(&mut g, P0, 100);
    g.config.terr_upkeep = false;
    let ev = income(&mut g, P0);
    assert!(!ev.iter().any(|x| matches!(x, Event::Upkeep { .. })));
    assert_eq!(g.territory_upkeep(P0), None);
    // Waehrend die Fabrik gebaut wird, gibt es weder Ertrag noch Unterhalt.
    g.config.terr_upkeep = true;
    let fid = g.home_factory(P0).unwrap().id;
    g.state.factories.get_mut(&fid).unwrap().building = 2;
    give_area(&mut g, P0, 0);
    let ev = income(&mut g, P0);
    assert!(!ev
        .iter()
        .any(|x| matches!(x, Event::Upkeep { .. } | Event::Dissolved { .. })));
    assert_eq!(g.pieces_of(P0).count(), 2, "nichts loest sich auf");
    assert_eq!(g.state.area[0], 0);
}

#[test]
fn a_negative_area_dissolves_the_smallest_figure_that_covers_the_shortfall() {
    let mut g = v3_game(&[]);
    let a = g.setup_unit(P0, c(&g, 3, 0), 1, 1);
    let b = g.setup_unit(P0, c(&g, 4, 0), 1, 1);
    let hero = g.setup_unit(P0, c(&g, 5, 0), 4, 4);
    // Unterhalt 6, nach dem Ertrag liegen 5 in der Kasse: es fehlt 1.
    let e = earned(&g, P0);
    give_area(&mut g, P0, 5 - e);
    let ev = income(&mut g, P0);
    // Eine Stufe-1-Figur (die zuletzt gebaute) deckt das Minus allein; der Held bleibt.
    assert_eq!(
        ev.iter()
            .filter_map(|x| match x {
                Event::Dissolved { piece, level, .. } => Some((*piece, *level)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![(b, 1)]
    );
    assert!(
        ev.contains(&Event::Upkeep {
            player: P0,
            paid: 5
        }),
        "{ev:?}"
    );
    assert_eq!(g.state.area[0], 0);
    assert!(g.state.pieces.contains_key(&a) && g.state.pieces.contains_key(&hero));
    assert!(!g.state.pieces.contains_key(&b));
    assert!(
        g.state.occupant[c(&g, 4, 0).idx()].is_none(),
        "das Feld ist frei"
    );
    g.check_invariants().unwrap();
}

#[test]
fn a_deep_minus_dissolves_the_largest_figure_first_until_it_is_covered() {
    let mut g = v3_game(&[]);
    let a = g.setup_unit(P0, c(&g, 3, 0), 1, 1);
    let b = g.setup_unit(P0, c(&g, 4, 0), 1, 1);
    let hero = g.setup_unit(P0, c(&g, 5, 0), 4, 4);
    // Unterhalt 6, in der Kasse 1: es fehlen 5, keine Figur deckt das allein. Der Held geht,
    // dann fehlt noch 1: eine Stufe-1-Figur.
    let e = earned(&g, P0);
    give_area(&mut g, P0, 1 - e);
    let ev = income(&mut g, P0);
    let gone: Vec<(PieceId, u8)> = ev
        .iter()
        .filter_map(|x| match x {
            Event::Dissolved { piece, level, .. } => Some((*piece, *level)),
            _ => None,
        })
        .collect();
    assert_eq!(gone, vec![(hero, 4), (b, 1)]);
    assert!(g.state.pieces.contains_key(&a));
    assert!(
        ev.contains(&Event::Upkeep {
            player: P0,
            paid: 1
        }),
        "{ev:?}"
    );
    assert_eq!(g.state.area[0], 0, "nie unter 0");
    g.check_invariants().unwrap();
}

#[test]
fn a_cut_off_figure_still_costs_upkeep() {
    let mut g = v3_game(&[]);
    // Auf Land, das nicht mit der Fabrik zusammenhaengt: wehrlos, aber nicht gratis.
    paint(&mut g, P0, &[(8, 8)]);
    let far = g.setup_unit(P0, c(&g, 8, 8), 3, 3);
    give_area(&mut g, P0, 50);
    assert!(!g.piece_connected(&g.state.pieces[&far]), "abgeschnitten");
    let ev = income(&mut g, P0);
    assert!(
        ev.contains(&Event::Upkeep {
            player: P0,
            paid: 3
        }),
        "{ev:?}"
    );
}

#[test]
fn the_king_upgrade_is_always_offered_and_costs_gold_by_the_new_fields() {
    use crate::cards::Card;
    let mut g = v3_game(&[]);
    assert!(g.king_upgrade_on());
    assert!(
        !g.state.market.contains(&Card::KingBoost),
        "der Boost wird nicht mehr gezogen, er liegt immer aus"
    );
    let k2 = typ(&g, 0, 2);
    let king = king_id(&g, P0);
    // Quadrat: Stufe 1 bringt 4 Diagonalfelder, Stufe 2 vier Felder zwei geradeaus, Stufe 3 acht (L).
    assert_eq!(g.territory_card_price_for(P0, Card::KingBoost), [0, 4, 0]);
    let king_boost = |g: &Game| {
        buys(g).into_iter().any(|a| {
            matches!(
                a,
                Action::BuyCard {
                    card: Card::KingBoost,
                    ..
                }
            )
        })
    };
    stock(&mut g, P0, k2, 3);
    assert!(!king_boost(&g), "3 Kat 2 reichen nicht");
    stock(&mut g, P0, k2, 1);
    assert!(king_boost(&g));
    buy(&mut g, Card::KingBoost, None);
    assert_eq!(g.state.player_perks[0].king_boost, 1);
    assert_eq!(storage_of(&g, P0, k2), 0);
    assert_eq!(
        g.state.card_lock,
        Some(king),
        "der Koenig ruht in diesem Zug"
    );
    assert_eq!(g.territory_card_price_for(P0, Card::KingBoost), [0, 4, 0]);
    g.state.player_perks[0].king_boost = 2;
    assert_eq!(g.territory_card_price_for(P0, Card::KingBoost), [0, 8, 0]);
    g.state.player_perks[0].king_boost = 3;
    assert!(
        !g.territory_card_takeable(P0, Card::KingBoost),
        "hoechste Stufe"
    );
}

#[test]
fn the_old_king_boost_card_stays_when_the_upgrade_switch_is_off() {
    use crate::cards::Card;
    let mut cfg = v3_config(2);
    cfg.terr_kingup = false;
    let g = with_deposits_cfg(cfg, &[]);
    assert!(!g.king_upgrade_on());
    assert_eq!(g.territory_card_price_for(P0, Card::KingBoost), [0, 3, 0]);
    assert!(
        !g.territory_offers().contains(&Card::KingBoost)
            || g.state.market.contains(&Card::KingBoost),
        "nur wenn sie gezogen wurde"
    );
}

#[test]
fn the_king_upgrade_works_on_hexagon_boards_with_six_fields_per_step() {
    use crate::cards::Card;
    let mut cfg = Config::new(2, 4);
    cfg.territory = true;
    cfg.set_v3(true);
    cfg.board_corners = 6;
    let mut g = Game::new(cfg).expect("Sechseckbrett");
    assert!(g.king_upgrade_on());
    assert_eq!(
        [1, 2, 3].map(|l| g.king_boost_extra(l)),
        [6, 6, 12],
        "Felder, die der Koenig auf freiem Brett dazubekommt"
    );
    // In der Mitte des Bretts: ohne Boost die 6 Nachbarn, jede Stufe bringt genau die Felder dazu.
    let king = king_id(&g, P0);
    let mid = g.map.board.cell_at(0, 0).expect("Mitte");
    g.setup_teleport(king, mid);
    let base = g.piece_destinations(king).len();
    assert_eq!(base, 6);
    for (level, extra) in [(1u8, 6usize), (2, 6), (3, 12)] {
        g.state.player_perks[0].king_boost = level;
        let now = g.piece_destinations(king).len();
        let before = base
            + [6usize, 6, 12][..usize::from(level) - 1]
                .iter()
                .sum::<usize>();
        assert_eq!(now - before, extra, "Stufe {level}");
    }
    assert!(g.territory_offers().contains(&Card::KingBoost));
}

#[test]
fn on_a_square_board_the_boost_levels_add_four_four_and_eight_fields() {
    let mut g = v3_game(&[]);
    let king = king_id(&g, P0);
    let mid = c(&g, 5, 5);
    g.setup_teleport(king, mid);
    let base = g.piece_destinations(king).len();
    assert_eq!(base, 4);
    let mut total = base;
    for (level, extra) in [(1u8, 4usize), (2, 4), (3, 8)] {
        g.state.player_perks[0].king_boost = level;
        total += extra;
        assert_eq!(g.piece_destinations(king).len(), total, "Stufe {level}");
    }
}

#[test]
fn there_is_no_king_upgrade_on_the_triangle_board() {
    let mut cfg = Config::new(3, 2);
    cfg.territory = true;
    cfg.set_v3(true);
    let g = Game::new(cfg).expect("Dreiecksbrett");
    assert!(!g.king_upgrade_on());
    assert!(!g
        .territory_offers()
        .contains(&crate::cards::Card::KingBoost));
}

#[test]
fn bots_survive_upkeep_and_the_king_upgrade_without_breaking_the_rules() {
    for seed in 0..6u64 {
        let bots: Vec<Box<dyn Bot>> = vec![
            Box::new(AreaBot::new(seed)),
            Box::new(GreedyBot::new(seed + 1)),
        ];
        let g = play(2, seed, 150, bots);
        assert!(g.state.round > 1);
        g.check_invariants().unwrap();
    }
}

// ----- Siebte Aenderung: Ecke nur am Wegende, neue Karten, Preise und eigene Karten --------------

fn set_bend(g: &mut Game, piece: PieceId, n: u8) {
    g.state.pieces.get_mut(&piece).unwrap().perks.bend = n;
}

#[test]
fn the_corner_card_only_bends_the_last_fields_in_version_3() {
    let mut g = card_game(&[]);
    // Stufe 3 auf (5,5), ein Blocker auf (5,7): "hoch, hoch, rechts" ist versperrt, "rechts, hoch, hoch"
    // und "hoch, rechts, hoch" fuehren auf (6,7), und beide haben ihre Ecke nicht am letzten Feld.
    let unit = g.setup_unit(P0, c(&g, 5, 5), 3, 3);
    g.setup_unit(P1, c(&g, 5, 7), 1, 1);
    let target = c(&g, 6, 7);
    assert!(g.config.terr_bend_tail, "Version 3 hat die neue Regel");
    set_bend(&mut g, unit, 1);
    assert!(
        !g.piece_destinations(unit).contains(&target),
        "mit einer Karte darf nur das letzte Feld abbiegen"
    );
    set_bend(&mut g, unit, 2);
    assert!(
        g.piece_destinations(unit).contains(&target),
        "mit zwei Karten auch das vorletzte"
    );
    set_bend(&mut g, unit, 0);
    assert!(
        !g.piece_destinations(unit).contains(&target),
        "ohne Karte nur gerade"
    );
    // Alte Regel (Version 2): eine Ecke irgendwo auf dem Weg.
    g.config.terr_bend_tail = false;
    set_bend(&mut g, unit, 1);
    assert!(g.piece_destinations(unit).contains(&target));
}

#[test]
fn one_corner_card_lets_the_last_field_turn_either_way() {
    let mut g = card_game(&[]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    set_bend(&mut g, unit, 1);
    let dest = g.piece_destinations(unit);
    // "hoch, links/rechts" und "rechts, hoch/runter" sind Ecken am letzten Feld.
    for (x, y) in [(4, 6), (6, 6), (6, 4), (6, 6), (4, 4), (5, 7), (7, 5)] {
        assert!(dest.contains(&c(&g, x, y)), "({x},{y})");
    }
    assert!(
        !dest.contains(&c(&g, 5, 5)),
        "zurueck auf das Startfeld nie"
    );
}

#[test]
fn the_corner_card_costs_one_gold_and_two_bread() {
    use crate::cards::Card;
    let mut g = card_game_no_bread(&[Card::Bend]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let (k1, k2, k3) = (typ(&g, 0, 1), typ(&g, 0, 2), typ(&g, 0, 3));
    stock(&mut g, P0, k3, 1);
    stock(&mut g, P0, k2, 1);
    assert!(buys(&g).is_empty(), "mit einem Brot nicht bezahlbar");
    stock(&mut g, P0, k3, 1);
    assert_eq!(buys(&g).len(), 1, "1 Gold und 2 Brot reichen");
    let mut no_gold = card_game_no_bread(&[Card::Bend]);
    no_gold.setup_unit(P0, c(&no_gold, 5, 5), 2, 2);
    stock(&mut no_gold, P0, k3, 5);
    assert!(buys(&no_gold).is_empty(), "ohne Gold nicht bezahlbar");
    stock(&mut g, P0, k1, 5);
    buy(&mut g, Card::Bend, Some(unit));
    assert_eq!(storage_of(&g, P0, k1), 5, "Eisen bleibt unberuehrt");
    assert_eq!(storage_of(&g, P0, k2), 0);
    assert_eq!(storage_of(&g, P0, k3), 0);
    assert_eq!(g.state.pieces[&unit].perks.bend, 1);
}

#[test]
fn the_pass_card_lets_a_unit_move_through_its_own_figures_only() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Pass]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    g.setup_unit(P0, c(&g, 5, 6), 1, 1);
    g.setup_unit(P1, c(&g, 7, 5), 1, 1);
    let own_behind = c(&g, 5, 7);
    let enemy_blocked = c(&g, 8, 5);
    let on_own = c(&g, 5, 6);
    assert!(!g.piece_destinations(unit).contains(&own_behind));
    let k1 = typ(&g, 0, 1);
    stock(&mut g, P0, k1, 5);
    buy(&mut g, Card::Pass, Some(unit));
    assert_eq!(g.state.pieces[&unit].perks.pass, 1);
    // Die Figur ruht in diesem Zug (terr_cardlock=1): fuer die Pruefung die Sperre loesen.
    g.state.card_lock = None;
    let dest = g.piece_destinations(unit);
    assert!(dest.contains(&own_behind), "ueber die eigene Figur hinweg");
    assert!(!dest.contains(&on_own), "nie auf ein eigenes Feld");
    assert!(!dest.contains(&enemy_blocked), "Gegner versperren weiter");
}

#[test]
fn the_settle_card_paints_the_free_neighbours_at_once() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Settle]);
    strip(&mut g, P0, 3);
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    paint(&mut g, P1, &[(3, 1)]);
    // Eine fremde Figur auf (4,0) bleibt, wie sie ist; die Nachbarn (2,0) gehoeren schon uns.
    g.setup_unit(P1, c(&g, 4, 0), 1, 1);
    let cells = g.settle_cells(&g.state.pieces[&unit].clone());
    assert_eq!(
        cells,
        vec![c(&g, 3, 1)],
        "nur das fremde Land, ohne Figur und ohne eigenes"
    );
    let k1 = typ(&g, 0, 1);
    let k2 = typ(&g, 0, 2);
    stock(&mut g, P0, k1, 3);
    stock(&mut g, P0, k2, 2);
    buy(&mut g, Card::Settle, Some(unit));
    assert_eq!(g.state.land[c(&g, 3, 1).idx()], Some(P0));
    g.check_invariants().unwrap();
}

#[test]
fn the_settle_card_does_not_destroy_a_factory_and_needs_connected_land() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Settle]);
    strip(&mut g, P0, 2);
    let unit = g.setup_unit(P0, c(&g, 2, 0), 2, 2);
    let fcell = c(&g, 2, 1);
    g.set_land(fcell, Some(P1));
    g.state.insert_factory(P1, fcell);
    let cells = g.settle_cells(&g.state.pieces[&unit].clone());
    assert!(!cells.contains(&fcell), "gegnerische Fabrik bleibt");
    // Eine Figur weitab vom eigenen Land faerbt nichts.
    let lone = g.setup_unit(P0, c(&g, 7, 7), 2, 2);
    assert!(g.settle_cells(&g.state.pieces[&lone].clone()).is_empty());
    assert!(!g.card_fits_piece(Card::Settle, &g.state.pieces[&lone].clone()));
}

#[test]
fn the_smelter_and_mining_cards_double_a_harvest_for_good() {
    use crate::cards::Card;
    let g0 = v3_game(&[]);
    let (k1, k2) = (typ(&g0, 0, 1), typ(&g0, 0, 2));
    let mut g = v3_game(&[((1, 0), k1), ((2, 0), k2)]);
    g.state.market = vec![Card::Smelter, Card::Mining];
    g.state.area = vec![10_000; 2];
    g.config.terr_occupy = false;
    strip(&mut g, P0, 2);
    // Schmelze 4 Eisen + 1 Gold, Bergbau 2 Eisen + 3 Gold.
    stock(&mut g, P0, k1, 6);
    stock(&mut g, P0, k2, 5);
    buy(&mut g, Card::Smelter, None);
    assert!(g.state.player_perks[0].smelter);
    // Ernte: Eisen alle 3 Runden, Gold alle 5.
    let before = storage_of(&g, P0, k1);
    for _ in 0..3 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1) - before, 2, "Eisen doppelt");
    assert_eq!(
        storage_of(&g, P0, k2),
        5 - 1,
        "Gold noch nicht gewachsen (3 von 5 Runden)"
    );
    g.state.bought_this_turn = false;
    buy(&mut g, Card::Mining, None);
    assert!(g.state.player_perks[0].mining);
    for _ in 0..2 {
        income(&mut g, P0);
    }
    // 5 Gold - 1 (Schmelze) - 3 (Bergbau) = 1, dazu die doppelte Ernte von 2.
    assert_eq!(storage_of(&g, P0, k2), 3, "Gold doppelt");
    // Beide Karten liegen danach nicht mehr aus: schon vorhanden.
    g.state.bought_this_turn = false;
    assert!(buys(&g).is_empty());
}

#[test]
fn the_charge_card_adds_a_field_to_every_move_for_one_turn() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Charge]);
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let k1 = typ(&g, 0, 1);
    let k2 = typ(&g, 0, 2);
    // Sturmangriff: 3 Eisen + 1 Gold.
    stock(&mut g, P0, k1, 3);
    stock(&mut g, P0, k2, 1);
    let far = c(&g, 5, 8);
    assert!(!g.piece_destinations(unit).contains(&far));
    buy(&mut g, Card::Charge, None);
    assert!(g.state.charge);
    assert!(g.state.card_lock.is_none(), "keine Figur gesperrt");
    assert!(
        g.piece_destinations(unit).contains(&far),
        "drei statt zwei Felder"
    );
    let mv = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { piece, .. } if *piece == king_id(&g, P0)))
        .unwrap();
    g.apply(mv).unwrap();
    assert!(!g.state.charge, "nur in diesem Zug");
}

#[test]
fn the_double_move_card_gives_a_second_action() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::DoubleMove]);
    let k1 = typ(&g, 0, 1);
    let k2 = typ(&g, 0, 2);
    stock(&mut g, P0, k1, 4);
    stock(&mut g, P0, k2, 3);
    buy(&mut g, Card::DoubleMove, None);
    assert_eq!(g.state.extra_actions, 1);
    let first = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { .. }))
        .unwrap();
    g.apply(first).unwrap();
    assert_eq!(g.state.current, P0, "noch einmal am Zug");
    let second = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { .. }))
        .unwrap();
    g.apply(second).unwrap();
    assert_eq!(g.state.current, P1);
}

#[test]
fn prices_pools_and_custom_cards_come_from_the_rules() {
    use crate::cards::Card;
    let mut cfg = Config::new(2, 1);
    cfg.set_all(
        "territory=1,terr_v3=1,price.bend=5/0/1,pool.hop=0,custom.0=Doppelecke|bend*2+hop|3/2/0",
    )
    .unwrap();
    assert_eq!(cfg.card_tweaks.price.get(&Card::Bend), Some(&[5, 0, 1]));
    assert!(cfg.card_tweaks.off.contains(&Card::Hop));
    let g = Game::new(cfg.clone()).unwrap();
    assert_eq!(g.territory_card_price(Card::Bend), [5, 0, 1]);
    let custom = Card::Custom(0);
    assert_eq!(g.card_name(custom), "Doppelecke");
    assert_eq!(g.territory_card_price(custom), [3, 2, 0]);
    assert!(g.card_text(custom).contains("Ecke ×2"));
    assert!(cfg.card_tweaks.target(custom) == crate::cards::CardTarget::Unit);
    // Zurueck auf den Standardpreis, Karte wieder im Vorrat, eigene Karte loeschen.
    cfg.set_all("price.bend=,pool.hop=1,custom.0=").unwrap();
    assert!(cfg.card_tweaks.price.is_empty());
    assert_eq!(
        cfg.card_tweaks.off,
        crate::cards::CardTweaks::standard().off,
        "nur die Standard-Aus-Karten fehlen im Pool"
    );
    assert!(!cfg.card_tweaks.defined(custom));
}

#[test]
fn bad_card_rules_are_rejected_with_a_message() {
    let mut cfg = Config::new(2, 1);
    for bad in [
        "price.nonsense=1/1/1",
        "price.bend=a/b",
        "price.bend=1/2/3/4",
        "pool.bend=2",
        "custom.9=X|bend|1/0/0",
        "custom.0=|bend|1/0/0",
        "custom.0=X|king_boost|1/0/0",
        "custom.0=X|factory_shift|1/0/0",
        "custom.0=X|bend*7|1/0/0",
        "custom.0=X||1/0/0",
        "custom.0=X|bend",
    ] {
        assert!(cfg.set_all(bad).is_err(), "{bad}");
    }
    assert!(cfg.card_tweaks.custom.iter().all(Option::is_none));
}

#[test]
fn a_custom_card_applies_its_blocks_one_after_the_other() {
    use crate::cards::Card;
    let mut cfg = v3_config(2);
    cfg.set_all("custom.0=Sturmecke|bend*2+charge|2/1/0")
        .unwrap();
    let mut g = with_deposits_cfg(cfg, &[]);
    g.state.market = vec![Card::Custom(0)];
    g.state.area = vec![10_000; 2];
    let unit = g.setup_unit(P0, c(&g, 5, 5), 3, 3);
    let (k1, k2) = (typ(&g, 0, 1), typ(&g, 0, 2));
    stock(&mut g, P0, k1, 2);
    stock(&mut g, P0, k2, 1);
    assert_eq!(buys(&g).len(), 1);
    buy(&mut g, Card::Custom(0), Some(unit));
    assert_eq!(g.state.pieces[&unit].perks.bend, 2, "zwei Ecken");
    assert!(g.state.charge, "und der Sturmangriff");
    g.check_invariants().unwrap();
}

#[test]
fn a_custom_card_of_instant_blocks_needs_no_figure() {
    use crate::cards::Card;
    let mut cfg = v3_config(2);
    cfg.set_all("custom.1=Kriegskasse|mining+smelter|1/1/0")
        .unwrap();
    let mut g = with_deposits_cfg(cfg, &[]);
    g.state.market = vec![Card::Custom(1)];
    let (k1, k2) = (typ(&g, 0, 1), typ(&g, 0, 2));
    stock(&mut g, P0, k1, 1);
    stock(&mut g, P0, k2, 1);
    let actions = buys(&g);
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::BuyCard { target: None, .. }));
    buy(&mut g, Card::Custom(1), None);
    assert!(g.state.player_perks[0].mining && g.state.player_perks[0].smelter);
}

#[test]
fn the_version_3_market_is_drawn_at_random_from_a_big_pool() {
    use crate::cards::Card;
    let mut seen = std::collections::BTreeSet::new();
    let mut markets = std::collections::BTreeSet::new();
    for seed in 0..60 {
        let mut cfg = Config::new(2, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.market_size = 5;
        let g = Game::new(cfg).unwrap();
        assert_eq!(g.state.market.len(), 5);
        assert!(
            !g.state.market.contains(&Card::KingBoost),
            "liegt immer aus, wird nicht gezogen"
        );
        seen.extend(g.state.market.iter().copied());
        markets.insert(g.state.market.clone());
    }
    assert!(
        markets.len() > 20,
        "viele verschiedene Maerkte: {}",
        markets.len()
    );
    for card in [
        Card::Shorten,
        Card::Bend,
        Card::Hop,
        Card::Pass,
        Card::PawnDiagonal,
        Card::Settle,
        Card::Mining,
        Card::Smelter,
        Card::FactoryShift,
    ] {
        assert!(seen.contains(&card), "{card:?} wird gezogen");
    }
    // Doppelzug und Sturmangriff liegen standardmaessig nicht aus (Version 3.2) ...
    for card in Card::V3_DEFAULT_OFF {
        assert!(
            !seen.contains(&card),
            "{card:?} wird standardmaessig nicht gezogen"
        );
    }
    // ... lassen sich aber mit pool.<karte>=1 wieder einschalten.
    let mut back = std::collections::BTreeSet::new();
    for seed in 0..60 {
        let mut cfg = Config::new(2, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.market_size = 5;
        cfg.set_all("pool.charge=1,pool.double_move=1").unwrap();
        back.extend(Game::new(cfg).unwrap().state.market);
    }
    assert!(back.contains(&Card::Charge) && back.contains(&Card::DoubleMove));
}

// ----- Version 3.2: Diagonalzug nur fuer Bauern -------------------------------------------------

#[test]
fn the_diagonal_card_only_fits_a_pawn_and_only_once() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::PawnDiagonal]);
    let pawn = g.setup_unit(P0, c(&g, 5, 5), 1, 1);
    let knight = g.setup_unit(P0, c(&g, 2, 2), 2, 2);
    let (pawn_p, knight_p) = (
        g.state.pieces[&pawn].clone(),
        g.state.pieces[&knight].clone(),
    );
    assert!(g.card_fits_piece(Card::PawnDiagonal, &pawn_p));
    assert!(
        !g.card_fits_piece(Card::PawnDiagonal, &knight_p),
        "nur Stufe 1"
    );
    let k1 = typ(&g, 0, 1);
    let k2 = typ(&g, 0, 2);
    stock(&mut g, P0, k1, 3);
    stock(&mut g, P0, k2, 1);
    let offers = buys(&g);
    assert_eq!(offers.len(), 1, "genau ein Ziel: der Bauer");
    buy(&mut g, Card::PawnDiagonal, Some(pawn));
    assert!(g.state.pieces[&pawn].perks.pawn_diag);
    let again = g.state.pieces[&pawn].clone();
    assert!(
        !g.card_fits_piece(Card::PawnDiagonal, &again),
        "nur einmal je Figur"
    );
}

#[test]
fn a_pawn_with_the_diagonal_card_moves_and_hits_diagonally() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::PawnDiagonal]);
    // Der Bauer steht auf eigenem, verbundenem Land (sonst ist er wehrlos und schlaegt nichts).
    paint(&mut g, P0, &[(0, 0), (1, 0), (0, 1), (1, 1)]);
    let pawn = g.setup_unit(P0, c(&g, 1, 1), 1, 1);
    g.setup_unit(P1, c(&g, 2, 2), 1, 1);
    g.setup_unit(P0, c(&g, 2, 0), 1, 1);
    let straight = g.piece_destinations(pawn);
    for (x, y) in [(0, 2), (2, 2), (2, 0), (0, 0)] {
        assert!(
            !straight.contains(&c(&g, x, y)),
            "ohne Karte nicht ({x},{y})"
        );
    }
    g.state.pieces.get_mut(&pawn).unwrap().perks.pawn_diag = true;
    let dest = g.piece_destinations(pawn);
    assert!(dest.contains(&c(&g, 0, 2)), "frei: ziehen");
    assert!(dest.contains(&c(&g, 2, 2)), "Gegner: schlagen");
    assert!(!dest.contains(&c(&g, 2, 0)), "eigene Figur: nein");
    for (x, y) in [(1, 2), (2, 1), (1, 0), (0, 1)] {
        assert!(dest.contains(&c(&g, x, y)), "gerade bleibt ({x},{y})");
    }
    // Der Zug klappt auch wirklich (Bruecke zum Ziel faerbt Land).
    g.apply(Action::Move {
        piece: pawn,
        to: c(&g, 0, 2),
    })
    .unwrap();
    assert_eq!(g.state.pieces[&pawn].cell, c(&g, 0, 2));
    g.check_invariants().unwrap();
}

#[test]
fn the_diagonal_card_works_only_on_square_boards() {
    use crate::board::GridKind;
    use crate::cards::Card;
    assert!(Card::PawnDiagonal.available_on(GridKind::Square));
    assert!(!Card::PawnDiagonal.available_on(GridKind::Hex));
    assert!(!Card::PawnDiagonal.available_on(GridKind::Triangle));
    assert_eq!(Card::from_key("pawn_diagonal"), Some(Card::PawnDiagonal));
}

#[test]
fn cards_that_do_nothing_on_the_hex_board_are_not_drawn_there() {
    use crate::cards::Card;
    for seed in 0..30 {
        let mut cfg = Config::new(2, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.board_corners = 6;
        cfg.market_size = 11;
        let g = Game::new(cfg).unwrap();
        assert!(!g.state.market.contains(&Card::FactoryShift));
    }
}

#[test]
fn whole_games_with_every_card_on_offer_keep_every_invariant() {
    use crate::cards::Card;
    for (players, seed) in [(2u8, 11u64), (2, 12), (3, 13), (4, 14)] {
        let mut cfg = Config::new(players, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.max_rounds = 50;
        cfg.score_after = 50;
        cfg.market_size = 11;
        cfg.set_all("custom.0=Mix|bend+hop+pass|3/2/0,custom.1=Kasse|mining+smelter|2/2/0")
            .unwrap();
        let mut g = Game::new(cfg).unwrap();
        assert!(g.state.market.contains(&Card::Settle));
        let mut bots: Vec<Box<dyn Bot>> = (0..players)
            .map(|i| match i % 2 {
                0 => Box::new(RandomBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                _ => Box::new(GreedyBot::new(seed + u64::from(i))) as Box<dyn Bot>,
            })
            .collect();
        let mut steps = 0;
        let mut bought = std::collections::BTreeSet::new();
        while g.outcome().is_none() {
            let me = g.current_player().idx();
            let a = bots[me].choose(&g);
            if let Action::BuyCard { card, .. } = a {
                bought.insert(card);
            }
            g.apply(a).unwrap();
            g.check_invariants()
                .unwrap_or_else(|e| panic!("{players}p seed {seed} Zug {steps}: {e}"));
            steps += 1;
            assert!(steps < 30_000);
        }
    }
}

// ----- Version 3.1: Verzoegerung, Unterhalt im Quadrat, Ertrag je Stufe, Brot-Lager -------------

/// Irgendeinen Zug des Spielers am Zug ausfuehren (Zugende, wenn nichts anderes folgt).
fn play_a_move(g: &mut Game) {
    let mv = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { .. }))
        .expect("ein Zug ist moeglich");
    g.apply(mv).unwrap();
}

#[test]
fn a_delayed_charge_works_only_in_the_next_own_turn() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::Charge]);
    g.config.terr_delay = true;
    let unit = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    let (k1, k2) = (typ(&g, 0, 1), typ(&g, 0, 2));
    stock(&mut g, P0, k1, 3);
    stock(&mut g, P0, k2, 1);
    let far = c(&g, 5, 8);
    buy(&mut g, Card::Charge, None);
    assert!(!g.state.charge, "noch nicht im selben Zug");
    assert!(g.state.armed_charge[0] && !g.state.armed_charge[1]);
    assert!(!g.piece_destinations(unit).contains(&far));
    assert!(
        !buys(&g).iter().any(|a| matches!(
            a,
            Action::BuyCard {
                card: Card::Charge,
                ..
            }
        )),
        "zweimal bereit geht nicht"
    );
    play_a_move(&mut g);
    assert_eq!(g.state.current, P1);
    assert!(!g.state.charge, "der Gegner bekommt nichts");
    assert!(
        g.state.armed_charge[0],
        "bleibt bereit, sichtbar fuer den Gegner"
    );
    play_a_move(&mut g);
    assert_eq!(g.state.current, P0);
    assert!(g.state.charge, "jetzt wirkt er");
    assert!(!g.state.armed_charge[0]);
    assert!(
        g.piece_destinations(unit).contains(&far),
        "drei statt zwei Felder"
    );
    play_a_move(&mut g);
    play_a_move(&mut g);
    assert!(!g.state.charge, "und nur in diesem einen Zug");
    g.check_invariants().unwrap();
}

#[test]
fn a_delayed_double_move_gives_the_second_action_one_turn_later() {
    use crate::cards::Card;
    let mut g = card_game(&[Card::DoubleMove]);
    g.config.terr_delay = true;
    let (k1, k2) = (typ(&g, 0, 1), typ(&g, 0, 2));
    stock(&mut g, P0, k1, 4);
    stock(&mut g, P0, k2, 3);
    buy(&mut g, Card::DoubleMove, None);
    assert_eq!(g.state.extra_actions, 0);
    assert_eq!(g.state.armed_extra[0], 1);
    // Eine zweite Karte waere zu viel: der Doppelzug liegt schon bereit.
    g.state.bought_this_turn = false;
    assert!(buys(&g).is_empty());
    play_a_move(&mut g); // P0 beendet den Zug
    assert_eq!(g.state.current, P1);
    assert_eq!(g.state.extra_actions, 0);
    play_a_move(&mut g);
    assert_eq!(g.state.current, P0);
    assert_eq!(g.state.extra_actions, 1, "die zweite Aktion ist da");
    assert_eq!(g.state.armed_extra[0], 0);
    play_a_move(&mut g);
    assert_eq!(g.state.current, P0, "noch einmal am Zug");
    play_a_move(&mut g);
    assert_eq!(g.state.current, P1);
    assert_eq!(g.state.extra_actions, 0);
    g.check_invariants().unwrap();
}

#[test]
fn the_delay_is_a_switch_that_is_off_by_default_and_changes_the_card_text() {
    use crate::cards::Card;
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1").unwrap();
    assert!(!cfg.terr_delay && !cfg.terr_upkeep_sq && !cfg.terr_yield_lvl && cfg.terr_bread == 3);
    cfg.set_all("terr_delay=1").unwrap();
    assert!(cfg.terr_delay);
    let g = with_deposits_cfg(cfg, &[]);
    assert!(g.card_text(Card::DoubleMove).contains("nächsten Zug"));
    assert!(g.card_text(Card::Charge).contains("nächsten Zug"));
    assert!(!g.card_text(Card::Shorten).contains("nächsten Zug"));
    let plain = v3_game(&[]);
    assert!(!plain.card_text(Card::DoubleMove).contains("nächsten Zug"));
}

#[test]
fn squared_upkeep_charges_the_level_squared_and_dissolves_the_big_one() {
    let mut g = v3_game(&[]);
    g.config.terr_upkeep_sq = true;
    g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    g.setup_unit(P0, c(&g, 4, 0), 3, 3);
    give_area(&mut g, P0, 100);
    assert_eq!(g.territory_upkeep(P0), Some(4 + 9), "2² + 3²");
    let ev = income(&mut g, P0);
    assert!(
        ev.contains(&Event::Upkeep {
            player: P0,
            paid: 13
        }),
        "{ev:?}"
    );
    // Held (16) und ein Bauer (1): es fehlen 3, nur der Held deckt das allein.
    let mut g = v3_game(&[]);
    g.config.terr_upkeep_sq = true;
    let pawn = g.setup_unit(P0, c(&g, 3, 0), 1, 1);
    let hero = g.setup_unit(P0, c(&g, 5, 0), 4, 4);
    let e = earned(&g, P0);
    give_area(&mut g, P0, 14 - e);
    let ev = income(&mut g, P0);
    assert_eq!(
        ev.iter()
            .filter_map(|x| match x {
                Event::Dissolved { piece, level, .. } => Some((*piece, *level)),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![(hero, 4)]
    );
    assert!(g.state.pieces.contains_key(&pawn));
    assert_eq!(g.state.area[0], 13, "14 minus der Unterhalt des Bauern");
    g.check_invariants().unwrap();
}

#[test]
fn squared_upkeep_needs_the_upkeep_switch_and_changes_nothing_by_default() {
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1,terr_upkeep=0,terr_upkeep_sq=1")
        .unwrap();
    assert!(cfg.validate().is_err());
    cfg.set_all("terr_upkeep=1").unwrap();
    assert!(cfg.validate().is_ok());
    let mut g = v3_game(&[]);
    g.setup_unit(P0, c(&g, 3, 0), 4, 4);
    assert_eq!(g.territory_upkeep(P0), Some(4), "ohne Schalter wie bisher");
}

#[test]
fn a_figure_harvests_as_often_as_its_level_with_yield_by_level() {
    let g0 = v3_game(&[]);
    let k1 = typ(&g0, 0, 1);
    let run = |level: u8, switch: bool, smelter: bool| -> u32 {
        let mut g = v3_game(&[((1, 0), k1)]);
        g.config.terr_yield_lvl = switch;
        strip(&mut g, P0, 2);
        g.setup_unit(P0, c(&g, 1, 0), level, level);
        if smelter {
            g.state.player_perks[0].smelter = true;
        }
        // Der Eisen-Takt ist 3: der Zaehler steht auf 2, nach dem Ertrag auf 3 = Ernte.
        let field = c(&g, 1, 0);
        g.state.age[field.idx()] = 2;
        give_area(&mut g, P0, 10_000);
        let before = storage_of(&g, P0, k1);
        income(&mut g, P0);
        g.check_invariants().unwrap();
        storage_of(&g, P0, k1) - before
    };
    assert_eq!(run(3, false, false), 1, "ohne Schalter wie bisher");
    assert_eq!(run(1, true, false), 1);
    assert_eq!(run(3, true, false), 3, "Stufe 3 erntet dreifach");
    assert_eq!(run(4, true, false), 4);
    assert_eq!(run(3, true, true), 4, "Schmiede: das +1 kommt danach dazu");
    // Der Koenig erntet einfach.
    let mut g = v3_game(&[((0, 0), k1)]);
    g.config.terr_yield_lvl = true;
    let home = c(&g, 0, 0);
    g.state.age[home.idx()] = 2;
    give_area(&mut g, P0, 10_000);
    let before = storage_of(&g, P0, k1);
    income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k1) - before, 1);
}

#[test]
fn yield_by_level_needs_the_occupy_rule() {
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1,terr_occupy=0,terr_yield_lvl=1")
        .unwrap();
    assert!(cfg.validate().is_err());
    cfg.set_all("terr_occupy=1").unwrap();
    assert!(cfg.validate().is_ok());
}

#[test]
fn with_the_bread_stock_kat3_is_harvested_into_the_factory_and_pays_cards() {
    use crate::cards::Card;
    let g0 = v3_game(&[]);
    let k3 = typ(&g0, 0, 3);
    let mut g = v3_game(&[((1, 0), k3)]);
    g.config.terr_bread = 2;
    g.config.validate().expect("Regeln gueltig");
    strip(&mut g, P0, 2);
    g.setup_unit(P0, c(&g, 1, 0), 1, 1);
    give_area(&mut g, P0, 10_000);
    // Takt 2: nach der ersten Runde noch nichts, nach der zweiten ein Brot.
    income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k3), 0);
    let ev = income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k3), 1, "{ev:?}");
    assert!(
        ev.iter().any(|e| matches!(
            e,
            Event::Income {
                bonus: 0,
                kat3: 0,
                ..
            }
        )),
        "kein 2^k-Bonus mehr"
    );
    assert_eq!(g.connected_kat3(P0), 0);
    // Brot bezahlt Tempo und Nachschub, Eisen Kampf und Bauten.
    assert_eq!(g.territory_card_price(Card::Shorten), [0, 0, 3]);
    assert_eq!(g.territory_card_price(Card::Pass), [1, 0, 1]);
    assert_eq!(g.territory_card_price(Card::DoubleMove), [1, 2, 4]);
    assert_eq!(g.territory_card_price(Card::Bend), [0, 1, 2]);
    // Ein eigener Preis hat Vorrang.
    let mut cfg = g.config.clone();
    cfg.set_all("price.shorten=1/0/1").unwrap();
    let h = with_deposits_cfg(cfg, &[]);
    assert_eq!(h.territory_card_price(Card::Shorten), [1, 0, 1]);
}

#[test]
fn bread_is_a_store_by_default_with_the_counter_of_iron_and_needs_a_piece_on_the_field() {
    let g0 = v3_game(&[]);
    let (k1, k3) = (typ(&g0, 0, 1), typ(&g0, 0, 3));
    // Eisen und Brot liegen auf je einem eigenen Feld neben der Fabrik, beide Regeln wie geliefert.
    let mut g = v3_game(&[((1, 0), k1), ((2, 0), k3)]);
    assert_eq!(g.config.terr_bread, g.config.terr_ripen[0], "gleicher Takt");
    strip(&mut g, P0, 2);
    give_area(&mut g, P0, 10_000);
    // Ohne Figur auf den Feldern laeuft kein Zaehler, weder bei Eisen noch bei Brot.
    for _ in 0..6 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 0);
    assert_eq!(storage_of(&g, P0, k3), 0, "Brot muss besetzt sein");
    // Mit je einer Figur drauf: nach 3 Runden je ein Stueck, gleichzeitig.
    g.setup_unit(P0, c(&g, 1, 0), 1, 1);
    g.setup_unit(P0, c(&g, 2, 0), 1, 1);
    for _ in 0..2 {
        income(&mut g, P0);
    }
    assert_eq!(storage_of(&g, P0, k1), 0);
    assert_eq!(storage_of(&g, P0, k3), 0);
    income(&mut g, P0);
    assert_eq!(storage_of(&g, P0, k1), 1, "Eisen nach 3 Runden");
    assert_eq!(storage_of(&g, P0, k3), 1, "Brot nach 3 Runden");
    // Es gibt keinen 2^k-Flaechenbonus fuer Brot.
    assert_eq!(g.connected_kat3(P0), 0);
}

#[test]
fn the_default_price_table_can_still_be_overridden_per_card_in_all_three_resources() {
    use crate::cards::Card;
    let mut cfg = v3_config(2);
    cfg.set_all("price.shorten=2/1/4,price.king_boost=1/2/0")
        .unwrap();
    let g = with_deposits_cfg(cfg, &[]);
    assert_eq!(g.territory_card_price(Card::Shorten), [2, 1, 4]);
    assert_eq!(g.territory_card_price(Card::KingBoost), [1, 2, 0]);
    assert_eq!(g.territory_card_price(Card::Hop), [1, 0, 3], "Rest bleibt");
}

#[test]
fn bread_cards_can_only_be_bought_with_bread_in_the_store() {
    use crate::cards::Card;
    let mut g = card_game_no_bread(&[Card::Shorten]);
    g.config.terr_bread = 2;
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let (k1, k3) = (typ(&g, 0, 1), typ(&g, 0, 3));
    stock(&mut g, P0, k1, 50);
    assert!(buys(&g).is_empty(), "Eisen zahlt die Tempokarte nicht mehr");
    stock(&mut g, P0, k3, 3);
    assert!(!buys(&g).is_empty());
    buy(&mut g, Card::Shorten, Some(unit));
    assert_eq!(storage_of(&g, P0, k3), 0);
    assert_eq!(storage_of(&g, P0, k1), 50, "das Eisen bleibt unberuehrt");
    g.check_invariants().unwrap();
}

#[test]
fn the_bread_stock_cannot_be_combined_with_trading_and_needs_the_counter() {
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1,terr_bread=2").unwrap();
    assert!(cfg.validate().is_ok());
    assert!(!cfg.kat3_bonus_on());
    cfg.set_all("terr_trade=1").unwrap();
    assert!(cfg.validate().is_err());
    cfg.set_all("terr_trade=0,terr_cycle=0,terr_occupy=0,terr_move=0")
        .unwrap();
    assert!(cfg.validate().is_err(), "ohne Erntezaehler kein Brot-Takt");
}

#[test]
fn whole_games_with_all_new_switches_keep_every_invariant_on_every_board() {
    // Quadrat (4), Dreieck (3) und Sechseck-Bretter (6); jeder Bot-Typ, alle Schalter von 3.1.
    for (players, corners, seed) in [(2u8, 4u8, 21u64), (3, 3, 22), (2, 6, 23), (4, 4, 24)] {
        let mut cfg = Config::new(players, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.board_corners = corners;
        cfg.max_rounds = 60;
        cfg.score_after = 60;
        cfg.market_size = 11;
        cfg.set_all("terr_delay=1,terr_upkeep_sq=1,terr_yield_lvl=1,terr_bread=2,terr_fort=1")
            .unwrap();
        let mut g = Game::new(cfg).unwrap_or_else(|e| panic!("{players}p {corners}e: {e:?}"));
        let mut bots: Vec<Box<dyn Bot>> = (0..players)
            .map(|i| match i % 3 {
                0 => Box::new(RandomBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                1 => Box::new(GreedyBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                _ => Box::new(AreaBot::new(seed + u64::from(i))) as Box<dyn Bot>,
            })
            .collect();
        let mut steps = 0;
        while g.outcome().is_none() {
            let me = g.current_player().idx();
            let a = bots[me].choose(&g);
            g.apply(a).unwrap();
            g.check_invariants()
                .unwrap_or_else(|e| panic!("{players}p {corners}e seed {seed} Zug {steps}: {e}"));
            steps += 1;
            assert!(steps < 30_000);
        }
    }
}

// ----- Version 3.3: die Karte Fort (`terr_fort`) --------------------------------------------------

/// Version 3 mit `terr_fort`, viel Flaeche, ein eigener Streifen Land von der Fabrik bis (6,0).
fn fort_game() -> Game {
    let mut cfg = v3_config(2);
    cfg.set_all("terr_fort=1").unwrap();
    let mut g = with_deposits_cfg(cfg, &[]);
    g.state.area = vec![10_000; 2];
    strip(&mut g, P0, 6);
    g
}

/// Zahlt der Spieler 0 das Fort (3 Eisen + 1 Brot).
fn fort_money(g: &mut Game) {
    let (k1, k3) = (typ(g, 0, 1), typ(g, 0, 3));
    stock(g, P0, k1, 3);
    stock(g, P0, k3, 1);
}

fn fort_buy(g: &mut Game, piece: PieceId) {
    let fid = g.home_factory(P0).unwrap().id;
    g.apply(Action::BuyCard {
        card: crate::cards::Card::Fort,
        from: crate::action::PaySource::Factory(fid),
        target: Some(piece),
    })
    .unwrap();
}

#[test]
fn the_fort_is_a_switch_that_is_off_by_default_and_needs_the_version_3_cards() {
    use crate::cards::Card;
    let plain = v3_game(&[]);
    assert!(!plain.config.terr_fort);
    assert!(
        !plain.state.market.contains(&Card::Fort),
        "ohne Schalter kein Fort"
    );
    let g = fort_game();
    assert!(
        g.state.market.contains(&Card::Fort),
        "mit Schalter liegt es immer aus"
    );
    assert_eq!(g.territory_card_price(Card::Fort), [3, 0, 1]);
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1,terr_fort=1").unwrap();
    assert!(cfg.validate().is_ok());
    cfg.set_all("terr_cardany=0").unwrap();
    assert!(cfg.validate().is_err(), "Fort braucht Karten auf Figuren");
    cfg.set_all("terr_cardany=1,terr_cardset3=0").unwrap();
    assert!(
        cfg.validate().is_err(),
        "Fort braucht den Kartensatz von Version 3"
    );
}

#[test]
fn buying_the_fort_puts_it_under_the_chosen_piece_and_locks_that_piece() {
    use crate::cards::Card;
    let mut g = fort_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    fort_money(&mut g);
    let fid = g.home_factory(P0).unwrap().id;
    let buy_unit = Action::BuyCard {
        card: Card::Fort,
        from: crate::action::PaySource::Factory(fid),
        target: Some(unit),
    };
    assert!(g.legal_actions().contains(&buy_unit));
    // Der Koenig und ein Fabrikfeld kommen nie in Frage.
    let king = king_id(&g, P0);
    assert!(!g.legal_actions().iter().any(|a| matches!(
        a,
        Action::BuyCard { card: Card::Fort, target: Some(t), .. } if *t == king
    )));
    g.apply(buy_unit).unwrap();
    assert_eq!(g.state.forts[P0.idx()], Some(c(&g, 3, 0)));
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 1)), 0);
    assert_eq!(storage_of(&g, P0, typ(&g, 0, 3)), 0);
    assert!(
        g.piece_destinations(unit).is_empty(),
        "die Figur ruht in diesem Zug"
    );
    g.check_invariants().unwrap();
}

#[test]
fn a_piece_on_the_fort_moves_n_or_n_plus_one_fields() {
    let mut g = fort_game();
    let two = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let near = c(&g, 3, 2);
    let far = c(&g, 3, 3);
    assert!(g.piece_destinations(two).contains(&near));
    assert!(
        !g.piece_destinations(two).contains(&far),
        "ohne Fort nur genau 2"
    );
    fort_money(&mut g);
    fort_buy(&mut g, two);
    g.state.card_lock = None;
    let dests = g.piece_destinations(two);
    assert!(
        dests.contains(&near) && dests.contains(&far),
        "auf dem Fort 2 oder 3"
    );
    assert!(!dests.contains(&c(&g, 3, 4)), "nicht 4");
    // Ein Bauer zieht dort 1 oder 2 Felder.
    let mut h = fort_game();
    let pawn = h.setup_unit(P0, c(&h, 3, 0), 1, 1);
    fort_money(&mut h);
    fort_buy(&mut h, pawn);
    h.state.card_lock = None;
    let d = h.piece_destinations(pawn);
    assert!(d.contains(&c(&h, 3, 1)) && d.contains(&c(&h, 3, 2)));
    assert!(!d.contains(&c(&h, 3, 3)));
}

#[test]
fn the_fort_stays_behind_and_serves_any_own_piece_but_never_the_king() {
    let mut g = fort_game();
    let a = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let other = g.setup_unit(P0, c(&g, 5, 5), 2, 2);
    fort_money(&mut g);
    fort_buy(&mut g, a);
    g.state.card_lock = None;
    // Eine eigene Figur neben dem Fort hat keinen Vorteil.
    assert!(
        !g.piece_destinations(other).contains(&c(&g, 5, 8)),
        "weg vom Fort: genau 2"
    );
    // Die Figur auf dem Fort zieht weg: das Fort bleibt stehen.
    let step = Action::Move {
        piece: a,
        to: c(&g, 6, 0),
    };
    assert!(g.legal_actions().contains(&step), "3 Felder von dort");
    g.apply(step).unwrap();
    assert_eq!(g.state.forts[P0.idx()], Some(c(&g, 3, 0)));
    assert!(
        !g.piece_destinations(a).contains(&c(&g, 6, 3)),
        "nach dem Wegziehen wieder genau 2"
    );
    // Eine andere eigene Figur auf dem Fort bekommt es; der Koenig nicht.
    let b = g.setup_unit(P0, c(&g, 3, 0), 3, 3);
    assert!(
        g.piece_destinations(b).contains(&c(&g, 3, 4)),
        "Stufe 3 auf dem Fort: 4 Felder"
    );
    let king = g.state.pieces[&king_id(&g, P0)].clone();
    assert_eq!(g.fort_bonus(&king), 0);
}

#[test]
fn a_new_fort_moves_the_old_one_and_each_player_has_only_one() {
    let mut g = fort_game();
    let a = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let b = g.setup_unit(P0, c(&g, 5, 0), 2, 2);
    fort_money(&mut g);
    fort_buy(&mut g, a);
    assert_eq!(g.state.forts[P0.idx()], Some(c(&g, 3, 0)));
    // Auf dasselbe Feld nochmal ergibt keinen Kauf.
    g.state.bought_this_turn = false;
    g.state.card_lock = None;
    fort_money(&mut g);
    let again = g.legal_actions().into_iter().any(|x| {
        matches!(
            x,
            Action::BuyCard { card: crate::cards::Card::Fort, target: Some(t), .. } if t == a
        )
    });
    assert!(!again, "dort steht schon ein Fort");
    fort_buy(&mut g, b);
    assert_eq!(
        g.state.forts[P0.idx()],
        Some(c(&g, 5, 0)),
        "das alte ist weg"
    );
    g.check_invariants().unwrap();
}

#[test]
fn the_fort_is_destroyed_when_the_field_stops_belonging_to_its_owner() {
    let mut g = fort_game();
    let a = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    fort_money(&mut g);
    fort_buy(&mut g, a);
    let cell = c(&g, 3, 0);
    g.set_land(cell, Some(P1));
    assert_eq!(g.state.forts[P0.idx()], None, "Gegner faerbt das Feld");
    // Auch neutral gewordenes Land zerstoert es.
    let mut h = fort_game();
    let b = h.setup_unit(P0, c(&h, 3, 0), 2, 2);
    fort_money(&mut h);
    fort_buy(&mut h, b);
    let cell = c(&h, 3, 0);
    h.set_land(cell, None);
    assert_eq!(h.state.forts[P0.idx()], None);
}

#[test]
fn an_enemy_that_captures_on_the_fort_destroys_it_and_the_game_reports_it() {
    let mut g = fort_game();
    let a = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    fort_money(&mut g);
    fort_buy(&mut g, a);
    // Gegner: eigenes Land bis neben das Fort, ein Bauer schlaegt die Figur auf dem Fort.
    for x in 4..=9 {
        paint(&mut g, P1, &[(x, 9)]);
    }
    for y in 0..=9 {
        paint(&mut g, P1, &[(9, y)]);
    }
    paint(&mut g, P1, &[(8, 0), (7, 0), (6, 0), (5, 0)]);
    let hunter = g.setup_unit(P1, c(&g, 4, 0), 1, 1);
    paint(&mut g, P1, &[(4, 0)]);
    g.state.card_lock = None;
    g.state.bought_this_turn = false;
    g.setup_current(P1);
    let take = Action::Move {
        piece: hunter,
        to: c(&g, 3, 0),
    };
    assert!(
        g.legal_actions().contains(&take),
        "der Bauer darf die Figur auf dem Fort schlagen"
    );
    let events = g.apply(take).unwrap();
    assert_eq!(g.state.forts[P0.idx()], None);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::FortLost { player, .. } if *player == P0)));
    g.check_invariants().unwrap();
}

// ----- Version 3.4: Gelaende (`terr_terrain`) -------------------------------------------------------

use crate::terrain::Terrain;

/// Version 3 mit Gelaende auf der leeren Testkarte; das Gelaende setzt der Test selbst.
fn terrain_game() -> Game {
    let mut cfg = v3_config(2);
    cfg.set_all("terr_terrain=1").unwrap();
    let mut g = with_deposits_cfg(cfg, &[]);
    let n = g.map.board.len();
    std::sync::Arc::make_mut(&mut g.map).terrain = vec![Terrain::Field; n];
    g
}

fn set_terrain(g: &mut Game, cells: &[((i32, i32), Terrain)]) {
    for &((x, y), t) in cells {
        let cell = c(g, x, y);
        std::sync::Arc::make_mut(&mut g.map).terrain[cell.idx()] = t;
    }
}

#[test]
fn terrain_is_a_switch_that_is_off_by_default_and_changes_no_income() {
    let mut g = v3_game(&[]);
    assert!(!g.config.terr_terrain);
    assert!(g.map.terrain.iter().all(|&t| t == Terrain::Field));
    strip(&mut g, P0, 6);
    let before = g.state.area[0];
    income(&mut g, P0);
    assert_eq!(
        g.state.area[0] - before,
        7,
        "sieben Felder, je eine Flaeche"
    );
    assert_eq!(g.state.area_rest[0], 0);
    let mut cfg = Config::new(2, 1);
    cfg.set_all("terr_terrain=1").unwrap();
    assert!(cfg.validate().is_err(), "Gelaende braucht den Gebietsmodus");
    cfg.set_all("territory=1").unwrap();
    assert!(cfg.validate().is_ok());
}

#[test]
fn terrain_does_not_move_a_single_deposit() {
    let mut plain = v3_config(3);
    plain.seed = 17;
    let mut with = plain.clone();
    with.set_all("terr_terrain=1").unwrap();
    let a = Map::generate(&plain).unwrap();
    let b = Map::generate(&with).unwrap();
    assert_eq!(
        a.deposits, b.deposits,
        "gleiche Rohstoffe mit und ohne Gelaende"
    );
    assert!(b.terrain.contains(&Terrain::Earth));
    assert!(b.terrain.contains(&Terrain::Bog));
}

#[test]
fn earth_counts_half_and_bog_a_quarter_in_the_income() {
    let mut g = terrain_game();
    // Fabrikfeld und drei Felder daneben: Feld 4 + Erde 2 + Moor 1 + Moor 1 = 8 Viertel = 2 Flaeche
    strip(&mut g, P0, 3);
    set_terrain(
        &mut g,
        &[
            ((1, 0), Terrain::Earth),
            ((2, 0), Terrain::Bog),
            ((3, 0), Terrain::Bog),
        ],
    );
    let before = g.state.area[0];
    income(&mut g, P0);
    assert_eq!(g.state.area[0] - before, 2);
    assert_eq!(g.state.area_rest[0], 0);
}

#[test]
fn a_remainder_below_one_area_is_carried_into_the_next_round() {
    let mut g = terrain_game();
    // 4 + 2 + 1 + 2 = 9 Viertel je Runde: 2 Flaeche und 1 Viertel Rest, dann 10, 11, 12 ...
    strip(&mut g, P0, 3);
    set_terrain(
        &mut g,
        &[
            ((1, 0), Terrain::Earth),
            ((2, 0), Terrain::Bog),
            ((3, 0), Terrain::Earth),
        ],
    );
    let start = g.state.area[0];
    let mut gains = Vec::new();
    let mut rests = Vec::new();
    for _ in 0..4 {
        let before = g.state.area[0];
        income(&mut g, P0);
        gains.push(g.state.area[0] - before);
        rests.push(g.state.area_rest[0]);
    }
    assert_eq!(gains, vec![2, 2, 2, 3]);
    assert_eq!(rests, vec![1, 2, 3, 0]);
    // Nichts geht verloren: vier Runden zu 9 Vierteln sind genau 9 Flaeche.
    assert_eq!(g.state.area[0] - start, 9);
    g.check_invariants().unwrap();
}

#[test]
fn only_connected_land_counts_by_its_terrain_value() {
    let mut g = terrain_game();
    strip(&mut g, P0, 1);
    // ein abgetrenntes Moorfeld und ein abgetrenntes Feld zaehlen nicht
    paint(&mut g, P0, &[(5, 5), (6, 5)]);
    set_terrain(&mut g, &[((5, 5), Terrain::Field), ((6, 5), Terrain::Bog)]);
    let before = g.state.area[0];
    income(&mut g, P0);
    assert_eq!(
        g.state.area[0] - before,
        2,
        "nur (0,0) und (1,0) hangen an der Fabrik"
    );
}

#[test]
fn losing_the_factory_clears_the_remainder() {
    let mut g = terrain_game();
    strip(&mut g, P0, 0);
    set_terrain(&mut g, &[((0, 0), Terrain::Earth)]);
    income(&mut g, P0); // 2 Viertel: Rest 2
    assert_eq!(g.state.area_rest[0], 2);
    g.territory_forget(P0);
    assert_eq!(g.state.area[0], 0);
    assert_eq!(g.state.area_rest[0], 0);
}

#[test]
fn the_header_lists_the_terrain_only_when_the_switch_is_on() {
    use crate::replay::ReplayWriter;
    let mut cfg = v3_config(2);
    cfg.set_all("terr_terrain=1").unwrap();
    let g = Game::new(cfg).unwrap();
    let json = ReplayWriter::new(&g, &["a".into(), "b".into()]).finish(&g);
    assert!(json.contains("\"terr_terrain\":true"));
    assert!(json.contains("\"terrain\":["));
    let off = Game::new(v3_config(2)).unwrap();
    let json = ReplayWriter::new(&off, &["a".into(), "b".into()]).finish(&off);
    assert!(json.contains("\"terr_terrain\":false"));
    assert!(!json.contains("\"terrain\":["));
}

#[test]
fn whole_games_with_terrain_keep_every_invariant_on_every_board() {
    for (players, corners, seed) in [(2u8, 4u8, 31u64), (3, 3, 32), (2, 6, 33), (4, 4, 34)] {
        let mut cfg = Config::new(players, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.board_corners = corners;
        cfg.max_rounds = 60;
        cfg.score_after = 60;
        cfg.set_all("terr_terrain=1").unwrap();
        let mut g = Game::new(cfg).unwrap_or_else(|e| panic!("{players}p {corners}e: {e:?}"));
        let mut bots: Vec<Box<dyn Bot>> = (0..players)
            .map(|i| match i % 3 {
                0 => Box::new(RandomBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                1 => Box::new(GreedyBot::new(seed + u64::from(i))) as Box<dyn Bot>,
                _ => Box::new(AreaBot::new(seed + u64::from(i))) as Box<dyn Bot>,
            })
            .collect();
        let mut steps = 0;
        while g.outcome().is_none() {
            let me = g.current_player().idx();
            let a = bots[me].choose(&g);
            g.apply(a).unwrap();
            g.check_invariants()
                .unwrap_or_else(|e| panic!("{players}p {corners}e seed {seed} Zug {steps}: {e}"));
            steps += 1;
            assert!(steps < 30_000);
        }
    }
}

// ----- Version 3.6: die Karte Kamikaze (`terr_kamikaze`) ---------------------------------------

/// Version 3 mit `terr_kamikaze`, viel Flaeche, ein eigener Streifen Land von der Fabrik bis (6,0).
fn kamikaze_game() -> Game {
    let mut cfg = v3_config(2);
    cfg.set_all("terr_kamikaze=1").unwrap();
    let mut g = with_deposits_cfg(cfg, &[]);
    g.state.area = vec![10_000; 2];
    strip(&mut g, P0, 6);
    g
}

/// Zahlt der Spieler 0 die Kamikaze (2 Eisen + 2 Gold).
fn kamikaze_money(g: &mut Game) {
    let (k1, k2) = (typ(g, 0, 1), typ(g, 0, 2));
    stock(g, P0, k1, 2);
    stock(g, P0, k2, 2);
}

fn kamikaze_buy(g: &mut Game, piece: PieceId) {
    let fid = g.home_factory(P0).unwrap().id;
    g.apply(Action::BuyCard {
        card: crate::cards::Card::Kamikaze,
        from: crate::action::PaySource::Factory(fid),
        target: Some(piece),
    })
    .unwrap();
}

/// Ein harmloser Zug fuer den Spieler am Zug (der Koenig macht einen Schritt; ohne Zug: Passen),
/// denn ein Zug endet erst mit einem Zug oder, wenn es keinen gibt, mit Passen.
fn idle(g: &mut Game) -> Vec<Event> {
    let me = g.current_player();
    let king = king_id(g, me);
    let step = g
        .legal_actions()
        .into_iter()
        .find(|a| matches!(a, Action::Move { piece, .. } if *piece == king));
    g.apply(step.unwrap_or(Action::Pass)).unwrap()
}

/// Spieler 0 kauft die Karte auf `piece` und beide Spieler beenden ihren Zug: danach ist
/// Spieler 0 wieder dran und die Figur brennt.
fn kamikaze_ready(g: &mut Game, piece: PieceId) {
    kamikaze_money(g);
    kamikaze_buy(g, piece);
    idle(g);
    assert_eq!(g.current_player(), P1);
    idle(g);
    assert_eq!(g.current_player(), P0);
}

#[test]
fn the_kamikaze_is_a_switch_that_is_off_by_default_and_needs_the_version_3_cards() {
    use crate::cards::Card;
    let plain = v3_game(&[]);
    assert!(!plain.config.terr_kamikaze);
    assert!(!plain.state.market.contains(&Card::Kamikaze));
    let g = kamikaze_game();
    assert!(
        g.state.market.contains(&Card::Kamikaze),
        "mit Schalter liegt sie immer aus"
    );
    assert_eq!(g.territory_card_price(Card::Kamikaze), [2, 2, 0]);
    assert_eq!(Card::from_key("kamikaze"), Some(Card::Kamikaze));
    let mut cfg = Config::new(2, 1);
    cfg.set_all("territory=1,terr_v3=1,terr_kamikaze=1")
        .unwrap();
    assert!(cfg.validate().is_ok());
    cfg.set_all("terr_cardany=0").unwrap();
    assert!(
        cfg.validate().is_err(),
        "Kamikaze braucht Karten auf Figuren"
    );
    cfg.set_all("terr_cardany=1,terr_exact=0").unwrap();
    assert!(cfg.validate().is_err(), "Kamikaze braucht genaue Zuege");
}

#[test]
fn a_bought_kamikaze_rests_this_turn_and_the_next_round_and_burns_in_the_next_own_turn() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    kamikaze_money(&mut g);
    kamikaze_buy(&mut g, unit);
    assert_eq!(g.state.pieces[&unit].perks.kamikaze, 1);
    assert!(g.piece_destinations(unit).is_empty(), "in diesem Zug Pause");
    g.state.card_lock = None;
    assert!(
        g.piece_destinations(unit).is_empty(),
        "auch ohne Kartensperre: Pause"
    );
    idle(&mut g);
    assert_eq!(g.state.pieces[&unit].perks.kamikaze, 2, "jetzt brennt sie");
    // Der Gegner ist dran: die Figur steht und brennt, ihr Besitzer kann sie nicht ziehen.
    assert_eq!(g.current_player(), P1);
    idle(&mut g);
    assert!(g.state.pieces.contains_key(&unit));
    g.check_invariants().unwrap();
}

#[test]
fn the_kamikaze_cannot_go_on_the_king_or_on_a_piece_that_already_burns() {
    use crate::cards::Card;
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    kamikaze_money(&mut g);
    let king = king_id(&g, P0);
    assert!(!g.legal_actions().iter().any(|a| matches!(
        a,
        Action::BuyCard { card: Card::Kamikaze, target: Some(t), .. } if *t == king
    )));
    kamikaze_buy(&mut g, unit);
    g.state.bought_this_turn = false;
    g.state.card_lock = None;
    kamikaze_money(&mut g);
    assert!(
        !g.legal_actions().iter().any(|a| matches!(
            a,
            Action::BuyCard { card: Card::Kamikaze, target: Some(t), .. } if *t == unit
        )),
        "brennt schon"
    );
}

#[test]
fn a_burning_piece_can_only_attack_an_enemy_piece_exactly_in_range() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let target = g.setup_unit(P1, c(&g, 3, 2), 1, 1);
    let near = g.setup_unit(P1, c(&g, 6, 3), 1, 1);
    let _ = near;
    kamikaze_ready(&mut g, unit);
    assert_eq!(g.piece_destinations(unit), vec![c(&g, 3, 2)]);
    let _ = target;
    // Kein Zug auf leere Felder, auch keine normalen Zuege.
    assert!(!g.legal_actions().contains(&Action::Move {
        piece: unit,
        to: c(&g, 3, 1),
    }));
    assert!(g.legal_actions().contains(&Action::Move {
        piece: unit,
        to: c(&g, 3, 2),
    }));
}

#[test]
fn the_strike_kills_the_target_and_the_piece_between_and_the_path_turns_neutral() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 3, 3);
    let between = g.setup_unit(P1, c(&g, 3, 2), 1, 1);
    let target = g.setup_unit(P1, c(&g, 3, 3), 2, 2);
    let bystander = g.setup_unit(P1, c(&g, 5, 5), 1, 1);
    // Land des Gegners auf dem Weg.
    paint(&mut g, P1, &[(3, 1), (3, 2), (3, 3), (4, 3)]);
    kamikaze_ready(&mut g, unit);
    assert!(
        g.piece_destinations(unit).contains(&c(&g, 3, 3)),
        "Ziel in Reichweite, eine Figur davor wird uebersprungen"
    );
    let events = g
        .apply(Action::Move {
            piece: unit,
            to: c(&g, 3, 3),
        })
        .unwrap();
    assert!(!g.state.pieces.contains_key(&unit), "die Figur stirbt");
    assert!(
        !g.state.pieces.contains_key(&between),
        "die uebersprungene stirbt"
    );
    assert!(!g.state.pieces.contains_key(&target), "das Ziel stirbt");
    assert!(g.state.pieces.contains_key(&bystander), "andere bleiben");
    for (x, y) in [(3, 1), (3, 2), (3, 3)] {
        assert_eq!(g.state.land[c(&g, x, y).idx()], None, "({x},{y}) neutral");
    }
    assert_eq!(
        g.state.land[c(&g, 3, 0).idx()],
        Some(P0),
        "das Startfeld bleibt"
    );
    assert!(events.iter().any(|e| matches!(
        e,
        Event::KamikazeStrike { piece, killed, .. } if *piece == unit && killed.len() == 2
    )));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, Event::Captured { .. }))
            .count(),
        2
    );
    g.check_invariants().unwrap();
}

#[test]
fn only_an_enemy_piece_can_be_skipped_and_an_own_piece_blocks_the_way() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 3, 3);
    let _own = g.setup_unit(P0, c(&g, 3, 2), 1, 1);
    let _target = g.setup_unit(P1, c(&g, 3, 3), 2, 2);
    kamikaze_ready(&mut g, unit);
    assert!(
        !g.piece_destinations(unit).contains(&c(&g, 3, 3)),
        "die eigene Figur versperrt"
    );
    // Zwei gegnerische Figuren davor sind eine zu viel.
    let mut h = kamikaze_game();
    let unit = h.setup_unit(P0, c(&h, 3, 0), 3, 3);
    h.setup_unit(P1, c(&h, 3, 1), 1, 1);
    h.setup_unit(P1, c(&h, 3, 2), 1, 1);
    h.setup_unit(P1, c(&h, 3, 3), 1, 1);
    kamikaze_ready(&mut h, unit);
    assert!(h.piece_destinations(unit).is_empty());
}

#[test]
fn a_target_in_range_is_enough_a_target_too_close_or_too_far_is_not() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 3, 3);
    g.setup_unit(P1, c(&g, 3, 1), 1, 1);
    g.setup_unit(P1, c(&g, 3, 4), 1, 1);
    kamikaze_ready(&mut g, unit);
    assert!(
        g.piece_destinations(unit).is_empty(),
        "genau 3 Felder: (3,1) zu nah, (3,4) zu weit"
    );
}

#[test]
fn an_unused_burning_piece_is_gone_at_the_end_of_the_turn() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    kamikaze_ready(&mut g, unit);
    assert!(g.state.pieces.contains_key(&unit));
    let events = idle(&mut g);
    assert!(!g.state.pieces.contains_key(&unit), "ungenutzt: weg");
    assert!(events.iter().any(|e| matches!(
        e,
        Event::KamikazeGone { piece, .. } if *piece == unit
    )));
    g.check_invariants().unwrap();
}

#[test]
fn a_burning_piece_that_is_captured_in_the_rest_round_is_simply_captured() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    // Gegner mit einer Figur, die in einem Schritt zuschlagen kann.
    paint(&mut g, P1, &[(3, 1), (3, 2)]);
    for y in 0..=9 {
        paint(&mut g, P1, &[(9, y)]);
    }
    for x in 3..=9 {
        paint(&mut g, P1, &[(x, 9)]);
    }
    kamikaze_money(&mut g);
    kamikaze_buy(&mut g, unit);
    idle(&mut g);
    let hunter = g.setup_unit(P1, c(&g, 3, 1), 1, 1);
    let kill = Action::Move {
        piece: hunter,
        to: c(&g, 3, 0),
    };
    if g.legal_actions().contains(&kill) {
        g.apply(kill).unwrap();
        assert!(!g.state.pieces.contains_key(&unit));
    }
    g.check_invariants().unwrap();
}

#[test]
fn the_strike_destroys_an_enemy_factory_on_the_path_and_keeps_its_own() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    g.setup_unit(P1, c(&g, 3, 2), 1, 1);
    // Eine zweite eigene Fabrik auf dem Weg bleibt, eine gegnerische geht kaputt.
    paint(&mut g, P1, &[(3, 1), (3, 2)]);
    let f1 = g.setup_factory(P1, c(&g, 3, 1));
    kamikaze_ready(&mut g, unit);
    let events = g
        .apply(Action::Move {
            piece: unit,
            to: c(&g, 3, 2),
        })
        .unwrap();
    assert!(
        !g.state.factories.contains_key(&f1),
        "gegnerische Fabrik zerstoert"
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::FactoryDestroyed { .. })));
    g.check_invariants().unwrap();
}

#[test]
fn the_strike_cuts_off_land_that_hung_on_the_path_and_it_turns_neutral() {
    let mut g = kamikaze_game();
    // Eigener Streifen (0..6,0), dazu ein Arm (6,0)-(6,1)-(6,2), der nur ueber (5,0)/(6,0) haengt.
    paint(&mut g, P0, &[(6, 1), (6, 2), (6, 3)]);
    let unit = g.setup_unit(P0, c(&g, 4, 0), 2, 2);
    g.setup_unit(P1, c(&g, 6, 0), 1, 1);
    kamikaze_ready(&mut g, unit);
    assert!(g.piece_destinations(unit).contains(&c(&g, 6, 0)));
    let events = g
        .apply(Action::Move {
            piece: unit,
            to: c(&g, 6, 0),
        })
        .unwrap();
    // (5,0) und (6,0) sind neutral; der Arm (6,1..3) haengt nicht mehr an der Fabrik: neutral.
    for (x, y) in [(5, 0), (6, 0), (6, 1), (6, 2), (6, 3)] {
        assert_eq!(g.state.land[c(&g, x, y).idx()], None, "({x},{y})");
    }
    assert!(events.iter().any(|e| matches!(
        e,
        Event::CutOff { from, to: None, .. } if *from == P0
    )));
    assert_eq!(g.state.land[c(&g, 3, 0).idx()], Some(P0));
    g.check_invariants().unwrap();
}

#[test]
fn killing_the_enemy_king_wins_the_game_even_though_the_attacker_dies() {
    let mut g = kamikaze_game();
    let unit = g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    let enemy_king = king_id(&g, P1);
    kamikaze_ready(&mut g, unit);
    // Der gegnerische Koenig steht jetzt genau in Reichweite.
    let old = g.state.pieces[&enemy_king].cell;
    let new = c(&g, 3, 2);
    g.state.occupant[old.idx()] = None;
    g.state.pieces.get_mut(&enemy_king).unwrap().cell = new;
    g.state.occupant[new.idx()] = Some(enemy_king);
    assert!(g.piece_destinations(unit).contains(&new));
    let events = g
        .apply(Action::Move {
            piece: unit,
            to: new,
        })
        .unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::GameOver { .. })));
    assert_eq!(g.outcome(), Some(crate::state::Outcome::Winner(P0)));
}

#[test]
fn a_defenceless_burning_piece_cannot_attack() {
    let mut g = kamikaze_game();
    // Die Figur steht auf Land, das nicht mit der Fabrik zusammenhaengt.
    let unit = g.setup_unit(P0, c(&g, 3, 5), 2, 2);
    let cell = c(&g, 3, 5);
    g.set_land(cell, Some(P0));
    g.setup_unit(P1, c(&g, 3, 7), 1, 1);
    kamikaze_money(&mut g);
    let fid = g.home_factory(P0).unwrap().id;
    g.state.pieces.get_mut(&unit).unwrap().perks.kamikaze = 2;
    let _ = fid;
    assert!(g.piece_destinations(unit).is_empty());
}

#[test]
fn bots_never_buy_the_kamikaze_card() {
    use crate::cards::Card;
    let mut g = kamikaze_game();
    g.setup_unit(P0, c(&g, 3, 0), 2, 2);
    kamikaze_money(&mut g);
    assert!(g.legal_actions().iter().any(|a| matches!(
        a,
        Action::BuyCard {
            card: Card::Kamikaze,
            ..
        }
    )));
    assert!(!g.legal_actions_for_bot().iter().any(|a| matches!(
        a,
        Action::BuyCard {
            card: Card::Kamikaze,
            ..
        }
    )));
}

#[test]
fn whole_random_games_with_kamikaze_keep_every_invariant_and_really_use_the_card() {
    use crate::cards::Card;
    use crate::rng::Rng;
    let mut strikes = 0;
    let mut gone = 0;
    let mut bought = 0;
    for (players, corners, seed) in [
        (2u8, 4u8, 41u64),
        (3, 3, 42),
        (2, 6, 43),
        (4, 4, 44),
        (2, 4, 45),
    ] {
        let mut cfg = Config::new(players, seed);
        cfg.territory = true;
        cfg.set_v3(true);
        cfg.board_corners = corners;
        cfg.max_rounds = 80;
        cfg.score_after = 80;
        cfg.set_all("terr_kamikaze=1").unwrap();
        let mut g = Game::new(cfg).unwrap_or_else(|e| panic!("{players}p {corners}e: {e:?}"));
        let mut rng = Rng::new(seed);
        let mut steps = 0;
        while g.outcome().is_none() {
            // Wer am Zug ist, hat Eisen und Gold fuer die Karte (Zufallsspiele sparen sonst nie).
            let me = g.current_player();
            if let Some(fid) = g.home_factory(me).map(|f| f.id) {
                let corner = g.map.player_corners[me.idx()];
                let (k1, k2) = (typ(&g, corner, 1), typ(&g, corner, 2));
                g.setup_storage(fid, k1, 3);
                g.setup_storage(fid, k2, 3);
            }
            // Steht eine brennende Figur ohne Ziel da, stellt der Test ihr einen Gegner in Reichweite.
            let burning: Vec<PieceId> = g
                .pieces_of(me)
                .filter(|p| p.perks.kamikaze == 2)
                .map(|p| p.id)
                .collect();
            for id in burning {
                if !g.piece_destinations(id).is_empty() {
                    continue;
                }
                let piece = g.state.pieces[&id].clone();
                let crate::state::PieceKind::Unit { range, .. } = piece.kind else {
                    continue;
                };
                let Some(foe) = g.alive_players().into_iter().find(|&p| p != me) else {
                    continue;
                };
                for dir in 0..g.map.board.dir_count() {
                    let line = g.map.board.line(piece.cell, dir, usize::from(range));
                    if line.len() == usize::from(range)
                        && line.iter().all(|&c| g.state.piece_at(c).is_none())
                    {
                        g.setup_unit(foe, line[line.len() - 1], 1, 1);
                        g.territory_settle(foe, &mut Vec::new());
                        break;
                    }
                }
            }
            let legal = g.legal_actions();
            // Die Karte oft kaufen und brennende Figuren oft angreifen lassen, damit sie vorkommt.
            let wanted: Vec<Action> = legal
                .iter()
                .copied()
                .filter(|a| match a {
                    Action::BuyCard { card, .. } => *card == Card::Kamikaze,
                    Action::Move { piece, .. } => g.state.pieces[piece].perks.kamikaze == 2,
                    _ => false,
                })
                .collect();
            let a = if !wanted.is_empty() && rng.below(4) != 0 {
                *rng.pick(&wanted).unwrap()
            } else {
                *rng.pick(&legal).expect("eine Aktion")
            };
            let events = g.apply(a).unwrap();
            bought += events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        Event::CardBought {
                            card: Card::Kamikaze,
                            ..
                        }
                    )
                })
                .count();
            strikes += events
                .iter()
                .filter(|e| matches!(e, Event::KamikazeStrike { .. }))
                .count();
            gone += events
                .iter()
                .filter(|e| matches!(e, Event::KamikazeGone { .. }))
                .count();
            g.check_invariants()
                .unwrap_or_else(|e| panic!("{players}p {corners}e seed {seed} Zug {steps}: {e}"));
            steps += 1;
            assert!(steps < 30_000);
        }
    }
    println!("Angriffe {strikes}, verfallen {gone}, gekauft {bought}");
    assert!(strikes > 0, "mindestens ein Angriff kommt vor");
}
