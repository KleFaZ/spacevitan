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
cargo run --release -- --games 40 --bots stark,stark --cards lance,horse,recruit   # fester Markt
cargo run --release -- --games 40 --bots stark-pur,stark-pur --cards "" --gift horse   # Kartenwert messen
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
| `stark-pur` | wie `stark`, kauft aber nie Karten (Vergleichsgegner fuer Kartenmessungen) |
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

**Wege vorplanen (Premoves):** Eigene Figur anklicken, dann mit der Maus ueber das Zielfeld
fahren und **Shift** halten: der kuerzeste freie Weg wird gestrichelt gezeigt. **Shift+Klick**
laeuft ihn Zug um Zug von selbst (so weit es die Figur je Zug schafft, der Koenig ein Feld),
auch waehrend die Bots ziehen. Mit **Strg+Klick** (am Mac auch Cmd) waehlt man die Kacheln
einzeln; Strg loslassen oder Enter startet den Weg. **Esc** oder der Knopf *Route abbrechen*
stoppt. Eine Route bricht von selbst ab, wenn der Weg versperrt ist, und schlaegt unterwegs
nie; nur ein Gegner auf dem Zielfeld wird am Ende geschlagen. Mehrere Routen laufen nacheinander,
ein Zug je Runde.

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
| `cat1_distance` | 2 | das eine eigene Kat-1-Feld liegt so viele Schritte vom Start, in Richtung Mitte |
| `cat1_cells` | 1 | eigene Kat-1-Vorkommen je Ecke (wie `--kat1`) |
| `mid_cat1` | 1 | Kat-1-Felder je Spieler im Zwischenring, etwa auf halbem Weg zur Mitte (Zentrumsfarbe) |
| `center_colors` | Spieler/2 | neutrale Kat-1-Typen (Zentrumsfarben) rund um die Mitte |
| `center_cells` | 3 | Felder je Zentrumsfarbe in der Mitte |
| `center_radius` | 2 | so weit von der Mitte liegen sie (auf kleinen Karten weniger) |
| `upgrade_step` | 10 | 10 Quader/Baelle im Lager: Stufe +1; 20 aus zwei Regionen: Stufe +2; 0 = aus |
| `upgrade_region_min` | 3 | so viele Einheiten einer Farbe muessen im Lager liegen, damit die Region zaehlt |
| `regrow_cat1` / `regrow_cat2` / `regrow_cat3` | 20 / 5 / 3 | nach dem Abbau so viele Runden weg; 0 = fuer immer, 1 = jede Runde wieder da |
| `score_after` | 100 | ohne Sieger entscheidet nach so vielen Runden die Wertung (siehe unten); 0 = aus, dann Remis nach `max_rounds` |
| `center_mix` | 1 | so oft liegt jede Regionsfarbe (Quader und Ball) zusaetzlich rund um die Mitte |
| `max_upgrades` | 2 | hoechstens so viele Stufen durch Ausbau |
| `market_size` | 2 je Spieler (ab 5 Spielern 1) | Karten im Markt, jede Partie zufaellig gezogen; 0 = ohne Karten |
| `card_target_near` | 0 | 1 = Figurenkarten nur fuer eine Figur auf oder neben dem Zahlfeld |

Kat 1 wird also zur Mitte hin dichter: in jeder Ecke ein Feld der eigenen Farbe, im
Zwischenring eins je Spieler, in der Mitte drei je Zentrumsfarbe.

### Regionen, Abbau, Stufen

Jede Region (Ecke, auch die freien) hat genau einen Quader (Kat 2) und einen Ball (Kat 3) in
ihrer Farbe, in mehreren Adern; rund um die Mitte liegt jede Farbe noch einmal. Ein Vorkommen
ist nach dem Abbau weg und kommt wieder: Kat 1 nach 20 Runden, Quader nach 5, Baelle nach 3.
Das gilt fuer den Koenig und fuer Fabriken, hin und her laufen bringt also nichts. Auf dem
Brett sind abgebaute Vorkommen blass und zeigen die Runden bis zur Rueckkehr.

