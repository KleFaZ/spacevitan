//! Karte mit Rohstoffvorkommen, deterministisch aus einem Seed erzeugt, mit Fairnesspruefung.
//!
//! Regeln aus dem Regelwerk (Abschnitt 3):
//! - je Startecke 1 Kat-1-Typ, 2 Kat-2-Typen, 3 Kat-3-Typen
//! - Rohstoffe liegen primaer im Gebiet ihrer Ecke (Startposition bis Mitte), vereinzelt anderswo
//! - Kat 1 liegt immer im Gebiet der Ecke, der erste Fundort 1 bis 2 Schritte vom Start
//! - Adern: zusammenhaengende Felder; ein Feld kann mehrere Rohstoffe tragen
//! - die Startfelder selbst sind frei

use crate::board::{Board, BoardError};
use crate::config::{Config, ConfigError};
use crate::ids::{CellId, PlayerId, ResourceId};
use crate::rng::Rng;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceType {
    pub id: ResourceId,
    /// 1 = Hauptrohstoff (am seltensten), 2, 3.
    pub category: u8,
    /// Ecke (Index in `Board::corners`), zu deren Gebiet der Rohstoff gehoert.
    pub home_corner: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MapError {
    Config(ConfigError),
    Board(BoardError),
    /// Auch nach allen Versuchen keine faire Karte gefunden.
    GenerationFailed {
        attempts: u32,
    },
}

impl std::fmt::Display for MapError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MapError::Config(e) => write!(f, "{e}"),
            MapError::Board(e) => write!(f, "{e}"),
            MapError::GenerationFailed { attempts } => {
                write!(f, "keine faire Karte nach {attempts} Versuchen gefunden")
            }
        }
    }
}

impl std::error::Error for MapError {}

impl From<ConfigError> for MapError {
    fn from(e: ConfigError) -> Self {
        MapError::Config(e)
    }
}

