# spacevitan (Arbeitstitel SpaceCivTan)

Rundenbasiertes Strategiespiel: eine Mischung aus Schach, Siedler (Catan) und Civ.
Das Regelwerk liegt als Dokument beim Projekt; dieses Repository enthaelt die Umsetzung.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/game_core` | Regel-Kern in Rust: Karte, Rohstoffe, Zugregeln, Bots. Keine Grafik, keine externen Pakete. |
| `crates/cli` | Text-Oberflaeche zum Zuschauen (Bot gegen Bot) und fuer Statistik. |
| `crates/web` | WebAssembly-Schnittstelle: der Regel-Kern im Browser. |
| `viewer/` | Browser-Seite: im Browser gegen Bots spielen und Partien isometrisch ansehen. |
| `client/` | Godot-Client (Isometrie, Eingabe) – kommt als Naechstes. |
| `.github/workflows` | CI: Formatierung, Clippy, Tests unter Linux und Windows, Windows-`.exe` als Download. |

Module in `game_core`:

| Modul | Aufgabe |
|---|---|
| `board` | Quadrat-, Dreiecks- und Hexgitter: Nachbarn, gerade Linien, Abstaende, Gebiete |
| `map` | Kartengenerator (Rohstoff-Adern, Kat-1-Garantie, Fairnesspruefung) |
| `config` | alle einstellbaren Zahlen und Regeln (`Config::set` fuer `--rule` und den Browser) |
| `cards` | Kartenmarkt: Karten, Kosten, Bezahlung, Ziehen des Markts |
| `state` | Figuren, Fabriken, Beute, Gepaeck, Rohstoff-Buchfuehrung |
| `game` | Regel-Engine: erlaubte Aktionen, Ausfuehren, Zugende, Invariantenpruefung |
| `ai` | Starker Bot ("stark"): Vorausberechnung mit Alpha-Beta bzw. Best-Reply-Search, Ruhesuche, Bewertung von Material, Koenigssicherheit, Einkesseln und Wirtschaft |
| `bot` | Zufallsbot und einfacher Heuristik-Bot ("gierig"), `play_out` |
| `render` | Textansicht eines Spielstands |
| `rng` | deterministischer Zufall (gleicher Seed = gleiche Partie) |

## Ausprobieren

```
cargo run --release -- --show                  # eine Partie gierig gegen gierig ansehen
cargo run --release -- --games 200             # Statistik ueber 200 Partien
cargo run --release -- --games 50 --bots stark,gierig   # starker Bot gegen einfachen
cargo run --release -- --players 4 --games 50 --bots gierig,zufall
cargo run --release -- --games 200 --stock 5 --kat1 3   # Vorrat je Vorkommen, Kat-1-Felder je Ecke
cargo run --release -- --games 50 --bots stark,stark --rule upgrade_step=0,market_size=0
cargo run --release -- --help
```

Ohne Rust: Bei jedem Push baut GitHub Actions die `spacevitan.exe`. Sie liegt unter
*Actions → letzter Lauf → Artifacts → spacevitan-windows*. Per Doppelklick zeigt sie eine
neue Partie Runde fuer Runde; in einer Eingabeaufforderung gehen alle Optionen
(`spacevitan.exe --help`).

### Computergegner

| Bot | Spielweise |
|---|---|
| `stark` | rechnet voraus: eigene Zuege und die gefaehrlichste Antwort der Gegner, mehrere Zuege tief (bei 2 Spielern Alpha-Beta wie bei Schachprogrammen, ab 3 Spielern Best-Reply-Search). Wirtschaft, Koenigssicherheit und das Einkesseln gegnerischer Koenige stecken in der Bewertung. Rechnet mit festem Budget (`--nodes`, Standard 20000 Stellungen pro Zug), also auf jedem Rechner gleich. |
| `gierig` | feste Faustregeln ohne Vorausschau |
| `zufall` | zufaellige erlaubte Zuege (zum Testen der Regeln) |

Gewichte der Bewertung lassen sich zum Abstimmen ohne Neubau setzen, z. B.
`SPACEVITAN_WEIGHTS="trap=30,king_hunt=8" spacevitan --games 50 --bots stark,gierig`.

Jede Partie bekommt einen neuen Startwert (Seed), der am Ende angezeigt wird. Mit
`--seed <Wert>` laesst sich genau diese Partie wiederholen, z. B. um einen Fehler zu melden.

### Im Browser spielen

`viewer/` ist eine Seite mit zwei Modi: **Spielen** (du bist Spieler A gegen Bots, Standard
ist `stark`, oder mehrere Menschen an einem Geraet; mit *Spieler A: Bot* laeuft eine reine
Bot-Partie zum Zuschauen, mit ▶/⏸, Einzelschritt und Tempo) und **Partien ansehen** (Wiedergabe). Die Regeln laufen dabei als
WebAssembly direkt im Browser; es ist derselbe Rust-Kern wie in der CLI (`crates/web`,
`game_core::session`). Die CI baut `spacevitan.wasm` bei jedem Push nach `main` in den Branch
`web-build`.

```
git fetch origin +web-build:refs/remotes/origin/web-build
git show origin/web-build:spacevitan.wasm > spacevitan.wasm
python3 viewer/build.py spacevitan.html --wasm spacevitan.wasm --standalone
```

Die entstandene `spacevitan.html` laesst sich per Doppelklick im Browser oeffnen.

### Browser-Ansicht

`viewer/` spielt exportierte Partien isometrisch ab (Runde fuer Runde, mit Ereignissen und
Feld-Details). Eine Partie exportieren und in der Ansicht mit *Eigene Partie laden* oeffnen:

```
spacevitan.exe --seed 7 --export partie.json
```

Die Ansicht mit eingebetteten Beispielpartien bauen:

```
python3 viewer/build.py ansicht.html partie1.json partie2.json --standalone
```

Das JSON-Format (`crates/game_core/src/replay.rs`) liest spaeter auch der Godot-Client.

Legende der Textansicht: `A*` Koenig von A, `a2` Figur von A mit Stufe 2, `#A` Fabrik von A,
`$` Beute, Ziffern = Rohstoffkategorien auf dem Feld, `.` leer.

