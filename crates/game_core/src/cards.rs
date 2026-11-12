//! Kartenmarkt (Version 1): welche Karten es gibt, was sie kosten, wie bezahlt wird und welcher
//! Teil des Decks in einer Partie ausliegt. Die Wirkungen fuehrt die Regel-Engine aus
//! (`Game::do_buy`).
//!
//! Regeln des Markts:
//! - Vor der Partie liegen `Config::market_size` verschiedene Karten offen aus (zufaellig, durch
//!   den Seed festgelegt). Jede Karte gibt es beliebig oft.
//! - Pro Zug darf man hoechstens eine Karte kaufen, zusaetzlich zur Aktion des Zuges.
//! - Bezahlt wird aus **einem** Lager: dem Gepaeck des Koenigs oder dem Lager einer eigenen
//!   Fabrik. Welche Einheiten genau, waehlt die Engine (immer die haeufigsten Typen zuerst, so
//!   bleibt die Vielfalt im Lager erhalten).
//! - Eine Karte fuer eine Figur oder den Koenig wirkt nur auf eine Figur, die auf dem Zahlfeld
//!   oder direkt daneben steht.

use crate::board::GridKind;
use crate::ids::ResourceId;
use crate::map::Map;
use crate::rng::Rng;
use crate::state::Inventory;

/// Alle Karten des Decks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Card {
    /// Figur: Stufe +1 (Reichweite waechst mit).
    Training,
    /// Figur: Reichweite +1 (hoechstens zweimal).
    LongLegs,
    /// Figur: schlaegt zusaetzlich diagonal auf ein Nachbarfeld (nur Quadratkarten).
    DiagonalStrike,
    /// Figur: der erste Schlag gegen sie prallt ab.
    Shield,
    /// Koenig: ueberlebt einen Schlag.
    KingArmor,
    /// Koenig: zieht bis zu zwei Felder geradeaus.
    KingStep,
    /// Ein Bauer erscheint neben der zahlenden Fabrik; die Fabrik bleibt stehen.
    Recruit,
    /// Alle eigenen Fabriken ernten sofort noch einmal.
    DoubleShift,
    /// Eigene Fabriken ernten Kat 2 doppelt.
    Mining,
    /// Eigene Fabriken ernten Kat 3 doppelt.
    Farming,
    /// In diesem Zug eine zweite Aktion.
    DoubleMove,
    /// Set: der dritte Runenstein gibt einer Figur Stufe +2.
    Runestone,
    /// Set: der dritte Sternsplitter legt ein Kat-1-Vorkommen der eigenen Farbe unter den Koenig.
    Starshard,
}

/// Worauf eine Karte wirkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardTarget {
    /// Auf den Spieler oder seine Fabriken; keine Figur noetig.
    Player,
    /// Auf eine eigene Figur (nicht den Koenig) am Zahlfeld.
    Unit,
    /// Auf den eigenen Koenig am Zahlfeld.
    King,
}

/// Kosten einer Karte. `*_distinct`: alle Einheiten dieser Kategorie von verschiedenen Typen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    /// Beliebige Kat-1-Einheiten ("Farbe").
    pub color: u8,
    /// Kat-1-Einheiten, die nicht die eigene Farbe sind.
    pub foreign_color: u8,
    pub k2: u8,
    pub k2_distinct: bool,
    pub k3: u8,
    pub k3_distinct: bool,
}

impl Cost {
    pub fn units(&self) -> u32 {
        (self.color + self.foreign_color + self.k2 + self.k3) as u32
    }
}

const fn cost(color: u8, k2: u8, k2_distinct: bool, k3: u8, k3_distinct: bool) -> Cost {
    Cost {
        color,
        foreign_color: 0,
        k2,
        k2_distinct,
        k3,
        k3_distinct,
    }
}

/// Wie viele Teile ein Set hat.
pub const SET_SIZE: u8 = 3;
/// Wie oft "Lange Beine" auf einer Figur liegen darf.
pub const MAX_EXTRA_RANGE: u8 = 2;