impl From<BoardError> for MapError {
    fn from(e: BoardError) -> Self {
        MapError::Board(e)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Map {
    pub board: Board,
    pub resource_types: Vec<ResourceType>,
    /// Rohstoffe je Feld, sortiert nach Typ-ID.
    pub deposits: Vec<Vec<ResourceId>>,
    /// Gebiet je Feld (Index der Ecke), `None` = neutrale Mitte.
    pub territories: Vec<Option<usize>>,
    /// Ecke jedes Spielers (Index = Spieler).
    pub player_corners: Vec<usize>,
    /// Wie viele Generator-Versuche gebraucht wurden (fuer Statistik und Tests).
    pub attempts_used: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CornerStats {
    pub corner: usize,
    /// Kuerzester Abstand vom Start zu einem Vorkommen des eigenen Kat-1-Typs.
    pub cat1_first_distance: Option<u32>,
    /// Rohstoffvorkommen je Kategorie (Index 0 = Kat 1) im Radius um den Start.
    pub near_counts: [u32; 3],
    /// Vorkommen des eigenen Kat-1-Typs im eigenen Gebiet.
    pub cat1_in_territory: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FairnessIssue {
    Cat1TooFarOrMissing {
        corner: usize,
        distance: Option<u32>,
    },
    Cat1NotInTerritory {
        corner: usize,
        found: u32,
        expected: u32,
    },
    NearCountsUneven {
        category: u8,
        min: u32,
        max: u32,
    },
}

impl Map {
    /// Erzeugt eine faire Karte. Gleiche Einstellungen und gleicher Seed ergeben dieselbe Karte.
    pub fn generate(config: &Config) -> Result<Map, MapError> {
        config.validate()?;
        let board = Board::square(config.side)?;
        let territories = board.territories();
        let resource_types = make_types(config.corner_count());
        let mut base = Rng::new(config.seed);
        for attempt in 1..=config.max_generator_attempts {
            let mut rng = base.fork(attempt as u64);
            let Some(deposits) = place_all(config, &board, &territories, &resource_types, &mut rng)
            else {
                continue;
            };
            let map = Map {
                board: board.clone(),
                resource_types: resource_types.clone(),
                deposits,
                territories: territories.clone(),
                player_corners: config.player_corners(),
                attempts_used: attempt,
            };
            if map.fairness_issues(config).is_empty() {
                return Ok(map);
            }
        }
        Err(MapError::GenerationFailed {
            attempts: config.max_generator_attempts,
        })
    }

    /// Karte als Text: `S0`..`S3` = Startecken, Ziffern = Kategorien der Vorkommen auf dem Feld
    /// (seltenste zuerst), `.` = leer.
    pub fn render_ascii(&self) -> String {
        let side = self.board.side() as i32;
        let mut out = String::new();
        for y in 0..side {
            for x in 0..side {
                let cell = self.board.cell_at(x, y).expect("Feld liegt auf dem Brett");
                let text = if let Some(i) = self.board.corners().iter().position(|&c| c == cell) {
                    format!("S{i}")
                } else if self.deposits_at(cell).is_empty() {
                    ".".to_string()
                } else {
                    let mut cats: Vec<u8> = self
                        .deposits_at(cell)
                        .iter()
                        .map(|&r| self.category(r))
                        .collect();
                    cats.sort();
                    cats.iter().map(|c| c.to_string()).collect::<String>()
                };
                out.push_str(&format!("{text:>3}"));
            }
            out.push('\n');
        }
        out
    }

    pub fn resource_type(&self, id: ResourceId) -> &ResourceType {
        &self.resource_types[id.idx()]
    }

    pub fn category(&self, id: ResourceId) -> u8 {
        self.resource_type(id).category
    }

    pub fn deposits_at(&self, cell: CellId) -> &[ResourceId] {
        &self.deposits[cell.idx()]
    }

    pub fn start_cell(&self, player: PlayerId) -> CellId {
        self.board.corners()[self.player_corners[player.idx()]]
    }

    /// Alle Ecken, die keinem Spieler gehoeren (nur bei 2 Spielern).
    pub fn free_corners(&self) -> Vec<usize> {
        (0..self.board.corners().len())
            .filter(|c| !self.player_corners.contains(c))
            .collect()
    }

    pub fn types_of_corner(&self, corner: usize) -> impl Iterator<Item = &ResourceType> {
        self.resource_types
            .iter()
            .filter(move |t| t.home_corner == corner)
    }

    /// Der Kat-1-Typ einer Ecke.
    pub fn cat1_type_of_corner(&self, corner: usize) -> ResourceId {
        self.types_of_corner(corner)
            .find(|t| t.category == 1)
            .expect("jede Ecke hat einen Kat-1-Typ")
            .id
    }

    pub fn corner_stats(&self, config: &Config) -> Vec<CornerStats> {
        let mut out = Vec::new();
        for corner in 0..self.board.corners().len() {
            let start = self.board.corners()[corner];
            let dist = self.board.distances_from(start);
            let cat1 = self.cat1_type_of_corner(corner);
            let mut first: Option<u32> = None;
            let mut near = [0u32; 3];
            let mut cat1_in_territory = 0;
            for cell in self.board.cells() {
                for &res in self.deposits_at(cell) {
                    let d = dist[cell.idx()];
                    if res == cat1 {
                        first = Some(first.map_or(d, |f| f.min(d)));
                        if self.territories[cell.idx()] == Some(corner) {
                            cat1_in_territory += 1;
                        }
                    }
                    if d <= config.fairness_radius {
                        near[self.category(res) as usize - 1] += 1;
                    }
                }
            }
            out.push(CornerStats {
                corner,
                cat1_first_distance: first,
                near_counts: near,
                cat1_in_territory,
            });
        }
        out
    }

    /// Prueft die Karte auf Gleichbehandlung aller Startecken. Leer = fair.
    pub fn fairness_issues(&self, config: &Config) -> Vec<FairnessIssue> {
        let stats = self.corner_stats(config);
        let mut issues = Vec::new();
        for s in &stats {
            match s.cat1_first_distance {
                Some(d) if (1..=2).contains(&d) => {}
                other => issues.push(FairnessIssue::Cat1TooFarOrMissing {
                    corner: s.corner,
                    distance: other,
                }),
            }
            if s.cat1_in_territory != config.cells_per_type[0] {
                issues.push(FairnessIssue::Cat1NotInTerritory {
                    corner: s.corner,
                    found: s.cat1_in_territory,
                    expected: config.cells_per_type[0],
                });
            }
        }
        for cat in 0..3 {
            let values: Vec<u32> = stats.iter().map(|s| s.near_counts[cat]).collect();
            let (min, max) = (
                *values.iter().min().expect("mindestens eine Ecke"),
                *values.iter().max().expect("mindestens eine Ecke"),
            );
            if max - min > config.fairness_tolerance {
                issues.push(FairnessIssue::NearCountsUneven {
                    category: cat as u8 + 1,
                    min,
                    max,
                });
            }
        }
        issues
    }
}

/// Typen je Ecke: 1x Kat 1, 2x Kat 2, 3x Kat 3. IDs laufen ecken-weise durch.
fn make_types(corners: usize) -> Vec<ResourceType> {
    let mut types = Vec::new();
    for corner in 0..corners {
        for (category, count) in [(1u8, 1usize), (2, 2), (3, 3)] {
            for _ in 0..count {
                types.push(ResourceType {
                    id: ResourceId(types.len() as u16),
                    category,
                    home_corner: corner,
                });
            }
        }
    }
    types
}

/// Ein Platzierungsversuch. `None`, wenn irgendwo kein Platz mehr war.
fn place_all(
    config: &Config,
    board: &Board,
    territories: &[Option<usize>],
    types: &[ResourceType],
    rng: &mut Rng,
) -> Option<Vec<Vec<ResourceId>>> {
    let mut deposits: Vec<Vec<ResourceId>> = vec![Vec::new(); board.len()];
    let cap = config.max_resources_per_cell;
    let start_cells: Vec<CellId> = board.corners().to_vec();

    for corner in 0..board.corners().len() {
        let start = board.corners()[corner];
        let dist = board.distances_from(start);
        let in_home = |cell: CellId, deposits: &[Vec<ResourceId>]| {
            territories[cell.idx()] == Some(corner)
                && cell != start
                && deposits[cell.idx()].len() < cap
        };

        for ty in types.iter().filter(|t| t.home_corner == corner) {
            let size = config.cells_per_type[ty.category as usize - 1] as usize;

            let seeds: Vec<CellId> = board
                .cells()
                .filter(|&c| in_home(c, &deposits))
                .filter(|&c| ty.category != 1 || (1..=2).contains(&dist[c.idx()]))
                .collect();
            let seed = *rng.pick(&seeds)?;

            let mut vein = vec![seed];
            while vein.len() < size {
                let mut frontier: Vec<CellId> = vein
                    .iter()
                    .flat_map(|&c| board.neighbors(c))
                    .filter(|n| !vein.contains(n) && in_home(*n, &deposits))
                    .collect();
                frontier.sort();
                frontier.dedup();
                vein.push(*rng.pick(&frontier)?);
            }

            // Vereinzelte Vorkommen ausserhalb des Gebiets (nie bei Kat 1).
            if ty.category != 1 && size > 1 && rng.chance(config.stray_chance) {
                let far: Vec<CellId> = board
                    .cells()
                    .filter(|c| !vein.contains(c))
                    .filter(|c| !start_cells.contains(c))
                    .filter(|c| deposits[c.idx()].len() < cap)
                    .collect();
                if let Some(&spot) = rng.pick(&far) {
                    *vein.last_mut().expect("Ader hat mindestens ein Feld") = spot;
                }
            }

            for cell in vein {
                deposits[cell.idx()].push(ty.id);
            }
        }
    }
    for list in &mut deposits {
        list.sort();
    }
    Some(deposits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn cfg(players: u8, seed: u64) -> Config {
        Config::new(players, seed)
    }

    fn generate(players: u8, seed: u64) -> (Config, Map) {
        let c = cfg(players, seed);
        let m = Map::generate(&c).expect("Karte sollte erzeugbar sein");
        (c, m)
    }

    fn deposit_cells(map: &Map, ty: ResourceId) -> Vec<CellId> {
        map.board
            .cells()
            .filter(|&c| map.deposits_at(c).contains(&ty))
            .collect()
    }

    #[test]
    fn type_counts_follow_the_one_two_three_pattern() {
        let (_, m) = generate(4, 1);
        assert_eq!(m.resource_types.len(), 24);
        for cat in 1..=3u8 {
            let per_corner = m
                .resource_types
                .iter()
                .filter(|t| t.category == cat && t.home_corner == 0)
                .count();
            assert_eq!(per_corner, cat as usize, "Kategorie {cat}");
            let total = m
                .resource_types
                .iter()
                .filter(|t| t.category == cat)
                .count();
            assert_eq!(total, 4 * cat as usize);
        }
        for (i, t) in m.resource_types.iter().enumerate() {
            assert_eq!(t.id.idx(), i, "IDs sind fortlaufend");
        }
    }

    #[test]
    fn every_type_has_exactly_the_configured_number_of_deposits() {
        for seed in 0..20 {
            let (c, m) = generate(4, seed);
            for t in &m.resource_types {
                let n = deposit_cells(&m, t.id).len() as u32;
                assert_eq!(
                    n,
                    c.cells_per_type[t.category as usize - 1],
                    "{t:?} seed {seed}"
                );
            }
        }
    }

    #[test]
    fn category_one_is_the_rarest() {
        let (_, m) = generate(4, 3);
        let mut totals = [0usize; 3];
        for cell in m.board.cells() {
            for &r in m.deposits_at(cell) {
                totals[m.category(r) as usize - 1] += 1;
            }
        }
        assert!(totals[0] < totals[1] && totals[1] < totals[2], "{totals:?}");
    }

    #[test]
    fn start_cells_stay_empty() {
        for seed in 0..30 {
            let (_, m) = generate(4, seed);
            for &corner in m.board.corners() {
                assert!(m.deposits_at(corner).is_empty(), "seed {seed}");
            }
        }
    }

    #[test]
    fn cell_capacity_is_respected() {
        for seed in 0..30 {
            let (c, m) = generate(4, seed);
            assert!(m
                .deposits
                .iter()
                .all(|d| d.len() <= c.max_resources_per_cell));
        }
    }

    #[test]
    fn no_cell_lists_the_same_type_twice() {
        for seed in 0..30 {
            let (_, m) = generate(4, seed);
            for d in &m.deposits {
                let mut sorted = d.clone();
                sorted.dedup();
                assert_eq!(&sorted, d);
            }
        }
    }

    #[test]
    fn first_cat1_deposit_is_one_or_two_steps_from_the_start_for_every_corner() {
        for seed in 0..50 {
            let (c, m) = generate(4, seed);
            for s in m.corner_stats(&c) {
                let d = s.cat1_first_distance.expect("Kat 1 vorhanden");
                assert!((1..=2).contains(&d), "seed {seed}, Ecke {}: {d}", s.corner);
            }
        }
    }

    #[test]
    fn cat1_never_leaves_its_home_territory() {
        for seed in 0..50 {
            let (_, m) = generate(4, seed);
            for t in m.resource_types.iter().filter(|t| t.category == 1) {
                for cell in deposit_cells(&m, t.id) {
                    assert_eq!(
                        m.territories[cell.idx()],
                        Some(t.home_corner),
                        "seed {seed}"
                    );
                }
            }
        }
    }

    #[test]
    fn without_strays_everything_lies_in_its_home_territory() {
        for seed in 0..20 {
            let mut c = cfg(4, seed);
            c.stray_chance = 0.0;
            let m = Map::generate(&c).unwrap();
            for t in &m.resource_types {
                for cell in deposit_cells(&m, t.id) {
                    assert_eq!(m.territories[cell.idx()], Some(t.home_corner));
                }
            }
        }
    }

    #[test]
    fn with_certain_strays_some_deposits_lie_outside_their_territory() {
        let mut outside = 0;
        for seed in 0..10 {
            let mut c = cfg(4, seed);
            c.stray_chance = 1.0;
            let m = Map::generate(&c).unwrap();
            for t in m.resource_types.iter().filter(|t| t.category != 1) {
                outside += deposit_cells(&m, t.id)
                    .iter()
                    .filter(|cell| m.territories[cell.idx()] != Some(t.home_corner))
                    .count();
            }
        }
        assert!(outside > 20, "zu wenige verstreute Vorkommen: {outside}");
    }

    fn connected(map: &Map, cells: &[CellId]) -> bool {
        if cells.is_empty() {
            return true;
        }
        let mut seen = vec![cells[0]];
        let mut changed = true;
        while changed {
            changed = false;
            for &c in cells {
                if !seen.contains(&c)
                    && seen.iter().any(|&s| map.board.neighbors(s).any(|n| n == c))
                {
                    seen.push(c);
                    changed = true;
                }
            }
        }
        seen.len() == cells.len()
    }

    #[test]
    fn veins_are_connected_when_there_are_no_strays() {
        for seed in 0..20 {
            let mut c = cfg(4, seed);
            c.stray_chance = 0.0;
            c.cells_per_type = [3, 3, 4];
            let m = Map::generate(&c).unwrap();
            for t in &m.resource_types {
                assert!(connected(&m, &deposit_cells(&m, t.id)), "{t:?} seed {seed}");
            }
        }
    }

    #[test]
    fn same_seed_gives_the_same_map_and_other_seeds_differ() {
        let (_, a) = generate(4, 99);
        let (_, b) = generate(4, 99);
        assert_eq!(a, b);
        let distinct: std::collections::BTreeSet<Vec<Vec<ResourceId>>> =
            (0..10).map(|s| generate(4, s).1.deposits).collect();
        assert!(distinct.len() >= 9, "Karten sollten sich unterscheiden");
    }

    #[test]
    fn two_players_use_opposite_corners_and_leave_two_corners_free() {
        let (_, m) = generate(2, 5);
        assert_eq!(m.player_corners, vec![0, 2]);
        assert_eq!(m.free_corners(), vec![1, 3]);
        assert_eq!(m.start_cell(PlayerId(0)), m.board.cell_at(0, 0).unwrap());
        assert_eq!(m.start_cell(PlayerId(1)), m.board.cell_at(9, 9).unwrap());
        // Auch die freien Ecken haben ihren Kat-1-Typ nahe am Start.
        for corner in m.free_corners() {
            let ty = m.cat1_type_of_corner(corner);
            let start = m.board.corners()[corner];
            let d = deposit_cells(&m, ty)
                .iter()
                .map(|&c| m.board.distance(start, c))
                .min()
                .unwrap();
            assert!((1..=2).contains(&d));
        }
        assert_eq!(m.resource_types.len(), 24, "2 Spieler: Karte wie bei 4");
    }

    #[test]
    fn generation_succeeds_across_many_seeds_and_sizes() {
        for side in [6u32, 8, 10, 12, 16] {
            for seed in 0..60 {
                let mut c = cfg(4, seed);
                c.side = side;
                let m =
                    Map::generate(&c).unwrap_or_else(|e| panic!("side {side} seed {seed}: {e}"));
                assert!(m.fairness_issues(&c).is_empty());
            }
        }
    }

    #[test]
    fn generator_needs_few_attempts_on_the_default_map() {
        let mut histogram: BTreeMap<u32, u32> = BTreeMap::new();
        for seed in 0..200 {
            let (_, m) = generate(4, seed);
            *histogram.entry(m.attempts_used).or_default() += 1;
        }
        let worst = *histogram.keys().max().unwrap();
        assert!(
            worst <= 100,
            "Fairness zu streng eingestellt? {histogram:?}"
        );
        let first_try = histogram.get(&1).copied().unwrap_or(0);
        assert!(
            first_try >= 20,
            "weniger als 10 Prozent im ersten Versuch: {histogram:?}"
        );
    }

    #[test]
    fn fairness_checker_flags_a_missing_or_distant_cat1_deposit() {
        let (c, mut m) = generate(4, 7);
        let ty = m.cat1_type_of_corner(1);
        for d in &mut m.deposits {
            d.retain(|&r| r != ty);
        }
        let issues = m.fairness_issues(&c);
        assert!(
            issues.contains(&FairnessIssue::Cat1TooFarOrMissing {
                corner: 1,
                distance: None
            }),
            "{issues:?}"
        );
    }

    #[test]
    fn fairness_checker_flags_uneven_resource_density() {
        let (c, mut m) = generate(4, 7);
        // Alle Kat-3-Vorkommen im Umkreis der Ecke 0 verdoppeln.
        let start = m.board.corners()[0];
        let near: Vec<CellId> = m
            .board
            .cells()
            .filter(|&cell| m.board.distance(start, cell) <= c.fairness_radius)
            .collect();
        let extra: Vec<ResourceId> = m
            .resource_types
            .iter()
            .filter(|t| t.category == 3)
            .map(|t| t.id)
            .collect();
        for (i, cell) in near.iter().enumerate() {
            if *cell != start {
                m.deposits[cell.idx()] = vec![extra[i % extra.len()]];
            }
        }
        for cell in near.iter().filter(|&&c| c != start).take(12) {
            m.deposits[cell.idx()] = vec![extra[0], extra[1]];
        }
        let issues = m.fairness_issues(&c);
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, FairnessIssue::NearCountsUneven { category: 3, .. })),
            "{issues:?}"
        );
    }

    #[test]
    fn ascii_rendering_has_one_line_per_row_and_marks_the_corners() {
        let (_, m) = generate(4, 2);
        let text = m.render_ascii();
        assert_eq!(text.lines().count(), 10);
        assert!(text.lines().all(|l| l.chars().count() >= 30));
        for tag in ["S0", "S1", "S2", "S3"] {
            assert_eq!(text.matches(tag).count(), 1, "{tag}");
        }
    }

    /// Entwicklerwerkzeug: `cargo test -p game_core show_sample_maps -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn show_sample_maps() {
        for seed in [1u64, 2, 3] {
            let (c, m) = generate(4, seed);
            println!("Seed {seed}, Versuche: {}", m.attempts_used);
            print!("{}", m.render_ascii());
            for s in m.corner_stats(&c) {
                println!("  Ecke {}: {:?}", s.corner, s);
            }
        }
        let mut hist: BTreeMap<u32, u32> = BTreeMap::new();
        for seed in 0..500 {
            *hist.entry(generate(4, seed).1.attempts_used).or_default() += 1;
        }
        println!("Versuche bis zur fairen Karte (500 Seeds): {hist:?}");
    }

    #[test]
    fn invalid_configs_are_reported_not_panicked() {
        assert!(matches!(
            Map::generate(&cfg(3, 1)),
            Err(MapError::Config(ConfigError::UnsupportedPlayers(3)))
        ));
        let mut c = cfg(2, 1);
        c.side = 3;
        assert!(matches!(Map::generate(&c), Err(MapError::Config(_))));
    }

    #[test]
    fn impossible_density_fails_cleanly_instead_of_looping_forever() {
        let mut c = cfg(4, 1);
        c.side = 6;
        c.cells_per_type = [5, 9, 9];
        c.max_resources_per_cell = 1;
        c.max_generator_attempts = 20;
        assert_eq!(
            Map::generate(&c),
            Err(MapError::GenerationFailed { attempts: 20 })
        );
    }
}
