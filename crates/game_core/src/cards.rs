//! Kartenmarkt: welche Karten es gibt, was sie kosten, wie bezahlt wird und welcher Teil des
//! Decks in einer Partie ausliegt. Die Wirkungen fuehrt die Regel-Engine aus (`Game::do_buy`).
//!
//! Es gibt nur Karten, die angreifen helfen (Figuren staerker oder beweglicher, mehr Figuren,
//! mehr Tempo) oder die Wirtschaft beschleunigen; keine Verteidigung (kein Schild, keine
//! Ruestung), damit Partien entschieden werden.
//!
//! Regeln des Markts:
//! - Vor jeder Partie liegen `Config::market_size` verschiedene Karten offen aus, zufaellig aus
//!   dem Deck gezogen (der Seed legt sie fest). Jede Karte gibt es beliebig oft.
//! - Pro Zug darf man hoechstens eine Karte kaufen, zusaetzlich zur Aktion des Zuges.
//! - Bezahlt wird aus **einem** Lager: dem Gepaeck des Koenigs oder dem Lager einer eigenen
//!   Fabrik. Welche Einheiten genau, waehlt die Engine (die haeufigsten Typen zuerst, so bleibt
//!   die Vielfalt im Lager erhalten).
//! - Figurenkarten wirken auf eine beliebige eigene Figur (mit `Config::card_target_near` nur auf
//!   eine Figur am Zahlfeld oder daneben).
//!
//! Kosten und Wert: Der Ausbau einer Fabrik gibt fuer 10 Einheiten ab Kat 2 eine Stufe. Daran
//! messen sich die Preise: eine ganze Stufe kostet auch als Karte rund 8 bis 10 Einheiten.

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
    /// Figur: schlaegt bis zu zwei Felder weit geradeaus, auch mit Reichweite 1.
    Lance,
    /// Figur: springt ueber eine angrenzende Figur auf das Feld dahinter.
    Spring,
    /// Figur: springt auf jedes Feld im Abstand 2, ueber alles hinweg.
    Horse,
    /// Alle eigenen Figuren haben in diesem Zug Reichweite +1.
    Charge,
    /// Ein Bauer erscheint neben der zahlenden Fabrik; die Fabrik bleibt stehen.
    Recruit,
    /// In diesem Zug eine zweite Aktion.
    DoubleMove,
    /// Alle eigenen Fabriken ernten sofort noch einmal.
    DoubleShift,
    /// Eigene Fabriken ernten Kat 2 doppelt.
    Mining,
    /// Eigene Fabriken ernten Kat 3 doppelt.
    Farming,
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
    /// Auf eine eigene Figur (nicht den Koenig).
    Unit,
}

/// Kosten einer Karte. `k2_kinds`: davon mindestens so viele verschiedene Kat-2-Typen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Cost {
    /// Beliebige Kat-1-Einheiten ("Farbe").
    pub color: u8,
    /// Kat-1-Einheiten, die nicht die eigene Farbe sind.
    pub foreign_color: u8,
    pub k2: u8,
    pub k2_kinds: u8,
    pub k3: u8,
    pub k3_kinds: u8,
}

impl Cost {
    pub fn units(&self) -> u32 {
        (self.color + self.foreign_color + self.k2 + self.k3) as u32
    }
}

const fn cost(color: u8, k2: u8, k2_kinds: u8, k3: u8, k3_kinds: u8) -> Cost {
    Cost {
        color,
        foreign_color: 0,
        k2,
        k2_kinds,
        k3,
        k3_kinds,
    }
}

/// Wie viele Teile ein Set hat.
pub const SET_SIZE: u8 = 3;
/// Wie oft "Lange Beine" auf einer Figur liegen darf.
pub const MAX_EXTRA_RANGE: u8 = 2;

impl Card {
    pub const ALL: [Card; 14] = [
        Card::Training,
        Card::LongLegs,
        Card::DiagonalStrike,
        Card::Lance,
        Card::Spring,
        Card::Horse,
        Card::Charge,
        Card::Recruit,
        Card::DoubleMove,
        Card::DoubleShift,
        Card::Mining,
        Card::Farming,
        Card::Runestone,
        Card::Starshard,
    ];

