//! Flaechensicherung: Engstellen im Land (`Game::weak_points`), die Gewichte von "sehr stark" und
//! eine Messsonde, die zaehlt, wie oft ein Bot den Koenig an einer duennen Linie fuehrt und wie
//! oft sie gekappt wird.
//!
//! Leere Karte, 10x10 Quadrat; A startet auf (0,0), B auf (9,9).

use crate::ai::{SearchBot, SearchParams, VERY_STRONG_BUDGET};
use crate::area_bot::AreaBot;
use crate::area_bot::{exposure, leaf_value};
use crate::bot::Bot;
use crate::config::Config;
use crate::game::Game;
use crate::ids::{CellId, PlayerId};
use crate::map::Map;
use crate::state::Outcome;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

fn blank() -> Game {
    let mut config = Config::new(2, 1);
    config.territory = true;
    config.set_v3(true);
    let mut map = Map::generate(&config).expect("Karte");
    map.deposits = vec![Vec::new(); map.board.len()];
    Game::with_map(config, map)
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

fn row(y: i32, from: i32, to: i32) -> Vec<(i32, i32)> {
    (from..=to).map(|x| (x, y)).collect()
}

/// A: eine Linie von der Fabrik (0,0) nach (4,0), der Koenig am Ende.
fn thin_line() -> Game {
    let mut g = blank();
    paint(&mut g, P0, &row(0, 0, 4));
    let king = g.king(P0).expect("Koenig").id;
    let to = c(&g, 4, 0);
    g.setup_teleport(king, to);
    g
}

/// Dieselbe Linie, doppelt so breit: keine Engstelle.
fn thick_line() -> Game {
    let mut g = thin_line();
    paint(&mut g, P0, &row(1, 0, 4));
    g
}

/// B: ein Block (3,3)..(9,9) mit der Fabrik in der Ecke, dazu eine Figur auf (3,3), die faerben darf.
fn add_enemy_block(g: &mut Game) {
    for y in 3..=9 {
        paint(g, P1, &row(y, 3, 9));
    }
    let at = c(g, 3, 3);
    g.setup_unit(P1, at, 2, 2);
}

#[test]
fn a_thin_line_has_a_weak_point_at_every_cell_between_factory_and_king() {
    let g = thin_line();
    let weak = g.weak_points(P0);
    let cells: Vec<CellId> = weak.iter().map(|w| w.cell).collect();
    for x in 1..=3 {
        assert!(
            cells.contains(&c(&g, x, 0)),
            "Feld ({x},0) ist eine Engstelle"
        );
    }
    assert_eq!(weak.len(), 3, "Fabrik und Linienende sind keine Engstellen");
    let first = weak.iter().find(|w| w.cell == c(&g, 1, 0)).unwrap();
    assert_eq!(first.cells, 3, "hinter (1,0) haengen drei Felder");
    assert!(first.king, "und der Koenig");
    let last = weak.iter().find(|w| w.cell == c(&g, 3, 0)).unwrap();
    assert_eq!(last.cells, 1);
    assert!(last.king);
}

#[test]
fn a_double_wide_line_has_no_weak_point() {
    let g = thick_line();
    assert!(g.weak_points(P0).is_empty());
}

#[test]
fn weak_points_count_the_figures_behind_them() {
    let mut g = thin_line();
    let at = c(&g, 2, 0);
    g.setup_unit(P0, at, 3, 3);
    let weak = g.weak_points(P0);
    let w1 = weak.iter().find(|w| w.cell == c(&g, 1, 0)).unwrap();
    assert_eq!(w1.levels, 3);
    let w3 = weak.iter().find(|w| w.cell == c(&g, 3, 0)).unwrap();
    assert_eq!(w3.levels, 0, "die Figur steht vor der Engstelle (3,0)");
}

#[test]
fn a_player_without_a_standing_factory_has_no_weak_points() {
    let mut g = thin_line();
    g.state.factories.clear();
    assert!(g.weak_points(P0).is_empty());
}

/// B: ein Block (2,2)..(9,9) mit der Fabrik in der Ecke und eine Stufe-2-Figur auf (2,2): sie zieht
/// in einem Zug nach (2,0) und faerbt dabei (2,1) und (2,0).
fn add_striker(g: &mut Game) {
    for y in 2..=9 {
        paint(g, P1, &row(y, 2, 9));
    }
    let at = c(g, 2, 2);
    g.setup_unit(P1, at, 2, 2);
}

fn king_value(w: &crate::WeakPoint) -> i64 {
    if w.king {
        1000
    } else {
        i64::from(w.cells)
    }
}

#[test]
fn an_enemy_that_can_paint_a_weak_point_next_move_is_a_cut_threat() {
    let mut g = thin_line();
    add_striker(&mut g);
    assert_eq!(
        g.cut_threat(P0, king_value),
        1000,
        "(2,0) trennt den Koenig ab"
    );
}

#[test]
fn a_long_stroke_across_a_double_wide_neck_cuts_it_too() {
    let mut g = thick_line();
    // Stufe 2 von (2,2) nach (2,0): faerbt (2,1) und (2,0), also beide Reihen der Linie.
    add_striker(&mut g);
    assert_eq!(g.cut_threat(P0, king_value), 1000);
}

#[test]
fn a_single_step_cannot_cut_a_double_wide_neck() {
    let mut g = thick_line();
    for y in 2..=9 {
        paint(&mut g, P1, &row(y, 2, 9));
    }
    let at = c(&g, 2, 2);
    g.setup_unit(P1, at, 1, 1);
    assert_eq!(
        g.cut_threat(P0, king_value),
        0,
        "ein Feld faerbt nur eine Reihe"
    );
}

#[test]
fn a_helpless_enemy_figure_is_no_cut_threat() {
    let mut g = thin_line();
    // Die Figur steht auf Land, das nicht an B's Fabrik haengt: sie faerbt nichts.
    let at = c(&g, 2, 2);
    g.setup_unit(P1, at, 2, 2);
    assert_eq!(g.cut_threat(P0, king_value), 0);
}

#[test]
fn the_exposure_of_a_thin_line_grows_as_an_enemy_comes_closer_and_is_zero_for_a_thick_line() {
    let mut far = thin_line();
    // Gegner weit weg: die Ecke (9,9) mit einer Figur daneben.
    for cell in [(8, 9), (9, 9)] {
        paint(&mut far, P1, &[cell]);
    }
    let at = c(&far, 8, 9);
    far.setup_unit(P1, at, 2, 2);
    let mut near = thin_line();
    add_enemy_block(&mut near);
    let mut thick = thick_line();
    add_enemy_block(&mut thick);
    assert!(exposure(&near, P0) > exposure(&far, P0));
    assert_eq!(exposure(&thick, P0), 0);
}

#[test]
fn the_leaf_value_charges_a_cuttable_king_only_when_the_enemy_is_to_move() {
    let mut g = thin_line();
    add_striker(&mut g);
    g.state.current = P1;
    let enemy_to_move = leaf_value(&g, P0);
    let mut h = g.clone();
    h.state.current = P0;
    let own_move = leaf_value(&h, P0);
    assert!(
        own_move - enemy_to_move >= 500,
        "der Koenig kappbar kostet mindestens KING_CUT"
    );
}

#[test]
fn whole_games_with_sehr_stark_stay_legal() {
    let mut cfg = Config::new(2, 11);
    cfg.territory = true;
    cfg.set_v3(true);
    cfg.max_rounds = 25;
    cfg.score_after = 25;
    let mut g = Game::new(cfg).expect("Karte");
    let mut bots: Vec<Box<dyn Bot>> = vec![
        Box::new(SearchBot::with_params(5, SearchParams::with_budget(300)).with_area_eval()),
        Box::new(AreaBot::new(6)),
    ];
    let mut steps = 0;
    while g.outcome().is_none() {
        let me = g.current_player().idx();
        let action = bots[me].choose(&g);
        assert!(g.legal_actions().contains(&action));
        g.apply(action).unwrap();
        g.check_invariants()
            .unwrap_or_else(|e| panic!("Zug {steps}: {e}"));
        steps += 1;
        assert!(steps < 10_000);
    }
}

// ----- Messsonde ----------------------------------------------------------------------------------

#[derive(Default, Debug, Clone)]
struct Tether {
    games: u32,
    wins: u32,
    wins_by_capture: u32,
    lost_by_capture: u32,
    lost_after_king_cut: u32,
    turns: u64,
    thin_turns: u64,
    cuttable_turns: u64,
    /// Davon Zugenden, in denen das Modell der Bots (`cut_threat`) den Schnitt nicht sieht.
    unseen_cuttable: u64,
    king_cuts: u32,
    /// Davon Abtrennungen durch einen eigenen Zug.
    self_cuts: u32,
    rounds: u64,
}

fn make(kind: &str, seed: u64, nodes: u64, nodes_very: u64) -> Box<dyn Bot> {
    match kind {
        "stark" => Box::new(SearchBot::with_params(
            seed,
            SearchParams::with_budget(nodes),
        )),
        "sehr" => Box::new(
            SearchBot::with_params(seed, SearchParams::with_budget(nodes_very)).with_area_eval(),
        ),
        _ => Box::new(AreaBot::new(seed)),
    }
}

/// Steht der Koenig von `p` auf verbundenem Land? Waehrend eines Fabrik-Neubaus (nach einer
/// Zerstoerung) zaehlt er als verbunden: das ist keine Abtrennung durch eine Linie.
fn king_linked(g: &Game, p: PlayerId) -> bool {
    g.king(p)
        .is_none_or(|k| g.piece_connected(k) || g.rebuilding(p))
}

/// Kann der Gegner im naechsten Zug den Koenig von `p` von der Fabrik trennen?
fn king_cuttable(g: &Game, p: PlayerId) -> bool {
    if g.outcome().is_some() || g.current_player() == p {
        return false;
    }
    // Wer schon abgetrennt ist, kann nicht (noch einmal) gekappt werden.
    if !king_linked(g, p) {
        return false;
    }
    g.legal_actions().into_iter().any(|a| {
        let mut child = g.clone();
        child.apply_unchecked(a);
        !king_linked(&child, p)
    })
}

fn tether_series(
    a: &str,
    b: &str,
    seeds: std::ops::Range<u64>,
    nodes: u64,
    nodes_very: u64,
) -> [Tether; 2] {
    let mut out = [Tether::default(), Tether::default()];
    for seed in seeds {
        for flip in [false, true] {
            let mut cfg = Config::new(2, seed);
            cfg.territory = true;
            cfg.set_v3(true);
            let mut g = Game::new(cfg).expect("Karte");
            // `kinds[i]` sitzt auf Platz i; `who[i]` ist die Zeile der Statistik (0 = a, 1 = b).
            let who: [usize; 2] = if flip { [1, 0] } else { [0, 1] };
            let kinds = [if flip { b } else { a }, if flip { a } else { b }];
            let mut bots: Vec<Box<dyn Bot>> = (0..2)
                .map(|i| make(kinds[i], seed * 1000 + i as u64, nodes, nodes_very))
                .collect();
            let mut was_cut = [false, false];
            let mut ever_cut = [false, false];
            let mut steps = 0;
            while g.outcome().is_none() {
                let seat = g.current_player();
                let before: Vec<bool> =
                    (0..2).map(|i| king_linked(&g, PlayerId(i as u8))).collect();
                let action = bots[seat.idx()].choose(&g);
                g.apply(action).expect("legal");
                for i in 0..2 {
                    let now = king_linked(&g, PlayerId(i as u8));
                    if before[i] && !now {
                        out[who[i]].king_cuts += 1;
                        if i == seat.idx() {
                            out[who[i]].self_cuts += 1;
                        }
                        ever_cut[i] = true;
                    }
                    was_cut[i] = !now;
                }
                if g.current_player() != seat && g.outcome().is_none() {
                    let t = &mut out[who[seat.idx()]];
                    t.turns += 1;
                    if g.weak_points(seat).iter().any(|w| w.king) {
                        t.thin_turns += 1;
                    }
                    if king_cuttable(&g, seat) {
                        t.cuttable_turns += 1;
                        if g.cut_threat(seat, |w| i64::from(w.king)) == 0 {
                            t.unseen_cuttable += 1;
                        }
                    }
                }
                steps += 1;
                assert!(steps < 30_000);
            }
            for i in 0..2 {
                let t = &mut out[who[i]];
                t.games += 1;
                t.rounds += u64::from(g.state.round);
                match g.outcome() {
                    Some(Outcome::Winner(w)) => {
                        if w.idx() == i {
                            t.wins += 1;
                            t.wins_by_capture += 1;
                        } else {
                            t.lost_by_capture += 1;
                            if ever_cut[i] {
                                t.lost_after_king_cut += 1;
                            }
                        }
                    }
                    Some(Outcome::Points(w)) if w.idx() == i => t.wins += 1,
                    _ => {}
                }
            }
        }
    }
    out
}

/// `PROBE_PAIRS=stark:flaeche,sehr:stark PROBE_FROM=100 PROBE_N=10 cargo test --release -p game_core
/// king_tether_probe -- --ignored --nocapture`
#[test]
#[ignore = "Messsonde, dauert eine Weile"]
fn king_tether_probe() {
    let env = |k: &str, d: u64| {
        std::env::var(k)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(d)
    };
    let from = env("PROBE_FROM", 100);
    let n = env("PROBE_N", 10);
    let nodes = env("PROBE_NODES", 20_000);
    let nodes_very = env("PROBE_NODES_VERY", VERY_STRONG_BUDGET);
    let pairs = std::env::var("PROBE_PAIRS").unwrap_or_else(|_| "flaeche:flaeche".into());
    for pair in pairs.split(',') {
        let (a, b) = pair.split_once(':').expect("a:b");
        let r = tether_series(a, b, from..from + n, nodes, nodes_very);
        println!("--- {a} gegen {b}: {n} Seeds ab {from}, beide Sitze, stark {nodes} / sehr {nodes_very} Knoten ---");
        for (name, t) in [(a, &r[0]), (b, &r[1])] {
            let g = f64::from(t.games.max(1));
            let turns = t.turns.max(1) as f64;
            println!(
                "  {name:8} Siege {:2}/{} ({:.0} %), davon Koenigsfang {}, Koenig gefangen {} (davon nach Abtrennung {}), \
Koenig abgetrennt {:.2}/Partie (selbst {:.2}), Zugenden mit Koenig hinter Engstelle {:.0} %, Koenig kappbar {:.1} % (davon ungesehen {:.0} %), {:.0} Runden",
                t.wins,
                t.games,
                100.0 * f64::from(t.wins) / g,
                t.wins_by_capture,
                t.lost_by_capture,
                t.lost_after_king_cut,
                f64::from(t.king_cuts) / g,
                f64::from(t.self_cuts) / g,
                100.0 * t.thin_turns as f64 / turns,
                100.0 * t.cuttable_turns as f64 / turns,
                100.0 * t.unseen_cuttable as f64 / (t.cuttable_turns.max(1)) as f64,
                t.rounds as f64 / g,
            );
        }
    }
}
