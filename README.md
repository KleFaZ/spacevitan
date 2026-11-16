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
| `flaeche` | nur Gebietsmodus: spielt auf Land und Rohstoffe statt auf Figurenkampf. Rechnet einen eigenen Zug voraus und bewertet das Ergebnis mit der echten Engine (verbundenes Land, Vorkommen darauf nach Reife gewichtet, Figuren, Bedrohung von Koenig und Fabrik), also sind Abtrennen und Umschliessen schon eingerechnet. Baut nur, wenn die neue Figur besser ist als seine beste; kauft Karten, die der naechsten Figur oder dem Koenig nutzen. 0,1 ms je Zug (`area_bot.rs`) |
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

**Version 1:** Der Browser zeigt nur noch den Gebietsmodus (Abschnitt "Gebietsmodus" unten) und
heisst "Spacevitan Version 1". Das ist der Referenzstand: Commit `cbe9b53` (und der damals
veroeffentlichte Link); spaetere Aenderungen bauen darauf auf. Ein Git-Tag `v1` waere die uebliche
Marke dafuer (ein Tag ist nur ein Name fuer einen Commit), die Sitzungs-Umgebung, in der das hier
gebaut wird, lehnt das Hochladen von Tags aber ab; wer will, setzt ihn selbst mit
`git tag v1 cbe9b53 && git push origin v1`. **Version 1.1** (Commit `2971584`) hat dieselben Regeln,
dazu die Fabrik-Karte am Brett und 5 bis 8 Spieler im Menue. **Version 2** ist ein Regelversuch
(Abschnitt "Version 2" unten). Die aelteren Spielarten (Abbauen mit Nachwachsen, Brettspiel, Rollen) sind im
Menue nicht mehr waehlbar, bleiben aber im Spielkern und in der CLI (`--rule ...` ohne
`territory=1`) erhalten. Gespeicherte Menue-Staende aelterer Seiten werden ignoriert, soweit ihre
Felder weggefallen sind.

**Startmenue:** *Spielen* oeffnet zuerst ein Menue; eine Partie beginnt erst mit *Partie
starten*; *Neue Partie* oben neben *Spielen* oeffnet es jederzeit wieder. Dort stehen **Brettform** (Dreieck, Quadrat,
Fuenfeck bis Achteck), Spielerzahl, *Mit Bots auffuellen*, je Platz **Mensch oder Bot-Stufe** (stark, flaeche, gierig,
zufall; jeder Bot einzeln), Seed, Brettgroesse, Wertung, Kartenzahl und alle
Gebiets-Werte (Preise, Wieder anbinden, Umschliessen, Brettrand, Kartenpreis). Leere Felder gelten
als Standard, *Alles zuruecksetzen* leert sie. Die Einstellungen merkt sich der Browser. Technisch
schickt die Seite sie als Text `territory=1,name=wert,...` an `sv_set_rules` (`crates/web`); die
Namen sind dieselben wie bei `--rule`. Die Regeln der laufenden Partie stehen in der Zusammenfassung.

**Fabrik-Menue (nur beim Spielen):** Ein **Klick auf die eigene Fabrik** laesst ein Menue mit allen Aktionen aufploppen
(ein zweiter Klick, ein Klick aufs Brett oder Esc schliesst es; **steht eine eigene Figur auf der Fabrik, oeffnet der Klick
nicht das Menue, sondern waehlt die Figur** und zeigt ihre Zugfelder; das Menue ist dann ueber die Leiste erreichbar). Es zeigt: die vier **Figuren** mit Stufe, Unterhalt, Preis als Symbole mit
`vorhanden/noetig` und "Bauen" (gruen, wenn bezahlbar und die Fabrik frei ist) oder dem Grund ("in 6 Runden", "Fabrik
besetzt", "nicht am Zug"); das **Koenigs-Upgrade**; die **Karten** des Marktes (bezahlbar: gruen, sonst "fehlt: 2× Eisen",
"keine passende Figur", "schon gekauft"); Kat 3 tauschen, Fabrik ruecken und Fabrik umziehen. Ausser der Reihe zeigt der
Klick dasselbe, nur grau mit "nicht am Zug". Die Leiste neben der Fabrik (Stand in einem Satz) bleibt und klappt dasselbe
Menue per Klick auf. Die Rechnung steht im Viewer (`terrIncome`, `terrLevelStatus`, `renderFactoryCard`), der Spielkern
aendert sich dafuer nicht.

**Farben und Symbole:** Spielfelder haben die Farbe ihres Besitzers. Die Rohstoffe haben **feste Namen, Symbole und
Farben**: Kat 1 = **Eisen** (Erzbrocken, grau-blau), Kat 2 = **Gold** (Goldbarren, gelb), Kat 3 = **Brot** (Laib, braun),
unabhaengig vom Besitzer (das Land zeigt den Besitzer, das Symbol den Rohstoff). Jede Kachel mit Vorkommen traegt einen
**Rand in der Farbe des Rohstoffs** (zwei Vorkommen: zwei Raender ineinander); er liegt unter allen Figuren und bleibt
am Feldrand sichtbar. Steht eine Figur oder die Fabrik auf dem Feld, zeigt ausserdem ein **Abzeichen** am Feldrand, was
darunter liegt. Die Flaeche hat ein Symbol je Brettform (Quadrat, Sechseck, Dreieck) in der Spielerfarbe; die Figuren
1 bis 4 sind Scheiben mit der Stufenzahl und Randfarbe (weiss, silber, rot, gold) wie auf dem 2D-Brett. Alle Symbole
stehen einmal als Pfade in `symLayers` (`viewer/index.html`) und zeichnen Brett (Path2D) wie Menue (Inline-SVG).
Das ist nur Anzeige: Preise und Lager zaehlen im Gebietsmodus nach Kategorie.

**Uebersicht rechts neben dem Brett (Version 3):** Die rechte Spalte ist eine Uebersicht fuer den Spieler, der du bist
(in Bot-Partien und bei mehreren Menschen: der gerade Ziehende). Oben vier Kacheln: **Flaeche** (Symbol je Brettform in
der Spielerfarbe), **Eisen**, **Gold** und **Brot**, je mit dem Bestand und in Klammern dem Zuwachs, der am Ende deines
Zugs dazukommt, zum Beispiel `142 (+11)`; negativ steht rot da. Bei der Flaeche ist der Zuwachs netto (Land + Brot-Bonus −
Unterhalt), eine Zeile darunter rechnet ihn vor. Brot hat kein Lager: die Kachel zeigt die Zahl der verbundenen
Brot-Felder, der Bonus steht in der Zeile darunter. Darunter die **Figuren 1 bis 4** als Symbol: **grau**, solange sie
nicht bezahlbar sind, **in Farbe und mit gruenem Rahmen**, sobald sie es sind, mit `vorhanden/noetig` je Preisbestandteil
und "in n Runden"/"bereit"/"nie" (nie = kein Einkommen dafuer); dazu die **Koenigs-Stufe** mit Gold. Dann die Karte neben
der Fabrik und der Kartenmarkt. Lange Erklaerungen sind eingeklappt. Partie-Angaben (Seed, Regeln, Spielerkacheln, "Dieser
Zug", Bedienung, Legende) stehen eingeklappt unter *Weitere Angaben*. Auf schmalen Bildschirmen (unter 900 px) liegt die
Uebersicht unter dem Brett.

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
Feld-Details). Unter dem Brett schaltet **3D / 2D** die Ansicht um: 3D schraeg von oben mit
Figuren, 2D flach von oben wie bei lichess (Koordinaten im Brett, Reihe 1 unten, Figuren als
Marken mit Stufe, Punkten fuer die Reichweite und Abzeichen fuer Karten). Beides gilt auch beim
Spielen; die Wahl wird im Browser gemerkt. Eine Partie exportieren und in der Ansicht mit *Eigene Partie laden* oeffnen:

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
| `kat_roles` | 0 | 1 = Rollen der Kategorien: Rekrut immer im Markt und nur mit Kat 2, Bewegungskarten nur mit Kat 3, auch auf den Koenig (siehe unten) |
| `pawn_cost` | 4 | mit `kat_roles`: so viele Kat-2-Einheiten einer Farbe kostet ein Bauer |

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

