//! Textansicht eines Spielstands (fuer die CLI und zum Nachvollziehen von Tests).
//!
//! Jedes Feld ist 3 Zeichen breit:
//! - `A*` Koenig von Spieler A, `a2` Figur von Spieler A mit Stufe 2
//! - `#A` Fabrik von Spieler A (ohne Figur darauf)
//! - `$` Beute
//! - Ziffern: Kategorien der Rohstoffe auf dem Feld, `.` leer

use crate::game::Game;
use crate::ids::PlayerId;
use crate::state::PieceKind;

pub fn player_letter(p: PlayerId) -> char {
    (b'A' + p.0) as char
}

pub fn render_game(game: &Game) -> String {
    let board = &game.map.board;
    let side = board.side() as i32;
    let mut out = String::new();
    for y in 0..side {
        for x in 0..side {
            let cell = board.cell_at(x, y).expect("Feld liegt auf dem Brett");
            let text = if let Some(piece) = game.state.piece_at(cell) {
                match &piece.kind {
                    PieceKind::King => format!("{}*", player_letter(piece.owner)),
                    PieceKind::Unit { level, .. } => format!(
                        "{}{}",
                        player_letter(piece.owner).to_ascii_lowercase(),
                        level
                    ),
                }
            } else if let Some(f) = game.state.factory_at(cell) {
                format!("#{}", player_letter(f.owner))
            } else if game.state.loot.contains_key(&cell) {
                "$".to_string()
            } else if game.map.deposits_at(cell).is_empty() {
                ".".to_string()
            } else {
                let mut cats: Vec<u8> = game
                    .map
                    .deposits_at(cell)
                    .iter()
                    .map(|&r| game.map.category(r))
                    .collect();
                cats.sort();
                cats.iter().map(|c| c.to_string()).collect()
            };
            out.push_str(&format!("{text:>3}"));
        }
        out.push('\n');
    }
    out
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
        g.setup_unit(PlayerId(1), u, 3, 1);
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
    fn player_letters_start_at_a() {
        assert_eq!(player_letter(PlayerId(0)), 'A');
        assert_eq!(player_letter(PlayerId(3)), 'D');
    }
}