impl Card {
    pub const ALL: [Card; 13] = [
        Card::Training,
        Card::LongLegs,
        Card::DiagonalStrike,
        Card::Shield,
        Card::KingArmor,
        Card::KingStep,
        Card::Recruit,
        Card::DoubleShift,
        Card::Mining,
        Card::Farming,
        Card::DoubleMove,
        Card::Runestone,
        Card::Starshard,
    ];

    /// Kurzname fuer JSON und Kommandozeile.
    pub fn key(self) -> &'static str {
        match self {
            Card::Training => "training",
            Card::LongLegs => "long_legs",
            Card::DiagonalStrike => "diagonal_strike",
            Card::Shield => "shield",
            Card::KingArmor => "king_armor",
            Card::KingStep => "king_step",
            Card::Recruit => "recruit",
            Card::DoubleShift => "double_shift",
            Card::Mining => "mining",
            Card::Farming => "farming",
            Card::DoubleMove => "double_move",
            Card::Runestone => "runestone",
            Card::Starshard => "starshard",
        }
    }

    pub fn from_key(key: &str) -> Option<Card> {
        Card::ALL.into_iter().find(|c| c.key() == key)
    }

    pub fn name(self) -> &'static str {
        match self {
            Card::Training => "Ausbildung",
            Card::LongLegs => "Lange Beine",
            Card::DiagonalStrike => "Läuferschlag",
            Card::Shield => "Schild",
            Card::KingArmor => "Königsrüstung",
            Card::KingStep => "Königsschritt",
            Card::Recruit => "Rekrut",
            Card::DoubleShift => "Doppelschicht",
            Card::Mining => "Bergbau",
            Card::Farming => "Ackerbau",
            Card::DoubleMove => "Doppelzug",
            Card::Runestone => "Runenstein",
            Card::Starshard => "Sternsplitter",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Card::Training => "Figur am Zahlfeld: Stufe +1, die Reichweite wächst mit.",
            Card::LongLegs => "Figur am Zahlfeld: Reichweite +1 (höchstens zweimal).",
            Card::DiagonalStrike => {
                "Figur am Zahlfeld schlägt zusätzlich diagonal auf ein Nachbarfeld."
            }
            Card::Shield => "Figur am Zahlfeld: der erste Schlag gegen sie prallt ab.",
            Card::KingArmor => "König am Zahlfeld überlebt einen Schlag.",
            Card::KingStep => "König am Zahlfeld zieht bis zu zwei Felder geradeaus.",
            Card::Recruit => {
                "Ein Bauer erscheint neben der zahlenden Fabrik, die Fabrik bleibt stehen."
            }
            Card::DoubleShift => "Alle eigenen Fabriken ernten sofort noch einmal.",
            Card::Mining => "Eigene Fabriken ernten Kat 2 doppelt.",
            Card::Farming => "Eigene Fabriken ernten Kat 3 doppelt.",
            Card::DoubleMove => "In diesem Zug eine zweite Aktion.",
            Card::Runestone => "Set: der dritte Runenstein gibt einer Figur am Zahlfeld Stufe +2.",
            Card::Starshard => {
                "Set: der dritte Sternsplitter legt ein Kat-1-Vorkommen deiner Farbe unter den König."
            }
        }
    }

    pub fn cost(self) -> Cost {
        match self {
            Card::Training => cost(0, 2, true, 2, true),
            Card::LongLegs => cost(0, 2, true, 1, false),
            Card::DiagonalStrike => cost(0, 1, false, 1, false),
            Card::Shield => cost(0, 0, false, 2, false),
            Card::KingArmor => cost(1, 2, false, 2, false),
            Card::KingStep => cost(0, 3, true, 1, false),
            Card::Recruit => cost(1, 2, false, 2, false),
            Card::DoubleShift => cost(0, 0, false, 2, true),
            Card::Mining => cost(0, 3, true, 1, false),
            Card::Farming => cost(0, 1, false, 3, true),
            Card::DoubleMove => cost(0, 2, true, 2, true),
            Card::Runestone => cost(0, 1, false, 1, false),
            Card::Starshard => cost(0, 1, false, 2, false),
        }
    }

    pub fn target(self) -> CardTarget {
        match self {
            Card::Training
            | Card::LongLegs
            | Card::DiagonalStrike
            | Card::Shield
            | Card::Runestone => CardTarget::Unit,
            Card::KingArmor | Card::KingStep => CardTarget::King,
            _ => CardTarget::Player,
        }
    }

    /// Nur aus dem Lager einer Fabrik bezahlbar (die Fabrik ist Teil der Wirkung).
    pub fn needs_factory(self) -> bool {
        matches!(self, Card::Recruit)
    }

    /// Ist die Karte auf diesem Gitter sinnvoll? Diagonalen gibt es nur im Quadratgitter.
    pub fn available_on(self, grid: GridKind) -> bool {
        match self {
            Card::DiagonalStrike => grid == GridKind::Square,
            _ => true,
        }
    }

    pub fn is_set_piece(self) -> bool {
        matches!(self, Card::Runestone | Card::Starshard)
    }
}

