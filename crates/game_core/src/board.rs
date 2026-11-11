//! Spielbrett: Felder, Nachbarschaft, Richtungen, Abstaende, Gebiete der Startecken.
//!
//! Meilenstein 1 kennt nur das Quadratgitter (2 und 4 Spieler). Dreiecks- und Hexgitter
//! folgen; das Brett ist so angelegt, dass sie dieselbe Schnittstelle (`step`, `neighbors`,
//! `corners`) bedienen.

use crate::ids::CellId;
use std::collections::VecDeque;

pub const MAX_SQUARE_SIDE: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridKind {
    Square,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardError {
    /// Seitenlaenge ausserhalb von `2..=MAX_SQUARE_SIDE`.
    SideOutOfRange(u32),
}

impl std::fmt::Display for BoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardError::SideOutOfRange(s) => {
                write!(f, "Seitenlaenge {s} ausserhalb von 2..={MAX_SQUARE_SIDE}")
            }
        }
    }
}

impl std::error::Error for BoardError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    kind: GridKind,
    side: u32,
    dirs: usize,
    coords: Vec<(i32, i32)>,
    /// `steps[zelle][richtung]` = Nachbarfeld in dieser Richtung, falls vorhanden.
    steps: Vec<Vec<Option<CellId>>>,
    corners: Vec<CellId>,
}

