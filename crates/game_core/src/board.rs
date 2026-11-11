//! Spielbrett: Felder, Nachbarschaft, gerade Linien, Abstaende, Gebiete der Startecken.
//!
//! Drei Gitter, alle mit derselben Schnittstelle:
//! - `Square`: Quadrat aus Quadraten (2 und 4 Spieler)
//! - `Triangle`: gleichseitiges Dreieck aus Dreiecken (3 Spieler)
//! - `Hex`: regelmaessiges Vieleck aus Sechsecken (5 bis 8 Spieler)
//!
//! Nachbarn teilen eine Kante. `step(feld, richtung)` geht ein Feld in gerader Linie weiter;
//! im Dreiecksgitter folgt die gerade Linie einer Feldreihe (Zickzack zwischen Spitze-oben- und
//! Spitze-unten-Dreiecken). Weltkoordinaten (`pos`, `shape`) dienen Gebietsaufteilung und Grafik:
//! benachbarte Feldmitten haben im Quadrat- und Hexgitter den Abstand 1, Dreiecke die Kante 1.

use crate::ids::CellId;
use std::collections::{BTreeMap, VecDeque};

pub const MAX_SQUARE_SIDE: u32 = 64;
pub const MAX_TRIANGLE_SIDE: u32 = 40;
pub const MAX_HEX_SIDE: u32 = 30;

const SQRT3_2: f64 = 0.866_025_403_784_438_6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridKind {
    Square,
    Triangle,
    Hex,
}

impl GridKind {
    pub fn name(self) -> &'static str {
        match self {
            GridKind::Square => "square",
            GridKind::Triangle => "triangle",
            GridKind::Hex => "hex",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BoardError {
    /// Seitenlaenge ausserhalb des erlaubten Bereichs.
    SideOutOfRange(u32),
    /// Vieleck mit dieser Eckenzahl wird nicht unterstuetzt.
    CornerCount(usize),
}

impl std::fmt::Display for BoardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardError::SideOutOfRange(s) => {
                write!(f, "Seitenlaenge {s} ist fuer dieses Brett nicht erlaubt")
            }
            BoardError::CornerCount(n) => {
                write!(f, "ein Vieleck mit {n} Ecken wird nicht unterstuetzt")
            }
        }
    }
}

impl std::error::Error for BoardError {}

#[derive(Clone, Debug, PartialEq)]
pub struct Board {
    kind: GridKind,
    side: u32,
    /// Gitterkoordinaten: Quadrat (x, y), Dreieck (Reihe, Index in der Reihe), Hex axial (q, r).
    coords: Vec<(i32, i32)>,
    pos: Vec<(f64, f64)>,
    shapes: Vec<Vec<(f64, f64)>>,
    neighbors: Vec<Vec<CellId>>,
    /// `lines[feld][richtung]` = naechstes Feld in gerader Linie.
    lines: Vec<Vec<Option<CellId>>>,
    corners: Vec<CellId>,
    outline: Vec<(f64, f64)>,
    lookup: BTreeMap<(i32, i32), CellId>,
}

struct Raw {
    coords: Vec<(i32, i32)>,
    pos: Vec<(f64, f64)>,
    shapes: Vec<Vec<(f64, f64)>>,
    neighbors: Vec<Vec<CellId>>,
    lines: Vec<Vec<Option<CellId>>>,
    corners: Vec<CellId>,
    outline: Vec<(f64, f64)>,
}