Kat 1 erntet der Koenig, sobald er das Feld betritt, und traegt es im Gepaeck. Damit baut er
auf jedem freien Feld eine Fabrik (Mindestabstand 3 zu jeder anderen), oder er laedt es in
einer eigenen Fabrik ab: jede neue Farbe dort hebt die Stufe der Figur um eins.

Jede neue Fabrik ist Stufe 1. Stufe 2 braucht 10 Quader/Baelle, Stufe 3 braucht 20 aus zwei
Regionen (oder eine zweite Kat-1-Farbe). Gemessen mit `stark` gegen `stark`, 40 Partien, 2 Spieler:
erste Stufe-2-Figur um Runde 9, Stufe 3 in 22 von 40 Partien (um Runde 15), im Schnitt 31 Runden.

### Wertung nach 100 Runden

Hat nach `score_after` = 100 Runden niemand gewonnen, entscheidet die Wertung: Summe der
Figurenstufen (der Koenig zaehlt nicht) plus eine Einheit je Fabrik plus eine Einheit je Farbe
(Region oder Zentrumsfarbe), von der man etwas im Gepaeck oder in einem Fabriklager hat. Bei
Gleichstand gewinnt, wer mehr Rohstoffe besitzt, danach ist es Remis. Der Viewer zeigt die
laufende Wertung je Spieler.

Warum: Kat 1 ist begrenzt. Ohne Nachwachsen und ohne Wertung endeten mit `gierig` bei 3 Spielern
24 %, bei 5 Spielern 52 % der Partien remis (nur noch Koenige, die sich nicht fangen). Jetzt
sind es 0 bis 1 %. Mit `stark` bei 3 Spielern: 0 Remis in 24 Partien, 11 davon fielen erst durch
die Wertung (Runde 101), im Schnitt 82 Runden.

## Karten

Vor jeder Partie zieht der Seed `market_size` verschiedene Karten aus dem Deck (bei 2 Spielern
4 von 14), jede beliebig oft kaufbar. Pro Zug hoechstens ein Kauf, zusaetzlich zur Aktion.
Bezahlt wird aus einem Ort: Gepaeck des Koenigs oder eine eigene Fabrik. Figurenkarten wirken
auf eine beliebige eigene Figur (mit `card_target_near=1` nur auf oder neben dem Zahlfeld). Das
Deck staerkt nur Angriff und Wirtschaft, Verteidigungskarten gibt es nicht. Texte und Preise:
`crates/game_core/src/cards.rs`; der Entwurf mit 100 Karten steht im Projekt-Dokument.

| Karte | Wirkung | Preis (Quader und Baelle; "Farben" = Regionen) |
|---|---|---|
| Ausbildung | Figur: Stufe +1, Reichweite waechst mit | je Reichweite der Figur 2+2: Bauer 4, Reichweite 2: 8, Reichweite 3: 12; so viele Farben wie die Reichweite |
| Lange Beine | Figur: Reichweite +1, hoechstens zweimal | je Reichweite 1+2: 3, 6, 9; so viele Farben wie die Reichweite |
| Laeuferschlag | Figur schlaegt auch diagonal (nur Quadratkarten) | 3 |
| Lanze | Figur schlaegt zwei Felder weit geradeaus | 4 |
| Sprungfeder | Figur springt ueber eine Nachbarfigur | 3 |
| Pferd | Figur springt auf jedes Feld im Abstand 2 | 4 |
| Sturmangriff | in diesem Zug alle Figuren Reichweite +1 | 3 Baelle in 2 Farben |
| Rekrut | Bauer neben der zahlenden Fabrik, die Fabrik bleibt | 1 Farbe + 5 |
| Doppelzug | in diesem Zug eine zweite Aktion | 5, je 2 Farben |
| Doppelschicht | rund um die eigenen Fabriken waechst alles sofort nach und wird geerntet | 4 |
| Bergbau / Ackerbau | Fabriken ernten Kat 2 / Kat 3 doppelt | 5 |
| Runenstein (Set) | der dritte gibt einer Figur Stufe +2 | 6 (Quader in 3 Farben), Set 18 |
| Sternsplitter (Set) | der dritte legt ein Kat-1-Feld der eigenen Farbe unter den Koenig | 4 (Quader in 2 Farben), Set 12 |

