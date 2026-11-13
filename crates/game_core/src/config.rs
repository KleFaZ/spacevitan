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
    /// Vorrat je Rohstoffvorkommen in Einheiten; 0 = unerschoepflich.
    pub deposit_stock: u32,
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
    /// Ohne Sieger entscheidet nach so vielen Runden die Wertung (Figurenstufen, Fabriken,
    /// Farben). 0 = aus, dann gilt nur `max_rounds`.
    pub score_after: u32,
    /// Fairnesspruefung: Radius (in Schritten) um die Startecke.
    pub fairness_radius: u32,
    /// Fairnesspruefung: erlaubter Unterschied der Rohstoffzahl im Radius zwischen Ecken.
    pub fairness_tolerance: u32,
    /// Wie viele Generator-Versuche, bevor die Erzeugung aufgibt.
    pub max_generator_attempts: u32,
    /// Eigene Kat-1-Vorkommen liegen mindestens so viele Schritte vom Start entfernt, das erste
    /// genau so weit und in Richtung Kartenmitte. Standard 2: nicht auf und nicht neben dem Start.
    pub cat1_distance: u32,
    /// Neutrale Kat-1-Typen ("Zentrumsfarben") rund um die Kartenmitte, je mit
    /// `center_cells` Feldern, dazu im Zwischenring `mid_cat1` Felder je Spieler. Sie sind fuer
    /// jeden Spieler eine fremde Farbe.
    pub center_colors: u32,
    /// Zentrumsfarben liegen hoechstens so viele Schritte von der Kartenmitte entfernt.
    pub center_radius: u32,
    /// Ausbau: Je `upgrade_step` Einheiten ab Kat 2 im Lager steigt die Stufe der kommenden
    /// Figur um 1. Ausbaustufe k braucht Rohstoffe aus k Regionen (siehe `Game::upgrade_level`).
    /// 0 = kein Ausbau, nur Farben zaehlen.
    pub upgrade_step: u32,
    /// Eine Region zaehlt fuer den Ausbau erst, wenn so viele Einheiten ihrer Farbe im Lager
    /// liegen (ein einzelner Fund reicht nicht).
    pub upgrade_region_min: u32,
    /// Nach dem Abbau ist ein Vorkommen weg und kommt nach so vielen Runden wieder, je Kategorie
    /// (Kat 1 bis 4). 0 = fuer immer weg, 1 = jede Runde wieder da.
    pub regrow_rounds: [u32; MAX_CATEGORIES],
    /// Adern je Rohstofftyp fuer Kat 1 bis 4. Jede Region hat je Kategorie genau einen Typ in
    /// ihrer Farbe; er liegt in so vielen Adern zu je `cells_per_type` Feldern.
    pub veins_per_type: [u32; MAX_CATEGORIES],
    /// So viele Felder jeder Region-Farbe (je Kat 2 und hoeher) liegen zusaetzlich rund um die
    /// Kartenmitte: dort gibt es alle Farben.
    pub center_mix: u32,
    /// Hoechstens so viele Ausbaustufen.
    pub max_upgrades: u8,
    /// Karten, die im Markt ausliegen (verschiedene, je beliebig oft kaufbar); 0 = ohne Karten.
    pub market_size: u32,
    /// Figurenkarten nur fuer eine Figur auf oder neben dem Zahlfeld (Fabrik oder Koenig).
    pub card_target_near: bool,
    /// Rollen der Kategorien: Kat 1 baut Fabriken und gibt Stufen, Kat 2 kauft Bauern (Rekrut,
    /// immer im Markt, die Fabrik bleibt stehen), Kat 3 kauft Spezialzuege (Lanze, Sprungfeder,
    /// Pferd, Laeuferschlag, Lange Beine), auch fuer den Koenig. Aus = alte Preise und Koenig
    /// ohne Karten.
    pub kat_roles: bool,
    /// Mit `kat_roles`: Kat-2-Einheiten (eine Farbe) fuer einen Bauern.
    pub pawn_cost: u32,
    /// Felder je Zentrumsfarbe in der Mitte.
    pub center_cells: u32,
    /// Kat-1-Vorkommen je Spieler im Zwischenring (halber Weg zur Mitte, Zentrumsfarben).
    pub mid_cat1: u32,
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
            deposit_stock: 0,
            yield_per_round: 1,
            // Kat 1: genau ein Feld je Ecke; mehr Kat 1 gibt es zur Mitte hin.
            cells_per_type: [1, 2, 2, 2],
            stray_chance: 0.15,
            max_resources_per_cell: 2,
            max_rounds: 300,
            score_after: 100,
            fairness_radius: 4,
            fairness_tolerance: 2,
            max_generator_attempts: 500,
            cat1_distance: 2,
            center_colors: Config::default_center_colors(players),
            center_radius: 2,
            upgrade_step: 10,
            upgrade_region_min: 3,
            regrow_rounds: [20, 5, 3, 2],
            veins_per_type: [1, 2, 3, 3],
            center_mix: 1,
            max_upgrades: 2,
            market_size: Config::default_market_size(players),
            card_target_near: false,
            kat_roles: false,
            pawn_cost: 4,
            center_cells: 3,
            mid_cat1: 1,
        }
    }

    /// Zwei Karten je Spieler bis 4 Spieler, ab 5 Spielern eine je Spieler.
    pub fn default_market_size(players: u8) -> u32 {
        if players <= 4 {
            2 * players as u32
        } else {
            players as u32
        }
    }

    /// Setzt eine Regel-Einstellung nach Namen, z. B. aus `--rule upgrade_step=6` oder aus dem
    /// Browser. Bekannt sind die Regel-Felder dieser Struktur; Wahrheitswerte als 0 oder 1.
    pub fn set(&mut self, key: &str, value: i64) -> Result<(), String> {
        let as_u32 = |v: i64| u32::try_from(v).map_err(|_| format!("{key}: {v} ist ungueltig"));
        match key {
            "cat1_distance" => self.cat1_distance = as_u32(value)?,
            "center_colors" => self.center_colors = as_u32(value)?,
            "center_radius" => self.center_radius = as_u32(value)?,
            "upgrade_region_min" => self.upgrade_region_min = as_u32(value)?,
            "regrow_cat1" => self.regrow_rounds[0] = as_u32(value)?,
            "regrow_cat2" => self.regrow_rounds[1] = as_u32(value)?,
            "regrow_cat3" => self.regrow_rounds[2] = as_u32(value)?,
            "regrow_cat4" => self.regrow_rounds[3] = as_u32(value)?,
            "center_mix" => self.center_mix = as_u32(value)?,
            "yield_per_round" => self.yield_per_round = as_u32(value)?,
            "upgrade_step" => self.upgrade_step = as_u32(value)?,
            "max_upgrades" => {
                self.max_upgrades =
                    u8::try_from(value).map_err(|_| format!("{key}: {value} ist ungueltig"))?
            }
            "deposit_stock" => self.deposit_stock = as_u32(value)?,
            "min_factory_distance" => self.min_factory_distance = as_u32(value)?,
            "max_rounds" => self.max_rounds = as_u32(value)?,
            "score_after" => self.score_after = as_u32(value)?,
            "side" => self.side = as_u32(value)?,
            "cat1_cells" => self.cells_per_type[0] = as_u32(value)?,
            "market_size" => self.market_size = as_u32(value)?,
            "card_target_near" => self.card_target_near = value != 0,
            "kat_roles" => self.kat_roles = value != 0,
            "pawn_cost" => self.pawn_cost = as_u32(value)?,
            "center_cells" => self.center_cells = as_u32(value)?,
            "mid_cat1" => self.mid_cat1 = as_u32(value)?,
            other => return Err(format!("unbekannte Regel '{other}'")),
        }
        Ok(())
    }

    /// Wie `set`, fuer eine Liste `name=wert,name=wert`.
    pub fn set_all(&mut self, list: &str) -> Result<(), String> {
        for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
            let (key, value) = item
                .split_once('=')
                .ok_or_else(|| format!("'{item}': erwartet name=wert"))?;
            let value: i64 = value
                .trim()
                .parse()
                .map_err(|_| format!("'{item}': keine ganze Zahl"))?;
            self.set(key.trim(), value)?;
        }
        Ok(())
    }

    /// Eine Zentrumsfarbe je zwei Spieler (aufgerundet).
    pub fn default_center_colors(players: u8) -> u32 {
        (players as u32).div_ceil(2)
    }

    /// Fairness-Radius, den die Pruefung wirklich nutzt: auf kleinen Karten hoechstens zwei
    /// Fuenftel der Seitenlaenge, sonst zaehlt er schon die Rohstoffe der Nachbarn mit.
    pub fn effective_fairness_radius(&self) -> u32 {
        self.fairness_radius.min((self.side * 2 / 5).max(2))
    }

    /// Radius, den der Generator fuer die Zentrumsfarben wirklich nutzt: auf kleinen Karten
    /// schrumpft er (ein Fuenftel der Seitenlaenge), damit die Mitte nicht bis an die Startecken
    /// reicht.
    pub fn effective_center_radius(&self) -> u32 {
        self.center_radius.min((self.side / 5).max(1))
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
        if self.cat1_distance == 0 || self.cat1_distance > self.side / 2 {
            return Err(ConfigError::Invalid(
                "cat1_distance muss zwischen 1 und der halben Seitenlaenge liegen",
            ));
        }
        if self.pawn_cost == 0 || self.pawn_cost > 30 {
            return Err(ConfigError::Invalid(
                "pawn_cost muss zwischen 1 und 30 liegen",
            ));
        }
        if self.center_colors > 8 {
            return Err(ConfigError::Invalid("hoechstens 8 Zentrumsfarben"));
        }
        if self.center_colors > 0 && (self.center_radius == 0 || self.center_radius > self.side) {
            return Err(ConfigError::Invalid(
                "center_radius muss zwischen 1 und der Seitenlaenge liegen",
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
        let mut c = Config::new(2, 1);
        c.cat1_distance = 0;
        assert!(c.validate().is_err());
        c.cat1_distance = 6;
        assert!(c.validate().is_err(), "mehr als die halbe Seite");
        let mut c = Config::new(2, 1);
        c.center_colors = 9;
        assert!(c.validate().is_err());
        let mut c = Config::new(2, 1);
        c.center_radius = 0;
        assert!(c.validate().is_err());
        c.center_colors = 0;
        assert_eq!(
            c.validate(),
            Ok(()),
            "ohne Zentrumsfarben ist der Radius egal"
        );
    }

    #[test]
    fn rules_can_be_set_by_name() {
        let mut c = Config::new(4, 1);
        c.set_all("upgrade_step=6, regrow_cat2=7,center_colors=3")
            .unwrap();
        assert_eq!(c.upgrade_step, 6);
        assert_eq!(c.regrow_rounds, [20, 7, 3, 2]);
        assert_eq!(c.center_colors, 3);
        assert!(c.set("upgrade_step", -1).is_err());
        assert!(c.set("schach", 1).is_err());
        c.set("score_after", 0).unwrap();
        assert_eq!(c.score_after, 0);
        assert!(c.set_all("upgrade_step").is_err());
        assert!(c.set_all("upgrade_step=x").is_err());
    }

    #[test]
    fn one_center_colour_per_two_players() {
        let n: Vec<u32> = (2..=8).map(Config::default_center_colors).collect();
        assert_eq!(n, vec![1, 2, 2, 3, 3, 4, 4]);
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
