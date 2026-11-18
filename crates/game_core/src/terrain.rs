//! Version 3.4: Gelaende (`terr_terrain`). Jedes Feld ist Feld, Erde oder Moor. Das Gelaende aendert
//! nur den **Flaechenwert**, den ein Feld im Einkommen bringt: Feld = 1, Erde = 1/2, Moor = 1/4.
//! Gerechnet wird in Vierteln (4 / 2 / 1); ein Rest unter einer ganzen Flaeche wird in die naechste
//! Runde uebertragen (`State::area_rest`), damit nichts verloren geht.
//!
//! Die Verteilung folgt Zonen (Wunsch vom 18.11.2026): **Erde liegt in der Mitte** des Bretts,
//! **Moor in den Eckbereichen**, alles andere bleibt Feld. Es sind wenige Felder. Jeder Eckbereich
//! bekommt **genau gleich viele** Moorfelder, die Umgebung des Starts bleibt Feld, und von mehreren
//! Versuchen gewinnt der mit dem geringsten Unterschied im Flaechenwert rund um die Startecken.
//! Das Gelaende kommt aus einem eigenen Zufallsstrom: Rohstoffe liegen mit und ohne Gelaende an
//! denselben Stellen.

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

/// Anteil Erde an den Feldern der Mitte, in Prozent.
const EARTH_PERCENT: usize = 30;
/// Anteil Moor an den freien Feldern eines Eckbereichs, in Prozent.
const BOG_PERCENT: usize = 16;
/// Zur Mitte zaehlt ein Feld, dessen Abstand zur naechsten Ecke mindestens so viel Prozent des
/// groessten solchen Abstands auf dem Brett betraegt.
const MID_PERCENT: u32 = 70;
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
    let player_corners = config.player_corners();
    let mut best: Option<(u32, Vec<Terrain>)> = None;
    for attempt in 1..=ATTEMPTS {
        let mut rng = base.fork(attempt);
        let terrain = one_attempt(board, territories, &dist, radius, &mut rng);
        // Unterschied im Flaechenwert rund um die Startecken der Spieler
        let sums: Vec<u32> = player_corners
            .iter()
            .map(|&k| {
                board
                    .cells()
                    .filter(|c| dist[k][c.idx()] <= radius)
                    .map(|c| terrain[c.idx()].quarters())
                    .sum()
            })
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

/// Die Zonen des Bretts: die Mitte und je Ecke der Eckbereich. Felder nahe an einer Startecke
/// (bis `START_CLEAR`) gehoeren zu keiner Zone und bleiben Feld; ebenso die Felder genau zwischen
/// zwei Eckbereichen.
fn zones(
    board: &Board,
    territories: &[Option<usize>],
    dist: &[Vec<u32>],
) -> (Vec<CellId>, Vec<Vec<CellId>>) {
    let corners = board.corners().len();
    let nearest = |c: CellId| dist.iter().map(|d| d[c.idx()]).min().unwrap_or(0);
    let far = board.cells().map(nearest).max().unwrap_or(0);
    let mid_from = (far * MID_PERCENT).div_ceil(100).max(START_CLEAR + 1);
    let mut mid = Vec::new();
    let mut corner_zones: Vec<Vec<CellId>> = vec![Vec::new(); corners];
    for cell in board.cells() {
        let d = nearest(cell);
        if d <= START_CLEAR {
            continue;
        }
        if d >= mid_from {
            mid.push(cell);
        } else if let Some(g) = territories[cell.idx()] {
            corner_zones[g].push(cell);
        }
    }
    (mid, corner_zones)
}

/// `percent` Prozent von `size`, auf ganze Felder gerundet.
fn share(size: usize, percent: usize) -> usize {
    (size * percent + 50) / 100
}

fn one_attempt(
    board: &Board,
    territories: &[Option<usize>],
    dist: &[Vec<u32>],
    radius: u32,
    rng: &mut Rng,
) -> Vec<Terrain> {
    let n = board.len();
    let mut terrain = vec![Terrain::Field; n];
    let (mid, corner_zones) = zones(board, territories, dist);
    // Erde in der Mitte, in zusammenhaengenden Flecken
    let mut free = vec![false; n];
    for c in &mid {
        free[c.idx()] = true;
    }
    let earth = share(mid.len(), EARTH_PERCENT);
    grow(
        board,
        rng,
        &mid,
        &mut free,
        &mut terrain,
        Terrain::Earth,
        earth,
        5,
    );
    // Moor in den Eckbereichen: gleich viele je Ecke. Der Eckbereich teilt sich in "nah" (bis zum
    // Fairnessradius um die Ecke) und "fern"; in beiden Teilen bekommt jede Ecke gleich viele
    // Moorfelder, damit der Flaechenwert rund um jede Startecke fast gleich bleibt.
    let split: Vec<(Vec<CellId>, Vec<CellId>)> = corner_zones
        .iter()
        .enumerate()
        .map(|(g, cells)| {
            cells
                .iter()
                .copied()
                .partition(|c| dist[g][c.idx()] <= radius)
        })
        .collect();
    let near_quota = share(
        split.iter().map(|(near, _)| near.len()).min().unwrap_or(0),
        BOG_PERCENT,
    );
    let far_quota = share(
        split.iter().map(|(_, far)| far.len()).min().unwrap_or(0),
        BOG_PERCENT,
    );
    for (near, far) in &split {
        for (cells, quota) in [(near, near_quota), (far, far_quota)] {
            free.fill(false);
            for c in cells {
                free[c.idx()] = true;
            }
            grow(
                board,
                rng,
                cells,
                &mut free,
                &mut terrain,
                Terrain::Bog,
                quota,
                3,
            );
        }
    }
    terrain
}

/// Verteilt `quota` Felder der Sorte `kind` in zusammenhaengenden Flecken von hoechstens `patch`
/// Feldern innerhalb der Zellen `cells`, die noch `free` sind.
#[allow(clippy::too_many_arguments)]
fn grow(
    board: &Board,
    rng: &mut Rng,
    cells: &[CellId],
    free: &mut [bool],
    terrain: &mut [Terrain],
    kind: Terrain,
    quota: usize,
    patch: usize,
) {
    let mut left = quota;
    let mut guard = 0;
    while left > 0 && guard < 10_000 {
        guard += 1;
        let open: Vec<CellId> = cells.iter().copied().filter(|c| free[c.idx()]).collect();
        let Some(&seed) = rng.pick(&open) else {
            return;
        };
        let size = 2 + rng.below(patch.saturating_sub(1));
        let mut frontier = vec![seed];
        free[seed.idx()] = false;
        terrain[seed.idx()] = kind;
        left -= 1;
        let mut placed = 1;
        while placed < size && left > 0 && !frontier.is_empty() {
            let i = rng.below(frontier.len());
            let from = frontier[i];
            let next: Vec<CellId> = board
                .neighbors(from)
                .filter(|c| free[c.idx()] && cells.contains(c))
                .collect();
            let Some(&to) = rng.pick(&next) else {
                frontier.swap_remove(i);
                continue;
            };
            free[to.idx()] = false;
            terrain[to.idx()] = kind;
            frontier.push(to);
            placed += 1;
            left -= 1;
        }
    }
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
    fn every_corner_area_gets_exactly_the_same_number_of_bog_cells() {
        for players in [2u8, 3, 4, 5, 6, 8] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, territories, t) = terrain_of(&c);
                let corners = board.corners().len();
                let bog = |g: usize| {
                    board
                        .cells()
                        .filter(|cell| {
                            territories[cell.idx()] == Some(g) && t[cell.idx()] == Terrain::Bog
                        })
                        .count()
                };
                let first = bog(0);
                assert!(first > 0, "{players} Spieler, Seed {seed}: kein Moor");
                for g in 1..corners {
                    assert_eq!(
                        bog(g),
                        first,
                        "{players} Spieler, Seed {seed}: Eckbereich {g} hat eine andere Zahl Moor"
                    );
                }
            }
        }
    }

    #[test]
    fn earth_lies_in_the_middle_and_bog_in_the_corner_areas() {
        for players in [2u8, 3, 4, 5, 6, 8] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, territories, t) = terrain_of(&c);
                let dist: Vec<Vec<u32>> = board
                    .corners()
                    .iter()
                    .map(|&k| board.distances_from(k))
                    .collect();
                let (mid, corner_zones) = zones(&board, &territories, &dist);
                let earth = |cell: &CellId| t[cell.idx()] == Terrain::Earth;
                assert!(
                    mid.iter().any(earth),
                    "{players} Spieler, Seed {seed}: keine Erde"
                );
                // Erde nur in der Mitte, Moor nur in den Eckbereichen
                for cell in board.cells() {
                    match t[cell.idx()] {
                        Terrain::Earth => assert!(mid.contains(&cell), "Erde ausserhalb der Mitte"),
                        Terrain::Bog => assert!(
                            corner_zones.iter().any(|z| z.contains(&cell)),
                            "Moor ausserhalb der Eckbereiche"
                        ),
                        Terrain::Field => {}
                    }
                }
            }
        }
    }

    #[test]
    fn only_a_minority_of_the_board_is_not_field() {
        // Wunsch vom 18.11.2026: deutlich weniger Gelaendefelder (hoechstens ein Fuenftel)
        for players in [2u8, 3, 4, 5, 6, 8] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, _, t) = terrain_of(&c);
                let special = t.iter().filter(|&&x| x != Terrain::Field).count();
                assert!(
                    special * 5 <= board.len(),
                    "{players} Spieler, Seed {seed}: {special} von {} Feldern",
                    board.len()
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
