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

use std::collections::{BTreeMap, BTreeSet};

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
    /// Version 2, Figur: darf einen Schritt weniger ziehen (Stufe 3: 3 oder 2 Felder).
    Shorten,
    /// Version 2, Figur: darf auf dem Weg einmal um die Ecke gehen.
    Bend,
    /// Version 2, Figur: darf auf dem Weg ueber eine Figur springen.
    Hop,
    /// Version 2: die Fabrik darf einmal ein Feld diagonal rutschen (eigenes, verbundenes Land).
    FactoryShift,
    /// Version 2: der Koenig bekommt einen Boost (Stufe 1 bis 3), sofort und dauerhaft.
    KingBoost,
    /// Version 3, Figur: darf auf ihrem Weg ueber eigene Figuren hinwegziehen (Durchzug).
    Pass,
    /// Version 3, Figur: faerbt sofort alle freien Nachbarfelder in der eigenen Farbe (Siedler).
    Settle,
    /// Version 3: die Fabrik erntet Kat 1 (Eisen) dauerhaft doppelt (Schmiede).
    Smelter,
    /// Version 3: eine selbst gebaute Karte, Platz 0 bis `MAX_CUSTOM - 1`; ihre Teile, ihr Name
    /// und ihr Preis stehen in `CardTweaks` (Regel `custom.<platz>`).
    Custom(u8),
}

/// Wie viele selbst gebaute Karten es gleichzeitig geben darf.
pub const MAX_CUSTOM: u8 = 8;
const CUSTOM_KEYS: [&str; MAX_CUSTOM as usize] = [
    "custom0", "custom1", "custom2", "custom3", "custom4", "custom5", "custom6", "custom7",
];

/// Worauf eine Karte wirkt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardTarget {
    /// Auf den Spieler oder seine Fabriken; keine Figur noetig.
    Player,
    /// Auf eine eigene Figur (nicht den Koenig).
    Unit,
}