impl Board {
    fn finish(kind: GridKind, side: u32, raw: Raw) -> Board {
        let lookup = raw
            .coords
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, CellId(i as u32)))
            .collect();
        Board {
            kind,
            side,
            coords: raw.coords,
            pos: raw.pos,
            shapes: raw.shapes,
            neighbors: raw.neighbors,
            lines: raw.lines,
            corners: raw.corners,
            outline: raw.outline,
            lookup,
        }
    }

    /// Quadratgitter mit `side` x `side` Feldern (2 und 4 Spieler).
    ///
    /// Richtungen: 0 = +x, 1 = +y, 2 = -x, 3 = -y. Ecken im Uhrzeigersinn ab links oben.
    pub fn square(side: u32) -> Result<Board, BoardError> {
        if !(2..=MAX_SQUARE_SIDE).contains(&side) {
            return Err(BoardError::SideOutOfRange(side));
        }
        let s = side as i32;
        let id = |x: i32, y: i32| CellId((y * s + x) as u32);
        let inside = |x: i32, y: i32| (0..s).contains(&x) && (0..s).contains(&y);
        let deltas = [(1, 0), (0, 1), (-1, 0), (0, -1)];
        let mut raw = Raw {
            coords: Vec::new(),
            pos: Vec::new(),
            shapes: Vec::new(),
            neighbors: Vec::new(),
            lines: Vec::new(),
            corners: vec![id(0, 0), id(s - 1, 0), id(s - 1, s - 1), id(0, s - 1)],
            outline: vec![
                (-0.5, -0.5),
                (s as f64 - 0.5, -0.5),
                (s as f64 - 0.5, s as f64 - 0.5),
                (-0.5, s as f64 - 0.5),
            ],
        };
        for y in 0..s {
            for x in 0..s {
                let (fx, fy) = (x as f64, y as f64);
                raw.coords.push((x, y));
                raw.pos.push((fx, fy));
                raw.shapes.push(vec![
                    (fx - 0.5, fy - 0.5),
                    (fx + 0.5, fy - 0.5),
                    (fx + 0.5, fy + 0.5),
                    (fx - 0.5, fy + 0.5),
                ]);
                let line: Vec<Option<CellId>> = deltas
                    .iter()
                    .map(|&(dx, dy)| inside(x + dx, y + dy).then(|| id(x + dx, y + dy)))
                    .collect();
                raw.neighbors.push(line.iter().flatten().copied().collect());
                raw.lines.push(line);
            }
        }
        Ok(Board::finish(GridKind::Square, side, raw))
    }

    /// Gleichseitiges Dreieck aus `side` Reihen kleiner Dreiecke (3 Spieler), `side`² Felder.
    ///
    /// Reihe `r` hat `2r + 1` Felder; gerade Indizes zeigen mit der Spitze nach oben.
    /// Richtungen (gerade Feldreihen): 0 = rechts, 1 = rechts unten, 2 = links unten,
    /// 3 = links, 4 = links oben, 5 = rechts oben. Ecken: Spitze, rechts unten, links unten.
    pub fn triangle(side: u32) -> Result<Board, BoardError> {
        if !(2..=MAX_TRIANGLE_SIDE).contains(&side) {
            return Err(BoardError::SideOutOfRange(side));
        }
        let n = side as i32;
        let index = |r: i32, k: i32| -> Option<CellId> {
            if r < 0 || r >= n || k < 0 || k > 2 * r {
                None
            } else {
                Some(CellId((r * r + k) as u32))
            }
        };
        let h = SQRT3_2;
        let mut raw = Raw {
            coords: Vec::new(),
            pos: Vec::new(),
            shapes: Vec::new(),
            neighbors: Vec::new(),
            lines: Vec::new(),
            corners: vec![
                index(0, 0).expect("Spitze"),
                index(n - 1, 2 * (n - 1)).expect("rechte Ecke"),
                index(n - 1, 0).expect("linke Ecke"),
            ],
            outline: vec![
                (0.5, 0.0),
                (0.5 + n as f64 / 2.0, n as f64 * h),
                (0.5 - n as f64 / 2.0, n as f64 * h),
            ],
        };
        for r in 0..n {
            for k in 0..=2 * r {
                let left = -(r as f64) / 2.0;
                let top = r as f64 * h;
                let up = k % 2 == 0;
                raw.coords.push((r, k));
                if up {
                    let i = (k / 2) as f64;
                    raw.pos.push((left + i + 0.5, top + 2.0 * h / 3.0));
                    raw.shapes.push(vec![
                        (left + i + 0.5, top),
                        (left + i + 1.0, top + h),
                        (left + i, top + h),
                    ]);
                } else {
                    let j = ((k - 1) / 2) as f64;
                    raw.pos.push((left + j + 1.0, top + h / 3.0));
                    raw.shapes.push(vec![
                        (left + j + 0.5, top),
                        (left + j + 1.5, top),
                        (left + j + 1.0, top + h),
                    ]);
                }
                let left_n = index(r, k - 1);
                let right_n = index(r, k + 1);
                let vertical = if up {
                    index(r + 1, k + 1)
                } else {
                    index(r - 1, k - 1)
                };
                raw.neighbors
                    .push([left_n, right_n, vertical].into_iter().flatten().collect());
                // Gerade Feldreihen (siehe Doku oben)
                let lines = if up {
                    vec![right_n, vertical, vertical, left_n, left_n, right_n]
                } else {
                    vec![right_n, right_n, left_n, left_n, vertical, vertical]
                };
                raw.lines.push(lines);
            }
        }
        Ok(Board::finish(GridKind::Triangle, side, raw))
    }

    /// Regelmaessiges Vieleck mit `corners` Ecken und Seitenlaenge `side`, gefuellt mit
    /// Sechsecken (5 bis 8 Spieler). Ecken im Uhrzeigersinn.
    ///
    /// Richtungen: 0 = 0 Grad, 1 = 60 Grad, ... 5 = 300 Grad (Bildschirm, y nach unten).
    pub fn hex_polygon(corners: usize, side: u32) -> Result<Board, BoardError> {
        if !(3..=12).contains(&corners) {
            return Err(BoardError::CornerCount(corners));
        }
        if !(2..=MAX_HEX_SIDE).contains(&side) {
            return Err(BoardError::SideOutOfRange(side));
        }
        let n = corners as f64;
        let radius = side as f64 / (2.0 * (std::f64::consts::PI / n).sin());
        // Beim Sechseck liegen die Ecken auf Gitterrichtungen, dann ist das Brett symmetrisch.
        let offset = if corners == 6 {
            0.0
        } else {
            -std::f64::consts::FRAC_PI_2
        };
        let outline: Vec<(f64, f64)> = (0..corners)
            .map(|k| {
                let a = offset + 2.0 * std::f64::consts::PI * k as f64 / n;
                (radius * a.cos(), radius * a.sin())
            })
            .collect();
        let inside = |p: (f64, f64)| {
            (0..corners).all(|k| {
                let a = outline[k];
                let b = outline[(k + 1) % corners];
                (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0) >= -1e-6
            })
        };
        let world = |q: i32, r: i32| (q as f64 + r as f64 / 2.0, r as f64 * SQRT3_2);
        let m = (2.0 * radius).ceil() as i32 + 2;
        let mut cells: Vec<(i32, i32)> = Vec::new();
        for r in -m..=m {
            for q in -m..=m {
                if inside(world(q, r)) {
                    cells.push((q, r));
                }
            }
        }
        let index: BTreeMap<(i32, i32), CellId> = cells
            .iter()
            .enumerate()
            .map(|(i, &c)| (c, CellId(i as u32)))
            .collect();
        let deltas = [(1, 0), (0, 1), (-1, 1), (-1, 0), (0, -1), (1, -1)];
        let hex_r = 1.0 / 3f64.sqrt();
        let mut raw = Raw {
            coords: Vec::new(),
            pos: Vec::new(),
            shapes: Vec::new(),
            neighbors: Vec::new(),
            lines: Vec::new(),
            corners: Vec::new(),
            outline: outline.clone(),
        };
        for &(q, r) in &cells {
            let (x, y) = world(q, r);
            raw.coords.push((q, r));
            raw.pos.push((x, y));
            raw.shapes.push(
                (0..6)
                    .map(|j| {
                        let a = std::f64::consts::PI / 6.0 + std::f64::consts::PI / 3.0 * j as f64;
                        (x + hex_r * a.cos(), y + hex_r * a.sin())
                    })
                    .collect(),
            );
            let line: Vec<Option<CellId>> = deltas
                .iter()
                .map(|&(dq, dr)| index.get(&(q + dq, r + dr)).copied())
                .collect();
            raw.neighbors.push(line.iter().flatten().copied().collect());
            raw.lines.push(line);
        }
        for v in &outline {
            let best = (0..raw.pos.len())
                .min_by(|&a, &b| {
                    let da = (raw.pos[a].0 - v.0).powi(2) + (raw.pos[a].1 - v.1).powi(2);
                    let db = (raw.pos[b].0 - v.0).powi(2) + (raw.pos[b].1 - v.1).powi(2);
                    da.total_cmp(&db).then(a.cmp(&b))
                })
                .expect("Brett hat Felder");
            let cell = CellId(best as u32);
            if raw.corners.contains(&cell) {
                return Err(BoardError::SideOutOfRange(side));
            }
            raw.corners.push(cell);
        }
        Ok(Board::finish(GridKind::Hex, side, raw))
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

    /// Anzahl der Richtungen fuer gerade Linien (Quadrat 4, Dreieck und Hex 6).
    pub fn dir_count(&self) -> usize {
        match self.kind {
            GridKind::Square => 4,
            GridKind::Triangle | GridKind::Hex => 6,
        }
    }

    /// Gegenrichtung.
    pub fn opposite(&self, dir: usize) -> usize {
        (dir + self.dir_count() / 2) % self.dir_count()
    }

    /// Naechstes Feld in gerader Linie, oder `None` am Rand.
    pub fn step(&self, cell: CellId, dir: usize) -> Option<CellId> {
        self.lines[cell.idx()].get(dir).copied().flatten()
    }

    /// Alle Nachbarfelder (gemeinsame Kante).
    pub fn neighbors(&self, cell: CellId) -> impl Iterator<Item = CellId> + '_ {
        self.neighbors[cell.idx()].iter().copied()
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

    pub fn cell_at(&self, a: i32, b: i32) -> Option<CellId> {
        self.lookup.get(&(a, b)).copied()
    }

    /// Mittelpunkt in Weltkoordinaten.
    pub fn pos(&self, cell: CellId) -> (f64, f64) {
        self.pos[cell.idx()]
    }

    /// Umriss des Feldes in Weltkoordinaten.
    pub fn shape(&self, cell: CellId) -> &[(f64, f64)] {
        &self.shapes[cell.idx()]
    }

    /// Umriss des ganzen Bretts (das Vieleck) in Weltkoordinaten.
    pub fn outline(&self) -> &[(f64, f64)] {
        &self.outline
    }

    /// Ist das Dreiecksfeld mit der Spitze nach oben? (Nur fuer Dreiecke sinnvoll.)
    pub fn points_up(&self, cell: CellId) -> bool {
        self.kind == GridKind::Triangle && self.coords[cell.idx()].1 % 2 == 0
    }

    /// Startecken im Uhrzeigersinn. Beim Quadrat fuer 2 Spieler gehoeren die Ecken 0 und 2
    /// den Spielern, die Ecken 1 und 3 bleiben frei.
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

    /// Gebiet jeder Startecke: Jedes Feld gehoert der naechsten Ecke des Vielecks (Luftlinie von
    /// der Feldmitte zum Eckpunkt; damit sind die Gebiete auch dann gleich gross, wenn das
    /// Gitter nicht genau auf die Ecken passt). Gleich weit von mehreren Ecken = neutral (`None`).
    pub fn territories(&self) -> Vec<Option<usize>> {
        self.cells()
            .map(|cell| {
                let (x, y) = self.pos(cell);
                let d: Vec<f64> = self
                    .outline
                    .iter()
                    .map(|&(cx, cy)| (x - cx).powi(2) + (y - cy).powi(2))
                    .collect();
                let best = d.iter().copied().fold(f64::INFINITY, f64::min);
                let winners: Vec<usize> = (0..d.len()).filter(|&i| d[i] - best < 1e-9).collect();
                (winners.len() == 1).then(|| winners[0])
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

    fn all_boards() -> Vec<Board> {
        let mut v = vec![board(10), Board::triangle(10).unwrap()];
        for n in 5..=8 {
            v.push(Board::hex_polygon(n, 10).unwrap());
        }
        v
    }

    // ----- allgemein, fuer alle Gitter ------------------------------------------------------

    #[test]
    fn neighborhood_is_symmetric_and_without_self_loops_on_every_grid() {
        for b in all_boards() {
            for a in b.cells() {
                for n in b.neighbors(a) {
                    assert_ne!(n, a);
                    assert!(
                        b.neighbors(n).any(|back| back == a),
                        "{:?}: {a:?} <-> {n:?}",
                        b.kind()
                    );
                }
            }
        }
    }

    #[test]
    fn every_board_is_connected() {
        for b in all_boards() {
            let d = b.distances_from(b.corners()[0]);
            assert!(d.iter().all(|&x| x != u32::MAX), "{:?}", b.kind());
        }
    }

    #[test]
    fn straight_steps_go_to_neighbors_and_the_opposite_direction_leads_back() {
        for b in all_boards() {
            for c in b.cells() {
                for d in 0..b.dir_count() {
                    if let Some(next) = b.step(c, d) {
                        assert!(b.neighbors(c).any(|n| n == next), "{:?}", b.kind());
                        assert_eq!(
                            b.step(next, b.opposite(d)),
                            Some(c),
                            "{:?} {c:?} {d}",
                            b.kind()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_neighbor_is_reachable_by_some_straight_direction() {
        for b in all_boards() {
            for c in b.cells() {
                for n in b.neighbors(c) {
                    assert!(
                        (0..b.dir_count()).any(|d| b.step(c, d) == Some(n)),
                        "{:?}",
                        b.kind()
                    );
                }
            }
        }
    }

    #[test]
    fn coordinates_and_cell_at_round_trip_on_every_grid() {
        for b in all_boards() {
            for c in b.cells() {
                let (p, q) = b.coord(c);
                assert_eq!(b.cell_at(p, q), Some(c));
            }
            assert_eq!(b.cell_at(9999, 9999), None);
        }
    }

    #[test]
    fn corners_are_distinct_and_lie_near_the_polygon_vertices() {
        for b in all_boards() {
            let mut seen = b.corners().to_vec();
            seen.sort();
            seen.dedup();
            assert_eq!(seen.len(), b.corners().len(), "{:?}", b.kind());
            assert_eq!(b.corners().len(), b.outline().len());
            for (corner, v) in b.corners().iter().zip(b.outline()) {
                let (x, y) = b.pos(*corner);
                let d = ((x - v.0).powi(2) + (y - v.1).powi(2)).sqrt();
                assert!(d < 1.0, "{:?}: Ecke {d} vom Eckpunkt entfernt", b.kind());
            }
        }
    }

    #[test]
    fn each_corner_lies_in_its_own_territory_and_territories_are_close_in_size() {
        for b in all_boards() {
            let t = b.territories();
            let mut counts = vec![0usize; b.corners().len()];
            for owner in t.iter().flatten() {
                counts[*owner] += 1;
            }
            for (i, &corner) in b.corners().iter().enumerate() {
                assert_eq!(t[corner.idx()], Some(i));
            }
            let (min, max) = (*counts.iter().min().unwrap(), *counts.iter().max().unwrap());
            assert!(
                max as f64 <= min as f64 * 1.25,
                "{:?} mit {} Ecken: {counts:?}",
                b.kind(),
                b.corners().len()
            );
        }
    }

    #[test]
    fn shapes_surround_their_centers() {
        for b in all_boards() {
            for c in b.cells() {
                let s = b.shape(c);
                let (cx, cy) = (
                    s.iter().map(|p| p.0).sum::<f64>() / s.len() as f64,
                    s.iter().map(|p| p.1).sum::<f64>() / s.len() as f64,
                );
                let (x, y) = b.pos(c);
                assert!(
                    (cx - x).abs() < 1e-9 && (cy - y).abs() < 1e-9,
                    "{:?}",
                    b.kind()
                );
            }
        }
    }

    // ----- Quadrat ------------------------------------------------------------------------

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
        assert!(Board::triangle(1).is_err());
        assert!(Board::hex_polygon(6, 1).is_err());
        assert_eq!(Board::hex_polygon(13, 10), Err(BoardError::CornerCount(13)));
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
    fn square_cell_ids_are_row_major() {
        let b = board(8);
        assert_eq!(b.cell_at(3, 2), Some(CellId(2 * 8 + 3)));
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
        let mut counts = [0; 4];
        for owner in &b.territories() {
            counts[owner.expect("gerade Seitenlaenge hat keine neutralen Felder")] += 1;
        }
        assert_eq!(counts, [25, 25, 25, 25]);
    }

    #[test]
    fn odd_boards_have_a_neutral_middle_but_equal_territories() {
        let b = board(9);
        let t = b.territories();
        assert!(t.iter().any(|o| o.is_none()));
        let mut counts = [0; 4];
        for owner in t.iter().flatten() {
            counts[*owner] += 1;
        }
        assert!(counts.iter().all(|&c| c == counts[0]), "{counts:?}");
        assert_eq!(t[b.cell_at(4, 4).unwrap().idx()], None);
    }

    #[test]
    fn territories_are_rotationally_symmetric() {
        let b = board(10);
        let t = b.territories();
        for c in b.cells() {
            let (x, y) = b.coord(c);
            let rotated = b.cell_at(9 - y, x).unwrap();
            assert_eq!(t[rotated.idx()], Some((t[c.idx()].unwrap() + 1) % 4));
        }
    }

    // ----- Dreieck ------------------------------------------------------------------------

    /// Reihen-Kennzahlen eines Dreiecks: (waagrecht, links-unten-Reihe, rechts-unten-Reihe).
    fn lanes(b: &Board, c: CellId) -> (i32, i32, i32) {
        let (r, k) = b.coord(c);
        if k % 2 == 0 {
            (r, k / 2, r - k / 2)
        } else {
            let j = (k - 1) / 2;
            (r, j, r - j - 1)
        }
    }

    #[test]
    fn triangle_board_has_side_squared_cells_and_three_corners() {
        let b = Board::triangle(10).unwrap();
        assert_eq!(b.len(), 100);
        assert_eq!(b.corners().len(), 3);
        assert_eq!(b.dir_count(), 6);
        let up = b.cells().filter(|&c| b.points_up(c)).count();
        assert_eq!(up, 55, "1 + 2 + ... + 10 Dreiecke mit Spitze oben");
    }

    #[test]
    fn triangles_have_one_to_three_neighbors_and_the_corners_only_one() {
        let b = Board::triangle(10).unwrap();
        for c in b.cells() {
            let n = b.neighbors(c).count();
            assert!((1..=3).contains(&n));
            if b.corners().contains(&c) {
                assert_eq!(n, 1);
            }
        }
        let inner = b.cells().filter(|&c| b.neighbors(c).count() == 3).count();
        assert_eq!(
            inner,
            100 - 3 - 3 * (10 - 2),
            "3 Ecken, 24 Randfelder mit 2 Nachbarn"
        );
    }

    #[test]
    fn triangle_lines_stay_in_their_row() {
        let b = Board::triangle(10).unwrap();
        for c in b.cells() {
            let here = lanes(&b, c);
            for d in 0..6 {
                if let Some(n) = b.step(c, d) {
                    let there = lanes(&b, n);
                    match d {
                        0 | 3 => assert_eq!(here.0, there.0, "waagrecht"),
                        2 | 5 => assert_eq!(here.1, there.1, "links unten / rechts oben"),
                        _ => assert_eq!(here.2, there.2, "rechts unten / links oben"),
                    }
                }
            }
        }
        // Eine Kante entlang: von der Spitze nach rechts unten bis zur rechten Ecke.
        let apex = b.corners()[0];
        let path = b.line(apex, 1, 100);
        assert_eq!(path.last(), Some(&b.corners()[1]));
        assert_eq!(path.len(), 18, "9 Reihen, je zwei Schritte");
    }

    #[test]
    fn the_three_corners_are_equally_far_apart() {
        let b = Board::triangle(10).unwrap();
        let c = b.corners();
        let d01 = b.distance(c[0], c[1]);
        assert_eq!(d01, b.distance(c[1], c[2]));
        assert_eq!(d01, b.distance(c[2], c[0]));
    }

    #[test]
    fn triangle_territories_are_equal() {
        let b = Board::triangle(10).unwrap();
        let mut counts = [0; 3];
        for o in b.territories().iter().flatten() {
            counts[*o] += 1;
        }
        assert!(counts.iter().all(|&c| c == counts[0]), "{counts:?}");
    }

    // ----- Hex ----------------------------------------------------------------------------

    #[test]
    fn the_hexagon_board_is_the_classic_hex_board() {
        let b = Board::hex_polygon(6, 10).unwrap();
        assert_eq!(b.len(), 3 * 10 * 10 + 3 * 10 + 1);
        let six = b.cells().filter(|&c| b.neighbors(c).count() == 6).count();
        assert_eq!(six, 3 * 9 * 9 + 3 * 9 + 1, "alles ausser dem Rand");
        let mut counts = [0; 6];
        for o in b.territories().iter().flatten() {
            counts[*o] += 1;
        }
        assert!(counts.iter().all(|&c| c == counts[0]), "{counts:?}");
    }

    #[test]
    fn hex_lines_are_straight_in_world_coordinates() {
        for n in 5..=8 {
            let b = Board::hex_polygon(n, 10).unwrap();
            for c in b.cells() {
                for d in 0..6 {
                    let path = b.line(c, d, 3);
                    if path.len() == 3 {
                        let p0 = b.pos(c);
                        let p1 = b.pos(path[0]);
                        let p3 = b.pos(path[2]);
                        assert!((p3.0 - p0.0 - 3.0 * (p1.0 - p0.0)).abs() < 1e-9);
                        assert!((p3.1 - p0.1 - 3.0 * (p1.1 - p0.1)).abs() < 1e-9);
                    }
                }
            }
        }
    }

    #[test]
    fn hex_neighbors_are_one_unit_apart() {
        let b = Board::hex_polygon(7, 8).unwrap();
        for c in b.cells() {
            assert!(b.neighbors(c).count() <= 6);
            for n in b.neighbors(c) {
                let (a, z) = (b.pos(c), b.pos(n));
                let d = ((a.0 - z.0).powi(2) + (a.1 - z.1).powi(2)).sqrt();
                assert!((d - 1.0).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn hex_polygons_grow_with_the_number_of_corners_and_neighboring_corners_are_about_side_apart() {
        let mut last = 0;
        for n in 5..=8 {
            let b = Board::hex_polygon(n, 10).unwrap();
            assert!(b.len() > last, "{n} Ecken: {} Felder", b.len());
            last = b.len();
            let c = b.corners();
            for i in 0..n {
                let d = b.distance(c[i], c[(i + 1) % n]);
                assert!((9..=12).contains(&d), "{n} Ecken: Abstand {d}");
            }
        }
    }
}