### Rollen der Kategorien (`kat_roles=1`)

Jede Kategorie bekommt eine feste Aufgabe, damit Kat 2 und Kat 3 nicht nur Kartengeld sind:

- **Kat 1:** Fabriken bauen und die Stufe von Figuren (wie bisher: Farben im Lager).
- **Kat 2:** Bauern. Die Karte *Rekrut* liegt immer im Markt (zusaetzlich zu den gezogenen Karten),
  kostet nur `pawn_cost` (4) Kat-2-Einheiten einer Farbe und laesst die Fabrik stehen. Der Bauer
  traegt die eigene Farbe. Aufloesen der Fabrik bleibt der Weg zu hoeheren Stufen.
- **Kat 3:** Spezialzuege. Lanze, Sprungfeder, Pferd, Laeuferschlag und Lange Beine kosten
  die Summe ihres alten Preises, aber nur in Kat 3 (Pferd 4, Lanze 4, Sprungfeder 3 ...).
- **Der Koenig** darf Lanze, Sprungfeder, Pferd und Laeuferschlag bekommen. Mit Laeuferschlag
  geht er auch auf freie Diagonalfelder ("ueber die Ecke"); bei Figuren schlaegt er nur.
  Ausbildung und Lange Beine gehen nicht auf den Koenig.

Ohne die Regel aendert sich nichts. `market_size=0` liefert mit der Regel nur den Rekruten.

Gemessen mit `stark` gegen `stark` (Seeds ab 7000, sonst Standard, je ohne und mit `kat_roles=1`):

| Einstellung | Partien | Runden | Karten je Partie | Geschlagen je Partie | Stufe 3 erreicht |
|---|---|---|---|---|---|
| 2 Spieler, ohne Rollen | 24 | 32,2 | 2,7 | 2,0 | 13 von 24 |
| 2 Spieler, Rollen | 24 | 37,8 | 10,0 | 3,5 | 9 von 24 |
| 3 Spieler, ohne Rollen | 16 | 84,6 | 6,7 | 5,6 | 3 von 16 |
| 3 Spieler, Rollen | 16 | 82,1 | 19,4 | 7,8 | 2 von 16 |
| 2 Spieler, Rollen, `pawn_cost=8` | 24 | 35,9 | 6,5 | 2,5 | 11 von 24 |
| 3 Spieler, Rollen, `pawn_cost=8` | 16 | 73,9 | 9,9 | 5,6 | 2 von 16 |
| 2 Spieler, Rollen + nichts waechst nach | 24 | 56,9 | 1,6 | 1,6 | 2 von 24 |
| 3 Spieler, Rollen + nichts waechst nach | 16 | 99,7 | 1,9 | 2,2 | 0 von 16 |

Was daraus folgt:

- Mit Nachwachsen machen Kat 2 und Kat 3 jetzt etwas: Die KI kauft 3- bis 4-mal so viele Karten,
  allein der Rekrut kommt auf 3 (2 Spieler) bis 4,6 (3 Spieler) Kaeufe je Partie, und es wird mehr
  geschlagen.
- Die Rollen verdraengen die hohen Stufen: Stufe 3 wird seltener und spaeter erreicht, vermutlich weil
  die KI ihre Einheiten lieber in Bauern und Bewegungskarten steckt (nicht eigens geprueft).
- Ein Bauernpreis von 8 statt 4 bremst den Rekruten (0,5 statt 3 Kaeufe je Partie bei 2 Spielern),
  die hohen Stufen kommen fast wieder so oft vor wie ohne Rollen (11 statt 13 von 24; die Stichproben sind klein), und es
  werden noch 1,5- bis 2,5-mal so viele Karten gekauft wie ohne Rollen. Der Preis ist also der
  Hebel, um die Rollen auszubalancieren.
- Das Pferd (4 Kat 3) und die Lanze sind mit Abstand die meistgekauften Karten (Pferd 81 Kaeufe
  in 16 Dreierpartien, Lanze 39 in 24 Zweierpartien mit `pawn_cost=8`); die Karten sind also noch
  nicht ausgewogen.
- Ohne Nachwachsen fallen die Karten auf weniger als 2 je Partie (vermutlich, weil Kat 2 und
  Kat 3 schnell abgebaut sind; nicht eigens geprueft), und mit drei Spielern gibt es wieder Remis (2 von 16). Rollen und
  "nichts waechst nach" vertragen sich in dieser Form nicht.
- Der Koenig bekommt Karten: In 8 Zweierpartien gingen 12 von 73 Kaeufen (16 %) an den Koenig,
  vor allem Sprungfeder, Lanze und Pferd.

### Spielart "Brettspiel" (nichts waechst nach; im Browser nicht mehr im Menue)

Mit `--rule regrow_cat1=0,regrow_cat2=0,regrow_cat3=0,regrow_cat4=0` (frueher eine Vorlage im Startmenue): jedes Vorkommen wird einmal abgebaut, so liesse sich
das Spiel mit Plaettchen auf einem echten Brett spielen. Gemessen (Seeds ab 5000, sonst Standard):

| Bots | Partien | Remis | erst durch Wertung | Runden im Schnitt |
|---|---|---|---|---|
| gierig, 2 Spieler | 100 | 0 | 10 | 35 |
| gierig, 3 Spieler | 100 | 2 | 25 | 55 |
| stark, 2 Spieler | 24 | 0 | 4 | 41 |
| stark, 3 Spieler | 24 | 3 | 17 | 95 |

Mit Nachwachsen waren es bei `stark` und 3 Spielern 0 Remis und 82 Runden. Ohne Nachwachsen
erstarrt die starke KI also wieder oefter (Gleichstand in der Wertung, Stufe 3 kommt kaum vor).
Naheliegende Abhilfe, noch nicht gebaut: das Spiel in die Wertung schicken, sobald mehrere Runden
lang nichts geschlagen oder gebaut wurde, statt nach einer festen Rundenzahl.

### Gebietsmodus (`territory=1`, im Browser "Version 1")

Ein eigener Regelsatz neben dem alten Spiel (nichts davon aendert sich, wenn `territory` aus ist).
Gedacht als Vereinfachung: kein Abbauen, keine Fabrikbauten, keine Kartenmaerkte mit Farbzwang.

- Das Brett ist neutral. Nur die Startecke jedes Koenigs hat seine Farbe und traegt die **einzige**
  Fabrik des Spielers.
- Jedes Feld, auf das eine Figur zieht (alle Figuren, der ganze Weg), bekommt ihre Farbe; betritt ein
  Gegner es, kippt es. Zaehlt nur ueber Kanten (4 Nachbarn). Weil `Move` keinen Weg angibt, nimmt
  die Engine unter den kuerzesten Wegen den mit den meisten neu gefaerbten Feldern. Springer/Sprungfeder
  faerben ein Brueckenfeld und das Ziel.
- Land muss an der Fabrik haengen. Schneidet ein Gegner eine Linie durch, faellt alles Abgetrennte an
  ihn (`terr_cut_flips=0`: es wird neutral). Eine Figur, die nicht auf angebundenem Land steht, faerbt
  nichts, schlaegt nichts und betritt keine Gegnerfabrik, bis sie wieder verbunden ist ("wehrlos").
- **Wieder anbinden** (`terr_reconnect`, Standard an): Eine wehrlose Figur ist wieder aktiv, sobald sie
  ueber eine Kante an das mit ihrer Fabrik verbundene Land des Besitzers grenzt, und faerbt dann sofort ihr
  eigenes Feld (kippt es also, wenn es einem Gegner gehoert; das kann dessen Linie durchtrennen). Das gilt
  fuer eine Figur, die selbst dorthin zieht, genauso wie fuer eine, die schon wartet und neben die das eigene
  Land waechst. Kette: Das neue Feld kann die naechste wartende Figur wecken (`territory_settle`).