/// Kosten einer Karte. `k2_kinds`: davon mindestens so viele verschiedene Kat-2-Typen, also
/// Farben (jede Region hat genau einen Kat-2- und einen Kat-3-Typ in ihrer Farbe).
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
            Card::Shorten => "shorten",
            Card::Bend => "bend",
            Card::Hop => "hop",
            Card::FactoryShift => "factory_shift",
            Card::KingBoost => "king_boost",
            Card::Pass => "pass",
            Card::Settle => "settle",
            Card::Smelter => "smelter",
            Card::Custom(i) => CUSTOM_KEYS[usize::from(i).min(CUSTOM_KEYS.len() - 1)],
        }
    }

    /// Die Karten von Version 2 (nur im Gebietsmodus mit `terr_cards=1`).
    pub const V2: [Card; 5] = [
        Card::Shorten,
        Card::Bend,
        Card::Hop,
        Card::FactoryShift,
        Card::KingBoost,
    ];

    /// Die zusaetzlichen Karten von Version 3 (Kartensatz `terr_cardset3`); dazu kommen Karten aus
    /// Version 2 und die klassischen Doppelzug, Sturmangriff und Bergbau.
    pub const V3_NEW: [Card; 3] = [Card::Pass, Card::Settle, Card::Smelter];

    /// Der Vorrat, aus dem Version 3 den Markt zieht (dazu die selbst gebauten Karten).
    pub const V3_POOL: [Card; 11] = [
        Card::Shorten,
        Card::Bend,
        Card::Hop,
        Card::Pass,
        Card::Settle,
        Card::FactoryShift,
        Card::Charge,
        Card::Mining,
        Card::Smelter,
        Card::DoubleMove,
        Card::KingBoost,
    ];

    /// Bausteine fuer eigene Karten: was sich zu einer Karte zusammenstellen laesst.
    pub const BUILDING_BLOCKS: [Card; 8] = [
        Card::Shorten,
        Card::Bend,
        Card::Hop,
        Card::Pass,
        Card::Settle,
        Card::Charge,
        Card::Mining,
        Card::Smelter,
    ];

    pub fn from_key(key: &str) -> Option<Card> {
        if let Some(i) = CUSTOM_KEYS.iter().position(|k| *k == key) {
            return Some(Card::Custom(i as u8));
        }
        Card::ALL
            .into_iter()
            .chain(Card::V2)
            .chain(Card::V3_NEW)
            .find(|c| c.key() == key)
    }

    /// Wirkt die Karte sofort beim Kauf auf den Spieler (ohne Figur, ohne Platz neben der Fabrik)?
    /// Nur Version 3: Sturmangriff, Bergbau, Schmiede und Doppelzug.
    pub fn is_instant(self) -> bool {
        matches!(
            self,
            Card::Charge | Card::Mining | Card::Smelter | Card::DoubleMove
        )
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
            Card::Shorten => "Kürzer",
            Card::Bend => "Ecke",
            Card::Hop => "Sprung",
            Card::FactoryShift => "Fabrik rücken",
            Card::KingBoost => "Königs-Boost",
            Card::Pass => "Durchzug",
            Card::Settle => "Siedler",
            Card::Smelter => "Schmiede",
            Card::Custom(_) => "Eigene Karte",
        }
    }

    pub fn text(self) -> &'static str {
        match self {
            Card::Training => {
                "Eine Figur: Stufe +1, die Reichweite wächst mit. Preis je Reichweite der Figur."
            }
            Card::LongLegs => {
                "Eine Figur: Reichweite +1 (höchstens zweimal). Preis je Reichweite der Figur."
            }
            Card::DiagonalStrike => "Eine Figur schlägt zusätzlich diagonal auf ein Nachbarfeld.",
            Card::Lance => "Eine Figur schlägt bis zu zwei Felder weit geradeaus.",
            Card::Spring => "Eine Figur springt über eine Nachbarfigur auf das Feld dahinter.",
            Card::Horse => "Eine Figur springt auf jedes Feld im Abstand 2, über alles hinweg.",
            Card::Charge => "In diesem Zug haben alle deine Figuren Reichweite +1.",
            Card::Recruit => {
                "Ein Bauer erscheint neben der zahlenden Fabrik, die Fabrik bleibt stehen."
            }
            Card::DoubleMove => "In diesem Zug eine zweite Aktion.",
            Card::DoubleShift => "Rund um deine Fabriken wächst alles sofort nach und wird geerntet.",
            Card::Mining => "Deine Fabriken ernten Quader (Kat 2) doppelt.",
            Card::Farming => "Deine Fabriken ernten Bälle (Kat 3) doppelt.",
            Card::Runestone => "Set: der dritte Runenstein gibt einer Figur Stufe +2.",
            Card::Starshard => {
                "Set: der dritte Sternsplitter legt ein Kat-1-Vorkommen deiner Farbe unter den König."
            }
            Card::Shorten => "Die nächste Figur darf einen Schritt weniger ziehen (Stufe 3: 3 oder 2 Felder).",
            Card::Bend => "Die nächste Figur darf auf ihrem Weg einmal um die Ecke gehen.",
            Card::Hop => "Die nächste Figur darf auf ihrem Weg über eine Figur springen.",
            Card::FactoryShift => {
                "Deine Fabrik rückt einmal ein Feld diagonal (auf eigenes, verbundenes Land). Statt eines Zuges."
            }
            Card::KingBoost => {
                "Dein König wird besser: Stufe 1 auch diagonal, Stufe 2 zwei Felder gerade, Stufe 3 zwei vor und eins zur Seite."
            }
            Card::Pass => {
                "Die nächste Figur darf auf ihrem Weg über eigene Figuren hinwegziehen (Gegner versperren den Weg weiter)."
            }
            Card::Settle => {
                "Die nächste Figur färbt sofort alle freien Nachbarfelder in deiner Farbe (auch Land des Gegners)."
            }
            Card::Smelter => "Deine Fabrik erntet Eisen (Kat 1) dauerhaft doppelt.",
            Card::Custom(_) => "Eine selbst gebaute Karte.",
        }
    }

    /// Grundpreise, geeicht am Ausbau (10 Einheiten = 1 Stufe) und an der Kartenwert-Messung
    /// im README. Ausbildung und Lange Beine: je Reichweitenpunkt, siehe `cost_for`.
    pub fn cost(self) -> Cost {
        match self {
            // Je Reichweitenpunkt der Figur (`cost_for`): Bauer 4, Reichweite 3 also 12.
            Card::Training => cost(0, 2, 1, 2, 1),
            Card::LongLegs => cost(0, 1, 1, 2, 1),
            Card::DiagonalStrike => cost(0, 1, 1, 2, 1),
            Card::Lance => cost(0, 2, 1, 2, 1),
            Card::Spring => cost(0, 1, 1, 2, 1),
            Card::Horse => cost(0, 2, 1, 2, 1),
            // "Kinds" sind Farben, also Regionen: wertvolle Karten brauchen denselben Rohstoff
            // in mehreren Farben, einfache nur irgendeine.
            Card::Charge => cost(0, 0, 0, 3, 2),
            Card::Recruit => cost(1, 2, 1, 3, 1),
            Card::DoubleMove => cost(0, 2, 2, 3, 2),
            // Beliebig oft kaufbar: billiger als eine Ernte waere ein Geldautomat.
            Card::DoubleShift => cost(0, 2, 1, 2, 1),
            Card::Mining => cost(0, 3, 1, 2, 1),
            Card::Farming => cost(0, 2, 1, 3, 1),
            // Set: drei Teile, zusammen 18, jedes aus drei Farben. Zwei Stufen auf einer
            // Stufe-2-Figur kosten mit Ausbildung 8 + 12 = 20.
            Card::Runestone => cost(0, 3, 3, 3, 1),
            Card::Starshard => cost(0, 2, 2, 2, 1),
            // Version 2 hat eigene Preise (`Card::v2_price`); diese Zahlen sind nur ein Platzhalter.
            Card::Shorten
            | Card::Bend
            | Card::Hop
            | Card::FactoryShift
            | Card::KingBoost
            | Card::Pass
            | Card::Settle
            | Card::Smelter
            | Card::Custom(_) => cost(1, 0, 0, 0, 0),
        }
    }

    /// Ausbildung und Lange Beine: `cost()` gilt je Reichweitenpunkt der Zielfigur, siehe
    /// `cost_for`. Die Messung zeigt warum: Stufe +1 auf der ersten Stufe-3-Figur entscheidet die
    /// Partie, auf einem Bauern ist sie wenig wert.
    pub fn priced_by_range(self) -> bool {
        matches!(self, Card::Training | Card::LongLegs)
    }

    /// Preis fuer eine Zielfigur mit Reichweite `range`. Bei `priced_by_range` wird der
    /// Grundpreis mit der Reichweite malgenommen, und es braucht so viele Farben (Regionen) je
    /// Kategorie wie die Figur Reichweite hat, hoechstens 3. Eine Stufe-2-Figur auf Stufe 3 zu
    /// heben braucht also zwei Regionen, genau wie der Ausbau der Fabrik. Alle anderen Karten
    /// kosten immer `cost()`.
    pub fn cost_for(self, range: u8) -> Cost {
        self.cost_with(range, None)
    }

    /// Wie `cost_for`; `pawn_cost = Some(n)` gilt mit der Regel `kat_roles` (siehe `role_cost`).
    pub fn cost_with(self, range: u8, pawn_cost: Option<u8>) -> Cost {
        let base = match pawn_cost {
            Some(pawn) => self.role_cost(pawn),
            None => self.cost(),
        };
        if !self.priced_by_range() {
            return base;
        }
        let r = range.max(1);
        let kinds = |n: u8, k: u8| if n == 0 { 0 } else { k.max(r.min(3)).min(n) };
        let (k2, k3) = (base.k2 * r, base.k3 * r);
        Cost {
            k2,
            k2_kinds: kinds(k2, base.k2_kinds),
            k3,
            k3_kinds: kinds(k3, base.k3_kinds),
            ..base
        }
    }

    /// Preise mit der Regel `kat_roles`: Rekrut kostet nur Kat 2 (`pawn` Einheiten einer Farbe),
    /// Bewegungskarten nur Kat 3 (die Summe des alten Preises), alle anderen wie gehabt.
    pub fn role_cost(self, pawn: u8) -> Cost {
        let old = self.cost();
        match self {
            Card::Recruit => cost(0, pawn, 1, 0, 0),
            Card::DiagonalStrike | Card::Lance | Card::Spring | Card::Horse | Card::LongLegs => {
                cost(0, 0, 0, old.k2 + old.k3, old.k3_kinds)
            }
            _ => old,
        }
    }

    /// Bewegungskarten, die mit `kat_roles` auch auf den Koenig wirken duerfen.
    pub fn works_on_king(self) -> bool {
        matches!(
            self,
            Card::DiagonalStrike | Card::Lance | Card::Spring | Card::Horse
        )
    }

    /// Version 2: Preis in Einheiten Kat 1 und Kat 2 (bei Kartenfaktor 16; der Koenigs-Boost
    /// kostet je Stufe mehr, siehe `Game::territory_card_price_for`).
    pub fn v2_price(self) -> [u32; 2] {
        match self {
            Card::Shorten => [6, 0],
            Card::Bend => [10, 0],
            Card::Hop => [8, 0],
            Card::FactoryShift => [6, 0],
            Card::KingBoost => [0, 3],
            _ => [0, 0],
        }
    }

    /// Version 3 (`terr_cardset3`): Grundpreis in Einheiten Kat 1 (Eisen), Kat 2 (Gold) und Kat 3
    /// (Brot), ohne Kartenfaktor. Wer andere Preise will, setzt `price.<karte>=eisen/gold/brot`.
    /// Brot gibt es mit `terr_bonus` nicht im Lager; ein Brotpreis ist dann nie bezahlbar.
    pub fn v3_price(self) -> [u32; 3] {
        match self {
            Card::Shorten => [3, 0, 0],
            Card::Bend => [2, 1, 0],
            Card::Hop => [4, 0, 0],
            Card::Pass => [2, 0, 0],
            Card::Settle => [3, 2, 0],
            Card::FactoryShift => [3, 0, 0],
            Card::Charge => [2, 2, 0],
            Card::Mining => [2, 3, 0],
            Card::Smelter => [3, 2, 0],
            Card::DoubleMove => [4, 3, 0],
            Card::KingBoost => [0, 3, 0],
            _ => [0, 0, 0],
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
            | Card::Shorten
            | Card::Bend
            | Card::Hop
            | Card::Pass
            | Card::Settle
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
            Card::DiagonalStrike | Card::FactoryShift | Card::KingBoost => grid == GridKind::Square,
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
    draw_market_with(seed, size, grid, &[])
}

/// Wie `draw_market`, aber die Karten in `always` liegen immer aus (zusaetzlich zu den `size`
/// gezogenen). Mit `always` leer ist das Ergebnis dasselbe wie bei `draw_market`.
pub fn draw_market_with(seed: u64, size: usize, grid: GridKind, always: &[Card]) -> Vec<Card> {
    let mut deck: Vec<Card> = Card::ALL
        .into_iter()
        .filter(|c| c.available_on(grid) && !always.contains(c))
        .collect();
    let mut rng = Rng::new(seed ^ 0x6b61_7274_656e_0001);
    rng.shuffle(&mut deck);
    deck.truncate(size);
    deck.extend(always.iter().copied().filter(|c| c.available_on(grid)));
    deck.sort();
    deck
}

/// Karten im Gebietsmodus: nur Figurenfaehigkeiten, die die naechste gebaute Figur bekommt.
pub const TERRITORY_POOL: [Card; 5] = [
    Card::DiagonalStrike,
    Card::LongLegs,
    Card::Lance,
    Card::Spring,
    Card::Horse,
];

/// Markt im Gebietsmodus: `size` verschiedene Karten aus dem Vorrat, sortiert. Der Vorrat ist
/// `TERRITORY_POOL`, mit `v2` die Karten von Version 2 (`Card::V2`), mit `v3` zusaetzlich die von
/// Version 3 samt den selbst gebauten Karten (`Card::V3_POOL`, siehe `territory_pool`).
pub fn draw_market_territory(
    seed: u64,
    size: usize,
    grid: GridKind,
    v2: bool,
    kingup: bool,
) -> Vec<Card> {
    draw_market_pool(
        seed,
        size,
        territory_pool(grid, v2, false, kingup, &CardTweaks::default()),
    )
}

/// Wie `draw_market_territory`, mit Kartensatz von Version 3 (`v3`) und den Einstellungen des
/// Spielers (`tweaks`: Karten, die nicht gezogen werden, und selbst gebaute Karten).
pub fn draw_market_territory_with(
    seed: u64,
    size: usize,
    grid: GridKind,
    v2: bool,
    v3: bool,
    kingup: bool,
    tweaks: &CardTweaks,
) -> Vec<Card> {
    draw_market_pool(seed, size, territory_pool(grid, v2, v3, kingup, tweaks))
}

/// Der Vorrat, aus dem der Markt im Gebietsmodus gezogen wird. Karten mit `pool.<karte>=0` und
/// solche, die auf diesem Gitter nichts bewirken, fehlen; mit dem Koenigs-Upgrade
/// (`terr_kingup`) liegt der Boost immer aus und wird nicht gezogen.
pub fn territory_pool(
    grid: GridKind,
    v2: bool,
    v3: bool,
    kingup: bool,
    tweaks: &CardTweaks,
) -> Vec<Card> {
    let mut pool: Vec<Card> = if v3 {
        Card::V3_POOL.to_vec()
    } else if v2 {
        Card::V2.to_vec()
    } else {
        TERRITORY_POOL.to_vec()
    };
    if v3 {
        pool.extend(
            (0..MAX_CUSTOM)
                .map(Card::Custom)
                .filter(|c| tweaks.defined(*c)),
        );
    }
    pool.retain(|c| {
        tweaks.available_on(*c, grid)
            && !(kingup && *c == Card::KingBoost)
            && !tweaks.off.contains(c)
    });
    pool
}

fn draw_market_pool(seed: u64, size: usize, mut deck: Vec<Card>) -> Vec<Card> {
    let mut rng = Rng::new(seed ^ 0x6b61_7274_656e_0002);
    rng.shuffle(&mut deck);
    deck.truncate(size);
    deck.sort();
    deck
}

/// Eine selbst gebaute Karte (Version 3, "Baukasten"): ein Name, Bausteine mit Wiederholungen
/// (zum Beispiel `bend*2 + hop`) und ein eigener Preis in Kat 1 / 2 / 3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomCard {
    pub name: String,
    pub parts: Vec<(Card, u8)>,
    pub price: [u32; 3],
}

