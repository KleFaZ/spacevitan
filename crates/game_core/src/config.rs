//! Alle einstellbaren Zahlen an einer Stelle (siehe Regelwerk, Abschnitt "Einstellbare Parameter").

use crate::board::MAX_SQUARE_SIDE;

/// Kleinste Seitenlaenge, bei der das Gebiet einer Ecke genug Platz fuer alle Rohstoffe hat.
pub const MIN_SIDE: u32 = 6;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Meilenstein 1: 2 oder 4 Spieler (Quadratkarte).
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
    /// Felder je Rohstofftyp fuer Kat 1, 2 und 3 (Adern aus zusammenhaengenden Feldern).
    pub cells_per_type: [u32; 3],
    /// Wahrscheinlichkeit, dass das letzte Feld einer Kat-2/3-Ader irgendwo auf der Karte liegt.
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
                write!(
                    f,
                    "{n} Spieler werden in Meilenstein 1 noch nicht unterstuetzt (2 oder 4)"
                )
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
            cells_per_type: [2, 2, 2],
            stray_chance: 0.15,
            max_resources_per_cell: 2,
            max_rounds: 300,
            fairness_radius: 4,
            fairness_tolerance: 2,
            max_generator_attempts: 500,
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.players != 2 && self.players != 4 {
            return Err(ConfigError::UnsupportedPlayers(self.players));
        }
        if !(MIN_SIDE..=MAX_SQUARE_SIDE).contains(&self.side) {
            return Err(ConfigError::SideOutOfRange {
                side: self.side,
                min: MIN_SIDE,
                max: MAX_SQUARE_SIDE,
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

    /// Anzahl Startecken der Karte (Quadrat: immer 4, auch bei 2 Spielern).
    pub fn corner_count(&self) -> usize {
        4
    }

    /// Welche Ecke jeder Spieler bekommt. Bei 2 Spielern gegenueber (Ecken 0 und 2).
    pub fn player_corners(&self) -> Vec<usize> {
        match self.players {
            2 => vec![0, 2],
            _ => vec![0, 1, 2, 3],
        }
    }

    /// Anzahl Rohstoffkategorien (Meilenstein 1: 3; ab 6 Ecken kaeme eine vierte dazu).
    pub fn category_count(&self) -> u8 {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid_for_two_and_four_players() {
        assert_eq!(Config::new(2, 1).validate(), Ok(()));
        assert_eq!(Config::new(4, 1).validate(), Ok(()));
    }

    #[test]
    fn other_player_counts_are_rejected_for_now() {
        for n in [0u8, 1, 3, 5, 8] {
            assert_eq!(
                Config::new(n, 1).validate(),
                Err(ConfigError::UnsupportedPlayers(n))
            );
        }
    }

    #[test]
    fn side_limits_are_enforced() {
        let mut c = Config::new(2, 1);
        c.side = MIN_SIDE - 1;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::SideOutOfRange { .. })
        ));
        c.side = MAX_SQUARE_SIDE + 1;
        assert!(matches!(
            c.validate(),
            Err(ConfigError::SideOutOfRange { .. })
        ));
        c.side = MIN_SIDE;
        assert_eq!(c.validate(), Ok(()));
        c.side = MAX_SQUARE_SIDE;
        assert_eq!(c.validate(), Ok(()));
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
        c.cells_per_type = [2, 0, 2];
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
    fn two_players_sit_in_opposite_corners() {
        assert_eq!(Config::new(2, 1).player_corners(), vec![0, 2]);
        assert_eq!(Config::new(4, 1).player_corners(), vec![0, 1, 2, 3]);
    }
}