- **Umschliessen** (`terr_enclose`, Standard an): Eine zusammenhaengende neutrale Flaeche, die nur an eine
  einzige Farbe grenzt, geht ganz an diese Farbe. Grenzt sie an zwei Farben oder an keine, bleibt sie neutral.
  Auch Land ohne Fabrik und abgetrenntes Land zaehlt als Wand. **Der Brettrand zaehlt standardmaessig nicht
  als Wand** (`terr_enclose_edge=0`): eine Flaeche am Rand wird nie umschlossen. Mit `terr_enclose_edge=1`
  holt ein "L" aus der Ecke ein ganzes Quadrat (siehe Messung). Im Zug, der das Spiel entscheidet, laeuft
  nichts davon: das weisse Land des Verlierers bleibt weiss.
- Am Ende des eigenen Zugs sammelt die Fabrik alle verbundenen Landfelder als **Flaeche** ein, dazu
  `terr_yield` je Vorkommen auf verbundenen Feldern. Rohstoffvorkommen werden nie leergebaut.
- Figur bauen (Fabrik muss frei sein, die Figur erscheint genau darauf): Bauer `terr_pawn` Flaeche (50,
  etwa Runde 11); Stufe 2: `terr_area` Flaeche (150) + `terr_k1` Kat 1 (20); Stufe 3: `terr_area` +
  `terr_area_step` (200) Flaeche, dazu `terr_k2` Kat 2 (40); Stufe 4: `terr_area` + 2 x Aufschlag (250)
  Flaeche, dazu `terr_k3` Kat 3 (60); die Rohstoffe der niedrigeren Stufen kommen jeweils dazu. Stufe =
  Reichweite. Flaeche und Rohstoffe werden dabei verbraucht. Hoechste Stufe = Zahl der Kat-1-Farben,
  hoechstens 4.
- Eine Karte (nur Zusatzfaehigkeiten: Laeuferschlag, Lange Beine, Lanze, Sprungfeder, Pferd) liegt neben
  der Fabrik und wirkt automatisch auf die naechste gebaute Figur. Preis nur nach Kategorie, die alten
  Kat-2/3-Preise mal `terr_card_mult` (16): Pferd 32 Kat 2 + 32 Kat 3, Lange Beine 16 Kat 2 + 32 Kat 3.
- Betritt (oder ueberquert) ein Gegner die Fabrik, wird sie sofort zerstoert: kein Spielende, aber keine
  neuen Figuren; das Lager ist weg. Ohne Fabrik faerbt man frei, ohne abzutrennen und ohne Einkommen.
  Das Spiel endet nur durch den Fall eines Koenigs (sonst Wertung: eigene Felder + Summe der Stufen).

Bots: `stark` und `gierig` bewerten Land, Anbindung und Fabrik (`ai.rs`, `territory_value`).

**Messung** (Zweierpartien, Seeds 300-359, 60 Partien je Zeile, `stark` gegen `stark`; Bots sind kein
Spass-Test, nur ein Stresstest der Regeln):

| Stand | Siege A | Runden | 1. Figur | Stufe 2 / 3 / 4 ab Runde | Abtrennungen je Partie |
|---|---|---|---|---|---|
| Erste Fassung (Stufen je 100 Flaeche, Karten x3) | 68 % | 32 | 10 | 20 / 21 / - | 2,0 (6,9 Felder) |
| **Jetzt** (Stufen 150 / 200 / 250, Karten x16) | 45 % | 41 | 11 | 25 / 28 / 36 | 3,0 (10,5 Felder) |

Nach dem ersten Spieltest (Karten zu stark fuer den Preis, hoehere Figuren zu frueh) wurden Stufen und
Kartenpreise angepasst. Kartenwert = Siege eines Bots, der Karten kauft, gegen einen gleich starken,
der nie welche kauft (`stark` gegen `stark-pur`, beide Sitze, je 120 Partien):

| Stufenleiter | Kartenpreis | Kartenkaeufer gewinnt | Karten je Partie |
|---|---|---|---|
| 100 / 100 / 100 | x3 | 77 % | 2,9 |
| 100 / 100 / 100 | x8 | 68 % | 2,9 |
| 100 / 100 / 100 | x10 | 61 % | 2,7 |
| 100 / 100 / 100 | x12 | 45 % | 1,7 |
| 150 / 200 / 250 | x12 | 82 % | 1,8 |
| 150 / 200 / 250 | x14 | 80 % | 1,6 |
| 150 / 200 / 250 | x16 | 51 % | 0,1 |
| 150 / 200 / 250 | x20 bis x40 | 50-52 % | 0,0-0,1 |

Stufenleiter (Kartenpreis x12, `stark` gegen `stark`):

| Flaeche Stufe 2 / 3 / 4 | Stufe 2 / 3 / 4 ab Runde | Partielaenge |
|---|---|---|
| 100 / 100 / 100 | 17,0 / 22,8 / 25,5 | 32 |
| 150 / 200 / 250 | 25,1 / 27,7 / 34,6 | 37 |
| 150 / 225 / 300 | 28,8 / 38,9 / 48,2 | 42 |
| 150 / 200 / 250 und Rohstoffpreise x1,5 | 29,4 / 30,3 / 38,0 | 36 |
| 150 / 250 / 350 und Rohstoffpreise x1,5 | 44,0 / 53,7 / 58,6 | 48 |
| 200 / 300 / 400 | 60,2 / 68,3 / 72,2 | 54 |

**Wieder anbinden und Umschliessen** (`stark` gegen `stark`, je 60 Partien; "Aus" = beide Regeln abgeschaltet,
das entspricht genau dem Stand davor):

| Stand | Seeds | Siege A | Runden | Umschlossen je Partie | Wieder angebunden |
|---|---|---|---|---|---|
| Aus | 300 / 400 / 500 | 45 % / 45 % / 45 % | 41,5 / 38,6 / 44,0 | - | - |
| Nur Wieder anbinden | 300 | 48 % | 40,5 | - | 0,5 |
| Nur Umschliessen, Rand als Wand | 300 | 60 % | 29,9 | 3,2 (21 Felder) | - |
| Nur Umschliessen, Rand keine Wand | 300 / 400 | 60 % / 70 % | 36,8 / 37,9 | 4,5 / 4,1 (10 / 9 Felder) | - |
| **Beide, Rand keine Wand (Standard)** | 300 / 400 | 63 % / 63 % | 38,6 / 38,5 | 4,7 / 4,0 (10 / 8 Felder) | 0,4 |
| Beide, Rand als Wand | 300 / 400 | 60 % / 65 % | 29,5 / 32,8 | 3,2 / 3,4 (21 Felder) | 0,1 / 0,2 |

- Wieder anbinden aendert fuer sich kaum etwas, es wird selten ausgeloest.
- **Der Rand als Wand verkuerzt die Partien um etwa ein Viertel**: Ein "L" aus der Ecke umschliesst ein
  ganzes Quadrat (quadratischer Gewinn fuer linearen Aufwand). Deshalb ist der Standard "Rand keine Wand".
- **Offene Reibung: Umschliessen bevorzugt den ersten Spieler.** Ohne die Regeln gewinnt A in allen drei
  Messreihen 45 %, mit Umschliessen 60 bis 70 % (rund 18 Punkte mehr bei +-6 Punkten Standardfehler). Wer die
  Taschen zuerst schliesst, gewinnt. Moegliche Hebel (nicht gebaut): Obergrenze oder Mindestgroesse fuer
  Taschen, Ausgleich fuer den zweiten Spieler.
- Rechenzeit der starken KI: etwa 60 ms je Zug ohne, 80 bis 90 ms mit den neuen Regeln.

Was daran auffaellt:

- **Karten sind ueber den Preis kaum feinzustellen**: Die Rohstoffe ueber den Stufenpreisen hinaus haben
  sonst keinen Zweck, deshalb kauft der Bot Karten, sobald das Lager reicht, und gewinnt damit klar. Zwischen
  x14 und x16 faellt der Kauf von 1,6 auf 0,1 je Partie: entweder ein Geschenk oder unerreichbar. Und es
  wird dabei fast nur **Pferd** gekauft (bei x12 bis x14 ueber 95 %), die anderen Karten sind uninteressant.
- Bei den teuersten Stufenleitern (letzte zwei Zeilen) erreicht der Bot sie kaum noch: er gibt jede Flaeche
  sofort fuer 50er-Bauern aus und spart nie. Messwerte dieser Zeilen sagen also mehr ueber den Bot als
  ueber die Regel.
