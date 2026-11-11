# spacevitan (Arbeitstitel SpaceCivTan)

Rundenbasiertes Strategiespiel: eine Mischung aus Schach, Siedler (Catan) und Civ.
Das Regelwerk liegt als Dokument beim Projekt; dieses Repository enthaelt die Umsetzung.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/game_core` | Regel-Kern in Rust: Karte, Rohstoffe, Zugregeln, Bots. Keine Grafik, keine externen Pakete. |
| `crates/cli` | Text-Oberflaeche zum Zuschauen (Bot gegen Bot) und fuer Statistik. |
| `viewer/` | Browser-Ansicht: spielt exportierte Partien isometrisch ab. |
| `client/` | Godot-Client (Isometrie, Eingabe) – kommt als Naechstes. |
| `.github/workflows` | CI: Formatierung, Clippy, Tests unter Linux und Windows, Windows-`.exe` als Download. |

Module in `game_core`:

| Modul | Aufgabe |
|---|---|
| `board` | Quadratgitter, Nachbarn, Richtungen, Abstaende, Gebiete der Startecken |
| `map` | Kartengenerator (Rohstoff-Adern, Kat-1-Garantie, Fairnesspruefung) |
| `config` | alle einstellbaren Zahlen |
| `state` | Figuren, Fabriken, Beute, Gepaeck, Rohstoff-Buchfuehrung |
| `game` | Regel-Engine: erlaubte Aktionen, Ausfuehren, Zugende, Invariantenpruefung |
| `bot` | Zufallsbot und einfacher Heuristik-Bot ("gierig") |
| `render` | Textansicht eines Spielstands |
| `rng` | deterministischer Zufall (gleicher Seed = gleiche Partie) |

## Ausprobieren

```
cargo run --release -- --show                  # eine Partie gierig gegen gierig ansehen
cargo run --release -- --games 200             # Statistik ueber 200 Partien
cargo run --release -- --players 4 --games 50 --bots gierig,zufall
cargo run --release -- --help
```

Ohne Rust: Bei jedem Push baut GitHub Actions die `spacevitan.exe`. Sie liegt unter
*Actions → letzter Lauf → Artifacts → spacevitan-windows*. Per Doppelklick zeigt sie eine
neue Partie Runde fuer Runde; in einer Eingabeaufforderung gehen alle Optionen
(`spacevitan.exe --help`).

Jede Partie bekommt einen neuen Startwert (Seed), der am Ende angezeigt wird. Mit
`--seed <Wert>` laesst sich genau diese Partie wiederholen, z. B. um einen Fehler zu melden.

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

## Stand: Meilenstein 1

Umgesetzt: 2 und 4 Spieler auf der Quadratkarte; Kartengenerator mit Fairnesspruefung;
Koenig sammelt, baut Fabriken, laedt ab; Fabriken sammeln Kat 2/3 vom eigenen Feld und den
Nachbarfeldern (im Quadratgitter 5 Felder); Aufloesen in Figuren (Stufe = Zahl der
Kat-1-Farben, Reichweite = hoechste Kategorie); Stufe 1 zieht gerade, ab Stufe 2 frei;
Schlagen wie im Schach; Fabrik-Einnahme mit Beute, die nur Koenige aufnehmen; Koenig faellt =
Spieler raus; Unentschieden nach `max_rounds`.

Noch nicht umgesetzt: 3 und 5–8 Spieler (Dreiecks-/Hexfelder), verdeckte Karte, Forschung,
Kachel-Zukauf, Nebenfiguren aus Nebenrohstoffen, Nahrung und Kombos, Zugzeit, Netzwerkprotokoll,
Godot-Client.
