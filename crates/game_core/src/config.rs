//! Alle einstellbaren Zahlen an einer Stelle (siehe Regelwerk, Abschnitt "Einstellbare Parameter").

use crate::board::{Board, BoardError, GridKind, MAX_HEX_SIDE, MAX_SQUARE_SIDE, MAX_TRIANGLE_SIDE};

/// Hoechste Zahl an Rohstoffkategorien (Kat 1 bis Kat 4).
pub const MAX_CATEGORIES: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// 2 bis 8 Spieler. 2 und 4: Quadrat, 3: Dreieck, 5 bis 8: Vieleck aus Hexfeldern.
    pub players: u8,
    /// Kantenlaenge der Karte in Feldern (Abstand der Startecken benachbarter Spieler).
    pub side: u32,
    pub seed: u64,
    /// Mindestabstand zwischen zwei Fabriken in Schritten (Regelwerk: 3, einstellbar 2 oder 3).
    pub min_factory_distance: u32,
    /// Kosten einer Fabrik in Kat-1-Rohstoffen.
    pub factory_cost: u32,
    /// Menge je Rohstoffvorkommen und Runde.
    pub yield_per_round: u32,
    /// Felder je Rohstofftyp fuer Kat 1 bis 4 (Adern aus zusammenhaengenden Feldern).
    pub cells_per_type: [u32; MAX_CATEGORIES],
    /// Wahrscheinlichkeit, dass das letzte Feld einer Ader (ab Kat 2) irgendwo auf der Karte liegt.
    pub stray_chance: f64,
    /// Wie viele Rohstoffe ein Feld hoechstens tragen darf.
    pub max_resources_per_cell: usize,
    /// Spiel endet unentschieden nach so vielen Runden (jeder Spieler war einmal dran).
    pub max_rounds: u32,
    /// Fairnesspruefung: Radius (in Schritten) um die Startecke.
    pub fairness_radius: u32,
    /// Fairnesspruefung: erlaubter Unterschied der Rohstoffzahl im Radius zwischen Ecken.
    pub fairness_tolerance: u32,
    /// Wie viele Generator-Versuche, bevor die Erzeugung aufgibt.
    pub max_generator_attempts: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    UnsupportedPlayers(u8),
    SideOutOfRange { side: u32, min: u32, max: u32 },
    Invalid(&'static str),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::UnsupportedPlayers(n) => {
                write!(f, "{n} Spieler werden nicht unterstuetzt (2 bis 8)")
            }
            ConfigError::SideOutOfRange { side, min, max } => {
                write!(f, "Seitenlaenge {side} ausserhalb von {min}..={max}")
            }
            ConfigError::Invalid(why) => write!(f, "ungueltige Einstellung: {why}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn new(players: u8, seed: u64) -> Config {
        Config {
            players,
            side: 10,
            seed,
            min_factory_distance: 3,
            factory_cost: 1,
            yield_per_round: 1,
            cells_per_type: [2, 2, 2, 2],
            stray_chance: 0.15,
            max_resources_per_cell: 2,
            max_rounds: 300,
            fairness_radius: 4,
            fairness_tolerance: 2,
            max_generator_attempts: 500,
        }
    }

    /// Welches Gitter zur Spielerzahl gehoert.
    pub fn grid_kind(&self) -> GridKind {
        match self.players {
            3 => GridKind::Triangle,
            5..=8 => GridKind::Hex,
            _ => GridKind::Square,
        }
    }

    /// Erlaubte Seitenlaengen fuer das Gitter dieser Spielerzahl.
    pub fn side_range(&self) -> (u32, u32) {
        match self.grid_kind() {
            GridKind::Square => (6, MAX_SQUARE_SIDE),
            GridKind::Triangle => (6, MAX_TRIANGLE_SIDE),
            GridKind::Hex => (8, MAX_HEX_SIDE),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(2..=8).contains(&self.players) {
            return Err(ConfigError::UnsupportedPlayers(self.players));
        }
        let (min, max) = self.side_range();
        if !(min..=max).contains(&self.side) {
            return Err(ConfigError::SideOutOfRange {
                side: self.side,
                min,
                max,
            });
        }
        if self.min_factory_distance == 0 {
            return Err(ConfigError::Invalid(
                "min_factory_distance muss mindestens 1 sein",
            ));
        }
        if self.factory_cost == 0 {
            return Err(ConfigError::Invalid("factory_cost muss mindestens 1 sein"));
        }
        if self.yield_per_round == 0 {
            return Err(ConfigError::Invalid(
                "yield_per_round muss mindestens 1 sein",
            ));
        }
        if self.cells_per_type.contains(&0) {
            return Err(ConfigError::Invalid(
                "cells_per_type: jeder Wert mindestens 1",
            ));
        }
        if !(0.0..=1.0).contains(&self.stray_chance) {
            return Err(ConfigError::Invalid(
                "stray_chance muss zwischen 0 und 1 liegen",
            ));
        }
        if self.max_resources_per_cell == 0 {
            return Err(ConfigError::Invalid(
                "max_resources_per_cell muss mindestens 1 sein",
            ));
        }
        if self.max_rounds == 0 {
            return Err(ConfigError::Invalid("max_rounds muss mindestens 1 sein"));
        }
        if self.fairness_radius < 2 {
            return Err(ConfigError::Invalid(
                "fairness_radius muss mindestens 2 sein",
            ));
        }
        if self.max_generator_attempts == 0 {
            return Err(ConfigError::Invalid(
                "max_generator_attempts muss mindestens 1 sein",
            ));
        }
        Ok(())
    }

    /// Das Brett zu diesen Einstellungen.
    pub fn make_board(&self) -> Result<Board, BoardError> {
        match self.grid_kind() {
            GridKind::Square => Board::square(self.side),
            GridKind::Triangle => Board::triangle(self.side),
            GridKind::Hex => Board::hex_polygon(self.players as usize, self.side),
        }
    }

    /// Anzahl Startecken der Karte (Quadrat: immer 4, auch bei 2 Spielern).
    pub fn corner_count(&self) -> usize {
        match self.grid_kind() {
            GridKind::Square => 4,
            GridKind::Triangle => 3,
            GridKind::Hex => self.players as usize,
        }
    }

    /// Welche Ecke jeder Spieler bekommt. Bei 2 Spielern gegenueber (Ecken 0 und 2).
    pub fn player_corners(&self) -> Vec<usize> {
        match self.players {
            2 => vec![0, 2],
            n => (0..n as usize).collect(),
        }
    }

    /// Anzahl Rohstoffkategorien: 3 bis 5 Ecken, ab 6 Ecken kommt Kat 4 dazu.
    /// (Ob bei 7 und 8 Spielern weitere folgen, ist im Regelwerk noch offen.)
    pub fn category_count(&self) -> u8 {
        if self.corner_count() >= 6 {
            4
        } else {
            3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_for_two_to_eight_players() {
        for n in 2..=8 {
            assert_eq!(Config::new(n, 1).validate(), Ok(()), "{n} Spieler");
        }
    }

    #[test]
    fn other_player_counts_are_rejected() {
        for n in [0u8, 1, 9, 12] {
            assert_eq!(
                Config::new(n, 1).validate(),
                Err(ConfigError::UnsupportedPlayers(n))
            );
        }
    }

    #[test]
    fn the_grid_follows_the_number_of_players() {
        let kinds: Vec<GridKind> = (2..=8).map(|n| Config::new(n, 1).grid_kind()).collect();
        use GridKind::*;
        assert_eq!(kinds, vec![Square, Triangle, Square, Hex, Hex, Hex, Hex]);
        let corners: Vec<usize> = (2..=8).map(|n| Config::new(n, 1).corner_count()).collect();
        assert_eq!(corners, vec![4, 3, 4, 5, 6, 7, 8]);
        let cats: Vec<u8> = (2..=8)
            .map(|n| Config::new(n, 1).category_count())
            .collect();
        assert_eq!(cats, vec![3, 3, 3, 3, 4, 4, 4]);
    }

    #[test]
    fn side_limits_are_enforced() {
        for players in [2u8, 3, 6] {
            let mut c = Config::new(players, 1);
            let (min, max) = c.side_range();
            c.side = min - 1;
            assert!(matches!(
                c.validate(),
                Err(ConfigError::SideOutOfRange { .. })
            ));
            c.side = max + 1;
            assert!(matches!(
                c.validate(),
                Err(ConfigError::SideOutOfRange { .. })
            ));
            c.side = min;
            assert_eq!(c.validate(), Ok(()));
            c.side = max;
            assert_eq!(c.validate(), Ok(()));
        }
    }

    #[test]
    fn nonsense_values_are_rejected() {
        let mut c = Config::new(2, 1);
        c.min_factory_distance = 0;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.factory_cost = 0;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.yield_per_round = 0;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.cells_per_type = [2, 0, 2, 2];
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.stray_chance = 1.5;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.max_resources_per_cell = 0;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.max_rounds = 0;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.fairness_radius = 1;
        assert!(c.validate().is_err());
    }

    #[test]
    fn player_corners_cover_all_corners_except_with_two_players() {
        assert_eq!(Config::new(2, 1).player_corners(), vec![0, 2]);
        assert_eq!(Config::new(3, 1).player_corners(), vec![0, 1, 2]);
        assert_eq!(Config::new(4, 1).player_corners(), vec![0, 1, 2, 3]);
        assert_eq!(
            Config::new(7, 1).player_corners(),
            (0..7).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_valid_config_builds_its_board() {
        for n in 2..=8 {
            let c = Config::new(n, 1);
            let b = c.make_board().unwrap();
            assert_eq!(b.kind(), c.grid_kind());
            assert_eq!(b.corners().len(), c.corner_count());
        }
    }
}