- **Stufen 3 und 4 sind selten**: Stufe 4 kommt nur in jeder zweiten Partie vor (31 von 60), erst um Runde 36.
- **Schneeball**: Wer in Runde 20 mehr Land hat, gewinnt in 85 % der Partien (40 von 47).
- **Die Abtrennregel greift mehr** als in der ersten Fassung (3,0 statt 2,0 je Partie), vermutlich weil die
  Partien laenger sind; ob das Abgetrennte dem Gegner oder niemandem zufaellt, ist weiter unbestimmt (erste Fassung: 68 % gegen 70 %).
- **Fabriken fallen fast nie** (0,08 je Partie): Die Bots rennen lieber mit dem Koenig.
- **Anziehender Vorteil**: A 68 % in der ersten Fassung, jetzt 45 % (je 60 Partien, Rauschen etwa +-9 Punkte).
  Alle Zwischenstufen mit den neuen Preisen lagen bei 50-55 %.

### Version 2 (`terr_v2=1`, im Browser "Version 2")

Ein Regelversuch auf dem Gebietsmodus. **Alles liegt hinter Schaltern in `Config` (Standard aus), Version 1
bleibt unveraendert** (die Tests der alten Regeln laufen weiter). `terr_v2=1` setzt alle Schalter zusammen,
jeder laesst sich danach einzeln ueberschreiben (`--rule territory=1,terr_v2=1,terr_ripen2=5`); `terr_v2=0`
schaltet sie wieder aus. Die Browser-Seite "Version 2" schickt immer `territory=1,terr_v2=1` voraus; im Menue
steht dazu ein Feld "Version 2 an (1) oder aus (0)".

| Schalter | Version 2 | Wirkung |
|---|---|---|
| `terr_grow` | 2 | Figurenpreise nur in Flaeche: Stufe n kostet `terr_pawn` mal `terr_grow` hoch (n-1), also 50 / 100 / 200 / 400. 0 = Preise wie Version 1 (mit `terr_area`, `terr_k1..3`) |
| `terr_exact` | 1 | Eine Figur der Stufe n zieht **genau n Felder geradeaus** (wie ein Turm, nicht durch Figuren). Das Zielfeld darf ein Gegner sein (wird geschlagen), nie eine eigene Figur. Der Weg faerbt wie bisher. Der Koenig zieht weiter ein Feld |
| `terr_ripen1..4` | 3, 10, 0, 0 | Ein Vorkommen bringt erst Ertrag, wenn das Feld dem Besitzer so viele Runden gehoert (Kat 1: 3, Kat 2: 10). Umfaerben durch einen Gegner setzt die Uhr zurueck (`State::owned_since`), das eigene Feld noch einmal betreten nicht. Der Ertrag kommt am Ende des Zugs des Besitzers; hat er in dieser Runde schon gezogen, zaehlt die Uhr bis zur naechsten |
| `terr_cats` | 2 | Nur Kat 1 und Kat 2 gibt es (Kat 3 und 4 entfallen, das Brett hat deutlich weniger Rohstofffelder). Rohstoffe sind nur noch fuer Karten da, nie fuer Figuren |
| `terr_cards` | 1 | Der Markt kommt aus dem Pool der neuen Karten (`Card::V2`), Preise in Kat 1 und Kat 2 (mal `terr_card_mult` / 16) |

Karten in Version 2 (Preise bei `terr_card_mult=16`; nur eine liegt neben der Fabrik, ausser dem Koenigs-Boost):

| Karte | Preis | Wirkung |
|---|---|---|
| Kuerzer (`shorten`) | 6 Kat 1 | Die naechste gebaute Figur darf einen Schritt weniger ziehen (Stufe 3: 3 oder 2 Felder) |
| Ecke (`bend`) | 10 Kat 1 | Die naechste Figur darf auf ihrem Weg einmal um die Ecke gehen (dann bleibt es bei insgesamt n Feldern) |
| Sprung (`hop`) | 8 Kat 1 | Die naechste Figur darf auf ihrem Weg ueber eine Figur springen (ein Feld davon zaehlt mit) |
| Fabrik ruecken (`factory_shift`) | 6 Kat 1 | Statt eines Zuges: die Fabrik rutscht ein Feld diagonal, auf eigenes, mit der Fabrik verbundenes Land ohne Figur (`Action::ShiftFactory`). Nur Quadratbrett. Die Karte bleibt neben der Fabrik liegen, bis sie benutzt wird |
| Koenigs-Boost (`king_boost`) | 3 Kat 2, mal Stufe | Wirkt sofort und dauerhaft, braucht den Kartenplatz nicht, bis Stufe 3. Stufe 1: der Koenig darf auch ein Feld diagonal. Stufe 2: zusaetzlich zwei Felder gerade. Stufe 3: zusaetzlich zwei vor und eins zur Seite (ein "L"). Die Stufen gelten zusammen. Nur Quadratbrett. Je Stufe teurer (3, 6, 9 Kat 2) |

Pro Zug ist weiter nur **ein** Kartenkauf moeglich. Viewer: ein Zaehler auf dem Vorkommen zeigt die Runden bis
zum ersten Ertrag (blass bis dahin), Figuren tragen Abzeichen fuer Kuerzer (-1), Ecke (L) und Sprung (Bogen), der
Koenig einen Stern je Boost-Stufe, "Fabrik ruecken" bedient man ueber die Fabrik-Karte (Knopf, dann ein markiertes
Feld anklicken; Esc bricht ab). Die Wegplanung mit Shift/Strg bleibt, bricht aber ab, wo kein erlaubter Zug weiterfuehrt.

**Paritaet (Folge von "genau n")**: Auf dem Quadratbrett wechselt ein Zug der Laenge n die Schachbrett-Farbe des
Feldes genau dann, wenn n ungerade ist. Figuren der Stufe 2 und 4 bleiben also immer auf ihrer Feldfarbe (wie ein
Laeufer) und koennen nur Figuren schlagen, die auf derselben Farbe stehen; Stufe 1 und 3 wechseln mit jedem Zug. Der
Koenig (ein Feld) wechselt immer. Ecke und Sprung aendern daran nichts (der Weg hat weiter n Schritte); nur
**Kuerzer** (n-1 Schritte) bricht die Paritaet, beim Koenig der diagonale Boost-Schritt. Auf dem Sechseckbrett gibt es diese
Paritaet nicht. Ob das so gewollt ist, ist offen.

**Flaechen-Bot** (`--bots flaeche,...`, im Browser "Bot fläche"): siehe Tabelle der Computergegner. Messung unten.

**Messung** (2 Spieler, Seeds ab 100, je 40 Partien, `stark` mit `--nodes 4000` gegen `flaeche` je 20 Partien pro Sitz,
`stark` gegen `stark` 20 Partien mit Standardbudget; "Wertung" = nicht durch einen gefallenen Koenig entschieden,
sondern nach Runde 100). Das sind Stresstests der Regeln, keine Spielspass-Tests, und mit 20 bis 40 Partien rauscht
jede Zahl um 8 bis 15 Prozentpunkte:

| Paarung | Version 1 | Version 2 |
|---|---|---|
| `flaeche` gegen `gierig` (beide Sitze, 80 Partien) | 74 % `flaeche`, 43 Runden | 92 % `flaeche`, 49 Runden |
| `stark` gegen `flaeche` (beide Sitze, 40 Partien) | 88 % `stark`, 34 Runden | 68 % `stark`, 52 Runden |
| `flaeche` gegen `flaeche` | A 40 %, 36 Runden | A 48 %, 60 Runden, 16 von 40 nach Wertung |
| `gierig` gegen `gierig` | A 45 %, 45 Runden | A 50 %, 77 Runden, 10 von 40 nach Wertung |
| `stark` gegen `stark` (20 Partien) | A 80 %, 40 Runden | A 50 %, 87 Runden, 14 von 20 nach Wertung |

Was daran auffaellt (die Zahlen sind Hinweise, keine Beweise):

