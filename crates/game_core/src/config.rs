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
    /// Gebietsmodus: Felder sind neutral, nur die Startecke gehoert dem Spieler und hat die
    /// einzige Fabrik. Jede Figur faerbt beim Ziehen den Weg in ihrer Farbe; die Fabrik sammelt
    /// jede Runde die zusammenhaengenden Felder ein (siehe `territory.rs`).
    pub territory: bool,
    /// Gebiet: Flaeche, die ein Bauer (Stufe 1) kostet.
    pub terr_pawn: u32,
    /// Gebiet: Flaeche, die Stufe 2 zusaetzlich zu den Rohstoffen kostet.
    pub terr_area: u32,
    /// Gebiet: Aufschlag auf die Flaeche je weitere Stufe (Stufe 3: `terr_area + step`,
    /// Stufe 4: `terr_area + 2 * step`). 0 = alle hoeheren Stufen kosten gleich viel Flaeche.
    pub terr_area_step: u32,
    /// Gebiet: Kat 1 fuer Stufe 2 und mehr, Kat 2 ab Stufe 3, Kat 3 fuer Stufe 4 (verbraucht,
    /// die Preise der Stufen sind kumulativ).
    pub terr_k1: u32,
    pub terr_k2: u32,
    pub terr_k3: u32,
    /// Gebiet: Einheiten je Vorkommen auf zusammenhaengenden Feldern und Runde.
    pub terr_yield: u32,
    /// Gebiet: Abgetrenntes Land geht an den, der durchtrennt hat (`true`), sonst wird es neutral.
    pub terr_cut_flips: bool,
    /// Gebiet: Eine neutrale Flaeche, die ganz von einer Farbe umschlossen ist, faellt an diese
    /// Farbe.
    pub terr_enclose: bool,
    /// Gebiet: Der Brettrand zaehlt beim Umschliessen als Wand (`true`); sonst (Standard) wird eine
    /// neutrale Flaeche, die den Rand beruehrt, nie umschlossen.
    pub terr_enclose_edge: bool,
    /// Gebiet: Eine abgetrennte Figur wird aktiv und faerbt ihr Feld, sobald sie an ihr
    /// verbundenes Land grenzt.
    pub terr_reconnect: bool,
    /// Gebiet: Kartenpreise sind die alten Kat-2/3-Preise mal diese Zahl (nur nach Kategorie).
    pub terr_card_mult: u32,
    /// Version 2: Figuren ziehen geradeaus und genau so viele Felder, wie ihre Stufe angibt
    /// (wie ein Turm mit festem Schritt); Karten erlauben eine Ecke, einen Sprung oder einen
    /// Schritt weniger.
    pub terr_exact: bool,
    /// Version 2: Stufen kosten nur Flaeche und verdoppeln sich je Stufe (Stufe n kostet
    /// `terr_pawn * terr_grow^(n-1)`). 0 = Preise wie in Version 1.
    pub terr_grow: u32,
    /// Version 2: Ein Vorkommen der Kategorie 1 bis 4 gibt erst Ertrag, wenn das Feld so viele
    /// Runden dem Besitzer gehoert (0 = sofort). Ein umgefaerbtes Feld faengt von vorn an.
    pub terr_ripen: [u32; MAX_CATEGORIES],
    /// Version 2: Es gibt hoechstens so viele Rohstoffkategorien (0 = wie sonst 3 oder 4).
    pub terr_cats: u32,
    /// Version 2: 1 = Kartensatz von Version 2 (Kuerzer, Ecke, Sprung, Fabrik ruecken,
    /// Koenigs-Boost), 0 = Karten wie in Version 1.
    pub terr_cards: u32,
    /// Version 3: Jedes vom Besitzer geerntete Feld hat einen Erntezaehler. Ein Vorkommen der
    /// Kategorie 1 bis 4 wird alle `terr_ripen` Runden einmal geerntet (der Zaehler laeuft nur
    /// bei stehender Fabrik und verbundenem Feld und faengt beim Umfaerben von vorn an).
    pub terr_cycle: bool,
    /// Version 3: 1 = der Koenig darf die Fabrik versetzen und nach einer Zerstoerung neu bauen.
    pub terr_move: u32,
    /// Version 3: Kosten des Umzugs aus dem Lager der Fabrik, Kat 1 und Kat 2.
    pub terr_move_k1: u32,
    pub terr_move_k2: u32,
    /// Version 3: Bauzeit in Runden, wenn die Fabrik zerstoert wurde (Neubau ohne Kosten).
    /// Beim Umzug entspricht die Bauzeit der Entfernung zum alten Platz.
    pub terr_rebuild: u32,
    /// Version 3.2: auch nach einer Zerstoerung zaehlt die Entfernung: der Neubau dauert so lange
    /// wie ein Umzug von der alten Fabrik zum Koenig (halbe Entfernung, aufgerundet, mindestens
    /// 1), kostenlos und nur auf eigenem Land. `terr_rebuild` gilt dann nur noch, wenn der alte
    /// Platz unbekannt ist. 0 = fest `terr_rebuild` Runden, egal wo.
    pub terr_rebuild_dist: bool,
    /// Version 3 (alte Fassung): Kat 3 laesst sich gegen Flaeche tauschen: `k` Einheiten im Lager
    /// ergeben einmalig `2^k` Flaeche. Geht nur ohne `terr_bonus` (Kat 3 wird dann geerntet).
    pub terr_trade: bool,
    /// Version 3: Kat 3 wird nicht geerntet, sondern zaehlt als Bonus. Am Ende jedes eigenen Zuges
    /// bringen die `k` Kat-3-Felder auf dem mit der stehenden Fabrik verbundenen Land zusaetzlich
    /// `2^k` Flaeche (`k = 0` bringt nichts).
    pub terr_bonus: bool,
    /// Version 3: Der Erntezaehler eines Feldes laeuft nur, solange am Ende des eigenen Zuges
    /// eine eigene Figur (auch der Koenig) auf dem verbundenen Feld steht; ohne Figur steht er
    /// still (und erst mit ihr gibt es Ernte). Braucht `terr_cycle`.
    pub terr_occupy: bool,
    /// Version 3: Karten wirken auf eine gewaehlte eigene Einheit, auch auf eine schon gebaute
    /// (Kuerzer, Ecke, Sprung). Sie liegen nicht mehr neben der Fabrik; nur "Fabrik ruecken" liegt
    /// dort noch, bis sie benutzt wird. Braucht den Kartensatz von Version 2 (`terr_cards`).
    pub terr_cardany: bool,
    /// Version 3: Figurenpreise ab Stufe 2 mal diesen Faktor (Stufe 1 bleibt bei `terr_pawn`).
    /// Nur mit `terr_grow >= 2`; 1 = keine Wirkung.
    pub terr_up_mult: u32,
    /// Version 3: Was eine Verbesserung durch eine Karte kostet: 0 = nichts, 1 = die verbesserte
    /// Figur (auch der Koenig beim Boost) darf in diesem Zug nicht ziehen, 2 = der Kauf beendet
    /// den ganzen Zug. Gilt fuer Karten mit Figur als Ziel und den Koenigs-Boost.
    pub terr_cardlock: u32,
    /// Version 3: Zum Bearbeiten eines Feldes (Erntezaehler, Ernte) genuegt es, wenn es im
    /// Einflussbereich einer eigenen Figur liegt (geradeaus bis zur Reichweite, bis zur ersten
    /// Figur). Braucht `terr_occupy`.
    pub terr_control: bool,
    /// Kartengenerator: Kat-3-Vorkommen liegen moeglichst gleichmaessig verteilt (groesster
    /// Abstand zu den schon gesetzten) statt in zufaelligen Adern.
    pub kat3_spread: bool,
    /// Version 3: Kuerzer, Ecke und Sprung duerfen mehrmals auf dieselbe Figur gelegt werden, so
    /// oft es logisch geht (Stufe `n`: hoechstens `n - 1` mal, sonst gaebe es nichts mehr zu
    /// kuerzen, zu knicken oder zu ueberspringen).
    pub terr_stack: bool,
    /// Version 3: jede Figur kostet Unterhalt, ihre Stufe in Flaeche je Runde (der Koenig nichts).
    /// Er wird am Ende des eigenen Zuges mit dem Einkommen verrechnet, solange die Fabrik steht.
    /// Rutscht die Flaeche dabei unter 0, loest sich so lange eine Figur auf, bis es wieder
    /// reicht (`Game::territory_upkeep`).
    pub terr_upkeep: bool,
    /// Version 3: der Koenig wird direkt mit Kat 2 (Gold) besser, ohne Karte: der Koenigs-Boost
    /// liegt dann immer aus und kostet je Stufe so viele Kat 2, wie der Koenig dadurch
    /// zusaetzliche Zugfelder bekommt (`Game::king_boost_extra`).
    pub terr_kingup: bool,
    /// Kat 2 je zusaetzlichem Zugfeld beim Koenigs-Upgrade (Standard 1).
    pub terr_kingup_price: u32,
    /// Version 3: die Karte Ecke gilt nur fuer die letzten Felder des Weges: mit einer Karte darf
    /// das letzte Feld um die Ecke gehen, mit zwei Karten das letzte und das vorletzte, und so
    /// weiter. Aus: die Ecke darf irgendwo auf dem Weg liegen (Version 2).
    pub terr_bend_tail: bool,
    /// Version 3: erweiterter Kartensatz (`Card::V3_POOL`: Kuerzer, Ecke, Sprung, Durchzug, Siedler,
    /// Fabrik ruecken, Sturmangriff, Bergbau, Schmiede, Doppelzug, Koenigs-Boost) mit festen
    /// Preisen in Eisen / Gold / Brot (`Card::v3_price`, ohne Kartenfaktor) und selbst gebauten
    /// Karten. Braucht `terr_cards=1`.
    pub terr_cardset3: bool,
    /// Version 3.1: Sturmangriff und Doppelzug wirken erst im naechsten eigenen Zug. Gekauft wird
    /// beim jetzigen Zug, der Gegner sieht die Karte bereit liegen und kann reagieren.
    pub terr_delay: bool,
    /// Version 3.1: der Unterhalt einer Figur ist ihre Stufe im Quadrat (Stufe 4 = 16 Flaeche je
    /// Runde) statt ihre Stufe. Der Koenig zahlt weiterhin nichts. Braucht `terr_upkeep`.
    pub terr_upkeep_sq: bool,
    /// Version 3.9: Unterhalt nach Anzahl statt nach Stufe. Alle Figuren zusammen kosten `(k * n)^2`
    /// Flaeche je Runde, `n` ist die Zahl der Figuren (ohne Koenig, jede zaehlt gleich, egal welche
    /// Stufe), `k` dieser Wert (2 = `(2n)^2`: eine Figur 4, zwei 16, drei 36, vier 64). Das zwingt zu
    /// grossen Flaechen oder kleinen Heeren, und starke Figuren kosten nicht mehr als schwache. Reicht die
    /// Flaeche nicht, loest sich zuerst die schwaechste Figur auf (bei gleicher Stufe die zuletzt gebaute).
    /// 0 = aus (Standard); braucht `terr_upkeep`, ersetzt den Unterhalt nach Stufe und geht nicht mit
    /// `terr_upkeep_sq`.
    pub terr_upkeep_cnt: u32,
    /// Version 3.9: Spieler, die sich bei zu wenig Flaeche fuer den Unterhalt die Figur zum Aufloesen selbst
    /// aussuchen (Bit `i` = Spieler `i`; Standard 0). Alle anderen loesen automatisch auf, zuerst die
    /// schwaechste Figur. Die Oberflaeche setzt die Bits der Menschen. Braucht `terr_upkeep`.
    pub terr_upkeep_pick: u32,
    /// Version 3.10: Hungersnot. Jede `terr_famine`-te Runde (N, 2N, 3N ...) rechnet der Unterhalt nach Anzahl
    /// mit `terr_famine_k` statt `terr_upkeep_cnt` (zum Beispiel k=1 gegen k=2). Gilt fuer alle Spieler gleich und
    /// laesst sich voraussagen. 0 = aus (Standard); braucht `terr_upkeep_cnt`.
    pub terr_famine: u32,
    /// Version 3.10: das k der Hungersnot-Runde (Standard 2).
    pub terr_famine_k: u32,
    /// Version 3.10 (Kampagne): Flaeche, mit der Spieler A (der Mensch) beginnt, zum Beispiel als Belohnung aus
    /// dem vorigen Level. Die anderen Spieler beginnen wie immer mit 0. Standard 0.
    pub camp_bonus_area: u32,
    /// Version 3.1: eine Figur der Stufe `n` auf einem Vorkommen erntet `n` mal so viel je Ernte
    /// (der Takt bleibt, Karten wie Schmiede addieren danach ihr +1). Der Koenig erntet einfach.
    /// Braucht `terr_occupy` (die Figur steht auf dem Feld).
    pub terr_yield_lvl: bool,
    /// Version 3.1: Brot als Lager. Mit `n >= 1` wird Kat 3 (Brot) nicht mehr als Flaechenbonus
    /// gezaehlt, sondern wie Eisen und Gold alle `n` Runden geerntet (nur mit eigener Figur auf dem
    /// Feld) und liegt im Lager der Fabrik. Karten koennen dann in Brot bezahlt werden, die
    /// Standardpreise sind dann ueber Eisen, Gold und Brot verteilt (`Card::v3_price_bread`).
    /// Ab Version 3.3 ist das mit `n = 3` (gleicher Takt wie Eisen) der Standard von `set_v3`;
    /// 0 = aus (Kat 3 zaehlt wieder als Flaechenbonus `2^k`).
    pub terr_bread: u32,
    /// Version 3.3: die Karte Fort liegt im Markt. Sie setzt ein Fort auf das Feld einer gewaehlten
    /// eigenen Figur; eine eigene Figur (nie der Koenig) auf dem Fort zieht `n` oder `n + 1` Felder.
    /// Braucht `terr_cardset3` und `terr_cardany`. Standard aus.
    pub terr_fort: bool,
    /// Version 3.6: die Karte Kamikaze liegt immer im Markt. Die gewaehlte Figur setzt nach dem Kauf
    /// eine Runde aus, brennt und darf im naechsten eigenen Zug einmal angreifen (eine gegnerische
    /// Figur genau in Reichweite, eine gegnerische davor darf sie ueberspringen); sie stirbt dabei,
    /// der Weg wird neutral. Braucht `terr_cardset3`, `terr_cardany` und `terr_exact`. Standard aus.
    pub terr_kamikaze: bool,
    /// Version 3.4: Gelaende. Jedes Feld ist Feld (Flaechenwert 1), Erde (1/2) oder Moor (1/4); das
    /// zaehlt im Einkommen an Flaeche (siehe `terrain`). Standard aus.
    pub terr_terrain: bool,
    /// Version 3.9: im Gebietsmodus beliebig viele Karten je Zug kaufen (statt einer), solange das Lager
    /// bezahlen kann und die Karte noch passt (ein Kartenplatz neben der Fabrik, eine Figur, die sie
    /// aufnehmen kann). Jede Figur, die eine Karte bekommt, ruht in diesem Zug (`terr_cardlock = 1`).
    /// Geht nicht mit `terr_cardlock = 2` (dort ist eine Verbesserung schon der ganze Zug). Standard aus.
    pub terr_multibuy: bool,
    /// Kampagne: Spieler A bekommt die besseren Flaechen rund um seine Ecke. Der Generator waehlt dann unter
    /// seinen Verteilungen nicht die gleichmaessigste, sondern die, die A am meisten bevorzugt. Braucht
    /// `terr_terrain`. Standard aus.
    pub terr_favor: bool,
    /// Kampagne: welche Rohstoffkategorien als Vorkommen auf der Karte bleiben (Bit 0 = Eisen, 1 = Gold, 2 = Brot,
    /// 3 = Kat 4). 15 = alle (Standard), 0 = keine Vorkommen, 1 = nur Eisen. Die Karte wird wie sonst gewuerfelt, danach
    /// fallen die nicht gewaehlten Kategorien weg (fuer alle Ecken gleich, also bleibt sie fair).
    pub terr_dep: u32,
    /// Eigene Preise, Karten, die nicht gezogen werden, und selbst gebaute Karten (Version 3).
    pub card_tweaks: crate::cards::CardTweaks,
    /// Brettform unabhaengig von der Spielerzahl: 0 = wie bisher nach Spielerzahl, sonst die
    /// Eckenzahl 3 (Dreieck), 4 (Quadrat) oder 5 bis 8 (Vieleck aus Sechsecken). Dann gilt
    /// `players <= board_corners`, und die Ecken der Spieler waehlt `fair_corners`.
    pub board_corners: u8,
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
    /// Kat 3 zaehlt als Flaechenbonus `2^k` (und wird nicht geerntet): `terr_bonus` ohne Brot-Lager.
    pub fn kat3_bonus_on(&self) -> bool {
        self.terr_bonus && self.terr_bread == 0
    }

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
            score_after: 250,
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
            territory: false,
            terr_pawn: 50,
            terr_area: 150,
            terr_area_step: 50,
            terr_k1: 20,
            terr_k2: 40,
            terr_k3: 60,
            terr_yield: 1,
            terr_cut_flips: true,
            terr_enclose: true,
            terr_enclose_edge: false,
            terr_reconnect: true,
            terr_card_mult: 16,
            terr_exact: false,
            terr_grow: 0,
            terr_ripen: [0; MAX_CATEGORIES],
            terr_cats: 0,
            terr_cards: 0,
            terr_cycle: false,
            terr_move: 0,
            terr_move_k1: 0,
            terr_move_k2: 0,
            terr_rebuild: 0,
            terr_rebuild_dist: true,
            terr_trade: false,
            terr_bonus: false,
            terr_occupy: false,
            terr_cardany: false,
            terr_up_mult: 1,
            terr_cardlock: 0,
            terr_control: false,
            kat3_spread: false,
            terr_stack: false,
            terr_upkeep: false,
            terr_kingup: false,
            terr_kingup_price: 1,
            terr_bend_tail: false,
            terr_cardset3: false,
            terr_delay: false,
            terr_upkeep_sq: false,
            terr_upkeep_cnt: 0,
            terr_upkeep_pick: 0,
            terr_famine: 0,
            terr_famine_k: 2,
            camp_bonus_area: 0,
            terr_yield_lvl: false,
            terr_bread: 0,
            terr_fort: false,
            terr_kamikaze: false,
            terr_terrain: false,
            terr_multibuy: false,
            terr_favor: false,
            terr_dep: 15,
            card_tweaks: crate::cards::CardTweaks::standard(),
            board_corners: 0,
        }
    }

    /// Schaltet die Regeln von Version 3 ein (`on`; sie bauen auf Version 2 auf) oder alles bis
    /// zu Version 1 aus: Erntezaehler je Vorkommen, Umzug und Neubau der Fabrik durch den Koenig,
    /// weniger Rohstoffe, dazu Kat 3 als Flaechenbonus (`2^k` je Runde). Einzeln lassen sie sich
    /// danach wieder aendern.
    pub fn set_v3(&mut self, on: bool) {
        self.set_v2(on);
        self.terr_cycle = on;
        self.terr_move = u32::from(on);
        // Der Umzug der Fabrik kostet keine Rohstoffe (nur Zeit: halbe Entfernung, kein Ertrag).
        self.terr_move_k1 = 0;
        self.terr_move_k2 = 0;
        self.terr_rebuild = if on { 3 } else { 0 };
        self.terr_trade = false;
        self.terr_bonus = on;
        // Brot ist ein Lager mit Erntezaehler wie Eisen (alle 3 Runden, Figur muss draufstehen);
        // `terr_bread=0` bringt den alten 2^k-Flaechenbonus zurueck.
        self.terr_bread = if on { 3 } else { 0 };
        self.terr_occupy = on;
        self.terr_cardany = on;
        // Figuren ab Stufe 2 doppelt so teuer (50 / 200 / 400 / 800 Flaeche); eine Karte sperrt die
        // verbesserte Figur fuer den Zug; Kontrollfelder bleiben ein Schalter (Standard aus).
        self.terr_up_mult = if on { 2 } else { 1 };
        self.terr_cardlock = u32::from(on);
        self.terr_control = false;
        self.kat3_spread = on;
        self.terr_stack = on;
        // Figuren kosten Unterhalt (Stufe je Runde); der Koenig wird mit Kat 2 direkt besser.
        self.terr_upkeep = on;
        self.terr_kingup = on;
        // Karten kosten in Version 3 die Haelfte des Grundpreises (16 = voll).
        self.terr_card_mult = if on { 8 } else { 16 };
        // Erweiterter Kartensatz mit festen Preisen; die Ecke gilt nur fuer die letzten Felder.
        self.terr_cardset3 = on;
        self.terr_bend_tail = on;
        if on {
            self.terr_cats = 3;
            self.terr_ripen = [3, 5, 10, 0];
            // Deutlich weniger Rohstoffe als Version 2: je Ecke ein Kat-1-Feld, ein Kat-2-Feld
            // und zwei einzelne Kat-3-Felder (gleichmaessig ueber das Brett verteilt), dazu zwei
            // Zentrums-Felder Kat 1.
            self.veins_per_type = [1, 1, 2, 1];
            self.cells_per_type = [1, 1, 1, 2];
            self.center_cells = 2;
            self.center_mix = 0;
        } else {
            let d = Config::new(self.players, self.seed);
            self.veins_per_type = d.veins_per_type;
            self.cells_per_type = d.cells_per_type;
            self.center_cells = d.center_cells;
            self.center_mix = d.center_mix;
        }
    }

    /// Schaltet alle Regeln von Version 2 auf einmal ein (`on`) oder aus: genau-n-Bewegung,
    /// Flaechenpreise 50/100/200/400, Reifezeit 3 und 10 Runden, nur zwei Rohstoffkategorien und
    /// der neue Kartensatz. Einzeln lassen sie sich danach wieder aendern.
    pub fn set_v2(&mut self, on: bool) {
        self.terr_exact = on;
        self.terr_grow = if on { 2 } else { 0 };
        self.terr_ripen = if on {
            [3, 10, 0, 0]
        } else {
            [0; MAX_CATEGORIES]
        };
        self.terr_cats = if on { 2 } else { 0 };
        self.terr_cards = u32::from(on);
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
            "territory" => self.territory = value != 0,
            "terr_pawn" => self.terr_pawn = as_u32(value)?,
            "terr_area" => self.terr_area = as_u32(value)?,
            "terr_area_step" => self.terr_area_step = as_u32(value)?,
            "terr_k1" => self.terr_k1 = as_u32(value)?,
            "terr_k2" => self.terr_k2 = as_u32(value)?,
            "terr_k3" => self.terr_k3 = as_u32(value)?,
            "terr_yield" => self.terr_yield = as_u32(value)?,
            "terr_cut_flips" => self.terr_cut_flips = value != 0,
            "terr_enclose" => self.terr_enclose = value != 0,
            "terr_enclose_edge" => self.terr_enclose_edge = value != 0,
            "terr_reconnect" => self.terr_reconnect = value != 0,
            "terr_card_mult" => self.terr_card_mult = as_u32(value)?,
            "terr_v2" => self.set_v2(value != 0),
            "terr_exact" => self.terr_exact = value != 0,
            "terr_grow" => self.terr_grow = as_u32(value)?,
            "terr_ripen1" => self.terr_ripen[0] = as_u32(value)?,
            "terr_ripen2" => self.terr_ripen[1] = as_u32(value)?,
            "terr_ripen3" => self.terr_ripen[2] = as_u32(value)?,
            "terr_ripen4" => self.terr_ripen[3] = as_u32(value)?,
            "terr_cats" => self.terr_cats = as_u32(value)?,
            "terr_cards" => self.terr_cards = as_u32(value)?,
            "terr_v3" => self.set_v3(value != 0),
            "terr_cycle" => self.terr_cycle = value != 0,
            "terr_move" => self.terr_move = as_u32(value)?,
            "terr_move_k1" => self.terr_move_k1 = as_u32(value)?,
            "terr_move_k2" => self.terr_move_k2 = as_u32(value)?,
            "terr_rebuild" => self.terr_rebuild = as_u32(value)?,
            "terr_rebuild_dist" => self.terr_rebuild_dist = value != 0,
            "terr_trade" => self.terr_trade = value != 0,
            "terr_bonus" => self.terr_bonus = value != 0,
            "terr_occupy" => self.terr_occupy = value != 0,
            "terr_cardany" => self.terr_cardany = value != 0,
            "terr_up_mult" => self.terr_up_mult = as_u32(value)?,
            "terr_cardlock" => self.terr_cardlock = as_u32(value)?,
            "terr_control" => self.terr_control = value != 0,
            "kat3_spread" => self.kat3_spread = value != 0,
            "terr_stack" => self.terr_stack = value != 0,
            "terr_upkeep" => self.terr_upkeep = value != 0,
            "terr_kingup" => self.terr_kingup = value != 0,
            "terr_kingup_price" => self.terr_kingup_price = as_u32(value)?,
            "terr_bend_tail" => self.terr_bend_tail = value != 0,
            "terr_cardset3" => self.terr_cardset3 = value != 0,
            "terr_delay" => self.terr_delay = value != 0,
            "terr_upkeep_sq" => self.terr_upkeep_sq = value != 0,
            "terr_upkeep_cnt" => self.terr_upkeep_cnt = as_u32(value)?,
            "terr_upkeep_pick" => self.terr_upkeep_pick = as_u32(value)?,
            "terr_famine" => self.terr_famine = as_u32(value)?,
            "terr_famine_k" => self.terr_famine_k = as_u32(value)?,
            "camp_bonus_area" => self.camp_bonus_area = as_u32(value)?,
            "terr_yield_lvl" => self.terr_yield_lvl = value != 0,
            "terr_bread" => self.terr_bread = as_u32(value)?,
            "terr_fort" => self.terr_fort = value != 0,
            "terr_kamikaze" => self.terr_kamikaze = value != 0,
            "terr_terrain" => self.terr_terrain = value != 0,
            "terr_multibuy" => self.terr_multibuy = value != 0,
            "terr_favor" => self.terr_favor = value != 0,
            "terr_dep" => self.terr_dep = as_u32(value)?,
            "board_corners" => self.board_corners = as_u32(value)?.min(255) as u8,
            "veins1" => self.veins_per_type[0] = as_u32(value)?,
            "veins2" => self.veins_per_type[1] = as_u32(value)?,
            "veins3" => self.veins_per_type[2] = as_u32(value)?,
            "cells2" => self.cells_per_type[1] = as_u32(value)?,
            "cells3" => self.cells_per_type[2] = as_u32(value)?,
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
            if crate::cards::CardTweaks::handles(key.trim()) {
                self.card_tweaks.set(key.trim(), value)?;
                continue;
            }
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
        let n = if self.board_corners != 0 {
            self.board_corners
        } else {
            self.players
        };
        match (self.board_corners != 0, n) {
            (_, 3) => GridKind::Triangle,
            (_, 5..=8) => GridKind::Hex,
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
        if self.board_corners != 0
            && (!(3..=8).contains(&self.board_corners) || self.players > self.board_corners)
        {
            return Err(ConfigError::Invalid(
                "board_corners: 3 bis 8, und nicht weniger Ecken als Spieler",
            ));
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
        if self.territory
            && (self.terr_pawn == 0
                || self.terr_area == 0
                || self.terr_yield == 0
                || self.terr_card_mult == 0)
        {
            return Err(ConfigError::Invalid(
                "terr_pawn, terr_area, terr_yield und terr_card_mult muessen mindestens 1 sein",
            ));
        }
        if self.terr_cats == 1 || self.terr_cats > MAX_CATEGORIES as u32 {
            return Err(ConfigError::Invalid(
                "terr_cats: 0 oder eine Zahl von 2 bis 4",
            ));
        }
        if self.terr_grow == 1 {
            return Err(ConfigError::Invalid(
                "terr_grow: 0 (Preise wie Version 1) oder mindestens 2",
            ));
        }
        if self.terr_cards > 1 {
            return Err(ConfigError::Invalid("terr_cards: 0 oder 1"));
        }
        if self.terr_bonus && self.terr_trade {
            return Err(ConfigError::Invalid(
                "terr_bonus und terr_trade gehen nicht zusammen (Kat 3 wird nur eines von beiden)",
            ));
        }
        if self.terr_bread > 0 && self.terr_trade {
            return Err(ConfigError::Invalid(
                "terr_bread und terr_trade gehen nicht zusammen (Kat 3 wird nur eines von beiden)",
            ));
        }
        if self.terr_bread > 0 && !self.terr_cycle {
            return Err(ConfigError::Invalid(
                "terr_bread braucht terr_cycle (den Erntezaehler)",
            ));
        }
        if self.terr_fort && !(self.territory && self.terr_cardset3 && self.terr_cardany) {
            return Err(ConfigError::Invalid(
                "terr_fort braucht den Kartensatz von Version 3 (terr_cardset3) und Karten auf Figuren (terr_cardany)",
            ));
        }
        if self.terr_kamikaze
            && !(self.territory && self.terr_cardset3 && self.terr_cardany && self.terr_exact)
        {
            return Err(ConfigError::Invalid(
                "terr_kamikaze braucht den Kartensatz von Version 3 (terr_cardset3), Karten auf Figuren (terr_cardany) und genaue Zuege (terr_exact)",
            ));
        }
        if self.terr_terrain && !self.territory {
            return Err(ConfigError::Invalid(
                "terr_terrain braucht den Gebietsmodus (territory)",
            ));
        }
        if self.terr_multibuy && !self.territory {
            return Err(ConfigError::Invalid(
                "terr_multibuy braucht den Gebietsmodus (territory)",
            ));
        }
        if self.terr_multibuy && self.terr_cardlock == 2 {
            return Err(ConfigError::Invalid(
                "terr_multibuy geht nicht mit terr_cardlock=2 (dort ist eine Verbesserung der ganze Zug)",
            ));
        }
        if self.terr_dep > 15 {
            return Err(ConfigError::Invalid(
                "terr_dep: 0 bis 15 (Bit 0 Eisen, 1 Gold, 2 Brot, 3 Kat 4)",
            ));
        }
        if self.terr_favor && !self.terr_terrain {
            return Err(ConfigError::Invalid(
                "terr_favor braucht das Gelaende (terr_terrain)",
            ));
        }
        if self.terr_upkeep_cnt > 0 && !self.terr_upkeep {
            return Err(ConfigError::Invalid(
                "terr_upkeep_cnt braucht terr_upkeep (den Unterhalt)",
            ));
        }
        if self.terr_upkeep_cnt > 0 && self.terr_upkeep_sq {
            return Err(ConfigError::Invalid(
                "terr_upkeep_cnt ersetzt den Unterhalt nach Stufe, geht also nicht mit terr_upkeep_sq",
            ));
        }
        if self.terr_upkeep_pick > 0 && !self.terr_upkeep {
            return Err(ConfigError::Invalid(
                "terr_upkeep_pick braucht terr_upkeep (den Unterhalt)",
            ));
        }
        if self.terr_upkeep_pick >> 8 != 0 {
            return Err(ConfigError::Invalid(
                "terr_upkeep_pick: Bit i fuer Spieler i, hoechstens 8 Spieler",
            ));
        }
        if self.terr_upkeep_cnt > 20 {
            return Err(ConfigError::Invalid(
                "terr_upkeep_cnt: 0 (aus) bis 20 (k in (k*n)^2)",
            ));
        }
        if self.terr_famine > 0 && self.terr_upkeep_cnt == 0 {
            return Err(ConfigError::Invalid(
                "terr_famine braucht den Unterhalt nach Anzahl (terr_upkeep_cnt)",
            ));
        }
        if self.terr_famine > 1000 {
            return Err(ConfigError::Invalid(
                "terr_famine: 0 (aus) bis 1000 (jede N-te Runde)",
            ));
        }
        if self.terr_famine_k == 0 || self.terr_famine_k > 20 {
            return Err(ConfigError::Invalid(
                "terr_famine_k: 1 bis 20 (k in (k*n)^2)",
            ));
        }
        if self.camp_bonus_area > 1000 {
            return Err(ConfigError::Invalid(
                "camp_bonus_area: 0 bis 1000 Flaeche zum Start",
            ));
        }
        if self.terr_upkeep_sq && !self.terr_upkeep {
            return Err(ConfigError::Invalid(
                "terr_upkeep_sq braucht terr_upkeep (den Unterhalt)",
            ));
        }
        if self.terr_yield_lvl && !self.terr_occupy {
            return Err(ConfigError::Invalid(
                "terr_yield_lvl braucht terr_occupy (die Figur steht auf dem Feld)",
            ));
        }
        if self.terr_delay && !self.territory {
            return Err(ConfigError::Invalid("terr_delay braucht den Gebietsmodus"));
        }
        if self.terr_up_mult == 0 {
            return Err(ConfigError::Invalid("terr_up_mult muss mindestens 1 sein"));
        }
        if self.terr_cardlock > 2 {
            return Err(ConfigError::Invalid(
                "terr_cardlock: 0 (nichts), 1 (Figur gesperrt) oder 2 (ganzer Zug)",
            ));
        }
        if self.terr_stack && !self.terr_cardany {
            return Err(ConfigError::Invalid(
                "terr_stack braucht terr_cardany (Karten, die auf einer gebauten Figur liegen)",
            ));
        }
        if self.terr_control && !self.terr_occupy {
            return Err(ConfigError::Invalid(
                "terr_control braucht terr_occupy (die Regel, dass Felder bearbeitet werden muessen)",
            ));
        }
        if self.terr_occupy && !self.terr_cycle {
            return Err(ConfigError::Invalid(
                "terr_occupy braucht terr_cycle (den Erntezaehler)",
            ));
        }
        if self.terr_cardset3 && (self.terr_cards != 1 || !self.territory) {
            return Err(ConfigError::Invalid(
                "terr_cardset3 braucht den Gebietsmodus und den Kartensatz von Version 2 (terr_cards=1)",
            ));
        }
        if self.terr_cardany && self.terr_cards != 1 {
            return Err(ConfigError::Invalid(
                "terr_cardany braucht den Kartensatz von Version 2 (terr_cards=1)",
            ));
        }
        if self.terr_move > 1 {
            return Err(ConfigError::Invalid("terr_move: 0 oder 1"));
        }
        if self.terr_move == 1 && self.terr_rebuild == 0 {
            return Err(ConfigError::Invalid(
                "terr_rebuild: mit terr_move mindestens 1 Runde",
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
            GridKind::Hex => Board::hex_polygon(self.corner_count(), self.side),
        }
    }

    /// Anzahl Startecken der Karte (Quadrat: immer 4, auch bei 2 Spielern).
    pub fn corner_count(&self) -> usize {
        match self.grid_kind() {
            GridKind::Square => 4,
            GridKind::Triangle => 3,
            GridKind::Hex if self.board_corners != 0 => self.board_corners as usize,
            GridKind::Hex => self.players as usize,
        }
    }

    /// Welche Ecke jeder Spieler bekommt. Bei 2 Spielern gegenueber (Ecken 0 und 2).
    pub fn player_corners(&self) -> Vec<usize> {
        if self.board_corners != 0 {
            return fair_corners(self.board_corners as usize, self.players as usize);
        }
        match self.players {
            2 => vec![0, 2],
            n => (0..n as usize).collect(),
        }
    }

    /// Anzahl Rohstoffkategorien: 3 bis 5 Ecken, ab 6 Ecken kommt Kat 4 dazu.
    /// (Ob bei 7 und 8 Spielern weitere folgen, ist im Regelwerk noch offen.)
    pub fn category_count(&self) -> u8 {
        let base = if self.corner_count() >= 6 { 4 } else { 3 };
        match self.terr_cats {
            0 => base,
            n => base.min(n.min(MAX_CATEGORIES as u32) as u8),
        }
    }
}

/// Alle Ecken-Abbildungen eines regelmaessigen Vielecks mit `n` Ecken (Drehungen und
/// Spiegelungen) als Tabelle: `table[g][i]` ist die Ecke, auf die `i` abgebildet wird.
fn corner_symmetries(n: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::with_capacity(2 * n);
    for j in 0..n {
        out.push((0..n).map(|i| (i + j) % n).collect());
        out.push((0..n).map(|i| (j + n - i) % n).collect());
    }
    out
}

/// Sind alle Spieler auf diesen Ecken gleich gestellt? Das gilt, wenn das Brett eine Drehung
/// oder Spiegelung hat, die die Eckenmenge in sich abbildet und jede Spielerecke auf jede
/// andere bringt: Jeder Spieler hat dann dieselben Nachbarn in denselben Abstaenden.
pub fn corners_are_fair(n: usize, corners: &[usize]) -> bool {
    if corners.len() <= 1 {
        return true;
    }
    let mut mask = 0u32;
    for &c in corners {
        mask |= 1 << c;
    }
    let keeps: Vec<Vec<usize>> = corner_symmetries(n)
        .into_iter()
        .filter(|g| corners.iter().all(|&c| mask & (1 << g[c]) != 0))
        .collect();
    let first = corners[0];
    corners.iter().all(|&c| keeps.iter().any(|g| g[first] == c))
}

/// Alle Eckenmengen der Groesse `k` (aufsteigend sortiert, in Zahlenreihenfolge).
fn corner_subsets(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for mask in 0u32..(1 << n) {
        if mask.count_ones() as usize == k {
            out.push((0..n).filter(|&i| mask & (1 << i) != 0).collect());
        }
    }
    out
}

/// Kleinster Abstand zweier Spielerecken rund um das Vieleck (Schritte entlang des Rands).
fn min_corner_gap(n: usize, corners: &[usize]) -> usize {
    let mut best = n;
    for (a, &x) in corners.iter().enumerate() {
        for &y in &corners[a + 1..] {
            let d = y - x;
            best = best.min(d.min(n - d));
        }
    }
    best
}

/// Die Ecken fuer `k` Spieler auf einem Brett mit `n` Ecken: wenn es eine faire Verteilung gibt
/// (siehe `corners_are_fair`), die mit dem groessten Mindestabstand (bei Gleichstand die
/// kleinste in Zahlenreihenfolge); sonst die mit dem groessten Mindestabstand.
pub fn fair_corners(n: usize, k: usize) -> Vec<usize> {
    let k = k.min(n);
    let all = corner_subsets(n, k);
    let pick = |fair_only: bool| {
        all.iter()
            .filter(|s| !fair_only || corners_are_fair(n, s))
            .max_by(|a, b| {
                min_corner_gap(n, a)
                    .cmp(&min_corner_gap(n, b))
                    .then_with(|| b.cmp(a))
            })
            .cloned()
    };
    pick(true).or_else(|| pick(false)).unwrap_or_default()
}

/// Wie viele Spieler am Tisch sitzen muessen, damit niemand einen Vorteil hat: die kleinste
/// Zahl `m >= k` (hoechstens `n`), fuer die es eine faire Eckenverteilung gibt. Die fehlenden
/// `m - k` Plaetze besetzen Auffuell-Bots.
pub fn fair_player_count(n: usize, k: usize) -> usize {
    (k.min(n)..=n)
        .find(|&m| corners_are_fair(n, &fair_corners(n, m)))
        .unwrap_or(n)
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