/// Zieht die Karten, die in dieser Partie ausliegen: `size` verschiedene, sortiert. Gleicher
/// Seed, gleicher Markt.
pub fn draw_market(seed: u64, size: usize, grid: GridKind) -> Vec<Card> {
    let mut deck: Vec<Card> = Card::ALL
        .into_iter()
        .filter(|c| c.available_on(grid))
        .collect();
    let mut rng = Rng::new(seed ^ 0x6b61_7274_656e_0001);
    rng.shuffle(&mut deck);
    deck.truncate(size);
    deck.sort();
    deck
}

/// Waehlt aus `stock` die Einheiten, mit denen `cost` bezahlt wird, oder `None`, wenn es nicht
/// reicht. `own` ist die eigene Farbe (fuer "fremde Farbe").
pub fn plan_payment(
    map: &Map,
    own: ResourceId,
    stock: &Inventory,
    cost: &Cost,
) -> Option<Inventory> {
    let mut left = stock.clone();
    let mut pay = Inventory::new();
    let cat = |r: ResourceId| map.category(r);
    take(
        &mut left,
        &mut pay,
        |r| cat(r) == 1 && r != own,
        cost.foreign_color,
        false,
    )?;
    take(&mut left, &mut pay, |r| cat(r) == 1, cost.color, false)?;
    take(
        &mut left,
        &mut pay,
        |r| cat(r) == 2,
        cost.k2,
        cost.k2_distinct,
    )?;
    take(
        &mut left,
        &mut pay,
        |r| cat(r) == 3,
        cost.k3,
        cost.k3_distinct,
    )?;
    Some(pay)
}