- **`flaeche` schlaegt `gierig` klar, `stark` schlaegt `flaeche` aber trotzdem.** In Version 2 ist der Abstand zu `stark`
  kleiner (88 % auf 68 %): Land und Rohstoffe zaehlen dort mehr, reiner Figurenkampf weniger. Der Bot ist also
  brauchbar als "Wirtschaftsspieler", aber nicht der beste Gegner; er rechnet nur einen Zug voraus.
- **Version 2 ist langsamer**: Partien dauern gut doppelt so lang (`stark` gegen `stark`: 40 auf 87 Runden), und
  14 von 20 enden erst in der Wertung. Genau n Felder und die Wartezeit auf Rohstoffe nehmen dem Koenig-Jagen die
  Schnelligkeit; es werden auch weniger Figuren geschlagen (2,3 statt 3,1 je Partie bei `stark` gegen `stark`).
- **Der Vorteil des ersten Spielers ist weg** (`stark`: 80 % auf 50 %), vermutlich weil das frueh geschlossene
  Umschliessen (Version 1: wer die Taschen zuerst schliesst, gewinnt) durch "genau n" schwerer wird. Offen, nicht
  untersucht; mit 20 Partien auch gut Zufall.
- **Karten werden gekauft** (3 bis 8 je Partie, beide Spieler zusammen; in Version 1 fast nie), weil die Rohstoffe jetzt
  nur dafuer da sind. Welche, ist nicht ausgewertet; `stark` kennt die neuen Karten nicht gezielt (seine Bewertung hat
  keinen Wert fuer Ecke, Sprung, Kuerzer, Fabrik ruecken).
- **Der Flaechen-Bot hat in einer 3-Spieler-Partie 441 Flaeche ungenutzt gehortet**, weil eine Figur auf der Fabrik stand.
  Das ist behoben (die Figur wird weggezogen, wenn eine bessere bezahlbar waere), kostete aber in der Messung gegen
  `gierig` etwas (97 % auf 92 %).

### Version 3 (`terr_v3=1`, im Browser "Version 3")

Noch ein Regelversuch obendrauf. Wieder **alles hinter Schaltern in `Config` (Standard aus)**, Version 1 und 2
bleiben unveraendert. `terr_v3=1` setzt Version 2 (`set_v2`) und dazu die Schalter unten; jeder laesst sich danach
einzeln ueberschreiben, `terr_v3=0` schaltet sie aus. Die Browser-Seite schickt standardmaessig
`territory=1,terr_v3=1` voraus; im Menue waehlt das Feld "Regelstufe" 3, 2 oder 1 (Version 1 und 2 sind also weiter
spielbar).

| Schalter | Version 3 | Wirkung |
|---|---|---|
| `terr_cycle` | 1 | **Erntezaehler**: jedes Vorkommen hat einen Zaehler (`State::age`, Erntetakte seit dem letzten Besitzerwechsel). Ist er abgelaufen, wird es einmal geerntet, dann beginnt er von vorn. Periode je Kategorie `terr_ripen` = 3 / 5 (Kat 1 / 2; Kat 3 wird nicht geerntet). Der Zaehler laeuft nur, wenn eine **stehende** Fabrik das Feld ueber verbundenes Land erreicht, sonst steht er still und es wird nichts geerntet. Ein Umfaerben des Feldes setzt ihn zurueck. Periode 0 = jede Runde |
| `terr_bonus` | 1 | **Kat 3 als Flaechenbonus**: Kat 3 wird nicht geerntet und liegt nie im Lager. Am Ende jedes eigenen Zuges bringen die `k` Kat-3-Felder auf dem mit der stehenden Fabrik **verbundenen** Land zusaetzlich `2^k` Flaeche (`Game::kat3_bonus`; `k = 0` bringt nichts, gedeckelt bei `2^20`). Kein Zaehler, jede Runde. `Event::Income` traegt `kat3` und `bonus` |
| `terr_trade` | 0 | Alte Fassung (Version 3 vor der Aenderung): Kat 3 wird geerntet (`terr_ripen3`) und mit `Action::TradeKat3` einmalig gegen `2^k` Flaeche getauscht. Geht nur zusammen mit `terr_bonus=0` |
| `terr_occupy` | 1 | **Zaehler nur mit eigener Figur**: der Erntezaehler eines Feldes (und damit seine Ernte) laeuft nur, wenn am Ende des eigenen Zuges eine **eigene Figur (auch der Koenig)** auf dem Feld steht. Sonst **pausiert** er, ohne zurueckgesetzt zu werden. Umfaerben setzt ihn weiter zurueck; die Flaeche des Feldes zaehlt weiter, der Kat-3-Bonus bleibt passiv (`Game::own_piece_at`, `territory.rs`). Braucht `terr_cycle` |
| `terr_cardany` | 1 | **Karten auf jede eigene Figur**: Kuerzer, Ecke und Sprung wirken sofort auf eine gewaehlte eigene Figur, auch eine schon gebaute (`Action::BuyCard { target: Some(Figur) }`, `Event::CardBought` traegt `target`). Nicht auf den Koenig, nicht auf Stufe 1 (Reichweite < 2), nicht wenn sie die Faehigkeit schon hat. Der Kartenplatz neben der Fabrik bleibt nur fuer "Fabrik ruecken"; Koenigs-Boost wirkt wie bisher sofort. Braucht den Kartensatz von Version 2 |
| `terr_card_mult` | 8 (Version 2: 16) | Kartenpreise mal `terr_card_mult` / 16 (mindestens 1): Kuerzer 3, Ecke 5, Sprung 4, Fabrik ruecken 3 Kat 1, Koenigs-Boost 1 Kat 2 je Stufe |
| `terr_stack` | 1 | **Karten mehrfach auf dieselbe Figur**: Kuerzer, Ecke und Sprung duerfen so oft auf eine Figur, wie es logisch Sinn hat: Stufe `n` hoechstens `n - 1` mal (der letzte Schritt ist das Ziel). Kuerzer ×k: die Figur zieht `n`, `n-1`, ... bis `n-k` Felder (Stufe 3 mit zwei Karten: 3, 2 oder 1); Ecke ×k: bis zu `k` Ecken auf dem Weg; Sprung ×k: ueber bis zu `k` Figuren. `Perks::shorten`, `bend`, `hop` sind jetzt Zaehler (`u8`); im Zustands-JSON stehen Zahlen statt true/false. 0 = jede Karte nur einmal je Figur wie bisher. Braucht `terr_cardany`; auch `terr_control` zaehlt die Spruenge |
| `terr_cardlock` | 1 | **Karte sperrt die Figur**: 1 = die Figur, die eine Karte bekommt (beim Koenigs-Boost der Koenig), darf in diesem Zug nicht mehr ziehen (Zugfelder leer; `State::card_lock`, am Zugende geloescht); 0 = nichts; 2 = der Kauf beendet den ganzen Zug (Kuerzer, Ecke, Sprung, Koenigs-Boost; "Fabrik ruecken" nicht). Der Kauf bleibt ein Teil des Zuges, nur eine Karte je Zug; die gesperrte Figur kann aber noch durch eine andere Figur geschlagen werden |
| `terr_up_mult` | 2 | Figuren ab Stufe 2 kosten das `terr_up_mult`-Fache (Stufe 1 bleibt bei `terr_area`): bei 2 sind es 50 / 200 / 400 / 800 Flaeche (Version 2: 50 / 100 / 200 / 400) |
| `terr_upkeep` | 1 | **Unterhalt je Figur**: am Ende des eigenen Zuges, nach dem Einkommen, kostet jede Figur ihre Stufe in Flaeche (Bauer 1 ... Held 4, der Koenig nichts). Reicht die Flaeche nicht, loest sich so lange eine Figur auf, bis sie bei 0 liegt: zuerst die kleinste Figur, die den Fehlbetrag **allein** deckt, gibt es keine, die groesste (bei gleicher Stufe die zuletzt gebaute); ohne Rueckerstattung. Der Unterhalt gehoert zum Einkommen: steht die Fabrik nicht (zerstoert oder im Bau), faellt er nicht an. `Event::Upkeep`, `Event::Dissolved`; `Config::terr_upkeep` |
| `terr_kingup` | 1 | **Koenigs-Upgrade mit Gold**: der Koenigs-Boost liegt immer aus (nicht mehr im Markt, `draw_market_territory(.., kingup)`) und kostet **Kat 2 (Gold)**: `terr_kingup_price` je zusaetzlichem Zugfeld, das der Koenig auf freiem Brett dadurch bekommt. Quadrat 4 / 4 / 8 Felder (Stufe 1 / 2 / 3, also 4 / 4 / 8 Gold), Sechseck 6 / 6 / 12; auf dem Dreieck gibt es keine Zugfelder zusaetzlich, dort bleibt der Boost aus. Der Kauf ist weiter der Karten-Kauf des Zuges (hoechstens einer je Zug, mit `terr_cardlock=1` ruht der Koenig danach). Der Boost wirkt nun auch auf dem Sechseckbrett (`king_boost_moves`); `Game::king_upgrade_on`, `king_boost_extra`, `territory_offers` |
| `terr_kingup_price` | 1 | Gold je zusaetzlichem Zugfeld des Koenigs (nur mit `terr_kingup`) |
| `terr_control` | 0 | **Kontrollfelder** (Schalter, Standard aus): als bearbeitet zaehlen (Zaehler, Ernte, Kat-3-Bonus) auch alle Felder in Zugreichweite einer eigenen Figur: gerade Strahlen bis zur Reichweite, der erste Stein (egal wessen) stoppt den Strahl, ein Sprung geht ueber genau einen; der Koenig kontrolliert seine Nachbarn. Figuren, die nicht auf mit der Fabrik verbundenem Land stehen, kontrollieren nichts. Braucht `terr_occupy` (`Game::worked_cells`) |
| `kat3_spread` | 1 | Kat-3-Felder gleichmaessig: je Ecke zwei **einzelne** Felder statt einer Ader aus zwei (`cells3 = 2`, `veins3 = 1`), die Startpunkte nach groesstem Abstand zu den bisherigen Kat-3-Feldern und zum Brettrand gewaehlt, im eigenen Gebiet mindestens 2 Felder vom Start entfernt |
| `terr_move` | 1 | **Fabrik-Umzug / Neubau durch den Koenig** (`Action::RebuildFactory`, JSON `rebuild_factory`), siehe unten |
| `terr_move_k1`, `terr_move_k2` | 0, 0 | Preis des freiwilligen Umzugs aus dem Lager der Fabrik. **Seit der fuenften Aenderung kostenlos** (vorher 4 Kat 1 + 2 Kat 2); wer den Preis zurueckwill, setzt die Werte |
| `board_corners` | 0 | Brettform unabhaengig von der Spielerzahl: 3 = Dreieck, 4 = Quadrat, 5 bis 8 = Fuenfeck bis Achteck (Sechseck-Felder). 0 = nach Spielerzahl wie bisher. Die Startecken der Spieler waehlt der Spielkern **fair** (`config::fair_corners`, siehe unten) |
| `terr_rebuild` | 3 | Bauzeit in Runden, wenn die Fabrik zerstoert wurde (fest, egal wo, kostenlos) |
| `terr_cats` | 3 | Kat 1 bis 3 gibt es |
| `veins1..3`, `cells1..3`, `center_cells`, `center_mix` | 1/1/2, 1/1/1, 2, 0 | weniger Rohstoffe: 20 Vorkommen statt 29 auf dem 10x10-Brett (8 Kat 1, 4 Kat 2, 8 Kat 3; Kat 3 je Ecke als zwei einzelne Felder, vorher eine Ader aus zwei) |