    /// Kurzname fuer JSON und Kommandozeile.
    pub fn key(self) -> &'static str {
        match self {
            Card::Training => "training",
            Card::LongLegs => "long_legs",
            Card::DiagonalStrike => "diagonal_strike",
            Card::Lance => "lance",
            Card::Spring => "spring",
            Card::Horse => "horse",
            Card::Charge => "charge",
            Card::Recruit => "recruit",
            Card::DoubleMove => "double_move",
            Card::DoubleShift => "double_shift",
            Card::Mining => "mining",
            Card::Farming => "farming",
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
            Card::Lance => "Lanze",
            Card::Spring => "Sprungfeder",
            Card::Horse => "Pferd",
            Card::Charge => "Sturmangriff",
            Card::Recruit => "Rekrut",
            Card::DoubleMove => "Doppelzug",
            Card::DoubleShift => "Doppelschicht",
            Card::Mining => "Bergbau",
            Card::Farming => "Ackerbau",
            Card::Runestone => "Runenstein",
            Card::Starshard => "Sternsplitter",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Card::Training => "Eine Figur: Stufe +1, die Reichweite wächst mit.",
            Card::LongLegs => "Eine Figur: Reichweite +1 (höchstens zweimal).",
            Card::DiagonalStrike => "Eine Figur schlägt zusätzlich diagonal auf ein Nachbarfeld.",
            Card::Lance => "Eine Figur schlägt bis zu zwei Felder weit geradeaus.",
            Card::Spring => "Eine Figur springt über eine Nachbarfigur auf das Feld dahinter.",
            Card::Horse => "Eine Figur springt auf jedes Feld im Abstand 2, über alles hinweg.",
            Card::Charge => "In diesem Zug haben alle deine Figuren Reichweite +1.",
            Card::Recruit => {
                "Ein Bauer erscheint neben der zahlenden Fabrik, die Fabrik bleibt stehen."
            }
            Card::DoubleMove => "In diesem Zug eine zweite Aktion.",
            Card::DoubleShift => "Alle deine Fabriken ernten sofort noch einmal.",
            Card::Mining => "Deine Fabriken ernten Kat 2 doppelt.",
            Card::Farming => "Deine Fabriken ernten Kat 3 doppelt.",
            Card::Runestone => "Set: der dritte Runenstein gibt einer Figur Stufe +2.",
            Card::Starshard => {
                "Set: der dritte Sternsplitter legt ein Kat-1-Vorkommen deiner Farbe unter den König."
            }
        }
    }

    /// Preise, geeicht am Ausbau (10 Einheiten = 1 Stufe), siehe Kartenwert-Messung im README.
    pub fn cost(self) -> Cost {
        match self {
            Card::Training => cost(0, 4, 2, 4, 2),
            Card::LongLegs => cost(0, 2, 2, 2, 1),
            Card::DiagonalStrike => cost(0, 1, 1, 2, 1),
            Card::Lance => cost(0, 2, 1, 2, 1),
            Card::Spring => cost(0, 1, 1, 2, 1),
            Card::Horse => cost(0, 3, 2, 3, 2),
            Card::Charge => cost(0, 0, 0, 3, 2),
            Card::Recruit => cost(1, 2, 1, 3, 1),
            Card::DoubleMove => cost(0, 2, 2, 3, 2),
            Card::DoubleShift => cost(0, 0, 0, 2, 2),
            Card::Mining => cost(0, 3, 2, 2, 1),
            Card::Farming => cost(0, 2, 1, 3, 2),
            Card::Runestone => cost(0, 2, 1, 3, 1),
            Card::Starshard => cost(0, 2, 1, 2, 1),
        }
    }

    pub fn target(self) -> CardTarget {
        match self {
            Card::Training
            | Card::LongLegs
            | Card::DiagonalStrike
            | Card::Lance
            | Card::Spring
            | Card::Horse
            | Card::Runestone => CardTarget::Unit,
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
        0,
    )?;
    take(&mut left, &mut pay, |r| cat(r) == 1, cost.color, 0)?;
    take(&mut left, &mut pay, |r| cat(r) == 2, cost.k2, cost.k2_kinds)?;
    take(&mut left, &mut pay, |r| cat(r) == 3, cost.k3, cost.k3_kinds)?;
    Some(pay)
}

/// Nimmt `n` Einheiten, die `fits` erfuellen, davon mindestens `kinds` verschiedene Typen:
/// erst je eine Einheit der haeufigsten Typen, dann der Rest vom jeweils haeufigsten Typ (bei
/// Gleichstand die kleinste ID).
fn take(
    left: &mut Inventory,
    pay: &mut Inventory,
    fits: impl Fn(ResourceId) -> bool,
    n: u8,
    kinds: u8,
) -> Option<()> {
    let mut used: Vec<ResourceId> = Vec::new();
    for i in 0..n {
        let distinct = i < kinds;
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
            let m = draw_market(seed, 14, GridKind::Hex);
            assert!(!m.contains(&Card::DiagonalStrike));
            assert_eq!(m.len(), 13);
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
        let mixed = Cost {
            k2: 3,
            k2_kinds: 2,
            ..Cost::default()
        };
        let pay = plan_payment(&m, r(0), &stock, &mixed).expect("reicht");
        assert_eq!(
            pay,
            inv(&[(1, 2), (2, 1)]),
            "zwei Typen, der Rest vom haeufigsten"
        );
    }

    #[test]
    fn costs_with_kinds_need_different_types() {
        let m = map();
        let cost = Card::Training.cost();
        assert_eq!(
            (cost.k2, cost.k2_kinds, cost.k3, cost.k3_kinds),
            (4, 2, 4, 2)
        );
        let one_type = inv(&[(1, 10), (3, 10), (4, 10)]);
        assert!(
            plan_payment(&m, r(0), &one_type, &cost).is_none(),
            "nur ein Kat-2-Typ"
        );
        let varied = inv(&[(1, 10), (2, 1), (3, 10), (4, 1)]);
        assert_eq!(
            plan_payment(&m, r(0), &varied, &cost),
            Some(inv(&[(1, 3), (2, 1), (3, 3), (4, 1)]))
        );
    }

    #[test]
    fn there_are_no_defensive_cards() {
        for key in ["shield", "king_armor", "king_step"] {
            assert_eq!(Card::from_key(key), None, "{key}");
        }
    }

    #[test]
    fn a_whole_level_costs_about_as_much_as_the_factory_upgrade() {
        // Ausbau: 10 Einheiten je Stufe. Ausbildung (1 Stufe) und Runensteine (2 Stufen)
        // duerfen hoechstens wenig billiger sein, sonst kauft man Stufen statt auszubauen.
        assert!(Card::Training.cost().units() >= 8);
        assert!(3 * Card::Runestone.cost().units() >= 15);
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