impl Board {
    /// Quadratgitter mit `side` x `side` Feldern. `side` ist zugleich die Kantenlaenge
    /// zwischen den Startecken benachbarter Spieler (Standard: 10).
    ///
    /// Richtungen: 0 = +x, 1 = +y, 2 = -x, 3 = -y. Die Gegenrichtung von `d` ist `(d + 2) % 4`.
    /// Ecken im Uhrzeigersinn: links oben, rechts oben, rechts unten, links unten.
    pub fn square(side: u32) -> Result<Board, BoardError> {
        if !(2..=MAX_SQUARE_SIDE).contains(&side) {
            return Err(BoardError::SideOutOfRange(side));
        }
        let s = side as i32;
        let mut coords = Vec::with_capacity((side * side) as usize);
        for y in 0..s {
            for x in 0..s {
                coords.push((x, y));
            }
        }
        let id = |x: i32, y: i32| CellId((y * s + x) as u32);
        let deltas = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        let steps = coords
            .iter()
            .map(|&(x, y)| {
                deltas
                    .iter()
                    .map(|&(dx, dy)| {
                        let (nx, ny) = (x + dx, y + dy);
                        if (0..s).contains(&nx) && (0..s).contains(&ny) {
                            Some(id(nx, ny))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .collect();
        let corners = vec![id(0, 0), id(s - 1, 0), id(s - 1, s - 1), id(0, s - 1)];
        Ok(Board {
            kind: GridKind::Square,
            side,
            dirs: 4,
            coords,
            steps,
            corners,
        })
    }

    pub fn kind(&self) -> GridKind {
        self.kind
    }

    pub fn side(&self) -> u32 {
        self.side
    }

    pub fn len(&self) -> usize {
        self.coords.len()
    }

    pub fn is_empty(&self) -> bool {
        self.coords.is_empty()
    }

    pub fn cells(&self) -> impl Iterator<Item = CellId> {
        (0..self.coords.len() as u32).map(CellId)
    }

    pub fn dir_count(&self) -> usize {
        self.dirs
    }

    /// Nachbarfeld in Richtung `dir`, oder `None` am Rand.
    pub fn step(&self, cell: CellId, dir: usize) -> Option<CellId> {
        self.steps[cell.idx()].get(dir).copied().flatten()
    }

    /// Alle Nachbarfelder (gemeinsame Kante), in Richtungsreihenfolge.
    pub fn neighbors(&self, cell: CellId) -> impl Iterator<Item = CellId> + '_ {
        self.steps[cell.idx()].iter().flatten().copied()
    }

    /// Bis zu `max_len` Felder in gerader Linie ab `from` (ohne `from`), bis zum Rand.
    pub fn line(&self, from: CellId, dir: usize, max_len: usize) -> Vec<CellId> {
        let mut out = Vec::new();
        let mut cur = from;
        while out.len() < max_len {
            match self.step(cur, dir) {
                Some(next) => {
                    out.push(next);
                    cur = next;
                }
                None => break,
            }
        }
        out
    }

    pub fn coord(&self, cell: CellId) -> (i32, i32) {
        self.coords[cell.idx()]
    }

    pub fn cell_at(&self, x: i32, y: i32) -> Option<CellId> {
        let s = self.side as i32;
        if (0..s).contains(&x) && (0..s).contains(&y) {
            Some(CellId((y * s + x) as u32))
        } else {
            None
        }
    }

    /// Startecken im Uhrzeigersinn. Bei 2 Spielern gehoeren die Ecken 0 und 2 den Spielern,
    /// die Ecken 1 und 3 bleiben frei.
    pub fn corners(&self) -> &[CellId] {
        &self.corners
    }

    /// Kuerzeste Schrittzahl von `src` zu allen Feldern (`u32::MAX` = nicht erreichbar).
    pub fn distances_from(&self, src: CellId) -> Vec<u32> {
        let mut dist = vec![u32::MAX; self.len()];
        let mut queue = VecDeque::new();
        dist[src.idx()] = 0;
        queue.push_back(src);
        while let Some(cur) = queue.pop_front() {
            let d = dist[cur.idx()];
            for n in self.neighbors(cur) {
                if dist[n.idx()] == u32::MAX {
                    dist[n.idx()] = d + 1;
                    queue.push_back(n);
                }
            }
        }
        dist
    }

    pub fn distance(&self, a: CellId, b: CellId) -> u32 {
        self.distances_from(a)[b.idx()]
    }

    /// Gebiet jeder Startecke: Jedes Feld gehoert der geometrisch naechsten Ecke
    /// (Index in `corners()`). Liegt ein Feld gleich weit von mehreren Ecken, ist es
    /// neutral (`None`), das ist die Mitte zwischen den Gebieten.
    pub fn territories(&self) -> Vec<Option<usize>> {
        self.cells()
            .map(|cell| {
                let (x, y) = self.coord(cell);
                let mut best: Option<(i64, usize)> = None;
                let mut tie = false;
                for (i, corner) in self.corners.iter().enumerate() {
                    let (cx, cy) = self.coord(*corner);
                    let d = ((x - cx) as i64).pow(2) + ((y - cy) as i64).pow(2);
                    match best {
                        None => best = Some((d, i)),
                        Some((bd, _)) if d < bd => {
                            best = Some((d, i));
                            tie = false;
                        }
                        Some((bd, _)) if d == bd => tie = true,
                        _ => {}
                    }
                }
                if tie {
                    None
                } else {
                    best.map(|(_, i)| i)
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board(side: u32) -> Board {
        Board::square(side).unwrap()
    }

    #[test]
    fn rejects_sides_outside_the_supported_range() {
        assert_eq!(Board::square(0), Err(BoardError::SideOutOfRange(0)));
        assert_eq!(Board::square(1), Err(BoardError::SideOutOfRange(1)));
        assert_eq!(
            Board::square(MAX_SQUARE_SIDE + 1),
            Err(BoardError::SideOutOfRange(MAX_SQUARE_SIDE + 1))
        );
        assert!(Board::square(2).is_ok());
        assert!(Board::square(MAX_SQUARE_SIDE).is_ok());
    }

    #[test]
    fn default_board_has_100_cells() {
        let b = board(10);
        assert_eq!(b.len(), 100);
        assert_eq!(b.cells().count(), 100);
        assert_eq!(b.dir_count(), 4);
        assert_eq!(b.kind(), GridKind::Square);
    }

    #[test]
    fn neighbor_counts_are_two_at_corners_three_at_edges_four_inside() {
        let b = board(10);
        let (mut two, mut three, mut four) = (0, 0, 0);
        for c in b.cells() {
            match b.neighbors(c).count() {
                2 => two += 1,
                3 => three += 1,
                4 => four += 1,
                other => panic!("unerwartete Nachbarzahl {other}"),
            }
        }
        assert_eq!((two, three, four), (4, 4 * 8, 8 * 8));
    }

    #[test]
    fn neighborhood_is_symmetric() {
        let b = board(7);
        for a in b.cells() {
            for n in b.neighbors(a) {
                assert!(b.neighbors(n).any(|back| back == a), "{a:?} <-> {n:?}");
            }
        }
    }

    #[test]
    fn opposite_direction_leads_back() {
        let b = board(6);
        for c in b.cells() {
            for d in 0..b.dir_count() {
                if let Some(next) = b.step(c, d) {
                    assert_eq!(b.step(next, (d + 2) % 4), Some(c));
                }
            }
        }
    }

    #[test]
    fn step_outside_the_board_is_none() {
        let b = board(5);
        let corner = b.cell_at(0, 0).unwrap();
        assert_eq!(b.step(corner, 2), None);
        assert_eq!(b.step(corner, 3), None);
        assert!(b.step(corner, 0).is_some());
        assert_eq!(
            b.step(corner, 99),
            None,
            "ungueltige Richtung ist kein Absturz"
        );
    }

    #[test]
    fn coordinates_and_cell_at_round_trip() {
        let b = board(8);
        for c in b.cells() {
            let (x, y) = b.coord(c);
            assert_eq!(b.cell_at(x, y), Some(c));
        }
        assert_eq!(b.cell_at(-1, 0), None);
        assert_eq!(b.cell_at(0, 8), None);
    }

    #[test]
    fn corners_are_in_clockwise_order() {
        let b = board(10);
        let coords: Vec<_> = b.corners().iter().map(|&c| b.coord(c)).collect();
        assert_eq!(coords, vec![(0, 0), (9, 0), (9, 9), (0, 9)]);
    }

    #[test]
    fn distances_match_manhattan_distance() {
        let b = board(10);
        let a = b.cell_at(0, 0).unwrap();
        let z = b.cell_at(9, 9).unwrap();
        assert_eq!(b.distance(a, z), 18);
        assert_eq!(b.distance(a, a), 0);
        for c in b.cells() {
            let (x, y) = b.coord(c);
            assert_eq!(b.distance(a, c), (x + y) as u32);
        }
    }

    #[test]
    fn distance_is_symmetric_and_everything_is_reachable() {
        let b = board(6);
        for a in b.cells() {
            let da = b.distances_from(a);
            assert!(
                da.iter().all(|&d| d != u32::MAX),
                "Brett nicht zusammenhaengend"
            );
            for c in b.cells() {
                assert_eq!(da[c.idx()], b.distance(c, a));
            }
        }
    }

    #[test]
    fn line_stops_at_the_edge_and_respects_the_limit() {
        let b = board(10);
        let start = b.cell_at(7, 3).unwrap();
        let east = b.line(start, 0, 5);
        let coords: Vec<_> = east.iter().map(|&c| b.coord(c)).collect();
        assert_eq!(coords, vec![(8, 3), (9, 3)], "am Rand abgeschnitten");
        assert_eq!(b.line(start, 2, 3).len(), 3);
        assert_eq!(b.line(start, 2, 0).len(), 0);
    }

    #[test]
    fn four_corner_territories_are_equally_large_on_even_boards() {
        let b = board(10);
        let t = b.territories();
        let mut counts = [0; 4];
        for owner in &t {
            counts[owner.expect("gerade Seitenlaenge hat keine neutralen Felder")] += 1;
        }
        assert_eq!(counts, [25, 25, 25, 25]);
    }

    #[test]
    fn each_corner_lies_in_its_own_territory() {
        let b = board(10);
        let t = b.territories();
        for (i, &corner) in b.corners().iter().enumerate() {
            assert_eq!(t[corner.idx()], Some(i));
        }
    }

    #[test]
    fn odd_boards_have_a_neutral_middle_but_equal_territories() {
        let b = board(9);
        let t = b.territories();
        let neutral = t.iter().filter(|o| o.is_none()).count();
        assert!(neutral > 0, "ungerade Seitenlaenge hat eine neutrale Mitte");
        let mut counts = [0; 4];
        for owner in t.iter().flatten() {
            counts[*owner] += 1;
        }
        assert!(counts.iter().all(|&c| c == counts[0]), "{counts:?}");
        // Das Zentrumsfeld ist von allen Ecken gleich weit entfernt.
        let center = b.cell_at(4, 4).unwrap();
        assert_eq!(t[center.idx()], None);
    }

    #[test]
    fn territories_are_rotationally_symmetric() {
        let b = board(10);
        let t = b.territories();
        let s = 10;
        for c in b.cells() {
            let (x, y) = b.coord(c);
            // 90 Grad im Uhrzeigersinn: (x, y) -> (s-1-y, x); die Ecke wandert um einen Index weiter.
            let rotated = b.cell_at(s - 1 - y, x).unwrap();
            let owner = t[c.idx()].unwrap();
            assert_eq!(t[rotated.idx()], Some((owner + 1) % 4));
        }
    }
}