**Fabrik-Umzug** (freiwillig, die Fabrik steht und der Koenig steht auf verbundenem eigenem Land ohne Fabrik):

- Der Umzug ist **kostenlos** (`terr_move_k1/k2 = 0`; einstellbar), **die alte Fabrik faellt weg**, auf dem Feld des Koenigs entsteht
  die neue. Lager, Flaeche und die liegende Karte ziehen mit um.
- Dauer = **halbe Entfernung** zum alten Platz in Feldern (Schritte ueber Nachbarfelder), aufgerundet, mindestens 1
  Runde (`Game::relocation_rounds`): 1 bis 2 Felder = 1 Runde, 3 bis 4 = 2, ... quer ueber das 10x10-Brett (18) = 9.
  Der Viewer nennt die Dauer vorab: im Tooltip ueber jedem eigenen verbundenen Feld, in der Fabrik-Karte (fuer das
  Feld des Koenigs) und im Knopf.
- Die Zaehlung: bei der Bestellung steht `building = Runden + 1`; am Ende jedes eigenen Zuges sinkt der Wert um 1, bei 0
  steht die Fabrik (`Event::FactoryStands`). Bei n Runden steht sie also am Ende des n-ten folgenden eigenen Zuges,
  und in diesem Zug gibt es noch kein Einkommen.

**Zerstoerte Fabrik**: wer die Fabrik betritt, zerstoert sie wie bisher (Lager und Flaeche gehen verloren). Der Koenig
baut sie auf **eigenem Land** an seinem Standort neu, kostenlos, fest `terr_rebuild` = 3 Runden.

**Waehrend des Baus** (beide Faelle): kein Einkommen, die Erntezaehler stehen, nichts kaufen oder bauen, das Land ist
**nicht verbunden** (`connected_land` ist leer): alle Figuren sind **wehrlos** (sie faerben nichts, schlagen nicht,
ziehen aber), und der **Koenig darf sich nicht bewegen**. Wer auf den Bauplatz zieht, zerstoert die Fabrik. Das Land
wird dabei **nicht** an den Gegner umgefaerbt (Annahme: "als waere das Gebiet abgetrennt" gilt nur fuer Verteidigung,
Faerben und Ertrag); steht die Fabrik wieder, wird alles neu angebunden. Auch der Kat-3-Bonus fehlt in dieser Zeit.

Bots: `stark`, `gierig`, `flaeche` bauen eine zerstoerte Fabrik sofort neu. **Keiner zieht freiwillig um.** Der
Flaechen-Bot (`AreaBot`) und der Suchbot (`stark`) bewerten den Bonus (`2^k` je Runde) mit; `gierig` kennt ihn nicht.
`stark` kennt die Karten von Version 2/3 nicht gezielt.

**Messung** (2 Spieler, Seeds ab 100 bzw. 200, 40 Partien, `stark` mit `--nodes 4000`, Spiegel 16 Partien; "Wertung" =
Entscheidung erst nach Runde 100). Wieder Stresstests, und bei 16 bis 40 Partien rauscht jede Zahl um 8 bis 25
Prozentpunkte:

| Paarung | Version 2 | Version 3 (Bonus) |
|---|---|---|
| `flaeche` gegen `gierig` (beide Sitze) | 92 % `flaeche`, 49 Runden | 86 % `flaeche`, 74 Runden |
| `stark` gegen `flaeche` (beide Sitze, 80 Partien) | 68 % `stark`, 52 Runden | 92 % `stark`, 67 Runden |
| `flaeche` gegen `flaeche` | A 48 %, 60 Runden | A 48 %, 74 Runden, 16 von 40 nach Wertung |
| `gierig` gegen `gierig` | A 50 %, 77 Runden | A 45 %, 72 Runden |
| `stark` gegen `stark` (16 Partien) | A 50 %, 87 Runden | A 31 %, 97 Runden, 14 von 16 nach Wertung |

Was daran auffaellt:

- **Der Kat-3-Bonus ist der groesste Flaechenposten**: je Partie 4000 bis 8400 Bonus-Flaeche gegen 4000 bis 6100 aus
  den Feldern, also 40 bis 60 % des gesamten Flaecheneinkommens. Durch `2^k` waechst er mit jedem weiteren
  verbundenen Kat-3-Feld auf das Doppelte: bei `flaeche` gegen `flaeche` haben 13 % der Zugenden alle 8 Kat-3-Felder
  (256 Flaeche je Runde), 21 % keins. Wer die Kat-3-Felder der freien Ecken zuerst bekommt, haengt den anderen ab.
- **Kat 1 bleibt reichlich**: rund 110 Ernten Kat 1 je Partie (`flaeche`, `gierig`), 165 beim `stark`-Spiegel, dazu
  12 bis 21 Kat 2.
