//! Aktionen, Ereignisse und Fehler.

use crate::cards::Card;
use crate::ids::{CellId, FactoryId, PieceId, PlayerId, ResourceId};
use crate::state::{Inventory, Outcome};

/// Woraus eine Karte bezahlt wird.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaySource {
    /// Gepaeck des Koenigs.
    King,
    /// Lager einer eigenen Fabrik.
    Factory(FactoryId),
}

/// Pro Zug genau eine Aktion (dazu hoechstens ein Kartenkauf).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Figur (auch der Koenig) auf ein Feld ziehen; steht dort ein Gegner, wird er geschlagen.
    Move { piece: PieceId, to: CellId },
    /// Der Koenig baut auf seinem Feld eine Fabrik. Dafuer wird das Kat-1-Vorkommen `pay` auf
    /// diesem Feld dauerhaft verbraucht; es liegt danach als eine Einheit im Lager.
    BuildFactory { pay: ResourceId },
    /// Fabrik aufloesen und daraus eine Figur machen.
    SpawnPiece { factory: FactoryId },
    /// Eine Karte aus dem Markt kaufen (hoechstens einmal pro Zug, zusaetzlich zur Aktion).
    /// `target`: die Figur oder der Koenig, auf die die Karte wirkt.
    BuyCard {
        card: Card,
        from: PaySource,
        target: Option<PieceId>,
    },
    /// Nur erlaubt, wenn es sonst keine Aktion gibt.
    Pass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Moved {
        piece: PieceId,
        from: CellId,
        to: CellId,
    },
    Captured {
        piece: PieceId,
        owner: PlayerId,
        at: CellId,
        by: PlayerId,
    },
    FactoryBuilt {
        factory: FactoryId,
        owner: PlayerId,
        cell: CellId,
    },
    FactoryDestroyed {
        factory: FactoryId,
        owner: PlayerId,
        cell: CellId,
        by: PlayerId,
        loot: Inventory,
    },
    Spawned {
        piece: PieceId,
        owner: PlayerId,
        factory: FactoryId,
        cell: CellId,
        level: u8,
        range: u8,
    },
    Collected {
        player: PlayerId,
        cell: CellId,
        gained: Inventory,
    },
    Unloaded {
        player: PlayerId,
        factory: FactoryId,
        amount: Inventory,
    },
    FactoryYield {
        factory: FactoryId,
        gained: Inventory,
    },
    PlayerEliminated {
        player: PlayerId,
    },
    Passed {
        player: PlayerId,
    },
    GameOver {
        outcome: Outcome,
    },
    CardBought {
        player: PlayerId,
        card: Card,
        from: PaySource,
        target: Option<PieceId>,
        paid: Inventory,
    },
    /// Ein Schild (oder die Koenigsruestung) hat einen Schlag abgehalten.
    ShieldBlocked {
        piece: PieceId,
        owner: PlayerId,
        at: CellId,
        by: PlayerId,
    },
    /// Rekrut: ein Bauer neben der Fabrik.
    Recruited {
        piece: PieceId,
        owner: PlayerId,
        factory: FactoryId,
        cell: CellId,
    },
    /// Sternsplitter: neues Kat-1-Vorkommen.
    DepositCreated {
        cell: CellId,
        resource: ResourceId,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IllegalReason {
    NoSuchPiece,
    NotYourPiece,
    CellOutsideBoard,
    OccupiedByOwnPiece,
    CannotReach,
    UnknownResource,
    PayTypeIsNotCategory1,
    NoCategory1DepositHere,
    FactoryAlreadyHere,
    TooCloseToAnotherFactory,
    NoSuchFactory,
    NotYourFactory,
    FactoryHasNoCategory1,
    SpawnCellOccupied,
    PassNotAllowed,
    CardNotInMarket,
    AlreadyBoughtThisTurn,
    CannotPayCard,
    InvalidCardTarget,
    /// Sollte nie vorkommen: die Aktion ist unzulaessig, aber keine Einzelregel erklaert es.
    Unspecified,
}

impl std::fmt::Display for IllegalReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            IllegalReason::NoSuchPiece => "diese Figur gibt es nicht",
            IllegalReason::NotYourPiece => "die Figur gehoert einem anderen Spieler",
            IllegalReason::CellOutsideBoard => "das Feld liegt nicht auf der Karte",
            IllegalReason::OccupiedByOwnPiece => "auf dem Zielfeld steht eine eigene Figur",
            IllegalReason::CannotReach => "die Figur kann das Zielfeld nicht erreichen",
            IllegalReason::UnknownResource => "diesen Rohstoff gibt es nicht",
            IllegalReason::PayTypeIsNotCategory1 => {
                "bezahlt werden kann nur mit einem Kat-1-Rohstoff"
            }
            IllegalReason::NoCategory1DepositHere => {
                "auf dem Feld des Koenigs liegt kein solches Kat-1-Vorkommen"
            }
            IllegalReason::FactoryAlreadyHere => "hier steht schon eine Fabrik",
            IllegalReason::TooCloseToAnotherFactory => "zu nah an einer anderen Fabrik",
            IllegalReason::NoSuchFactory => "diese Fabrik gibt es nicht",
            IllegalReason::NotYourFactory => "die Fabrik gehoert einem anderen Spieler",
            IllegalReason::FactoryHasNoCategory1 => "die Fabrik hat keinen Kat-1-Rohstoff im Lager",
            IllegalReason::SpawnCellOccupied => "das Fabrikfeld ist besetzt",
            IllegalReason::PassNotAllowed => "Passen ist nur ohne andere Aktion erlaubt",
            IllegalReason::CardNotInMarket => "diese Karte liegt nicht im Markt",
            IllegalReason::AlreadyBoughtThisTurn => "in diesem Zug wurde schon eine Karte gekauft",
            IllegalReason::CannotPayCard => "die Rohstoffe an diesem Ort reichen nicht",
            IllegalReason::InvalidCardTarget => {
                "die Karte kann so nicht wirken (Figur nicht am Zahlfeld oder schon verbessert)"
            }
            IllegalReason::Unspecified => "unzulaessige Aktion",
        };
        write!(f, "{text}")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleError {
    GameOver,
    Illegal(IllegalReason),
}

impl std::fmt::Display for RuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuleError::GameOver => write!(f, "das Spiel ist schon vorbei"),
            RuleError::Illegal(r) => write!(f, "unzulaessiger Zug: {r}"),
        }
    }
}

impl std::error::Error for RuleError {}