/// Was der Spieler an den Karten von Version 3 eingestellt hat: eigene Preise, Karten, die nicht
/// gezogen werden, und selbst gebaute Karten. Kommt ueber die Regeln `price.<karte>`,
/// `pool.<karte>` und `custom.<platz>` (siehe `CardTweaks::set`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardTweaks {
    pub price: BTreeMap<Card, [u32; 3]>,
    pub off: BTreeSet<Card>,
    pub custom: [Option<CustomCard>; MAX_CUSTOM as usize],
}

fn parse_price(text: &str) -> Result<[u32; 3], String> {
    let mut out = [0u32; 3];
    let parts: Vec<&str> = text.split('/').map(str::trim).collect();
    if parts.is_empty() || parts.len() > 3 {
        return Err(format!(
            "'{text}': Preis ist eisen/gold/brot (hoechstens drei Zahlen)"
        ));
    }
    for (i, part) in parts.iter().enumerate() {
        out[i] = part
            .parse::<u32>()
            .ok()
            .filter(|n| *n <= 99)
            .ok_or_else(|| format!("'{text}': '{part}' ist keine Zahl von 0 bis 99"))?;
    }
    Ok(out)
}

impl CardTweaks {
    /// Gehoert dieser Regelname hierher (`price.`, `pool.` oder `custom.`)?
    pub fn handles(key: &str) -> bool {
        key.starts_with("price.") || key.starts_with("pool.") || key.starts_with("custom.")
    }