- **Zerstoerte Fabriken kosten kaum noch**: 1,4 bis 2,4 Neubauten je Partie bei den schwachen Bots, beim `stark` nur
  0,1 bis 0,7. Drei wehrlose Runden sind ein Fenster, das die Bots selten ausnutzen.
- **Partien sind laenger geworden** (Spiegel mit `stark`: 87 auf 97 Runden, 14 von 16 erst in der Wertung).
- **Vorteil des ersten Spielers** ist mit 16 bis 40 Partien nicht von Zufall zu trennen (`stark`-Spiegel: A nur 31 %).

**Zaehler nur mit Figur, Karten auf jede Figur (zweite Aenderung der Version 3).** Der Viewer zeigt den pausierten
Zaehler grau mit Pausenbalken; der Tooltip eines verbundenen Feldes sagt, ob es gerade bearbeitet wird. Ein Klick auf
**irgendeine** Figur (auch eine gegnerische, auch wenn man nicht am Zug ist) blendet alle ihre Ziehfelder ein (rot =
schlagen), ein zweiter Klick oder Esc blendet sie aus (`sv_destinations` im Spielkern, aendert nichts am Spiel). Eine
Karte, die eine gebaute Figur verbessert, loest einen **Blitz** auf dieser Figur aus (Schein, Ring, Strahl, der
Kartenname steigt auf; mit "weniger Bewegung" im System ohne Strahl und kuerzer). Kartenkauf im Viewer: Karte im Markt
anklicken, dann die golden markierte Figur auf dem Brett (Esc bricht ab).

Messung (2 Spieler, Seeds ab 100, 40 Partien Spiegel, 20 Partien je Sitz fuer `stark` mit `--nodes 2000`; wieder nur
ein Stresstest, bei dieser Stichprobe rauscht jede Zahl um 15 Prozentpunkte):

| | vorher (Bonus-Fassung) | jetzt |
|---|---|---|
| Ernte Kat 1 / Kat 2 je Partie (`flaeche`-Spiegel) | 114 / 13 | 44 / 9 |
| Karten je Partie (`flaeche`-Spiegel) | 6 | 12 |
| Runden im Schnitt (`flaeche`-Spiegel) | 67 | 77 |
| `stark` gegen `flaeche` (beide Sitze) | 92 % `stark` | 88 % `stark` (35 von 40) |

- Die Ernte sinkt auf gut ein Drittel, weil ein Feld nur zaehlt, solange jemand darauf steht. Die Bots bewerten das
  (`flaeche`: ein Vorkommen ohne Figur ist nur ein Drittel wert; `stark`: Gewicht `work` fuer eigene Figuren auf
  verbundenen Kat-1/2-Feldern); die Ernte bleibt trotzdem niedrig (ob andere Gewichte mehr herausholen, ist nicht
  untersucht).
- Der Fabrik-Umzug war damals mit 4 Kat 1 + 2 Kat 2 selten bezahlbar (Kat 2 kommt nur etwa 9-mal je Partie, davon ein
  Teil an den Gegner); seit der fuenften Aenderung ist er kostenlos.
- Die Karten sind jetzt billig genug, dass der Flaechen-Bot sie 12-mal je Partie kauft (vorher 6): Kuerzer 175,
  Sprung 111, Koenigs-Boost 110, Ecke 99 Kaeufe in 40 Partien.

**Dritte Aenderung der Version 3: Sperre, Preise, Ernte alle 5 Runden, Kat 3 verteilt, Kontrollfelder.** Die Karte
sperrt die verbesserte Figur fuer den Zug (`terr_cardlock=1`; der Viewer zeigt ein Pausenzeichen am Stein und sagt es
im Zugtext). Gewaehlt wurde die Figur-Sperre statt des ganzen Zuges, weil sie milder ist und keine Luecke laesst (die
Karte kann nur vor dem beendenden Zug gekauft werden); `terr_cardlock=2` macht den Kauf zum ganzen Zug. Figuren ab
Stufe 2 kosten das Doppelte (50 / 200 / 400 / 800), Kat 2 reift in 5 statt 10 Runden. Die Kontrollfelder sind ein
**Schalter** (`terr_control=1`, Standard aus), weil das noch eine Ueberlegung ist und nicht beschlossen.

Messung (2 Spieler, Seeds ab 100, 40 Partien `flaeche`-Spiegel; `stark` gegen `flaeche` 20 Partien je Sitz, `--nodes 2000`):

| Einstellung | Ernte Kat 1 / Kat 2 | Karten | Runden | Anmerkung |
|---|---|---|---|---|
| vorher (zweite Aenderung) | 44 / 9 | 12 | 77 | erste Figur Stufe 2/3/4 in Runde 18 / 27 / 36 |
| jetzt (`terr_cardlock=1`) | 45 / 21,6 | 12,6 | 79 | Stufe 2/3/4 in Runde 24 / 38 / 54 |
| `terr_cardlock=0` | 46,5 / 22 | 12,9 | | kaum Unterschied: die Sperre ist ein weicher Preis |
| `terr_cardlock=2` | 49 / 23 | 11,0 | | Kuerzer und Sprung etwa halb so oft gekauft |
| `terr_control=1` | 94 / 31 | 16,7 | | holt die Ernte fast auf den Stand vor der Besetzungsregel zurueck |
| `stark` gegen `flaeche` | 16 : 24 | | | vorher 35 : 5; mit `terr_up_mult=1` 31 : 9 |

- Mit **Kontrollfeldern** zaehlt fast jedes Feld in Reichweite, die Besetzungsregel (Zaehler nur mit Figur) verliert
  damit weitgehend ihre Wirkung. Das spricht eher gegen die Regel als Standard; sie bleibt ein Schalter zum Ausprobieren.
  Die Reichweite ist als Strahl gerechnet (Zugfelder einer Figur mit genau n Feldern sind nur das Feld n entfernt),
  was auch nicht ganz dem Zug entspricht.
- Die Preisverdopplung verschiebt das Gleichgewicht zur Wirtschaft: der Suchbot `stark` verliert jetzt gegen `flaeche`
  (16 : 24), weil seine Figuren teurer sind. Kleine Stichprobe, rund 15 Prozentpunkte Rauschen.
- **Kat 3 verteilt**: bei 300 Seeds stieg der Abstand zum naechsten anderen Kat-3-Feld von 1,27 auf 3,66, der
  Abstand eines Feldes zum naechsten Kat-3-Feld sank im Mittel von 2,55 auf 1,76, im schlechtesten Fall von 8 auf 5.

**Vierte Aenderung der Version 3: Karten stapeln, Reichweite beim Ueberfahren.** `terr_stack=1` laesst Kuerzer, Ecke und
Sprung mehrfach auf dieselbe Figur (siehe Tabelle). Die Sperre gilt weiter: eine Karte je Zug, und die Figur ruht
dabei, also kostet jede Schicht einen Zug mit dieser Figur. Im Viewer zeigt das Abzeichen die Anzahl (`−2`, Treppe mit
mehreren Knicken, mehrere Boegen), der Tooltip nennt sie ("Kuerzer ×2", "darf auch 3 oder 2 Felder ziehen"). Wer mit der
Maus auf **irgendeine** Figur zeigt, sieht ihre Ziehfelder (rot = schlagen), solange die Maus darauf steht; ein Klick
haelt sie weiter fest. Das gilt nur an der aktuellen Stellung einer laufenden Partie oder im Zuschau-Modus, denn die
Zuege kommen vom Spielkern (`sv_destinations`); in einer fertig geladenen Aufzeichnung gibt es sie nicht. Mit Shift oder
Strg (Wegplanung) und waehrend der Kartenwahl bleibt die Anzeige aus.

Messung (2 Spieler, Seeds ab 100, 40 Partien `flaeche`-Spiegel; `stark` gegen `flaeche` 20 Partien je Sitz, `--nodes 2000`):

| | ohne Stapeln (`terr_stack=0`) | mit Stapeln |
|---|---|---|
| Karten je Partie | 12,6 | 14,2 |
| Kuerzer / Koenigs-Boost / Sprung / Ecke (Kaeufe in 40 Partien) | 147 / 146 / 101 / 109 | 216 / 140 / 120 / 91 |
| geschlagene Figuren je Partie | 3,6 | 3,1 |
| Runden im Schnitt | 79,4 | 78,8 |
| `stark` gegen `flaeche` (beide Sitze) | 16 : 24 | 20 : 20 |