/// Nimmt `n` Einheiten, die `fits` erfuellen, jeweils vom haeufigsten Typ (bei Gleichstand die
/// kleinste ID). `distinct`: jeder Typ hoechstens einmal.
fn take(
    left: &mut Inventory,
    pay: &mut Inventory,
    fits: impl Fn(ResourceId) -> bool,
    n: u8,
    distinct: bool,
) -> Option<()> {
    let mut used: Vec<ResourceId> = Vec::new();
    for _ in 0..n {
        let best = left
            .iter()
            .filter(|&(r, _)| fits(r) && !(distinct && used.contains(&r)))
            .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))?
            .0;
        left.take(best, 1);
        pay.add(best, 1);
        used.push(best);
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn map() -> Map {
        Map::generate(&Config::new(4, 1)).expect("Karte")
    }

    fn r(n: u16) -> ResourceId {
        ResourceId(n)
    }

    fn inv(items: &[(u16, u32)]) -> Inventory {
        let mut i = Inventory::new();
        for &(res, n) in items {
            i.add(r(res), n);
        }
        i
    }

    #[test]
    fn every_card_has_a_unique_key_name_text_and_a_cost() {
        let mut keys: Vec<&str> = Card::ALL.iter().map(|c| c.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), Card::ALL.len());
        for c in Card::ALL {
            assert_eq!(Card::from_key(c.key()), Some(c));
            assert!(!c.name().is_empty() && !c.text().is_empty());
            assert!(c.cost().units() >= 1, "{c:?} kostet nichts");
        }
        assert_eq!(Card::from_key("schach"), None);
    }

    #[test]
    fn the_diagonal_strike_only_exists_on_square_boards() {
        assert!(Card::DiagonalStrike.available_on(GridKind::Square));
        assert!(!Card::DiagonalStrike.available_on(GridKind::Hex));
        assert!(!Card::DiagonalStrike.available_on(GridKind::Triangle));
        for seed in 0..50 {
            let m = draw_market(seed, 13, GridKind::Hex);
            assert!(!m.contains(&Card::DiagonalStrike));
            assert_eq!(m.len(), 12);
        }
    }

    #[test]
    fn the_market_is_random_but_fixed_by_the_seed() {
        let a = draw_market(5, 4, GridKind::Square);
        assert_eq!(a, draw_market(5, 4, GridKind::Square));
        assert_eq!(a.len(), 4);
        let mut sorted = a.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted, a, "sortiert und ohne Doppelte");
        let different = (0..20).any(|s| draw_market(s, 4, GridKind::Square) != a);
        assert!(different);
        assert!(draw_market(5, 0, GridKind::Square).is_empty());
    }

    #[test]
    fn payment_takes_the_most_common_types_first() {
        let m = map();
        // Kat 2: Typen 1, 2, 7; Kat 3: Typen 3, 4, 5.
        let stock = inv(&[(1, 5), (2, 1), (3, 4), (4, 1)]);
        let cost = Cost {
            k2: 2,
            k3: 2,
            ..Cost::default()
        };
        let pay = plan_payment(&m, r(0), &stock, &cost).expect("reicht");
        assert_eq!(pay, inv(&[(1, 2), (3, 2)]));
    }

    #[test]
    fn distinct_costs_need_different_types() {
        let m = map();
        let cost = Card::Training.cost();
        let one_type = inv(&[(1, 10), (3, 10), (4, 10)]);
        assert!(
            plan_payment(&m, r(0), &one_type, &cost).is_none(),
            "nur ein Kat-2-Typ"
        );
        let varied = inv(&[(1, 10), (2, 1), (3, 10), (4, 1)]);
        assert_eq!(
            plan_payment(&m, r(0), &varied, &cost),
            Some(inv(&[(1, 1), (2, 1), (3, 1), (4, 1)]))
        );
    }

    #[test]
    fn colours_and_foreign_colours_are_told_apart() {
        let m = map();
        let foreign = Cost {
            foreign_color: 1,
            ..Cost::default()
        };
        assert!(plan_payment(&m, r(0), &inv(&[(0, 3)]), &foreign).is_none());
        assert_eq!(
            plan_payment(&m, r(0), &inv(&[(0, 3), (6, 1)]), &foreign),
            Some(inv(&[(6, 1)]))
        );
        let any = Cost {
            color: 1,
            ..Cost::default()
        };
        assert_eq!(
            plan_payment(&m, r(0), &inv(&[(0, 3)]), &any),
            Some(inv(&[(0, 1)]))
        );
        assert!(
            plan_payment(&m, r(0), &inv(&[(1, 3)]), &any).is_none(),
            "Kat 2 ist keine Farbe"
        );
    }

    #[test]
    fn paying_never_touches_more_than_needed() {
        let m = map();
        let stock = inv(&[(0, 1), (1, 3), (2, 3), (3, 3), (4, 3), (5, 3)]);
        for card in Card::ALL {
            if let Some(pay) = plan_payment(&m, r(0), &stock, &card.cost()) {
                assert_eq!(pay.total() as u32, card.cost().units(), "{card:?}");
                for (res, n) in pay.iter() {
                    assert!(n <= stock.get(res));
                }
            }
        }
    }
}