    /// Setzt eine Regel: `price.bend=2/1/0` (Preis Eisen/Gold/Brot, leer = zurueck auf den
    /// Standard), `pool.bend=0` (Karte wird nicht gezogen; 1 = wird gezogen) oder
    /// `custom.0=Name|bend*2+hop|3/2/0` (eigene Karte auf Platz 0 bis 7; leer loescht sie).
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if let Some(name) = key.strip_prefix("price.") {
            let card = Card::from_key(name).ok_or_else(|| format!("'{key}': unbekannte Karte"))?;
            if value.is_empty() {
                self.price.remove(&card);
            } else {
                self.price.insert(card, parse_price(value)?);
            }
        } else if let Some(name) = key.strip_prefix("pool.") {
            let card = Card::from_key(name).ok_or_else(|| format!("'{key}': unbekannte Karte"))?;
            match value {
                "0" => self.off.insert(card),
                "1" => self.off.remove(&card),
                _ => return Err(format!("'{key}': 0 oder 1")),
            };
        } else if let Some(slot) = key.strip_prefix("custom.") {
            let slot: usize = slot
                .parse()
                .ok()
                .filter(|&i| i < usize::from(MAX_CUSTOM))
                .ok_or_else(|| format!("'{key}': Platz 0 bis {}", MAX_CUSTOM - 1))?;
            self.custom[slot] = if value.is_empty() {
                None
            } else {
                Some(parse_custom(value)?)
            };
        } else {
            return Err(format!("unbekannte Regel '{key}'"));
        }
        Ok(())
    }

    /// Gibt es diese Karte? Alle ausser den selbst gebauten Karten ohne Eintrag.
    pub fn defined(&self, card: Card) -> bool {
        match card {
            Card::Custom(i) => self.custom.get(usize::from(i)).is_some_and(|c| c.is_some()),
            _ => true,
        }
    }

    fn custom_of(&self, card: Card) -> Option<&CustomCard> {
        match card {
            Card::Custom(i) => self.custom.get(usize::from(i)).and_then(|c| c.as_ref()),
            _ => None,
        }
    }

    /// Die Bausteine einer Karte: eine normale Karte ist ihr eigener Baustein, eine selbst gebaute
    /// besteht aus mehreren (mit Wiederholungen).
    pub fn parts(&self, card: Card) -> Vec<(Card, u8)> {
        match card {
            Card::Custom(_) => self
                .custom_of(card)
                .map(|c| c.parts.clone())
                .unwrap_or_default(),
            _ => vec![(card, 1)],
        }
    }

    /// Worauf die Karte wirkt: eine selbst gebaute auf eine Figur, sobald ein Baustein eine braucht.
    pub fn target(&self, card: Card) -> CardTarget {
        match card {
            Card::Custom(_) => {
                if self
                    .parts(card)
                    .iter()
                    .any(|(c, _)| c.target() == CardTarget::Unit)
                {
                    CardTarget::Unit
                } else {
                    CardTarget::Player
                }
            }
            _ => card.target(),
        }
    }

    pub fn available_on(&self, card: Card, grid: GridKind) -> bool {
        match card {
            Card::Custom(_) => {
                self.defined(card) && self.parts(card).iter().all(|(c, _)| c.available_on(grid))
            }
            _ => card.available_on(grid),
        }
    }

    pub fn name(&self, card: Card) -> String {
        match self.custom_of(card) {
            Some(c) => c.name.clone(),
            None => card.name().to_string(),
        }
    }

    /// Text einer selbst gebauten Karte: woraus sie besteht.
    pub fn custom_text(&self, card: Card) -> Option<String> {
        let c = self.custom_of(card)?;
        let parts: Vec<String> = c
            .parts
            .iter()
            .map(|(p, n)| {
                if *n > 1 {
                    format!("{} ×{n}", p.name())
                } else {
                    p.name().to_string()
                }
            })
            .collect();
        Some(format!("Selbst gebaut aus: {}.", parts.join(" + ")))
    }

    /// Preis in Kat 1 / 2 / 3, wenn der Spieler ihn gesetzt hat (bei selbst gebauten Karten immer).
    pub fn price_override(&self, card: Card) -> Option<[u32; 3]> {
        match self.custom_of(card) {
            Some(c) => Some(c.price),
            None => self.price.get(&card).copied(),
        }
    }
}