Die Bots nutzen das Stapeln kaum geschickt (`flaeche` kauft einfach noch mehr Kuerzer), im Gleichgewicht der beiden
Bots aendert sich nichts, was ueber das Rauschen hinausgeht. Ob ein Mensch mit gestapeltem Kuerzer (ein Held mit
dreimal Kuerzer zieht 1 bis 4 Felder geradeaus, also wie ein Turm bis zur Reichweite 4) staerker spielt, ist nicht
gemessen: Die Regel macht aus "genau n Felder" eine Zugweite "bis n" fuer 3 Karten zu je 3 Kat 1 (bei `terr_card_mult=8`).

**Sonde: lohnt ein Umzug zur Mitte?** (`cargo test --release -p game_core rush_probe -- --ignored --nocapture`, 40
Partien je Zeile, beide Sitze). Ein Hilfsbot (`Rusher` in `v3_tests.rs`, kein Teil des Spiels) schickt den Koenig zur
Mitte und baut die Fabrik dort neu, sobald der Umzug bezahlbar ist; sonst spielt er wie der Flaechen-Bot:

| | gegen `flaeche` | gegen `stark` (2000 Knoten) |
|---|---|---|
| Bot ohne Umzug | 50 % | 18 % |
| Umzug zur Mitte, Preis 4 Kat 1 + 2 Kat 2 | 20 % (Umzug im Schnitt Runde 33) | 5 % (Runde 37) |
| Umzug zur Mitte, gratis (**jetzt Standard**) | 45 % (Runde 8) | 0 % (Runde 8, Partie nur 28 Runden) |

Die Kosten (2 Kat 2 gibt es erst nach etwa 20 Runden) verhindern den Umzug in den ersten Runden; ein frueher Umzug
ohne Preis ist gegen den schwachen Bot gleichwertig, gegen den starken Bot ein Verlust (wehrlos und der Koenig auf dem
Bauplatz). Das gilt fuer einen plumpen Hilfsbot; ein Mensch, der den Zeitpunkt abpasst, ist damit nicht widerlegt.
Nach der Umstellung auf den kostenlosen Umzug neu gemessen (40 Partien je Zeile): der Hilfsbot gewinnt 40 % gegen
`flaeche` und 8 % gegen `stark`; der Umzug macht ihn also nicht zur Gewinnstrategie. Ob er gegen `flaeche` schlechter
ist als der Bot ohne Umzug (50 %), laesst sich bei 40 Partien (rund 8 Prozentpunkte Rauschen) nicht sagen.

**Fuenfte Aenderung der Version 3: kostenloser Umzug, freie Brettform, Auffuell-Bots.**

- **Umzug kostenlos**, damit man ihn schon frueh nutzen kann (siehe oben). Die Wartezeit (halbe Entfernung) und die
  Folgen waehrend des Baus (kein Ertrag, wehrlos, Koenig gesperrt) bleiben der Preis.
- **Jedes Brett mit jeder Spielerzahl**, solange eine Ecke frei ist und niemand einen Vorteil hat. Die Startecken
  sind **fair**, wenn die Symmetrien des Bretts (Drehungen und Spiegelungen, die die Eckenmenge auf sich abbilden)
  jede Ecke in jede andere ueberfuehren (`config::corners_are_fair`, `fair_corners`, `fair_player_count`). Plaetze
  am Tisch bei gewuenschter Spielerzahl, *mit Auffuellen* (aus dem Menue-Test aller Bretter gelesen):

  | Brett | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
  |---|---|---|---|---|---|---|---|
  | Dreieck | 2 | 3 | | | | | |
  | Quadrat | 2 | **4** | 4 | | | | |
  | Fuenfeck | 2 | **5** | **5** | 5 | | | |
  | Sechseck | 2 | 3 | 4 | **6** | 6 | | |
  | Siebeneck | 2 | **7** | **7** | **7** | **7** | 7 | |
  | Achteck | 2 | **4** | 4 | **8** | **8** | **8** | 8 |

  Fett = es kommen Auffuell-Bots dazu. Ohne Auffuellen spielen genau die gewuenschten Spieler; wo oben Bots
  dazukommen wuerden, warnt das Menue, dass jemand einen Vorteil hat.
- **Auffuell-Bots** (`sv_plan`): gehen die gewuenschten Spieler nicht auf, kommt ein Bot dazu, bis die Eckenmenge fair
  ist (3 auf dem Quadrat: 4 Plaetze; 5 auf dem Sechseck: 6). Der Auffuell-Bot ist ein ganz normaler Spieler auf der
  freien Ecke, kein Sonderfall im Spielkern. Im Menue laesst sich *Mit Bots auffuellen* ausschalten (Standard: an);
  dann spielen genau die gewuenschten Spieler auf den am weitesten gestreuten Ecken, und das Menue warnt, dass jemand
  einen Vorteil haben kann.
- **Bot-Stufe je Platz**: `sv_new_game_list` startet eine Partie aus `seed|ecken|platzcodes`; Codes 0 gierig, 1 zufall,
  2 Mensch, 3 stark, 4 flaeche. Die Namen der Stufen sind grob: `stark` und `flaeche` liegen in den Messungen oben nahe
  beieinander (20 : 20 beim Stapeln), `gierig` und `zufall` sind klar schwaecher.
- Die Sitzverteilung der Bots auf den neuen Brettern liegt bei den kleinen Stichproben im Rauschen; einen echten
  Fairness-Test gibt es nur fuer die Symmetrie der Ecken, nicht fuer die Rohstoffverteilung jeder Brettform.

**Sechste Aenderung der Version 3: Unterhalt, Koenigs-Upgrade mit Gold, Symbole.**

- **Unterhalt** (`terr_upkeep=1`, siehe Tabelle): jede Figur kostet je Runde ihre Stufe. Gemessen mit den Bots
  (2 Spieler): rund 590 Flaeche Unterhalt je Partie gegen etwa 7 000 bis 8 000 Flaeche Einkommen, also eine **milde Bremse**,
  und nur etwa 0,03 aufgeloeste Figuren je Partie mit den normalen Bots; die Bots bauen so, dass die Flaeche nie unter
  0 faellt. Der Sieganteil der Bots untereinander aendert sich innerhalb des Rauschens. Spuerbar wird er nur, wenn Figuren
  billig sind (zum Beispiel `terr_pawn=1,terr_up_mult=1`, `flaeche` gegen `flaeche`, 30 Partien: 0,37 aufgeloeste Figuren je Partie). Ein Mensch, der viele
  Helden kauft und die Flaeche kurz haelt, wird ihn merken; das ist die Absicht.
- **Koenigs-Upgrade mit Gold** (`terr_kingup=1`): Kat 2 hatte bisher fast nur den Boost und die Karten zu bezahlen; jetzt
  ist Gold der direkte Weg, den Koenig zu verbessern, ohne dass die Karte im Markt liegen muss. Der Preis ist die Zahl der
  zusaetzlichen Zugfelder (Quadrat 4 / 4 / 8).
- **Anzeige**: Symbole und Namen fuer Flaeche, Eisen, Gold, Brot und die Figuren, Uebersicht mit Zuwachs, Rand der
  Ressourcen-Kacheln und Fabrik-Menue (siehe oben). `terr_upkeep`, `terr_kingup` und `terr_kingup_price` stehen im
  Startmenue in der Gruppe *Version 3*. Neue Events im Viewer: `upkeep` ("Unterhalt von A: −4 Fläche") und `dissolved`
  (roter Rahmen um das Feld der aufgeloesten Figur). Die Start-JSON enthaelt `king_up: {on, extra: [a, b, c], price}`.
- **Abgeschnittenes Land ist wehrlos**: alles, was nicht ueber Nachbarfelder an der Fabrik haengt, ist wehrlos (die Figuren
  darauf faerben nichts und schlagen nicht); das war schon so (`connected_land`, `helpless`) und aendert sich nicht.

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
