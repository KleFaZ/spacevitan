//! Version 3.4: Gelaende (`terr_terrain`). Jedes Feld ist Feld, Erde oder Moor. Das Gelaende aendert
//! nur den **Flaechenwert**, den ein Feld im Einkommen bringt: Feld = 1, Erde = 1/2, Moor = 1/4.
//! Gerechnet wird in Vierteln (4 / 2 / 1); ein Rest unter einer ganzen Flaeche wird in die naechste
//! Runde uebertragen (`State::area_rest`), damit nichts verloren geht.
//!
//! Die Verteilung (Wunsch vom 18.11.2026, zweite Fassung): **Moor** ist ein Viertel der Felder und
//! liegt eher in den Ecken, **Erde** ein Drittel, eher im Zwischenbereich zwischen Mitte und Ecke,
//! der Rest ist **Feld**, eher in der Mitte (dazu die Umgebung des Starts). Gemessen wird der
//! Abstand zur naechsten Startecke; ein glattes Rauschen mischt die Bereiche ein wenig und macht
//! Flecken statt starrer Ringe. Jedes Eckgebiet bekommt **genau gleich viele** Moor- und
//! Erdfelder, und von mehreren Versuchen gewinnt der mit dem geringsten Unterschied im
//! Flaechenwert rund um die Startecken. Das Gelaende kommt aus einem eigenen Zufallsstrom:
//! Rohstoffe liegen mit und ohne Gelaende an denselben Stellen.

use crate::board::Board;
use crate::config::Config;
use crate::ids::CellId;
use crate::rng::Rng;

/// Wie ein Feld aussieht und was es im Einkommen zaehlt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Terrain {
    /// Voller Flaechenwert (1).
    #[default]
    Field,
    /// Halber Flaechenwert (1/2).
    Earth,
    /// Viertel Flaechenwert (1/4).
    Bog,
}

impl Terrain {
    /// Flaechenwert in Vierteln: Feld 4, Erde 2, Moor 1.
    pub fn quarters(self) -> u32 {
        match self {
            Terrain::Field => 4,
            Terrain::Earth => 2,
            Terrain::Bog => 1,
        }
    }

    /// Ziffer fuer die JSON-Ausgabe und die Kartenansicht: 0 Feld, 1 Erde, 2 Moor.
    pub fn code(self) -> u8 {
        match self {
            Terrain::Field => 0,
            Terrain::Earth => 1,
            Terrain::Bog => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Terrain::Field => "Feld",
            Terrain::Earth => "Erde",
            Terrain::Bog => "Moor",
        }
    }
}

/// Anteil Moor und Erde an allen Feldern eines Eckgebiets, in Promille (Moor 1/4, Erde 1/3).
const BOG_PERMILLE: usize = 250;
const EARTH_PERMILLE: usize = 333;
/// Wie stark das Rauschen die Lage mischt (Prozent des Abstandsbereichs). 0 waeren starre Ringe.
const NOISE_WEIGHT: u64 = 30;
/// Wie oft das Rauschen mit den Nachbarn geglaettet wird (mehr = groessere Flecken).
const SMOOTH_ROUNDS: usize = 3;
/// Felder bis zu diesem Abstand vom Start bleiben Feld (die Startumgebung ist fuer alle gleich).
const START_CLEAR: u32 = 2;
/// Wie viele Verteilungen der Generator vergleicht.
const ATTEMPTS: u64 = 200;
/// Salz fuer den eigenen Zufallsstrom.
const SALT: u64 = 0x7E44_A1F5_90C3_5D21;