fn parse_custom(value: &str) -> Result<CustomCard, String> {
    let fields: Vec<&str> = value.split('|').map(str::trim).collect();
    let [name, parts, price] = fields[..] else {
        return Err(format!(
            "'{value}': erwartet Name|baustein+baustein|eisen/gold/brot"
        ));
    };
    if name.is_empty() || name.chars().count() > 24 || name.contains(['=', '|', ',']) {
        return Err(format!("'{name}': Name mit 1 bis 24 Zeichen, ohne = und |"));
    }
    let mut out: Vec<(Card, u8)> = Vec::new();
    for part in parts.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        let (key, times) = match part.split_once('*') {
            Some((k, n)) => (
                k.trim(),
                n.trim()
                    .parse::<u8>()
                    .ok()
                    .filter(|n| (1..=3).contains(n))
                    .ok_or_else(|| format!("'{part}': Wiederholungen von 1 bis 3"))?,
            ),
            None => (part, 1),
        };
        let card = Card::from_key(key).ok_or_else(|| format!("'{key}': unbekannter Baustein"))?;
        if !Card::BUILDING_BLOCKS.contains(&card) {
            return Err(format!(
                "'{key}' laesst sich nicht in eine eigene Karte einbauen (erlaubt: {})",
                Card::BUILDING_BLOCKS
                    .iter()
                    .map(|c| c.key())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        match out.iter_mut().find(|(c, _)| *c == card) {
            Some((_, n)) => *n = (*n + times).min(3),
            None => out.push((card, times)),
        }
    }
    if out.is_empty() {
        return Err(format!("'{value}': mindestens ein Baustein"));
    }
    Ok(CustomCard {
        name: name.to_string(),
        parts: out,
        price: parse_price(price)?,
    })
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
        // Kat 2: Typen 1, 4, 7, 10 (je Region einer); Kat 3: Typen 2, 5, 8, 11.
        let stock = inv(&[(1, 5), (4, 1), (2, 4), (5, 1)]);
        let cost = Cost {
            k2: 2,
            k3: 2,
            ..Cost::default()
        };
        let pay = plan_payment(&m, r(0), &stock, &cost).expect("reicht");
        assert_eq!(pay, inv(&[(1, 2), (2, 2)]));
        let mixed = Cost {
            k2: 3,
            k2_kinds: 2,
            ..Cost::default()
        };
        let pay = plan_payment(&m, r(0), &stock, &mixed).expect("reicht");
        assert_eq!(
            pay,
            inv(&[(1, 2), (4, 1)]),
            "zwei Farben, der Rest von der haeufigsten"
        );
    }

    #[test]
    fn costs_with_kinds_need_different_types() {
        let m = map();
        let cost = Card::Training.cost_for(2);
        assert_eq!(
            (cost.k2, cost.k2_kinds, cost.k3, cost.k3_kinds),
            (4, 2, 4, 2)
        );
        let one_type = inv(&[(1, 10), (2, 10), (5, 10)]);
        assert!(
            plan_payment(&m, r(0), &one_type, &cost).is_none(),
            "Kat 2 nur in einer Farbe"
        );
        let varied = inv(&[(1, 10), (4, 1), (2, 10), (5, 1)]);
        assert_eq!(
            plan_payment(&m, r(0), &varied, &cost),
            Some(inv(&[(1, 3), (4, 1), (2, 3), (5, 1)]))
        );
        let ritter = Card::Training.cost_for(3);
        assert_eq!((ritter.k2, ritter.k2_kinds, ritter.k3_kinds), (6, 3, 3));
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
        assert!(Card::Training.cost_for(2).units() >= 8);
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
