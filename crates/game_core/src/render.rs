//! Textansicht eines Spielstands (fuer die CLI und zum Nachvollziehen von Tests).
//!
//! Jedes Feld ist 3 Zeichen breit:
//! - `A*` Koenig von Spieler A, `a2` Figur von Spieler A mit Stufe 2
//! - `#A` Fabrik von Spieler A (ohne Figur darauf)
//! - `_A` Land von Spieler A (nur im Gebietsmodus)
//! - `$` Beute
//! - Ziffern: Kategorien der Rohstoffe auf dem Feld, `.` leer

use crate::board::{Board, GridKind};
use crate::game::Game;
use crate::ids::{CellId, PlayerId};
use crate::state::PieceKind;

pub fn player_letter(p: PlayerId) -> char {
    (b'A' + p.0) as char
}

/// Ordnet Feldbeschriftungen (je hoechstens 3 Zeichen) wie auf dem Brett an.
/// Quadrat: Zeilen und Spalten. Dreieck und Hex: nach Weltkoordinaten versetzt.
pub fn board_text(board: &Board, text: impl Fn(CellId) -> String) -> String {
    if board.kind() == GridKind::Square {
        let side = board.side() as i32;
        let mut out = String::new();
        for y in 0..side {
            for x in 0..side {
                let cell = board.cell_at(x, y).expect("Feld liegt auf dem Brett");
                out.push_str(&format!("{:>3}", text(cell)));
            }
            out.push('\n');
        }
        return out;
    }
    let row_height = 0.866_025_403_784_438_6;
    // Zeichen je Welteinheit: Nachbarn in einer Reihe liegen beim Hex 1, beim Dreieck 0,5 auseinander.
    let chars_per_unit = if board.kind() == GridKind::Hex {
        4.0
    } else {
        6.0
    };
    let placed: Vec<(i64, i64, String)> = board
        .cells()
        .map(|cell| {
            let (x, y) = board.pos(cell);
            let row = if board.kind() == GridKind::Hex {
                (y / row_height).round() as i64
            } else {
                (y / row_height).floor() as i64
            };
            ((x * chars_per_unit).round() as i64, row, text(cell))
        })
        .collect();
    let min_col = placed.iter().map(|p| p.0).min().unwrap_or(0);
    let min_row = placed.iter().map(|p| p.1).min().unwrap_or(0);
    let max_row = placed.iter().map(|p| p.1).max().unwrap_or(0);
    let width = (placed.iter().map(|p| p.0).max().unwrap_or(0) - min_col + 4) as usize;
    let mut rows = vec![vec![' '; width]; (max_row - min_row + 1) as usize];
    for (col, row, label) in placed {
        let label: String = format!("{label:>3}")
            .chars()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let start = (col - min_col) as usize;
        for (i, ch) in label.chars().enumerate() {
            rows[(row - min_row) as usize][start + i] = ch;
        }
    }
    let mut out = String::new();
    for row in rows {
        out.push_str(row.iter().collect::<String>().trim_end());
        out.push('\n');
    }
    out
}

pub fn render_game(game: &Game) -> String {
    board_text(&game.map.board, |cell| {
        if let Some(piece) = game.state.piece_at(cell) {
            match &piece.kind {
                PieceKind::King => format!("{}*", player_letter(piece.owner)),
                PieceKind::Unit { level, .. } => {
                    format!(
                        "{}{}",
                        player_letter(piece.owner).to_ascii_lowercase(),
                        level
                    )
                }
            }
        } else if let Some(f) = game.state.factory_at(cell) {
            format!("#{}", player_letter(f.owner))
        } else if game.state.loot.contains_key(&cell) {
            "$".to_string()
        } else if let Some(owner) = game.state.land[cell.idx()] {
            format!("_{}", player_letter(owner))
        } else if game.available_deposits(cell).is_empty() {
            ".".to_string()
        } else {
            let mut cats: Vec<u8> = game
                .available_deposits(cell)
                .iter()
                .map(|&r| game.map.category(r))
                .collect();
            cats.sort();
            cats.iter().map(|c| c.to_string()).collect()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn shows_kings_units_factories_and_loot() {
        let mut g = Game::new(Config::new(2, 4)).unwrap();
        let cell = |g: &Game, x, y| g.map.board.cell_at(x, y).unwrap();
        let u = cell(&g, 4, 4);
        g.setup_unit(PlayerId(1), u, 3, 3);
        let f = cell(&g, 6, 6);
        g.setup_factory(PlayerId(0), f);
        let l = cell(&g, 2, 7);
        g.state.loot.insert(l, crate::state::Inventory::new());
        let text = render_game(&g);
        let rows: Vec<&str> = text.lines().collect();
        assert_eq!(rows.len(), 10);
        assert_eq!(&rows[0][..3], " A*");
        assert_eq!(&rows[9][27..30], " B*");
        assert_eq!(&rows[4][12..15], " b3");
        assert_eq!(&rows[6][18..21], " #A");
        assert_eq!(&rows[7][6..9], "  $");
    }

    #[test]
    fn hex_and_triangle_games_render_every_king() {
        for players in [3u8, 5, 8] {
            let g = Game::new(Config::new(players, 3)).unwrap();
            let text = render_game(&g);
            for p in 0..players {
                assert!(
                    text.contains(&format!("{}*", player_letter(PlayerId(p)))),
                    "{players}: {text}"
                );
            }
        }
    }

    #[test]
    fn player_letters_start_at_a() {
        assert_eq!(player_letter(PlayerId(0)), 'A');
        assert_eq!(player_letter(PlayerId(3)), 'D');
    }
}