## Tests

```
cargo test --workspace
```

- Unit-Tests je Modul und je Regel (`game_tests.rs`: ein Szenario pro Regel des Regelwerks)
- Selbstspiel (`tests/selfplay.rs`): komplette Partien mit Bots, nach jedem Zug werden alle
  Invarianten geprueft (z. B. kein Rohstoff entsteht oder verschwindet unverbucht)
- Fuzzing: `legal_actions` und `apply` muessen bei jeder Aktion, auch bei Unsinn, uebereinstimmen

## Regeln, die man umstellen kann (`--rule name=wert`)

| Name | Standard | Bedeutung |
|---|---|---|
| `cat1_distance` | 2 | eigene Kat-1-Vorkommen so viele Schritte vom Start, in Richtung Mitte |
| `center_colors` | Spieler/2 | neutrale Kat-1-Typen (Zentrumsfarben) rund um die Mitte |
| `center_radius` | 2 | so weit von der Mitte liegen sie (auf kleinen Karten weniger) |
| `foreign_build_bonus` | 1 | Fabrik auf fremder Farbe bekommt die eigene dazu: gleich Stufe 2 |
| `upgrade_step` | 10 | je 10 Einheiten ab Kat 2 im Lager (aus mindestens Stufe+1 Typen) Stufe +1; 0 = aus |
| `max_upgrades` | 2 | hoechstens so viele Stufen durch Ausbau |
| `market_size` | 2 je Spieler (ab 5 Spielern 1) | Karten im Markt; 0 = ohne Karten |

## Karten (Version 1)

Vor der Partie liegen `market_size` verschiedene Karten offen aus, jede beliebig oft kaufbar.
Pro Zug hoechstens ein Kauf, zusaetzlich zur Aktion. Bezahlt wird aus einem Lager: Gepaeck des
Koenigs oder eine eigene Fabrik. Karten fuer Figuren wirken auf eine Figur am Zahlfeld oder
daneben. Im Deck: Ausbildung, Lange Beine, Laeuferschlag (nur Quadratkarten), Schild,
Koenigsruestung, Koenigsschritt, Rekrut, Doppelschicht, Bergbau, Ackerbau, Doppelzug sowie die
Sets Runenstein und Sternsplitter (drei Teile ergeben den Bonus). Kosten und Texte stehen in
`crates/game_core/src/cards.rs`, der Entwurf mit 100 Karten im Projekt-Dokument.

## Stand: Meilenstein 1

Umgesetzt: 2 bis 8 Spieler (2 und 4: Quadrat, 3: Dreieck aus Dreiecksfeldern, 5 bis 8:
regelmaessiges Vieleck aus Hexfeldern; ab 6 Spielern eine vierte Rohstoffkategorie); Kartengenerator mit Fairnesspruefung;
Koenig sammelt, baut Fabriken nur auf Kat-1-Vorkommen (das Vorkommen ist danach weg; Kat 1 liegt
zwei Felder vom Start Richtung Mitte, dazu Zentrumsfarben), laedt ab; Fabrik auf fremder Farbe =
Stufe 2; Ausbau ueber Kat 2/3 im Lager; Kartenmarkt mit 13 Karten; Fabriken sammeln Kat 2/3 vom eigenen Feld und den
Nachbarfeldern (im Quadratgitter 5 Felder); Aufloesen in Figuren (Stufe = Zahl der
Kat-1-Farben, Reichweite = Stufe); Stufe 1 zieht gerade, ab Stufe 2 frei;
Schlagen wie im Schach; Fabrik-Einnahme mit Beute, die nur Koenige aufnehmen; Koenig faellt =
Spieler raus; Unentschieden nach `max_rounds`; Computergegner `stark`, `gierig`, `zufall`.

Noch nicht umgesetzt: verdeckte Karte, die restlichen Karten aus dem Entwurf (Nahrung, Sabotage,
Waffen mit Fernkampf), Kachel-Zukauf, Zugzeit, Netzwerkprotokoll, Godot-Client.