/// Gelaende fuer das ganze Brett. Ohne `terr_terrain` ist alles Feld.
pub fn make_terrain(config: &Config, board: &Board, territories: &[Option<usize>]) -> Vec<Terrain> {
    let n = board.len();
    if !config.terr_terrain {
        return vec![Terrain::Field; n];
    }
    let mut base = Rng::new(config.seed ^ SALT);
    let corner_cells = board.corners().to_vec();
    let dist: Vec<Vec<u32>> = corner_cells
        .iter()
        .map(|&c| board.distances_from(c))
        .collect();
    let radius = config.effective_fairness_radius().max(4);
    // Alles, was nicht vom Zufall abhaengt, einmal berechnen (nicht in jedem der Versuche).
    let layout = Layout::new(board, territories, &dist);
    let around_start: Vec<Vec<usize>> = config
        .player_corners()
        .iter()
        .map(|&k| {
            board
                .cells()
                .filter(|c| dist[k][c.idx()] <= radius)
                .map(|c| c.idx())
                .collect()
        })
        .collect();
    let mut best: Option<(u32, Vec<Terrain>)> = None;
    for attempt in 1..=ATTEMPTS {
        let mut rng = base.fork(attempt);
        let terrain = one_attempt(board, &layout, &mut rng);
        // Unterschied im Flaechenwert rund um die Startecken der Spieler
        let sums: Vec<u32> = around_start
            .iter()
            .map(|cells| cells.iter().map(|&i| terrain[i].quarters()).sum())
            .collect();
        let spread = sums.iter().max().unwrap_or(&0) - sums.iter().min().unwrap_or(&0);
        if best.as_ref().is_none_or(|(s, _)| spread < *s) {
            best = Some((spread, terrain));
        }
        if spread == 0 {
            break;
        }
    }
    best.map(|(_, t)| t)
        .unwrap_or_else(|| vec![Terrain::Field; n])
}

/// Wie viel Flaechenwert (in Vierteln) im Umkreis um die Ecke liegt; fuer Tests und Messungen.
pub fn quarters_within(board: &Board, terrain: &[Terrain], corner: CellId, radius: u32) -> u32 {
    let d = board.distances_from(corner);
    board
        .cells()
        .filter(|c| d[c.idx()] <= radius)
        .map(|c| terrain[c.idx()].quarters())
        .sum()
}

/// `permille` Promille von `size`, auf ganze Felder gerundet.
fn share(size: usize, permille: usize) -> usize {
    (size * permille + 500) / 1000
}

/// Glattes Rauschen ueber das Brett als Rang 0..1000 je Feld: Zufallswerte, mehrmals mit den
/// Nachbarn gemittelt, damit benachbarte Felder aehnlich ausfallen (Flecken).
fn noise_ranks(board: &Board, rng: &mut Rng) -> Vec<u64> {
    let n = board.len();
    let mut v: Vec<u64> = (0..n).map(|_| rng.below(1_000_000) as u64).collect();
    for _ in 0..SMOOTH_ROUNDS {
        let prev = v.clone();
        for cell in board.cells() {
            let mut sum = prev[cell.idx()];
            let mut count = 1;
            for nb in board.neighbors(cell) {
                sum += prev[nb.idx()];
                count += 1;
            }
            v[cell.idx()] = sum / count;
        }
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (v[i], i));
    let mut rank = vec![0u64; n];
    for (r, &i) in order.iter().enumerate() {
        rank[i] = r as u64 * 1000 / n as u64;
    }
    rank
}

/// Abstand jedes Feldes zur naechsten Startecke in Feldschritten.
pub(crate) fn nearest_corner(board: &Board, dist: &[Vec<u32>]) -> Vec<u32> {
    board
        .cells()
        .map(|c| dist.iter().map(|d| d[c.idx()]).min().unwrap_or(0))
        .collect()
}

/// Luftlinie jedes Feldes zur naechsten Startecke (in Tausendsteln einer Feldbreite). Sie bestimmt
/// die Lage, denn sie macht aus den Eckbereichen runde Ecken statt Rauten entlang des Brettrands.
pub(crate) fn corner_air_distance(board: &Board) -> Vec<u64> {
    board
        .cells()
        .map(|c| {
            let (x, y) = board.pos(c);
            board
                .corners()
                .iter()
                .map(|&k| {
                    let (kx, ky) = board.pos(k);
                    (((x - kx).powi(2) + (y - ky).powi(2)).sqrt() * 1000.0).round() as u64
                })
                .min()
                .unwrap_or(0)
        })
        .collect()
}

/// Der feste Teil der Verteilung (ohne Zufall), einmal je Karte berechnet.
struct Layout {
    /// Je Ecke das Gebiet, zuletzt die Felder genau zwischen zwei Gebieten.
    groups: Vec<Group>,
    /// Grundwert der Lage je Feld: Luftlinie zur naechsten Startecke (0..1000, mal 100).
    position: Vec<u64>,
}

struct Group {
    /// Anzahl Moor- und Erdfelder dieser Gruppe.
    bog: usize,
    earth: usize,
    /// Felder der Gruppe ausser der Startumgebung (die bleibt Feld).
    open: Vec<CellId>,
}

