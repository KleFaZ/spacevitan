//! Typisierte IDs, damit Zellen, Spieler, Figuren usw. nicht verwechselt werden koennen.

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident($inner:ty)) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub $inner);

        impl $name {
            pub fn idx(self) -> usize {
                self.0 as usize
            }
        }
    };
}

id_type!(
    /// Ein Feld der Karte.
    CellId(u32)
);
id_type!(
    /// Ein Spieler (0-basiert, in Zugreihenfolge).
    PlayerId(u8)
);
id_type!(
    /// Eine Figur (inklusive Koenig).
    PieceId(u32)
);
id_type!(
    /// Eine Fabrik.
    FactoryId(u32)
);
id_type!(
    /// Ein Rohstofftyp.
    ResourceId(u16)
);
