//! Version 3.4: Gelaende (`terr_terrain`). Jedes Feld ist Feld, Wald oder Moor. Das Gelaende aendert
//! nur den **Flaechenwert**, den ein Feld im Einkommen bringt: Feld = 1, Wald = 1/2, Moor = 1/4.
//! Gerechnet wird in Vierteln (4 / 2 / 1); ein Rest unter einer ganzen Flaeche wird in die naechste
//! Runde uebertragen (`State::area_rest`), damit nichts verloren geht.
//!
//! Die Verteilung ist fair: Jedes Gebiet (Ecke) bekommt **genau gleich viele** Wald- und
//! Moorfelder, die Umgebung des Starts bleibt Feld, und von mehreren Versuchen gewinnt der mit dem
//! geringsten Unterschied im Flaechenwert rund um die Startecken. Das Gelaende kommt aus einem
//! eigenen Zufallsstrom: Rohstoffe liegen mit und ohne Gelaende an denselben Stellen.

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
    Forest,
    /// Viertel Flaechenwert (1/4).
    Bog,
}

impl Terrain {
    /// Flaechenwert in Vierteln: Feld 4, Wald 2, Moor 1.
    pub fn quarters(self) -> u32 {
        match self {
            Terrain::Field => 4,
            Terrain::Forest => 2,
            Terrain::Bog => 1,
        }
    }

    /// Ziffer fuer die JSON-Ausgabe und die Kartenansicht: 0 Feld, 1 Wald, 2 Moor.
    pub fn code(self) -> u8 {
        match self {
            Terrain::Field => 0,
            Terrain::Forest => 1,
            Terrain::Bog => 2,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Terrain::Field => "Feld",
            Terrain::Forest => "Wald",
            Terrain::Bog => "Moor",
        }
    }
}

/// Anteil Wald und Moor je Gebiet in Prozent der freien Felder.
const FOREST_PERCENT: usize = 24;
const BOG_PERCENT: usize = 12;
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
        let terrain = one_attempt(board, territories, &dist, &mut rng);
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

fn one_attempt(
    board: &Board,
    territories: &[Option<usize>],
    dist: &[Vec<u32>],
    rng: &mut Rng,
) -> Vec<Terrain> {
    let n = board.len();
    let corners = board.corners().len();
    let mut terrain = vec![Terrain::Field; n];
    // Gruppen: je Ecke ein Gebiet, dazu die neutrale Mitte (Index `corners`).
    let mut groups: Vec<Vec<CellId>> = vec![Vec::new(); corners + 1];
    for cell in board.cells() {
        let g = territories[cell.idx()].unwrap_or(corners);
        // frei = weiter als START_CLEAR von jeder Startecke
        if dist.iter().all(|d| d[cell.idx()] > START_CLEAR) {
            groups[g].push(cell);
        }
    }
    // Gleich viele Felder je Ecken-Gebiet: der kleinste Wert gibt die Zahl vor.
    let smallest = groups[..corners].iter().map(Vec::len).min().unwrap_or(0);
    for (g, cells) in groups.iter().enumerate() {
        let size = if g < corners { smallest } else { cells.len() };
        let forest = size * FOREST_PERCENT / 100;
        let bog = size * BOG_PERCENT / 100;
        let mut free: Vec<bool> = vec![false; n];
        for c in cells {
            free[c.idx()] = true;
        }
        grow(
            board,
            rng,
            cells,
            &mut free,
            &mut terrain,
            Terrain::Forest,
            forest,
            6,
        );
        grow(
            board,
            rng,
            cells,
            &mut free,
            &mut terrain,
            Terrain::Bog,
            bog,
            4,
        );
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
        assert_eq!(Terrain::Forest.quarters(), 2);
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
    fn every_territory_gets_exactly_the_same_number_of_forest_and_bog_cells() {
        for players in [2u8, 3, 4, 5, 6] {
            for seed in 1..=12u64 {
                let c = config(players, seed, true);
                let (board, territories, t) = terrain_of(&c);
                let corners = board.corners().len();
                let count = |g: usize, kind: Terrain| {
                    board
                        .cells()
                        .filter(|cell| territories[cell.idx()] == Some(g) && t[cell.idx()] == kind)
                        .count()
                };
                for kind in [Terrain::Forest, Terrain::Bog] {
                    let first = count(0, kind);
                    assert!(
                        first > 0,
                        "{players} Spieler, Seed {seed}: kein {}",
                        kind.name()
                    );
                    for g in 1..corners {
                        assert_eq!(
                            count(g, kind),
                            first,
                            "{players} Spieler, Seed {seed}: Gebiet {g} hat andere Zahl {}",
                            kind.name()
                        );
                    }
                }
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
    fn the_land_value_around_each_start_is_nearly_equal() {
        // Unterschied (in Vierteln) im Umkreis um die Startecken der Spieler, ueber viele Karten
        let mut worst = 0;
        for players in [2u8, 3, 4, 5, 6] {
            for seed in 1..=20u64 {
                let c = config(players, seed, true);
                let (board, _, t) = terrain_of(&c);
                let radius = c.effective_fairness_radius().max(4);
                let sums: Vec<u32> = c
                    .player_corners()
                    .iter()
                    .map(|&k| quarters_within(&board, &t, board.corners()[k], radius))
                    .collect();
                let spread = sums.iter().max().unwrap() - sums.iter().min().unwrap();
                worst = worst.max(spread);
            }
        }
        // Gemessen: hoechstens 4 Viertel (ein volles Feld) bei rund 90 Vierteln im Umkreis.
        assert!(worst <= 6, "groesster Unterschied {worst} Viertel");
    }
}