impl Layout {
    fn new(board: &Board, territories: &[Option<usize>], dist: &[Vec<u32>]) -> Layout {
        let corners = board.corners().len();
        let nearest = nearest_corner(board, dist);
        let air = corner_air_distance(board);
        let far = air.iter().copied().max().unwrap_or(1).max(1);
        let position: Vec<u64> = air.iter().map(|a| (a * 1000 / far) * 100).collect();
        // Gruppen: je Ecke das Gebiet, dazu die Felder genau zwischen zwei Gebieten (Index
        // `corners`).
        let mut cells_of: Vec<Vec<CellId>> = vec![Vec::new(); corners + 1];
        for cell in board.cells() {
            cells_of[territories[cell.idx()].unwrap_or(corners)].push(cell);
        }
        // Gleich viele je Ecke: das kleinste Gebiet gibt die Zahl vor, groessere behalten ihre
        // mittleren Felder als Feld.
        let smallest = cells_of[..corners].iter().map(Vec::len).min().unwrap_or(0);
        let groups = cells_of
            .iter()
            .enumerate()
            .map(|(g, cells)| {
                let size = if g < corners { smallest } else { cells.len() };
                Group {
                    bog: share(size, BOG_PERMILLE),
                    earth: share(size, EARTH_PERMILLE),
                    open: cells
                        .iter()
                        .copied()
                        .filter(|c| nearest[c.idx()] > START_CLEAR)
                        .collect(),
                }
            })
            .collect();
        Layout { groups, position }
    }
}