### Kartenwert-Messung (Stand vor den Regionsfarben)

`--gift` schenkt Spieler A eine Karte, sobald sie wirken kann (Figurenkarten an seine staerkste
Figur, sobald er eine hat; Fabrikkarten mit der ersten Fabrik; Sets komplett). Beide Seiten
`stark-pur`, kein Markt, 40 Partien mit Seeds 3000 bis 3039. Punkte = Siege + halbe Remis.
Ohne Geschenk holt A 62,5 % (Anzugsvorteil). Zufallsschwankung bei 40 Partien: etwa ±10 Punkte,
nur grosse Unterschiede sind belastbar.

| Geschenk an A | Punkte A | Unterschied |
|---|---|---|
| nichts | 62,5 % | |
| 2 / 4 Einheiten in Runde 3 | 71 / 88 % | +9 / +25 |
| 2 / 4 Einheiten mit der ersten Figur | 69 / 71 % | +6 / +9 |
| 2 / 4 Einheiten mit der ersten Fabrik | 60 / 68 % | −3 / +5 |
| Runenstein-Set | 100 % | +38 |
| Ausbildung | 92 % | +30 |
| Lange Beine | 89 % | +26 |
| Pferd, Bergbau | 66 % | +4 |
| Rekrut, Doppelschicht, Ackerbau | 64 % | +1 |
| Sprungfeder, Lanze, Laeuferschlag | 60 bis 63 % | um 0 |
| Sternsplitter-Set | 58 % | −5 |

Ergebnis: Reichweite auf der ersten starken Figur (meist Stufe 3 um Runde 9) entscheidet die
Partie. Darum kosten Ausbildung und Lange Beine je Reichweitenpunkt, der Runenstein mehr. Die
Bauernkarten (Sprungfeder, Lanze, Laeuferschlag, Pferd) bringen auf einer starken Figur nichts
und bleiben billig; Pferd wurde von 6 auf 4 gesenkt. Doppelschicht ist beliebig oft kaufbar und
kostet darum mehr als eine Ernte (2 auf 4). Sturmangriff und Doppelzug wirken nur einen Zug und
lassen sich so nicht messen.

## Stand: Meilenstein 1

Umgesetzt: 2 bis 8 Spieler (2 und 4: Quadrat, 3: Dreieck aus Dreiecksfeldern, 5 bis 8:
regelmaessiges Vieleck aus Hexfeldern; ab 6 Spielern eine vierte Rohstoffkategorie); Kartengenerator mit Fairnesspruefung;
Koenig sammelt, erntet Kat 1 beim Betreten und baut damit auf jedem freien Feld Fabriken (Kat 1 liegt
zwei Felder vom Start Richtung Mitte, dazu Zentrumsfarben, kommt nach 20 Runden wieder), laedt ab; Quader und Baelle in der Farbe ihrer Region, Abbau mit Nachwachsen;
Ausbau ueber Quader/Baelle im Lager, Stufe 3 erst mit zweiter Region; Kartenmarkt mit 14 Karten (nur Angriff und Wirtschaft); Fabriken sammeln Kat 2/3 vom eigenen Feld und den
Nachbarfeldern (im Quadratgitter 5 Felder); Aufloesen in Figuren (Stufe = Zahl der
Kat-1-Farben, Reichweite = Stufe); Stufe 1 zieht gerade, ab Stufe 2 frei;
Schlagen wie im Schach; Fabrik-Einnahme mit Beute, die nur Koenige aufnehmen; Koenig faellt =
Spieler raus; nach 100 Runden Wertung (`score_after`), Remis erst bei Gleichstand oder nach `max_rounds`; Computergegner `stark`, `gierig`, `zufall`.

Noch nicht umgesetzt: verdeckte Karte, die restlichen Karten aus dem Entwurf (Nahrung, Sabotage,
Waffen mit Fernkampf), Kachel-Zukauf, Zugzeit, Netzwerkprotokoll, Godot-Client.