fn one_attempt(board: &Board, layout: &Layout, rng: &mut Rng) -> Vec<Terrain> {
    let mut terrain = vec![Terrain::Field; board.len()];
    let rank = noise_ranks(board, rng);
    let key = |c: &CellId| layout.position[c.idx()] + rank[c.idx()] * NOISE_WEIGHT;
    let mut open: Vec<CellId> = Vec::new();
    for group in &layout.groups {
        // Nach Lage sortiert: nah an der Ecke zuerst (Moor), dann der Zwischenbereich (Erde),
        // der Rest (Feld) liegt in der Mitte.
        open.clear();
        open.extend_from_slice(&group.open);
        open.sort_by_key(|c| (key(c), c.idx()));
        for (i, c) in open.iter().enumerate() {
            terrain[c.idx()] = if i < group.bog {
                Terrain::Bog
            } else if i < group.bog + group.earth {
                Terrain::Earth
            } else {
                Terrain::Field
            };
        }
    }
    terrain
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(players: u8, seed: u64, on: bool) -> Config {
        let mut c = Config::new(players, seed);
        c.set_all("territory=1,terr_v3=1").unwrap();
        c.terr_terrain = on;
        c
    }

    fn terrain_of(c: &Config) -> (Board, Vec<Option<usize>>, Vec<Terrain>) {
        let board = c.make_board().unwrap();
        let territories = board.territories();
        let t = make_terrain(c, &board, &territories);
        (board, territories, t)
    }

    #[test]
    fn the_map_carries_exactly_the_terrain_made_once_for_its_board() {
        // Das Gelaende haengt nicht von den Vorkommen ab; wie oft der Generator die Vorkommen
        // neu wuerfelt (attempts_used), aendert es nicht. Auch Karten mit mehreren Versuchen.
        let mut retried = false;
        for players in 2..=8u8 {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (_, _, direct) = terrain_of(&c);
                let map = crate::map::Map::generate(&c).unwrap();
                retried |= map.attempts_used > 1;
                assert_eq!(map.terrain, direct, "{players} Spieler, Seed {seed}");
            }
        }
        assert!(
            retried,
            "der Test soll auch Karten mit Wiederholungen sehen"
        );
    }

    #[test]
    fn values_are_four_two_one_quarters() {
        assert_eq!(Terrain::Field.quarters(), 4);
        assert_eq!(Terrain::Earth.quarters(), 2);
        assert_eq!(Terrain::Bog.quarters(), 1);
    }

    #[test]
    fn without_the_switch_everything_is_field() {
        let (_, _, t) = terrain_of(&config(2, 3, false));
        assert!(t.iter().all(|&x| x == Terrain::Field));
    }

    #[test]
    fn the_same_seed_gives_the_same_terrain_and_other_seeds_differ() {
        let a = terrain_of(&config(2, 5, true)).2;
        let b = terrain_of(&config(2, 5, true)).2;
        let c = terrain_of(&config(2, 6, true)).2;
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn every_corner_area_gets_exactly_the_same_number_of_bog_and_earth_cells() {
        for players in [2u8, 3, 4, 5, 6, 7, 8] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, territories, t) = terrain_of(&c);
                let corners = board.corners().len();
                for kind in [Terrain::Bog, Terrain::Earth] {
                    let count = |g: usize| {
                        board
                            .cells()
                            .filter(|cell| {
                                territories[cell.idx()] == Some(g) && t[cell.idx()] == kind
                            })
                            .count()
                    };
                    let first = count(0);
                    assert!(
                        first > 0,
                        "{players} Spieler, Seed {seed}: kein {}",
                        kind.name()
                    );
                    for g in 1..corners {
                        assert_eq!(
                            count(g),
                            first,
                            "{players} Spieler, Seed {seed}: Eckgebiet {g} hat eine andere Zahl {}",
                            kind.name()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn bog_lies_near_the_corners_earth_in_between_and_field_in_the_middle() {
        for players in [2u8, 3, 4, 5, 6, 7, 8] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, _, t) = terrain_of(&c);
                let dist: Vec<Vec<u32>> = board
                    .corners()
                    .iter()
                    .map(|&k| board.distances_from(k))
                    .collect();
                let nearest = nearest_corner(&board, &dist);
                let air = corner_air_distance(&board);
                // mittlere Luftlinie zur naechsten Ecke je Sorte, ohne die Startumgebung
                let mean = |kind: Terrain| {
                    let cells: Vec<u64> = board
                        .cells()
                        .filter(|cell| t[cell.idx()] == kind && nearest[cell.idx()] > START_CLEAR)
                        .map(|cell| air[cell.idx()])
                        .collect();
                    cells.iter().sum::<u64>() as f64 / 1000.0 / cells.len().max(1) as f64
                };
                let (bog, earth, field) = (
                    mean(Terrain::Bog),
                    mean(Terrain::Earth),
                    mean(Terrain::Field),
                );
                assert!(
                    bog < earth && earth < field,
                    "{players} Spieler, Seed {seed}: Abstand Moor {bog:.2}, Erde {earth:.2}, Feld {field:.2}"
                );
            }
        }
    }

    #[test]
    fn about_a_quarter_is_bog_and_a_third_is_earth() {
        // Wunsch vom 18.11.2026: Moor ein Viertel, Erde ein Drittel der Felder, der Rest Feld
        for players in [2u8, 3, 4, 5, 6, 7, 8] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, _, t) = terrain_of(&c);
                let n = board.len() as f64;
                let share = |kind: Terrain| t.iter().filter(|&&x| x == kind).count() as f64 / n;
                let (bog, earth) = (share(Terrain::Bog), share(Terrain::Earth));
                assert!(
                    (0.19..=0.27).contains(&bog),
                    "{players} Spieler, Seed {seed}: Moor {bog:.3}"
                );
                assert!(
                    (0.27..=0.36).contains(&earth),
                    "{players} Spieler, Seed {seed}: Erde {earth:.3}"
                );
            }
        }
    }

    #[test]
    fn the_start_surroundings_stay_field() {
        for seed in 1..=10u64 {
            let c = config(4, seed, true);
            let (board, _, t) = terrain_of(&c);
            for &corner in board.corners() {
                let d = board.distances_from(corner);
                for cell in board.cells() {
                    if d[cell.idx()] <= START_CLEAR {
                        assert_eq!(t[cell.idx()], Terrain::Field, "Seed {seed}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_terrain_adds_at_most_one_field_of_unfairness_around_the_starts() {
        // Unterschied (in Vierteln) im Umkreis um die Startecken der Spieler. Die Bretter fuer 5, 7
        // und 8 Spieler sind selbst nicht ganz gleich; das Gelaende darf dazu hoechstens ein
        // volles Feld (4 Viertel) an Unterschied bringen.
        for players in [2u8, 3, 4, 5, 6, 7, 8] {
            for seed in 1..=20u64 {
                let c = config(players, seed, true);
                let (board, _, t) = terrain_of(&c);
                let radius = c.effective_fairness_radius().max(4);
                let spread = |terrain: &[Terrain]| {
                    let sums: Vec<u32> = c
                        .player_corners()
                        .iter()
                        .map(|&k| quarters_within(&board, terrain, board.corners()[k], radius))
                        .collect();
                    sums.iter().max().unwrap() - sums.iter().min().unwrap()
                };
                let base = spread(&vec![Terrain::Field; board.len()]);
                let with = spread(&t);
                assert!(
                    with <= base + 4,
                    "{players} Spieler, Seed {seed}: Unterschied {with} statt {base} Vierteln"
                );
            }
        }
    }
}
