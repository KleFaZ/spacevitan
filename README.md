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
cargo run --release -- --games 20 --bots sehr-stark,stark --rule territory=1,terr_v3=1   # die zwei staerksten gegeneinander
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
| `sehr-stark` | die Suche von `stark` (Alpha-Beta, Ruhesuche), aber mit der Land-Bewertung von `flaeche` an den Blaettern (`SearchBot::with_area_eval`, `area_bot::leaf_value`): verbundenes Land, Vorkommen, Figuren und duenne Verbindungen, die ein Gegner kappen koennte. Eigenes Budget (`--nodes-sehr`, Standard 20000), etwa 200 ms je Zug (hoechstens 0,5 s). Nur im Gebietsmodus anders als `stark`. |
| `super-stark` | wie `sehr-stark` (gleiche Suche, gleiches Budget), aber die Blattbewertung zahlt mehr fuer Land, solange noch freier Platz ist, und belohnt eine Fabrik, die Richtung Brettmitte gerueckt ist (kauft deshalb "Fabrik ruecken"); seit Version 3.6 schlaegt er sinnvolle Figuren zuerst, setzt gegnerischen Koenig und Fabrik unter Druck, nutzt Umschliess-Chancen und verschiebt die Fabrik am Anfang mit dem Koenig. Im Browser "Super schwer". |
| `stark-pur` | wie `stark`, kauft aber nie Karten (Vergleichsgegner fuer Kartenmessungen) |
| `gierig` | feste Faustregeln ohne Vorausschau |
| `flaeche` | nur Gebietsmodus: spielt auf Land und Rohstoffe statt auf Figurenkampf. Rechnet einen eigenen Zug voraus und bewertet das Ergebnis mit der echten Engine (verbundenes Land, Vorkommen darauf nach Reife gewichtet, Figuren, Bedrohung von Koenig und Fabrik), also sind Abtrennen und Umschliessen schon eingerechnet. Baut nur, wenn die neue Figur besser ist als seine beste; kauft Karten, die der naechsten Figur oder dem Koenig nutzen. Sichert die Flaeche (siehe "Siebte Aenderung der Version 3"): rechnet genau nach, ob ein Gegner im naechsten Zug den Koenig oder Land von der Fabrik trennen kann, und raeumt das vor dem Bauen aus. 0,6 ms je Zug (`area_bot.rs`) |
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

**Schnellstart:** Das Menue beginnt mit drei Zeilen: **Gegner** (Leicht = gierig, Mittel = flaeche, Schwer = stark, Sehr schwer
= sehr stark, Super schwer = super-stark), **Spieler** (2, 3 oder 4; bei 3 wird das Dreieck-Brett gewaehlt, damit alle gleich stehen, sonst das Quadrat) und
**Regeln** als Paket: *Standard* (Version 3 pur), *Neue Regelideen* (die vier Schalter der Version 3.1, siehe unten), *Schnelle
Partie* (Seite 8, Wertung nach Runde 40) und *Viele Karten* (12 im Markt). Ein Paket bleibt nur markiert, solange die
Einstellungen genau dazu passen; aendert man danach ein Feld, steht dort "Eigene Regeln". Darunter: *Partie starten*, *Zu zweit
an einem Geraet* (die ersten beiden Plaetze Mensch) und *Zuschauen (Bot gegen Bot)* (alle Plaetze Bot, in der gewaehlten Stufe).
Alles Weitere (Seed, Brettform, Plaetze einzeln, alle Regelfelder, Karten-Editor) steht unter **Erweiterte Einstellungen**.
Mit Freund per Link spielen gibt es noch nicht (dafuer braeuchte es einen Spiel-Server).

**Startmenue:** *Spielen* oeffnet zuerst ein Menue; eine Partie beginnt erst mit *Partie
starten*; *Neue Partie* oben neben *Spielen* oeffnet es jederzeit wieder. Dort stehen **Brettform** (Dreieck, Quadrat,
Fuenfeck bis Achteck), Spielerzahl, *Mit Bots auffuellen*, je Platz **Mensch oder Bot-Stufe** (super stark, sehr stark, stark, flaeche, gierig,
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

**Handy (Bildschirm bis 700 px):** Die Kopfzeile ist schlank: nur "Spacevitan" und gleich daneben **Neue Partie** (die Reiter
*Spielen* und *Partien ansehen* sind dort ausgeblendet). Im 2D-Modus fuellt das Brett die ganze Breite; darueber stehen die vier
Ressourcen in kleinen Kacheln (Flaeche, Eisen, Gold, Brot mit Zuwachs je Runde, ohne Erklaertext), darunter die **vier Spielkarten**.
Ein Sechseck wird in 2D so gedreht, dass zwei gegenueberliegende **Seiten** parallel zum Bildschirmrand liegen (nicht die Ecken).

- **Fabrik antippen** (eigene Fabrik, keine Figur darauf): ein Popup mit dem Baumenue ploppt **ueber der Fabrik** auf (bei Platzmangel
  darunter oder daneben, immer im sichtbaren Ausschnitt). Es zeigt nur die vier Figuren (bunt = baubar, sonst "in 3 Runden"), dazu
  Kat-3-Tausch, Fabrik ruecken und Umziehen, falls moeglich. Die geschlossene Statusleiste unter dem Brett entfaellt auf dem Handy
  (sie bleibt nur bei "Fabrik besetzt" und im Bau).
- **Grauer Figur-Marker:** Ist die Fabrik frei und etwas baubar, steht auf ihr eine halbtransparente graue Figur der **hoechsten**
  bezahlbaren Stufe.
- **Spielkarten:** Die Karten des Marktes liegen unter dem Brett als Karten mit Bild, Namen, Kurzeffekt und Preis (Lager/Preis).
  **Bunt mit goldenem Rand** = jetzt kaufbar, **grau** = nicht. Antippen oeffnet ein Fenster mit Erklaerung, Preis, dem, was
  fehlt, und dem Knopf *Kaufen* (bei Karten auf eine Figur: *Figur waehlen*, dann die Figur auf dem Brett antippen). Auf dem
  Desktop stehen dieselben Karten in der Seitenleiste.
- **Leuchtender Koenig:** Laesst sich der Koenig mit Gold verbessern (Regel `terr_kingup`), pulsiert er golden (in 2D und 3D).
  Antippen waehlt ihn wie gewohnt und zeigt eine Blase mit Stufe, Preis und dem Knopf *Verbessern*.

**Handy: Zoom, Minimap, Zwei-Schritt-Tippen (nur 2D, Bildschirm bis 700 px):** Unter dem Brett stehen **− / +** mit drei Stufen
(*Ganzes Brett*, *Mittel* ca. 52 px je Feld, *Nah* ca. 76 px). Ab *Mittel* liegt das Brett in einem Rahmen zum Verschieben
(normales Wischen), unten rechts zeigt eine **Minimap** das ganze Brett mit Landfarben, Figuren und dem sichtbaren Ausschnitt;
Antippen oder Ziehen auf der Minimap verschiebt den Ausschnitt. **Pinch** (zwei Finger) springt zwischen den Stufen;
Doppeltippen zum Zoomen gibt es bewusst nicht, weil es mit dem Zwei-Schritt-Tippen kollidieren wuerde. Waehlt man eine Figur, rollt
der Ausschnitt sie samt ihren Zielfeldern ins Bild. **Zwei-Schritt-Tippen** (nur mit Touch): erst die Figur, dann ein Zielfeld
antippen: das Feld wird auf dem Brett markiert (ohne Rueckfrage-Leiste); erst das zweite Tippen auf dasselbe Feld zieht; ein anderes
Zielfeld aendert die Wahl. Das schuetzt vor Fehltipps auf kleinen Feldern. Die Leinwand
ist auf etwa 14 Mio. Pixel begrenzt (sehr grosse Bretter zoomen dann etwas weicher).

**Karten-Editor (Startmenue, Version 3):** Unter *Karten* steht eine Tabelle aller Karten des Pools mit Haken (im Pool oder
nicht) und drei Preisfeldern (Eisen / Gold / Brot; leer = Standardpreis, grauer Wert = Standard). Darunter baut man eigene
Karten: Name, Bausteine mit **+/−** (jeder bis ×3, z. B. *Ecke ×2 + Sturmangriff*) und Preis (leer = Summe der Bausteine
als Richtwert); bis zu acht. Der Editor schickt `price.*`, `pool.*` und `custom.*` an den Spielkern (siehe die achte Aenderung der
Version 3) und merkt sich den Stand im Browser; *Karten zuruecksetzen* und *Alles zuruecksetzen* stellen den Standard wieder
her. Die Liste der Karten und Bausteine holt die Seite mit `sv_cards()` vom Spielkern, sie steht nicht noch einmal im Viewer.

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
| `score_after` | 250 (bis 19.11.2026: 100) | ohne Sieger entscheidet nach so vielen Runden die Wertung (siehe unten); 0 = aus, dann Remis nach `max_rounds` |
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

### Wertung nach 250 Runden

Hat nach `score_after` = 250 Runden (Standard seit dem 19.11.2026, vorher 100) niemand gewonnen, entscheidet die Wertung: Summe der
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
  neuen Figuren; das Lager ist weg. Ohne Fabrik faerbt man frei, ohne abzutrennen und ohne Einkommen
  (**ab Version 3.7 nur ohne Neubau-Regel**: mit `terr_move > 0`, also in Version 3, sind alle Figuren ohne stehende
  Fabrik wehrlos, siehe Version 3.7).
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

> **Seit 3.3** ist Brot (Kat 3) in Version 3 standardmaessig ein **Lager mit Erntezaehler** (`terr_bread=3`, siehe
> "Version 3.3"). Alles unten zu Kat 3 als Flaechenbonus `2^k` (`terr_bonus`) und zu den Eisen/Gold-Preisen gilt jetzt nur noch
> mit `terr_bread=0` (im Browser: Paket *Alter Brot-Bonus*).

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
| `terr_rebuild` | 3 | Bauzeit in Runden, wenn die Fabrik zerstoert wurde, solange `terr_rebuild_dist` aus ist (oder der alte Platz unbekannt ist); kostenlos |
| `terr_rebuild_dist` | 1 | **Version 3.2**: auch nach einer Zerstoerung zaehlt die Entfernung (halbe Strecke vom alten Platz der Fabrik bis zum Koenig, aufgerundet, mindestens 1 Runde, wie beim Umzug); `0` = fest `terr_rebuild` Runden |
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
baut sie auf **eigenem Land** an seinem Standort neu, kostenlos. Die Dauer ist seit Version 3.2 (`terr_rebuild_dist=1`) wie beim
Umzug die **halbe Entfernung zum alten Platz** der Fabrik, aufgerundet, mindestens 1 Runde (`Game::rebuild_rounds_lost`; der
Platz steht in `State::lost_factory` und im Bild als `lost_factory`). Mit `terr_rebuild_dist=0` gilt wieder fest `terr_rebuild`
= 3 Runden, egal wo. Achtung, Folge der Regel: steht der Koenig nahe am alten Platz, ist der Neubau fast gratis (1 Runde);
wer die Zerstoerung teurer will, setzt `terr_rebuild_dist=0` oder spaeter einen Mindestwert.

**Waehrend des Baus** (beide Faelle): kein Einkommen, die Erntezaehler stehen, nichts kaufen oder bauen, das Land ist
**nicht verbunden** (`connected_land` ist leer): alle Figuren sind **wehrlos** (sie faerben nichts, schlagen nicht,
ziehen aber), und der **Koenig darf sich nicht bewegen**. Wer auf den Bauplatz zieht, zerstoert die Fabrik. Das Land
wird dabei **nicht** an den Gegner umgefaerbt (Annahme: "als waere das Gebiet abgetrennt" gilt nur fuer Verteidigung,
Faerben und Ertrag); steht die Fabrik wieder, wird alles neu angebunden. Auch der Kat-3-Bonus fehlt in dieser Zeit.

Bots: `stark`, `gierig`, `flaeche` bauen eine zerstoerte Fabrik sofort neu. **Keiner zieht freiwillig um** (seit Version 3.7
ziehen die rechnenden Bots um, wenn die Fabrik sonst zerstoert wuerde und der Umzug billiger ist). Der
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

**Siebte Aenderung der Version 3: die Bots sichern ihre Flaeche, neuer Bot `sehr-stark`.**

Beobachtung beim Spielen: die Bots fuehren den Koenig weit hinaus und verbinden ihn nur ueber eine duenne Linie mit der
Fabrik; wird die Linie durchtrennt, ist der Koenig wehrlos und steht still. Die Messung dazu (`king_tether_probe` in
`safety_tests.rs`, 12 Seeds, beide Sitze = 24 Partien je Zeile, Standardbudget):

- Beim Messen fiel auf, dass `flaeche` nur auf Schlagen und Koenigsfang achtete, nicht auf das Kappen: sie verlor den Koenig
  gegen `stark` in 9 von 24 Partien, in 5 davon nach einer Abtrennung. `stark` selbst verlor ihn dabei nie.
- **Neu in der Engine** (`territory.rs`): `Game::weak_points` (alle Engstellen des verbundenen Landes, ein Durchlauf nach
  Tarjan, mit Zahl der Felder, Figurenstufen und Koenig dahinter), `Game::cut_threat` (rechnet fuer jeden Zug einer
  gegnerischen Figur, die faerben darf, genau nach, was danach nicht mehr an der Fabrik haengt; **auch lange Striche ueber
  zwei Reihen**, nicht nur ein einzelnes Feld) und `Game::weak_exposure` (anteilige Gefahr einer Engstelle je nach
  Abstand gegnerischer Figuren, fuer Kappungen in zwei Zuegen).
- **`flaeche`** zieht das in `risk` ab (Koenig 500, Land doppelt, Figuren je Stufe) und loest eine akute Kappgefahr vor dem
  Bauen und Kartenkaufen mit einem Zug auf, wenn der sie deutlich entschaerft. Ergebnis gegen `stark` (24 Partien):
  Koenig gefangen **9 auf 3**, Abtrennungen je Partie **1,38 auf 0,25**, Siege von `flaeche` 14 auf 18. Damit ist `flaeche`
  etwas staerker als `stark` (die Beschriftung im Menue sagt jetzt "schwer"). Die Tabelle weiter oben (`stark` 92 % gegen
  `flaeche`) wurde mit `--nodes 4000` und vor mehreren spaeteren Aenderungen der Version 3 gemessen; beim Standardbudget
  und heutigen Regeln gewann schon die alte `flaeche` 14 von 24 Partien. `stark` selbst spielt unveraendert (gleiche
  Statistik wie vor der Aenderung, `stark` gegen `gierig` und `stark` gegen `stark` mit denselben Seeds).
- **`sehr-stark`** (neu): erste Fassung war `stark` mit zusaetzlichen Gewichten fuer duenne Linien; sie schlug `stark` nur
  knapp (14:10) und verlor gegen die neue `flaeche` (10:14), denn `flaeche` bewertet Land einfach besser. Deshalb nutzt
  `sehr-stark` jetzt die Suche von `stark` mit der Blattbewertung von `flaeche`. Eine Variante, die die anteilige Gefahr auch
  dann abzieht, wenn man selbst am Zug ist, war schwaecher (12:12 gegen beide) und wurde verworfen.
  Messung (zwei Seed-Bereiche, je 24 Partien, `sehr-stark` zuerst): gegen `stark` 16:8 und 16:8 (zusammen **32:16**),
  gegen die neue `flaeche` 18:6 und 16:8 (zusammen **34:14**).
- **Offen / ehrlich**: `sehr-stark` ist am staerksten, aber nicht der vorsichtigste Bot: sein Koenig steht in 36 bis 44 %
  der Zugenden hinter einer Engstelle (`flaeche`: 17 bis 26 %, `stark`: 25 bis 35 %), er wurde in 2 bis 6 von 24 Partien gefangen
  (meist nach einer Abtrennung), `flaeche` gegen ihn in 7 bis 9. Wer einen Bot will, der den Koenig am wenigsten riskiert,
  nimmt `flaeche`. Mit 24 Partien je Zeile liegt jede Zahl um etwa zehn Prozentpunkte im Rauschen; die Reihenfolge
  `sehr-stark` > `flaeche` > `stark` ist aus zwei Seed-Bereichen gleich herausgekommen, mehr kann man daraus nicht lesen.
- Rechenzeit: `flaeche` 0,6 ms (vorher 0,2), `sehr-stark` etwa 200 ms je Zug (hoechstens 0,5 s), `stark` etwa 110 ms. Im
  Browser rechnet das Wasm-Modul etwa zwei- bis dreimal so lang.
- Nachmessen: `PROBE_PAIRS=sehr:stark,sehr:flaeche PROBE_FROM=200 PROBE_N=12 cargo test --release -p game_core king_tether_probe -- --ignored --nocapture`
  (Bots `stark`, `sehr`, sonst `flaeche`; `PROBE_NODES`, `PROBE_NODES_VERY` stellen das Budget).

**Achte Aenderung der Version 3: neuer Kartensatz, Zufallsmarkt, Karten-Editor.**

Beobachtung beim Spielen: die Karten machen das Spiel taktisch, aber es gab nur drei Figurenkarten (Kuerzer, Ecke, Sprung),
und der Markt lag fast fest. Jetzt zieht jede Partie ihre Karten zufaellig aus einem groesseren Pool, die Preise lassen sich
aendern, und man kann eigene Karten bauen.

- **Ecke nur am Wegende** (`terr_bend_tail=1`, in Version 3 an): eine Figur der Stufe n zieht n Felder geradeaus; mit der
  Karte *Ecke* darf nur das **letzte** Feld abbiegen (also "n−1 geradeaus, dann links oder rechts"), mit zwei Ecken auch das
  vorletzte und so weiter (Stufe 3 mit zwei Ecken: "1 geradeaus, dann 2 Felder in einer neuen Richtung" oder mit zwei
  getrennten Knicken am Ende). Die Karte kostet jetzt **2 Eisen + 1 Gold**. Mit `terr_bend_tail=0` gilt wieder die alte Regel
  (eine Ecke irgendwo auf dem Weg). Der Rueckschritt ist nie erlaubt; blockiert eine Figur den Weg, faellt dieser Zug weg.
- **Neuer Kartensatz** (`terr_cardset3=1`, in Version 3 an; braucht `terr_cards=1`): feste Preise in Eisen / Gold / Brot
  statt Bruchteilen des Grundpreises. Der Pool (ohne den Koenigs-Boost, der immer ausliegt):

  | Karte | Wirkung | Preis (Eisen / Gold / Brot), ab 3.3 | bis 3.2 / mit `terr_bread=0` |
  |---|---|---|---|
  | Kuerzer | Figur darf einen Schritt weniger ziehen | 0 / 0 / 3 | 3 / 0 / 0 |
  | Ecke | Figur darf am Wegende abbiegen | 0 / 1 / 2 | 2 / 1 / 0 |
  | Sprung | Figur darf ueber eine Figur springen | 1 / 0 / 3 | 4 / 0 / 0 |
  | Durchzug | Figur darf ueber **eigene** Figuren hinwegziehen (Gegner versperren weiter) | 1 / 0 / 1 | 2 / 0 / 0 |
  | Siedler | Figur faerbt sofort alle freien Nachbarfelder (auch Gegnerland, nie Felder mit Figur oder gegnerische Fabrik; nur auf verbundenem Land) | 1 / 2 / 2 | 3 / 2 / 0 |
  | Fabrik ruecken | Fabrik rueckt ein Feld diagonal (nur Quadrat) | 3 / 0 / 0 | 3 / 0 / 0 |
  | Sturmangriff | sofort: in diesem Zug alle Figuren Reichweite +1 | 3 / 1 / 0 | 2 / 2 / 0 |
  | Bergbau | sofort und dauerhaft: +1 Gold je Ernte | 2 / 3 / 0 | 2 / 3 / 0 |
  | Schmiede | sofort und dauerhaft: +1 Eisen je Ernte | 4 / 1 / 0 | 3 / 2 / 0 |
  | Doppelzug | sofort: eine zweite Aktion in diesem Zug | 1 / 2 / 4 | 4 / 3 / 0 |
  | Diagonalzug (3.2) | **nur auf einen Bauern (Stufe 1)**: er darf zusaetzlich ein Feld diagonal ziehen und schlagen (nur Quadrat) | 3 / 1 / 0 | 3 / 1 / 0 |
  | Koenigs-Boost | immer im Markt, Preis mal die Stufe, die der Koenig bekaeme | 0 / 3 / 0 | 0 / 3 / 0 |

  (Die Tabelle ist die von 3.3; die erste Preisspalte steht in `Card::v3_price_bread`, die zweite in `Card::v3_price`.)

  *Sofort*-Karten brauchen keine Figur und keinen Platz. Die uebrigen wirken auf eine gewaehlte eigene Figur (ab Stufe 2,
  nie der Koenig) und lassen sich stapeln wie bisher (`terr_stack`). Karten, die auf einem Brett nichts bewirken (Fabrik
  ruecken auf Sechseck und Dreieck), werden dort nicht gezogen. Siedler und Durchzug sind neu in der Engine
  (`Perks.pass`, `Game::settle_cells`), Schmiede ist ein Spieler-Perk (`PlayerPerks.smelter`).
- **Zufallsmarkt**: `Game::new` zieht `market_size` Karten aus dem Pool (`cards::territory_pool`, Seed bestimmt die Auswahl),
  so dass sich Spiele je Partie anders anfuehlen. Test: 60 Seeds ergeben mehr als 20 verschiedene Maerkte, und jede Karte des
  Pools taucht auf.
- **Preise, Pool und eigene Karten als Regeln** (`--rule`, im Browser der *Karten-Editor* im Startmenue):
  - `price.<karte>=eisen/gold/brot` setzt den Preis (leer = Standard), z. B. `price.bend=1/0/0`;
  - `pool.<karte>=0` nimmt eine Karte aus dem Pool, `=1` legt sie zurueck;
  - `custom.0` bis `custom.7` = `Name|baustein*n+baustein|eisen/gold/brot` baut eine eigene Karte, z. B.
    `custom.0=Sturmecke|bend*2+charge|2/1/0`. Bausteine: `shorten`, `bend`, `hop`, `pass`, `settle`, `charge`, `mining`,
    `smelter` (je bis ×3; Namen bis 24 Zeichen ohne `=`, `|`, `,`). Die Wirkungen der Bausteine gelten zusammen auf die
    gewaehlte Figur bzw. sofort; ohne Preis rechnet der Editor die Summe der Bausteine als Richtwert aus. Eigene Karten
    liegen im Pool wie alle anderen; falsche Regeln melden einen Fehler mit der Stelle.
  Kartenkatalog fuer eigene Oberflaechen: `replay::catalog_json()` bzw. `sv_cards()` im Wasm (Karten mit Name, Text, Ziel,
  Standardpreis, ob nur Quadrat/Sofort, dazu die Bausteine).
- **Anzeige**: Abzeichen fuer *Durchzug* (Pfeil durch einen Strich), Ernte-Vorschau mit Schmiede/Bergbau, Hinweise fuer
  Sturmangriff und Doppelzug in der Zugleiste, die Fabrik-Karte nennt die Kosten je Karte.
- **Offen / ehrlich**: die Preise sind Schaetzungen, keine Messung; ob eine Karte zu stark oder zu schwach ist, zeigt erst
  eine Messung wie bei den alten Karten (`--gift`, siehe unten), die fuer den neuen Satz noch aussteht. Die Bots kaufen die
  neuen Karten nicht gezielt: `flaeche` nutzt nur Ecke, Sprung, Kuerzer und den Koenigs-Boost; `stark` und `sehr-stark`
  bewerten jeden Kauf mit der Vorausrechnung, planen aber nicht gezielt mit Siedler oder Durchzug. Eigene Karten sind
  ungeprueft im Gleichgewicht: ein Preis unter dem Richtwert macht sie schnell zur besten Karte.

### Version 3.10 (Wuensche vom 20.11.2026, dritter Teil)

- **Paket "Unterhalt nach Anzahl" jetzt mit k=1** (eine Figur 1, zwei 4, drei 9, vier 16, fuenf 25). Messung siehe 3.9: k=1
  beseitigt die Mauern (hoechstens 6 statt 16 gleichzeitige Figuren), die Ernte bleibt bei etwa der Haelfte. k=2 bleibt ueber das
  Feld "Unterhalt nach Anzahl (k)" einstellbar.
- **Hungersnot: `terr_famine=N`, `terr_famine_k=K`** (Standard aus, K = 2). Jede N-te Runde (N, 2N, 3N ...) rechnet der Unterhalt nach
  Anzahl mit K statt `terr_upkeep_cnt`, also zum Beispiel k=1 gegen k=2 (viermal so viel). Gilt fuer alle Spieler gleich und
  ist vorhersehbar. `Game::famine_round()` und `Game::upkeep_k()` im Spielkern (die Runde zaehlt `State::round`),
  `Config::validate` verlangt den Unterhalt nach Anzahl und K von 1 bis 20. Bots rechnen die Hungersnot in ihrer Suche mit, weil
  sie `territory_upkeep` benutzen. Im Browser: eine orange Leiste ueber dem Zug sagt die naechste Hungersnot an ("in 3 Runden,
  dann kostet eine Figur 4 statt 1, zwei 16 statt 4 ..."), in der Runde selbst ist sie kraeftiger ("Hungersnot in Runde 10!"); die
  Anzeige der Figurenkosten und die Auflose-Auswahl rechnen mit dem k der Runde. Paket "Mit Hungersnot" (k=1, jede 10. Runde) und
  zwei Felder in den erweiterten Einstellungen. FAMINE_MESSUNG
- **Kampagne: Aenderungen an Level 1 und 2.** Level 1 ist jetzt 7x7 (ungerade: die Mitte erreichen beide gleich schnell, Wertung nach
  Runde 40, dritter Stern bis Runde 30; Messung: starker Bot gegen leichten 30 von 30, 25 davon per Koenigsfang, im Schnitt Runde 31).
  Beide Level schalten das Koenigs-Upgrade ab (`terr_kingup=0`, es gibt ja kein Gold); in Level 1 sind zudem Karten und
  Gold-Kacheln ausgeblendet.
- **Kampagne: Belohnung.** Wer ein Level gewinnt, nimmt einen Flaechenboost ins naechste mit: Startflaeche fuer Spieler A,
  je nach den (besten) Sternen des vorigen Levels 0 / 8 / 15 / 30 (`CAMP_REWARD` im Viewer, ein Bauer kostet 15). Spielkern:
  `camp_bonus_area=N` (Spieler A beginnt mit N Flaeche, die anderen mit 0; `Config::validate`: hoechstens 1000). Die Geschichte des
  naechsten Levels und das Ergebnisfenster nennen den Boost.

### Version 3.9 (Wuensche vom 20.11.2026, zweiter Teil)

- **Startmenue: Form des Bretts.** Neue Zeile "Form des Bretts" (Automatisch, Quadrat, Dreieck, Fuenfeck, Sechseck,
  Siebeneck, Achteck). "Automatisch" verhaelt sich wie bisher (die Form folgt der Spielerzahl); wer eine Form waehlt,
  behaelt sie, und die Spielerzahl-Knoepfe darueber werden gesperrt (hoechstens so viele Spieler wie Ecken). Passen die
  Spieler nicht auf die Ecken (zum Beispiel 3 auf dem Quadrat), fuellt der Spielkern wie bisher mit Bots auf. Die Wahl
  wird mit dem Menue gespeichert (`shapeAuto`); aeltere Staende gelten als "Automatisch".
- **Ungerade Feldgroessen.** Die Zeile "Feldgroesse" bietet jetzt alle Groessen von 6 bis 16 (gesperrt, was die Form nicht
  hergibt: Dreieck ab 6, Vielecke ab 8). Test `odd_field_sizes_start_and_play_on_every_board_shape`: Seiten 7, 9, 11, 13, 15
  starten und laufen auf jeder Form, mit Gelaende, 2 und voller Spielerzahl.
- **Ist die Mitte fuer alle gleich weit?** `Board::centre_report_for` (Wasm: `sv_centre`) nimmt als Mittelfelder die Felder,
  deren weiteste Ecke am naechsten liegt, und vergleicht, wie weit jede genutzte Ecke zum naechsten Mittelfeld hat. Das
  Menue schreibt das Ergebnis unter die Auswahl. Gemessen (Seitenlaengen 6 bis 30, siehe Tests im `board.rs`):
  - Quadrat: immer gleich; ungerade Seite = **ein** Mittelfeld fuer alle, gerade Seite = vier Mittelfelder, jede Ecke hat
    eines gleich nah. Gleich weit ist die Mitte also auch bei gerader Seite, nur gemeinsam ist sie erst bei ungerader.
  - Dreieck: immer gleich (ein Mittelfeld, bei Seiten durch 3 drei).
  - Sechseck: immer gleich, ein Mittelfeld.
  - Fuenfeck: gleich nur bei Seite 6 bis 9 (und einzelnen grossen Seiten); bei 10 bis 17 nicht.
  - Siebeneck: im Menuebereich (8 bis 16) nur bei 11, 13 und 16 gleich.
  - Achteck: nie. Bei zwei gegenueberliegenden Spielern gilt die Pruefung nur fuer deren Ecken, dort ist sie gleich.
- **Karten kaufen: `terr_multibuy=1`** (Standard aus, im Menue "Karten kaufen": "Eine je Zug" oder "So viele du bezahlen
  kannst"). Statt einer Karte je Zug darf man so viele kaufen, wie das Lager bezahlen kann und die Karte noch passt
  (die Karte neben der Fabrik hat weiter nur einen Platz, eine Figur nimmt nur auf, was zu ihr passt). Jede Figur, die
  eine Karte bekommt, ruht in diesem Zug (`terr_cardlock=1`), auch wenn sie nicht die letzte war: dafuer gibt es
  `State::card_locks` (die vorher verbesserten Figuren; `card_lock` bleibt die zuletzt verbesserte). Zusammen mit
  `terr_cardlock=2` (eine Verbesserung ist der ganze Zug) geht der Schalter nicht, `Config::validate` meldet das. Bots
  wissen nichts von dem Schalter und kaufen, was ihre Bewertung ergibt (20 Partien stark gegen stark, Seite 9,
  `market_size=12`: kein Absturz, 1,9 Karten je Partie).
- **Unterhalt nach Anzahl: `terr_upkeep_cnt=k`** (Standard 0 = aus; im Menue Regelpaket "Unterhalt nach Anzahl" mit k=2).
  Alle Figuren zusammen (ohne Koenig, jede zaehlt gleich, die Stufe ist egal) kosten je Runde `(k*n)^2` Flaeche, bei
  k=2: eine Figur 4, zwei 16, drei 36, vier 64, fuenf 100. Das ersetzt den Unterhalt nach Stufe (geht also nicht mit
  `terr_upkeep_sq`, braucht `terr_upkeep`, k hoechstens 20). Reicht die Flaeche nicht, faellt zuerst die
  schwaechste Figur weg (bei gleicher Stufe die zuletzt gebaute); der Unterhalt wird nach jeder Figur neu gerechnet.
  Im Menue "Unterhalt nach Anzahl (k)" bei den erweiterten Regeln. Die Kommandozeile zeigt jetzt
  "Figuren gleichzeitig (ohne Koenig, je Spieler zu Rundenbeginn)" als Mittel und Hoechstwert.
- **Messung: verschwinden die Mauern?** 16 Partien stark gegen stark, Seite 10, `territory=1,terr_v3=1,score_after=120`, Seeds 11 bis 26,
  ohne Regel gegen `terr_upkeep_cnt=2` und `terr_upkeep_cnt=1` (Zahlen aus der Kommandozeile; 16 Partien sind wenig, Siegquoten
  von 50 bis 56 % sagen nichts):

  | | ohne | k=1 | k=2 |
  |---|---|---|---|
  | Figuren gleichzeitig je Spieler (Mittel / Hoechstwert) | 3,9 / 16 | 1,9 / 6 | 1,2 / 3 |
  | Figuren gebaut je Partie | 15,1 | 8,2 | 5,5 |
  | geschlagen je Partie | 4,9 | 3,8 | 2,6 |
  | aufgeloest je Partie | 0 | 0 | 0,25 |
  | Ernte je Partie (Kat 1 / 2 / 3) | 48 / 19 / 23 | 26 / 9 / 12 | 13 / 6 / 6 |
  | Erste Figur Stufe 2 (Runde, in wie vielen Partien) | 96,8 (10 von 16) | 77,2 (10 von 16) | 62,9 (7 von 16) |
  | Runden im Schnitt | 98,9 | 81,9 | 84,8 |
  | Siege erst nach Wertung | 8 von 16 | 3 von 16 | 4 von 16 |
  | Landanteil des Fuehrenden am Ende | 83 % | 93 % | 97 % |

  Ja, die Mauern verschwinden (Hoechstwert 16 gleichzeitige Figuren gegen 6 bei k=1 und 3 bei k=2), aber der Preis ist hoch:
  die Ernte sinkt bei k=1 auf etwa die Haelfte, bei k=2 auf etwa ein Viertel, weil kaum noch Figuren auf Vorkommen stehen,
  und die Partien enden haeufiger mit einem gefangenen Koenig (13 oder 12 von 16 gegen 8 von 16). Der erste Bauer kostet bei k=2
  schon 4 Flaeche je Runde, mehr als der Unterhalt nach Stufe verlangt. Die Bots rechnen den Unterhalt nur in ihrer Suche
  mit und planen nicht darauf; ein Mensch kann sich besser darauf einstellen. k=1 ist der sanftere Ausgangspunkt: noch
  wenige Figuren je Spieler, aber die Wirtschaft laeuft weiter.
- **Welche Figur sich aufloest, waehlt der Mensch (`terr_upkeep_pick`, Bitmaske der Spieler).** Reicht die Flaeche nicht fuer
  den Unterhalt, wartet das Spiel (`State::dissolve_due`, `Event::DissolveDue`): erlaubt ist nur noch `Action::Dissolve`
  mit einer eigenen Figur (nie der Koenig); danach wird neu gerechnet, reicht es immer noch nicht, kommt die naechste
  Wahl, sonst wird der Unterhalt gezahlt und der Zug ist zu Ende. Die Sitzung (`Session::new`) setzt die Bits der
  Menschen-Plaetze selbst, Bots loesen weiter automatisch auf und rechnen ihre Zuege so, als taeten das auch die
  Menschen (`SearchBot`/`AreaBot` setzen die Maske in ihrer Kopie auf 0). Im Browser: Fenster "Zu wenig Flaeche fuer den
  Unterhalt" mit der Liste der Figuren (Stufe, Feld, wie viel Unterhalt sie spart; der Zeiger auf einem Eintrag
  markiert die Figur auf dem Brett rot), Klick = "Wirklich diese Figur aufloesen?" mit "Zurueck" / "Ja, aufloesen",
  danach ist sie weg. Gilt auch fuer den Unterhalt nach Stufe.
- **Kampagne (Phase 1: Level 1 und 2).** Knopf "Kampagne" im Startmenue oeffnet die Levelliste (Sterne je Level, Level 2 erst
  nach einem Stern in Level 1), dann die Geschichte mit Ziel und Sternen, "Los geht's" startet. Der Spielkern bekommt die
  Regeln des Levels (`CAMPAIGN` im Viewer: Regeltext, Platzarten, Brettform, fester Seed, `res` = welche Rohstoffe es gibt,
  `cards` = ob es Karten gibt); das Menue und seine gemerkten Einstellungen bleiben unberuehrt. Sterne: 1 gewonnen, 2 Koenig
  gefangen (Bonus), 3 Koenig gefangen bis Runde `fast` (Level 1: 20, Level 2: 30). Der beste Stand je Level steht im
  `localStorage` (`spacevitan.campaign.v1`, ohne Speicher gilt er nur fuer die Sitzung). Im Spiel zeigt eine gelbe Leiste Level
  und Ziel; am Ende kommt das Ergebnisfenster mit Sternen und "Nochmal" / "Naechstes Level" / "Kampagne".
  - Level 1 "Der Ueberfall": 6x6, nur Flaeche (`terr_dep=0`, keine Karten, `terr_pawn=15`), mehr Flaeche fuer den Spieler
    (`terr_favor=1`), Gegner leicht, Wertung nach Runde 30. Messung: ein starker Bot als Spieler gewinnt 29 von 30 Partien
    gegen den leichten Bot, im Schnitt in Runde 23; Seed 7 (der feste Seed des Levels) in Runde 20 durch Koenigsfang.
  - Level 2 "Das erste Eisen": 8x8, nur Eisen (`terr_dep=1`), im Markt nur die Karte "Kuerzer" (3 Eisen), Gegner mittel,
    Wertung nach Runde 40. Messung: starker Bot gegen mittleren Bot gewinnt 16 von 20 (80 %).
  - Spielkern-Schalter dafuer (seit dc8d07f): `terr_favor=1` gibt Spieler A die besseren Flaechen (mehr Feld, weniger Moor in
    seiner Ecke; Bedingung `terr_terrain`), `terr_dep=N` laesst nur die gewaehlten Rohstoffkategorien als Vorkommen stehen
    (Bit 0 Eisen, 1 Gold, 2 Brot, 3 Kat 4; 15 = alle); die Karte wird erst gewuerfelt, auf Fairness geprueft und dann
    gefiltert, bleibt also fuer alle gleich.
  - Noch nicht gebaut: Brot und Karten (Level 3), Gold (letztes Level des zweiten Kapitels, zwei Karten zum Mitnehmen),
    Dreieckskarte mit verbuendetem Bot, Karte "Figur aufwerten", globaler Haushalt, Geschichte zwischen den Kapiteln.

### Version 3.8 (Wuensche vom 20.11.2026)

- **Startmenue:** Spielerzahl (2 bis 8) und Feldgroesse (6, 8, 10, 12, 14, 16 Felder je Kante) stehen direkt im
  Schnellstart, die erweiterten Einstellungen muessen nicht mehr geoeffnet werden. Die Brettform folgt der
  Spielerzahl (3 Dreieck, 2 und 4 Quadrat, ab 5 Vieleck mit so vielen Ecken). Gesperrt ist, was die Form nicht
  hergibt (ab 5 Spielern mindestens 8). Die Feldgroesse bleibt beim Wechsel des Regelpakets erhalten und gehoert zu
  keinem Paket; "Schnelle Partie" setzt nur noch die Wertung nach Runde 40.
- **Uebersicht:** Die Klammer hinter Eisen, Gold und Brot ist jetzt der Durchschnitt je Runde mit zwei Nachkommastellen
  (Ertrag der besetzten Vorkommen geteilt durch ihre Erntezeit, zum Beispiel ein Eisenfeld mit Erntezeit 3 = +0,33);
  Flaeche zeigt Land plus Brot-Bonus minus Unterhalt ebenfalls mit zwei Stellen. Besetzt man ein Vorkommen, sieht man
  den ungefaehren Gewinn sofort. Der Tooltip nennt zusaetzlich, was am Ende des naechsten Zugs konkret dazukommt.
  Das "in N Runden" bei Figuren, Koenig-Upgrade und Karten rechnet die Ernte jetzt Runde fuer Runde durch (vorher stand
  dort "nie", wenn gerade nichts reif war, obwohl bald etwas kam).
- **PC-Layout:** In der rechten Spalte stehen unter den Ressourcen jetzt die Karten, klein und vier je Reihe (halb so
  breit wie vorher); "Figuren bauen", Koenig-Upgrade und Kartenplatz rutschen darunter. Auf dem Handy bleibt alles wie
  zuvor.
- **Tempo bei vielen Spielern:** "super stark" brauchte bei 6 Spielern nativ im Schnitt etwa 1,15 s je Zug (im
  Browser 1 bis 3 s). Zwei Aenderungen, die Zuege bleiben bei gleichem Budget dieselben:
  1. `Board::distances_from` liefert die Entfernungszeile jetzt aus einem Speicher am Brett (das Brett aendert sich nie).
     Die Breitensuche machte im Profil 66 % aller Rechenschritte aus (Bewertung der Blaetter, `press_value`,
     `exposure`); danach 13,6 statt 48,8 Milliarden Schritte im selben Lauf.
  2. `ai::scaled_budget`: Bis 110 Felder (Quadrat mit Seitenlaenge 10, also 2 bis 4 Spieler) gilt das volle
     Knotenbudget, darueber weniger (144 Felder 55 %, 194 = 35 %, 331 = 15 % (6 Spieler), 561 = 8 % (8 Spieler)).
     Stufen statt Gleitkomma, damit Browser und Kommandozeile gleich rechnen. `--nodes-sehr` ist das Budget fuer das
     kleine Brett.
  Messung (6 Spieler, 40 Runden, nativ): vorher rund 1,15 s je Zug (Runde 1 bis 11), nachher 0,28 s im Schnitt und
  hoechstens 0,95 s; im Browser etwa das Doppelte. Bei 2 bis 4 Spielern aendert sich nichts.

### Version 3.7 (Wuensche vom 19.11.2026, dritter Teil): ohne Fabrik wehrlos, Rangfolge der Bots

- **Ohne stehende Fabrik sind alle Figuren wehrlos** (Fehler laut Meldung mit Bild: "meine Fabrik wurde zerstoert, aber meine Figuren koennen alle
  noch schlagen"). Ursache: `Game::piece_connected` galt ohne Fabrik als "immer verbunden" (der alte Satz "wer keine Fabrik mehr hat, braucht keinen
  Zusammenhang mehr"). Jetzt gilt mit der Neubau-Regel (`terr_move > 0`, Version 3) in beiden Faellen, **zerstoert** und **im Neubau**:
  - keine Figur (auch der Koenig nicht) schlaegt etwas, betritt eine gegnerische Fabrik, faerbt etwas (auch der Koenig auf dem Weg nicht),
    benutzt Siedler oder greift als Kamikaze an; sie koennen ziehen. Der Gegner dagegen schlaegt sie ganz normal (wehrlos = ungeschuetzt).
  - Der Koenig muss fuer den Neubau **auf eigenem Land stehen** (`RebuildFactory`, kostenlos wie bisher). Steht er daneben, geht er erst dorthin
    (unterwegs faerbt er nichts). **Ausnahme:** hat der Spieler gar kein eigenes Land mehr, darf er dort neu bauen, wo er steht (`rebuild_plan`;
    das Feld wird dabei sein Land). Das habe ich ergaenzt, weil er sonst fuer immer gesperrt waere (wehrlos kann er kein Land mehr holen).
  - Sein Land bleibt, wie es ist (nichts wird abgetrennt, solange er keine Fabrik hat: `cut_off` ueberspringt ihn wie bisher). Steht die
    Fabrik wieder, binden sich Figuren neben dem verbundenen Land sofort wieder an (`territory_settle`).
  - **Ohne Neubau-Regel (Version 2, `terr_move = 0`) bleibt alles wie vorher**: dort kommt die Fabrik nie wieder, wer sie verloren hat, wuerde
    sonst fuer immer wehrlos sein. Das ist eine Annahme von mir; sag Bescheid, wenn es auch dort gelten soll.
  Tests (`territory_tests`): `a_piece_of_a_player_without_a_factory_is_defenceless`, `without_a_factory_no_piece_can_capture_or_enter_an_enemy_factory`
  (Schlag, Fabrik betreten, Koenigsfang; der Gegner schlaegt sie), `a_piece_can_capture_again_once_the_rebuilt_factory_stands` (Neubau, dann wieder
  schlagbereit), `a_defenceless_king_walks_back_to_his_land_before_he_can_rebuild`, `a_king_without_any_land_may_rebuild_where_he_stands`,
  `without_the_rebuild_rule_a_player_without_a_factory_plays_on_freely` (Version 2). Neun alte Tests liessen eine Figur "ohne Fabrik" als Abkuerzung
  ziehen; sie setzen jetzt die Fabrik des Ziehenden dorthin (`stand_on`).
- **Rangfolge der rechnenden Bots** (`stark`, `sehr-stark`, `super-stark`, `flaeche`; `zufall` bleibt absichtlich zufaellig und `gierig` absichtlich
  gierig): **1. Koenig schuetzen, 2. Fabrik (oder durchrechnen, was ein Umzug mit ein paar wehrlosen Runden kostet), 3. eigene Macht ausbauen,
  4. gegnerischen Koenig herausnehmen.** So ist es umgesetzt (alles in `area_bot.rs` und `ai.rs`):
  1. **Koenig.** Wie bisher ueber die Suche (Koenigsfang ist Gewinn bzw. Niederlage, `WIN` = 1 Million; die Bewertung kappt alles andere darunter) und
     in `risk` (`KING_LOST` = 1 Million fuer einen bedrohten Koenig). Die Abkuerzungen vor der Suche (Eroeffnung, Fabrik rutschen) laufen
     nur in ruhiger Lage (`calm`), ein Kartenkauf nur, wenn die Vorausrechnung ihn lohnt. Zuschlaege der Jagd (`hunt`, hoechstens 1000) und Druck (`press`) liegen weit darunter. Ein **sofort moeglicher
     Koenigsfang** bleibt natuerlich der beste Zug (er beendet die Partie), auch wenn die 4. Stufe sonst zuletzt kommt: die 4. Stufe meint den Plan, den
     Koenig zu jagen, nicht, einen Gewinn liegen zu lassen.
  2. **Fabrik.** Die neue Bewertung kennt den Ausfall: `outage_rounds` (im Neubau die Restzeit; nach Zerstoerung die Schritte des Koenigs bis zum naechsten
     eigenen Feld plus Bauzeit) mal `idle_per_round` (ein Achtel des Landwerts + ein Viertel der Figurenwerte + 60) plus bei zerstoerter Fabrik 800 fuer
     Lager und Flaeche. Dasselbe gilt fuer den Gegner (seine zerstoerte Fabrik ist ein Gewinn). `risk` zaehlt eine bedrohte Fabrik jetzt mit
     `factory_loss_cost` (800 + die Runden, die er wehrlos waere, mal Wert je Runde) statt fest 800. **Umzug als Ausweg:** kann ein Gegner die Fabrik im
     naechsten Zug zerstoeren und kann der Koenig umziehen (`RebuildFactory` mit stehender Fabrik), ist der Umzug ein Kandidat der Zugsuche (alle Suchbots)
     bzw. von `best_by_value` (`flaeche`); die Bewertung rechnet nach, ob die wehrlosen Runden billiger sind als der Verlust. Vorher haben die Bots nie
     freiwillig umgesetzt. Beim `flaeche`-Bot geht ein Zug, der die Fabrik rettet, vor Bauen und Kartenkauf. Der `stark`-Bot bewertet im Gebietsmodus ohne
     Fabrik ebenfalls Figuren als wehrlos (`helpless`) und zieht fuer jede Runde bis zur neuen Fabrik Wert ab.
  3. **Eigene Macht** ist unveraendert der Rest der Bewertung (Land 10 je Feld, Vorkommen, Figurenwerte).
  4. **Koenigsjagd** ist `press`, `offence` und die klassischen `pressure`/`trap`-Terme von `stark`; sie sind klein gegen Land und Figurenwerte.
  Szenario-Tests (`area_bot::drive_tests`): `the_king_comes_first_for_every_thinking_bot` (ein Koeder der Stufe 4 zum freien Schlagen, aber der Koenig
  ist bedroht: alle vier Bots retten ihn), `the_factory_comes_second_every_thinking_bot_saves_it_when_it_can`, `a_doomed_factory_is_moved_when_that_is_cheaper_than_losing_it`
  (alle Bots ziehen um, wenn jeder andere Zug die Fabrik verliert), `without_a_factory_the_king_walks_back_to_his_land_and_rebuilds`.
  Stellschraube `guard` in `Drive` (1 = an, 0 = altes Verhalten, nur zum Messen mit `SPACEVITAN_DRIVE="guard=0"`).
  **Messung** (Version 3, `--nodes-sehr 5000`, beide Sitzordnungen; Skript `duelv.sh`; neue Rangfolge `guard=1` gegen altes Verhalten `guard=0`,
  beide `super-stark`, in der neuen Engine mit wehrlosen Figuren ohne Fabrik):
  | Lauf | `guard=1` : `guard=0` |
  |---|---|
  | 100 Runden, Seeds 901.., 2 x 24 Partien | 23:25 |
  | 250 Runden, Seeds 951.., 2 x 14 Partien | 16:12 |
  | zusammen | **39:37** |
  | Stufen: `super-stark` gegen `sehr-stark`, 250 Runden, Seeds 1201.., 2 x 20 Partien | **29:11** (72 %) |
  Ehrlich dazu: **Messbar besser ist es nicht, nur nicht schlechter.** Eine Fabrik wird in diesen Partien selten zerstoert (0,42 je Partie), die Regel greift
  also nur selten; bei 76 Partien liegen erst Unterschiede ab etwa 15 Prozentpunkten ausserhalb des Rauschens. Dass die Bots die Rangfolge einhalten,
  zeigen die Szenario-Tests oben, nicht die Duelle. `zufall` und `gierig` kennen die Rangfolge nicht (sie sind absichtlich schwach).

### Version 3.6 (Wuensche vom 19.11.2026, zweiter Teil)

- **Karte Kamikaze** (`terr_kamikaze=1`; im Browser-Menue standardmaessig an, in der Engine standardmaessig aus; braucht
  `terr_cardset3`, `terr_cardany`, `terr_exact`). Sie liegt dann immer im Markt, Preis **2 Eisen + 2 Gold** (aenderbar im Karten-Editor).
  Ablauf (Zustand `Perks.kamikaze`: 0 keine, 1 gerade gekauft, 2 brennt):
  1. Kauf auf eine eigene Einheit (nie der Koenig, nicht auf eine, die schon brennt). Die Figur **brennt ab dem Kauf** (im Viewer blinkt
     sie orange/rot mit Flammen) und kann in diesem Zug nicht ziehen (`Zustand 1`).
  2. Am Ende des eigenen Zuges faengt sie "richtig" Feuer (`Zustand 2`). Danach ist der Gegner dran: er sieht, dass sie kommt, und kann
     sie schlagen oder ausweichen (eine Runde Pause, wie gewuenscht).
  3. Im naechsten eigenen Zug darf sie **nur noch angreifen** (`Game::kamikaze_strikes`): Ziel ist eine gegnerische Figur **genau in
     Reichweite** (so weit, wie die Figur zieht; Karten wie Kuerzer, Ecke und Sprung gelten weiter), und **eine gegnerische Figur auf dem
     Weg darf uebersprungen werden** und stirbt mit. Eigene Figuren versperren den Weg wie sonst auch (ausser mit Durchzug). Wehrlose
     Figuren (nicht auf verbundenem Land) greifen nicht an, wie sonst auch.
  4. Der Angriff (`Game::do_kamikaze`): alle gegnerischen Figuren auf dem Weg sterben (ist es der Koenig, scheidet der Spieler aus),
     die brennende Figur auch, **alle Felder des Weges werden neutral** (gegnerische Fabrik auf dem Weg zerstoert, eigene bleibt),
     danach wie bei jedem Zug: Land, das nicht mehr an der Fabrik haengt, wird neutral (auch eigenes), Umschliessen und Wiederanbinden.
  5. **Zieht man sie nicht, ist sie am Ende dieses Zuges weg** (Ereignis `kamikaze_gone`). Ein Zug endet nur mit einem Zug, also muss man
     fuer "nicht ziehen" etwas anderes tun.
  Annahmen, die du anders haben kannst: die Figur stirbt beim Angriff (Name!); nur gegnerische Figuren werden uebersprungen und getoetet;
  der Koenig darf Ziel sein; es geht nur auf gegnerische Figuren (nicht auf leere Felder oder Fabriken); Bots **kaufen die Karte nicht**
  (`Game::legal_actions_for_bot`), sie kennen aber die Gefahr, weil ihre Suche die Zuege des Gegners mitzaehlt.
  Tests: `v3_tests` ab `kamikaze_game` (Kauf und Pause, Angriff mit Ueberspringen, Weg neutral, Abtrennen, Fabrik, Koenig, Verfall,
  fuenf Zufalls-Ganzspiele mit 99 Kaeufen und 46 Angriffen, in denen alle Invarianten halten).
- **Spielende-Fenster** im Viewer: am Ende einer Partie oeffnet sich nach einem Moment ein Fenster *Gewonnen!* / *Verloren* /
  *Unentschieden* (bei mehreren Menschen und beim Zuschauen "Spieler X gewinnt"), mit dem Grund (Koenig geschlagen oder Wertung mit
  den Punkten) und den Knoepfen *Brett ansehen* und *Neue Partie* (Esc und Klick daneben schliessen es). Je Partie einmal.
- **Standard 250 Runden** (`score_after = 250`, vorher 100; `max_rounds` bleibt 300). Die Zahlen in den aelteren Messungen dieser Datei
  stammen noch aus Partien mit 100 Runden.

- **Bot `super-stark` ("Super schwer") schlaegt zuerst und geht den Gegner staerker an** (Wuensche vom 19.11.: Umschliessen bewusst
  nutzen, am Anfang die Fabrik mit dem Koenig 1 bis 2 Felder verschieben, teilweise aggressiver; dazu der Fehler, dass Gegner die Fabrik
  rueckten, statt eine Figur zu schlagen). Neue Stellschrauben in `area_bot::Drive` (`SPACEVITAN_DRIVE="open=2,enclose=100,press=4,hunt=50,offence=30"`),
  die Standardwerte sind die gemessenen:
  1. **Schlagen zuerst** (`hunt=50`; **der Fehler**): Die Abkuerzung "Fabrik ruecken sofort kaufen" (`push`) lief *vor* der Zugsuche und
     sah Schlaege gar nicht an (`calm` zaehlt nur Gefahr fuer die eigenen Sachen). Jetzt prueft `area_bot::captures` zuerst alle Schlaege:
     **sinnvoll** ist ein Schlag, wenn er die Partie entscheidet, die Figur danach nicht zurueckgeschlagen werden kann oder der Tausch
     lohnt (Wert der geschlagenen Figur groesser als der der schlagenden). Gibt es einen sinnvollen Schlag, entfaellt in ruhiger Lage der
     Kartenkauf und das Fabrik-Rutschen davor, und der Schlag bekommt in der Zugsuche 50 % vom Wert der geschlagenen Figur als Zuschlag.
     Ein Schlag, der nur Figur gegen gleich Wertiges tauscht und verliert, bekommt nichts: die Vorausrechnung bestimmt, ob er sich lohnt
     ("nicht sinnlos reinlaufen"). Test: `hunt_takes_a_sound_capture_before_shifting_the_factory` (Ausgangslage kauft Fabrik ruecken,
     jetzt schlaegt er) und `a_free_capture_is_sound_and_a_losing_trade_is_not`. Messhilfe `capture_audit` (`cargo test --release -p game_core capture_audit -- --ignored --nocapture`):
     Der alte Super-Bot nutzte nur **19 von 28** Zuegen mit einem sicheren Schlag (68 %), mit `hunt=100` **25 von 29** (86 %); `sehr-stark` 10 von 11.
  2. **Druck** (`press=4`): eigene Figuren (nach Stufe) nahe am gegnerischen Koenig und seiner Fabrik zaehlen (Naehe bis 5 Felder), staerker bei
     vollem Brett. 3. **Angriffswert** (`offence=30`): wer am Zug ist, bewertet mit, was er dem Gegner im naechsten Zug abtrennen koennte.
  4. **Umschliessen** (`enclose=100`): ein weiteres Feld, das eine neutrale Flaeche ganz einschliessen wuerde, zaehlt als Chance
     (`enclosure_chance`). 5. **Eroeffnung** (`open=2`, bis Runde 8): der Koenig laeuft zwei Felder Richtung Mitte und baut die Fabrik dort
     neu (kostenloser Umzug); nur in ruhiger Lage.
  **Messung** (Version 3, 100 Runden, `--nodes-sehr 5000`, 2 x 24 Partien je Variante, beide Sitzordnungen, neu gegen alt; Skript `duelv.sh`):
  | Variante | neu : alt |
  |---|---|
  | Eroeffnung `open=1` / `open=2` | 23:25 / 26:22 |
  | Umschliessen `enclose=50` | 25:23 |
  | Angriffswert `offence=30` | 25:23 |
  | Druck `press=4` (Seeds 1, 101, 201) | 29:19, 26:22, 31:17 = **86:58** (60 %) |
  | Schlagen zuerst `hunt=1` / `50` / `100` | 23:25 / 23:25 / 22:26 |
  | Druck + Schlagen (`press=4,hunt=50`) | 25:23 |
  | **alles zusammen** (die neuen Standardwerte; Seeds 601 / 801) | **34:14** / **37:11** = 71:25 (74 %) |
  | alles zusammen, 250 Runden (Seeds 701, 2 x 14 Partien) | **21:7** (75 %) |
  Ehrlich dazu: **Einzeln** ist ausser dem Druck keine Idee messbar besser (24 Partien zeigen nur Unterschiede ab etwa 20 Prozentpunkten); `hunt`
  haelt die Staerke und behebt den Fehler. Dass die Kombination deutlich vorn liegt, die Teilkombination Druck + Schlagen aber nicht (25:23),
  zeigt, dass einzelne Teile erst zusammen wirken oder dass ein Teil davon Zufall ist; die beiden Reihen mit der Kombination (96 Partien,
  verschiedene Startzahlen) sind aber eindeutig. Umschliessen: mit `enclose=150` schliessen die Bots in 6 Partien 48 statt 32 Felder ein
  (nur Stichprobe) - es bleiben kleine Gewinne, weil der Brettrand beim Umschliessen standardmaessig nicht zaehlt (`terr_enclose_edge`).

### Version 3.5 (Wuensche vom 19.11.2026)

- **"Fabrik ruecken" wirkt sofort** (Engine, gilt im Gebietsmodus immer): Wer die Karte kauft, muss die Fabrik **gleich** auf ein
  Diagonalfeld rutschen lassen. Nach dem Kauf sind nur noch die Zuege `shift_factory` erlaubt (kein Zug, kein Bau, kein Kauf, kein
  Tausch); das Rutschen ist der Zug dieser Runde (wie vorher auch). Die Karte ist **nur kaufbar, wenn die Fabrik rutschen kann**
  (`Game::factory_shift_cells`: Diagonalfeld, eigenes mit der Fabrik verbundenes Land, ohne Figur, nur Quadratbrett); so bleibt nie
  eine nutzlose Karte liegen. Eine bereits liegende Karte (alter Stand, Wiedergabe) zwingt ebenfalls zum Rutschen, solange es ein Ziel gibt.
  Im Viewer erscheinen die grossen Zielmarker sofort, ein Tipp auf etwas anderes (auch Koenig oder Fabrik) tut nichts ausser den
  Hinweis zu zeigen, Abbrechen gibt es nicht (`shiftForced` im Viewer). Hat die Fabrik kein Zielfeld, steht an der Karte "kein Zielfeld".
- **Neuer Bot `super-stark`** ("Super schwer", Sitzcode 6): die Suche von `sehr-stark` (gleiches Budget, gleiche Zugsuche), aber mit
  einer anderen Blattbewertung und einer Regel fuer die Fabrik (`area_bot::Drive`, `SearchBot::super_strong`):
  1. **Mehr Land, solange Platz ist**: Wert je Flaeche 10 plus bis zu 6 bei ganz leerem Brett, sinkend mit dem Anteil freier Felder
     (`land`; gilt fuer eigenes und gegnerisches Land, der Bot raeumt also auch dem Gegner Land weg).
  2. **Fabrik Richtung Mitte**: 20 Punkte je Feldbreite, die die Fabrik naeher an der Brettmitte steht als die Startecke, hoechstens
     6 Feldbreiten (`centre`, `cap`; rund vier Diagonalschritte, auf dem 10er-Brett also bis in die Mitte).
  3. **Vorbereitung**: liegt "Fabrik ruecken" im Markt, ist sie bezahlbar und gehoert dem Bot ein **freies** Diagonalfeld Richtung Mitte,
     gibt die Bewertung 30 Punkte (`ready`); so haelt die Suche das Feld frei und holt Figuren dorthin.
  4. **Kauf ohne Zoegern** (`push`): ist die Karte kaufbar, kann die Fabrik damit naeher an die Mitte (`can_advance`) und ist die Lage
     ruhig (`calm`: Gefahr unter 300), kauft er sie sofort, ohne die Kaufrechnung der anderen Karten. Das Rutschen selbst waehlt die Suche
     (die Karte erzwingt es). Ist die Obergrenze erreicht, hoert er auf.
  Stellschrauben zum Abstimmen: `SPACEVITAN_DRIVE="land=6,centre=20,cap=6,ready=30,push=1"`.
  **Messung** (je Seitenwechsel: der Bot sitzt einmal als erster, einmal als zweiter Spieler; Version 3, `--nodes-sehr 5000`):
  - Karte "Fabrik ruecken" im Markt (`--cards factory_shift,settle`, 2 x 24 Partien je Variante): nur Land-Gewicht ohne Mitte-Drang
    **24:24**; mit Mitte-Drang und `ready=30` **37:11** (77 %), mit Obergrenze 4 Feldbreiten 34:14.
  - Zufaelliger Markt (die Karte liegt nur in etwa jeder dritten Partie): Seeds 1 bis 24 **29:19**, Seeds 25 bis 48 **26:22**, zusammen
    **55:41** (57 %, 96 Partien). Das ist nur schwach ueber dem Zufall (Streuung etwa 5 Prozentpunkte); der Vorsprung kommt aus den Partien, in denen die
    Karte da ist. Die erste Fassung (`ready=12`, ohne Kaufregel) lag bei 36:36 ueber 72 Partien, also **gleichauf** mit `sehr-stark`.
  - Verhalten: er rutscht in Partien mit der Karte oft 4 bis 8 Mal und steht dann nah an der Mitte; ab Runde 60 hat er im Schnitt etwa 4
    bis 13 Felder mehr Land als `sehr-stark` (8 Partien, nur zur Orientierung).
  - **Offen / ehrlich**: Der Vorteil haengt an der Karte (Kosten 3 Eisen der eigenen Farbe, kommt aus dem Lager der Fabrik und ist frueh
    knapp); die Fabrik **ohne** Karte umzuziehen (Umzug mit Bauzeit) kann er nicht gezielt. Dass die Mitte-Fabrik die Ursache des
    Vorsprungs ist, zeigt der Vergleich mit dem Kontrollbot (Land-Gewicht allein: 24:24), aber nicht, *warum* sie hilft (kuerzere Wege,
    mehr Land um die Fabrik, schnellere Verbindung?). Mit dem Standardbudget (20000 Stellungen, Karte im Markt, Seeds 101 bis 110, beide Sitzordnungen) **15:5**, also dasselbe Bild wie bei 5000.
- **Bot-Duelle am 19.11.**: Der erste Zug ist ein starker, aber schwankender Vorteil (Seeds 1 bis 12: Erster gewinnt 67 %, Seeds 13 bis 36: 40 %),
  darum immer beide Sitzordnungen messen und mindestens 48 Partien je Variante nehmen; 24 Partien zeigen nur Unterschiede ab etwa 20 Prozentpunkten.

### Version 3.4: Gelaende (`terr_terrain`, Standard aus; Wunsch vom 18.11.2026)

Jedes Feld ist **Feld** (zaehlt 1 Flaeche), **Erde** (1/2) oder **Moor** (1/4). Das ist die einzige Wirkung: es aendert, wie viel
Flaeche ein Feld im Einkommen bringt. Rohstoffe, Erntezaehler, Zugweite und Wertung bleiben, wie sie sind. Im Menue: *Erweiterte
Einstellungen → Gelaende*, und im Paket *Neue Regelideen* ist es eingeschaltet. Standard ist es **aus**, weil es die Wirtschaft stark
bremst (siehe Messung) und noch niemand damit gespielt hat.

Entwicklung am 18.11.2026: Zuerst gab es *Feld / Wald / Moor* mit grossen Zeichen. Auf den zweiten Wunsch hin heisst die mittlere Art
**Erde**, die Anzeige ist ein kleiner Kreis, und die Verteilung folgte Zonen. Der dritte Wunsch (gilt jetzt) legt die **Haeufigkeit**
fest, nicht den Wert: Die Werte bleiben 1, 1/2 und 1/4, aber **Moor ist ein Viertel der Felder, Erde ein Drittel, der Rest Feld**
(rund 42 %).

- **Rechnung**: in Vierteln (`Terrain::quarters`: Feld 4, Erde 2, Moor 1). Das Einkommen je Runde ist die Summe der Viertel aller mit der
  Fabrik verbundenen Felder durch 4; der Rest unter einer ganzen Flaeche wird in die naechste Runde uebertragen (`State::area_rest`,
  0 bis 3), damit nichts verloren geht. Fabrik zerstoert oder Land vergessen setzt den Rest auf 0. Die Preise bleiben in ganzen Flaechen.
  Die Anzeige im Viewer rechnet mit Bruchteilen (zum Beispiel "+3,5"). Im Mittel zaehlt ein Feld damit
  0,42 · 1 + 0,33 · 0,5 + 0,25 · 0,25 = rund 0,65 Flaeche (bei der Verteilung ueber das ganze Brett, die Startumgebung bleibt Feld).
- **Verteilung** (`crates/game_core/src/terrain.rs`): eigener Zufallsstrom aus dem Seed, die Rohstoffe liegen mit und ohne Gelaende an
  denselben Stellen (Test `terrain_does_not_move_a_single_deposit`).
  - *Haeufigkeit*: je Eckgebiet 25 % Moor (`BOG_PERMILLE`) und 33,3 % Erde (`EARTH_PERMILLE`), der Rest Feld. Felder bis Abstand 2
    von einer Startecke bleiben Feld. Jedes Eckgebiet bekommt **genau gleich viele** Moor- und Erdfelder (das kleinste Gebiet gibt die
    Zahl vor; bei Brettern mit ungleichen Gebieten bleiben die mittleren Felder der groesseren Feld). Test
    `about_a_quarter_is_bog_and_a_third_is_earth` prueft 19 bis 27 % Moor und 27 bis 36 % Erde ueber 2 bis 8 Spieler und je 12 Seeds.
  - *Lage*: nach der **Luftlinie zur naechsten Startecke** (nicht nach Feldschritten; so werden die Eckbereiche rund, nicht zu Rauten
    entlang des Brettrands). Nah an der Ecke liegt eher Moor, im Zwischenbereich eher Erde, in der Mitte eher Feld. Ein geglaettetes
    Rauschen (`NOISE_WEIGHT` 30 %, `SMOOTH_ROUNDS` 3) mischt die Bereiche etwas und macht Flecken statt starrer Ringe. Test
    `bog_lies_near_the_corners_earth_in_between_and_field_in_the_middle`: im Mittel Moor naeher an der Ecke als Erde, Erde naeher als Feld.
  - Von 200 Verteilungen gewinnt die mit dem kleinsten Unterschied im Flaechenwert rund um die Startecken der Spieler. Die Bretter fuer
    5, 7 und 8 Spieler sind selbst nicht ganz gleich gross (Basis ohne Gelaende: 5 Spieler 4, 7 Spieler 20, 8 Spieler 24 Viertel
    Unterschied rund um die Startecken); das Gelaende bringt dazu hoechstens ein volles Feld (4 Viertel) Unterschied, geprueft ueber
    2 bis 8 Spieler und je 20 Seeds (Test `the_terrain_adds_at_most_one_field_of_unfairness_around_the_starts`).
- **Anzeige**: auf **jedem** Feld sitzt unten links ein kleiner runder Kreis wie bei den Rohstoff-Abzeichen: ganzer Kreis = Feld (1),
  halber Kreis = Erde (1/2), Viertelkreis = Moor (1/4), in Graustufen (Feld hell, Erde mittel, Moor dunkel; keine Spielerfarbe). Der
  Hintergrund ist nach Wert **schraffiert**, seit dem 18.11. (Wunsch am Nachmittag) **umgekehrt**: Feld gar nicht, Erde einfach (eine
  Richtung, dazu ein grauer Hauch), Moor am staerksten (gekreuzt), und beide mit **halb so vielen Linien** wie vorher (doppelter
  Linienabstand). Rohstoff-Abzeichen (Figur steht auf dem Vorkommen) ruecken dafuer rechts neben den Kreis. Die Legende hat
  eine Zeile dazu, die Feldinfo (Info-Fenster, siehe unten) nennt Art und Wert. Das Gelaende steht im Replay-Kopf als `terrain`
  (0 Feld, 1 Erde, 2 Moor je Feld).
- **Bots** werten Land nach seinem Flaechenwert (`Game::land_quarters` in `area_bot.rs` und `ai.rs`); ohne Schalter ist alles wie vorher.
- **Messung** (2 Bots `flaeche`, 80 Partien, Seeds 1 bis 80, Version 3). Ohne Gelaende: erste Figur Stufe 2 in Runde 34,7, Stufe 3
  in 61,2 (in 80 von 80 Partien), Stufe 4 in 83,0 (in 60 von 80). Mit der Verteilung dieser Fassung (Moor 25 %, Erde 33 %):
  45,8 / 78,9 (in 58 von 80) / 92,8 (in 5 von 80). Das ist eine **starke Bremse**: Stufe 2 kommt gut 11 Runden spaeter, Stufe 4 wird
  praktisch nicht mehr erreicht. Zum Vergleich die fruehere Fassung mit rund 15 % Gelaendefeldern: +0,9 / +3,6 Runden. Gewinner und
  Landanteil aendern sich kaum (Siege 39 / 61 % statt 40 / 60 %), die Partie wird nur langsamer. Stellschrauben, falls es zu
  langsam ist: weniger Moor und Erde, billigere Stufen (zum Beispiel Stufenpreise um etwa ein Drittel senken) oder ein Einkommen,
  das die Verluste ausgleicht.
- **Kartenerzeugung (Korrektur 18.11., nach Meldung "Bots ziehen nicht, Start klappt teils nicht")**: `Map::generate` rechnete das Gelaende
  in der Versuchsschleife der Vorkommen-Fairness, also bei jedem verworfenen Versuch neu (bis zu 200 Gelaende-Verteilungen je Aufruf).
  Das Gelaende haengt nicht von den Vorkommen ab, es wird jetzt einmal je Karte berechnet; Gruppen, Luftlinien und Umkreise werden vorab
  berechnet (`Layout`). Die Karte je Seed ist unveraendert (Test `the_map_carries_exactly_the_terrain_made_once_for_its_board`).
  Gemessen (nativ, Release): schlechtester Fall 5 Spieler 82 auf 4 ms, 8 Spieler 21 auf 12 ms. Ob das die gemeldete Stoerung war, ist
  **nicht bewiesen**; nachgestellt werden konnte sie weder im Browser noch nativ.
- **Fehlerleiste im Viewer**: Unerwartete Fehler (Zeichnen, Anzeige, Bot-Zug anzeigen) erscheinen als rote Leiste oben (Antippen blendet
  sie aus; die harmlose Browser-Meldung "ResizeObserver loop completed" wird nicht angezeigt, und der Beobachter rechnet die Groesse jetzt im
  naechsten Bild statt sofort, dann tritt sie gar nicht mehr auf) und halten die Bots nicht mehr an: der Zug ist im Spielkern schon gespielt, scheitert nur die Anzeige, laeuft die Partie weiter.
- **Offen**: eine eigene Gelaende-Wirkung auf Bewegung (Moor bremst) wurde nicht gebaut, das waere eine zweite Idee. Die Anteile und das
  Rauschen sind Konstanten in `terrain.rs`, noch kein Menuefeld.

### Version 3.3 (Wuensche vom 18.11.2026)

- **Brot ist ein Lager mit Erntezaehler wie Eisen und muss besetzt sein** (`terr_bread=3` in `Config::set_v3`; vorher nur ein
  Schalter im Paket *Neue Regelideen*). Eine eigene Figur muss auf dem Brot-Feld stehen, sonst laeuft der Zaehler nicht
  (`terr_occupy`); alle 3 Runden (Takt von Eisen, `terr_ripen1`) kommt ein Stueck ins Lager der Fabrik. Der `2^k`-Flaechenbonus fuer
  verbundene Brot-Felder entfaellt. Zurueck zum alten Stand: `terr_bread=0` (Viewer: Paket *Alter Brot-Bonus*). Der Takt ist eine
  Zahl, keine Kopplung: wer `terr_ripen1` aendert, muss `terr_bread` mit aendern.
- **Kartenpreise ueber Eisen, Gold und Brot verteilt** (Tabelle bei "Neuer Kartensatz", `Card::v3_price_bread`). Leitidee: *Eisen*
  fuer Kampf und Bauten (Diagonalzug, Sturmangriff, Fabrik ruecken, Schmiede), *Brot* fuer Bewegung und Nachschub (Kuerzer, Ecke,
  Sprung, Durchzug, Siedeln, Doppelzug), *Gold* fuer Dauerhaftes und den Koenig (Bergbau, Koenigs-Boost). Die Gesamtsumme je Karte
  bleibt auf +-1 wie vorher (Test `the_bread_prices_spread_over_all_three_resources`). `price.<karte>=eisen/gold/brot` aendert jeden Preis.
- **Messung** (`flaeche` gegen `flaeche`, 40 Partien, Seeds ab 100, `--rule territory=1,terr_v3=1`, ohne und mit `terr_bread=0`):

  | | neu (Brot-Lager) | alt (`terr_bread=0`) |
  |---|---|---|
  | Figuren erzeugt je Partie | 7,0 | 8,8 |
  | geschlagen je Partie | 0,5 | 3,8 |
  | Stufe 2 / 3 / 4 erreicht (Runde) | 33 / 60 / 84 (Stufe 4 in 33 von 40) | 25 / 41 / 57 (39 von 40) |
  | Sieg erst nach der Wertung | 38 von 40 | 27 von 40 |
  | Ernte je Partie Kat 1 / 2 / 3 | 38 / 16 / 97 | 56 / 30 / 0 |

  Ehrlich gelesen: ohne den Flaechenbonus waechst die Flaeche deutlich langsamer, die Partien werden friedlicher und fast nie vor der
  Wertung entschieden. Brot wird von den Bots reichlich geerntet (zwei Brot-Felder je Ecke gegen ein Eisen-Feld), aber nur zu einem
  Teil ausgegeben; es ist heute eher Ueberschuss als Engpass. Naheliegende Stellschrauben, nicht gebaut: Brot zahlt einen Teil der
  Stufen (Figur verbessern), weniger Brot-Felder je Ecke, hoehere Brotpreise.
- **Viewer-Korrektur nebenbei**: Der Kaufdialog schrieb bei *Bergbau*, *Schmiede*, *Sturmangriff* und *Doppelzug* faelschlich "Die Figur
  darf danach in diesem Zug nicht ziehen". Diese Sofort-Karten sperren keine Figur (die Engine tat das nie), der Satz ist weg.
- **Viewer-Korrekturen zum Fort** (18.11.): In der flachen Ansicht verdeckte das Rohstoff-Symbol den Zielpunkt auf Feldern mit Vorkommen;
  die Zielpunkte werden jetzt nach Vorkommen und Figuren gemalt. Der Kartentext des Forts nennt das Stapeln nicht mehr (ein Fort je Spieler).
  Auf dem Handy liegen fuenf Karten (vier und das Fort) in einer Reihe, sechs in zwei Reihen zu drei, sonst vier je Reihe.
- **Fort-Karte** (`terr_fort=1`, Standard aus; Idee vom 18.11., Einschaetzung und Entscheidungen siehe unten). Die Karte *Fort* liegt dann
  **immer** im Markt (wie der Koenigs-Boost, nicht im Zufallspool; `Card::Fort`, Preis 3 Eisen + 1 Brot, mit `terr_bread=0` 4 Eisen).
  - **Kauf**: sie wirkt wie die anderen Figurenkarten (`terr_cardany`): man waehlt eine eigene Figur, das Fort entsteht **auf deren Feld**
    (`State::forts`, ein Fort je Spieler; ein neues versetzt das alte). Die Figur ruht in diesem Zug (`terr_cardlock`). Ort: eigenes
    Land, mit der stehenden Fabrik verbunden, kein Fabrikfeld, dort noch kein Fort; nie auf dem Koenig (`Game::fort_fits`).
  - **Wirkung**: jede eigene Figur auf dem Fort (nie der Koenig) zieht **n oder n+1 Felder** (`Game::fort_bonus`, in `exact_walk` die
    laengste Weglaenge, gespiegelt zu *Kuerzer*; ohne `terr_exact` Reichweite +1). Das Fort bleibt stehen, wenn die Figur wegzieht; die
    naechste Figur darauf profitiert wieder. **Nur die Zugweite** aendert sich: Ertrag je Stufe, Unterhalt, Preise und Wertung rechnen mit
    der echten Stufe.
  - **Zerstoerung**: sobald das Feld nicht mehr dem Besitzer gehoert (`set_land`: ein Gegner schlaegt oder faerbt es, Land wird abgetrennt
    und neutral oder faellt an den Gegner). `Event::FortLost` meldet es; eine Invariante prueft, dass jedes Fort auf Land des Besitzers
    steht. Auf nicht verbundenem Land bleibt das Fort stehen (die Figuren sind dort wehrlos).
  - **Viewer**: Zinnenrand und kleiner Turm auf dem Feld, Karte mit Turm-Symbol; Schalter im Menue ("Fort-Karte") und im Paket *Neue Regelideen*.
  - **Tests**: Schalter und Pflichtregeln, Kauf unter einer Figur (Sperre, Preis, nie der Koenig), n oder n+1 fuer Stufe 1 und 2, Fort bleibt
    stehen und dient der naechsten Figur, ein Fort je Spieler, Zerstoerung durch Faerben und durch Schlagen (mit `FortLost`), ganze Partien mit
    Zufallsbots und Invarianten (`whole_games_with_all_new_switches_keep_every_invariant_on_every_board`).
  - **Entscheidungen, die ich fuer dich getroffen habe** (der Satz "Es soll zerstoert werden wenn." brach ab): zerstoert wird, wenn das Feld dem
    Besitzer nicht mehr gehoert; das Fort entsteht unter einer eigenen Figur statt auf einem leeren Feld (so braucht es keine neue
    Aktion und keine Feldauswahl).
  - **Offen**: Preis ist geschaetzt (`price.fort=eisen/gold/brot`, im Karten-Editor noch nicht einstellbar); die Bots (`stark` kauft es
    manchmal, `flaeche` nie) planen es nicht, eine Messung steht aus. Ob der Held auf dem Fort 5 Felder ziehen darf, ist nicht gedeckelt.
- **Koenig antippen** (Viewer): ein Klick waehlt den Koenig nur (Ziehfelder), erst der **zweite Klick hintereinander** oeffnet die Blase
  *Fabrik hier bauen / Koenig verbessern*, ein dritter schliesst sie.

### Version 3.2 (Wuensche vom 17.11.2026)

- **Doppelzug und Sturmangriff liegen standardmaessig nicht im Markt** (`Card::V3_DEFAULT_OFF`, `CardTweaks::standard()`, benutzt
  von `Config::new`). Im Karten-Editor tragen sie das Etikett *Standard aus*; `pool.charge=1,pool.double_move=1` legt sie zurueck
  in den Pool. Sie bleiben im Katalog (`sv_cards`: `default_off`) und als Bausteine eigener Karten.
- **Neue Karte Diagonalzug** (`Card::PawnDiagonal`, Schluessel `pawn_diagonal`, 3 Eisen + 1 Gold): wirkt nur auf einen **Bauern**
  (`card_fits_piece`: Stufe 1, nur einmal je Figur). Die Figur darf zusaetzlich ein Feld diagonal ziehen oder schlagen
  (`Perks.pawn_diag`, in `piece_destinations`; der Weg bekommt wie jeder Sprung eine Bruecke). Der Perk bleibt auch, wenn der Bauer
  spaeter aufsteigt. Nur Quadratbrett. Sie liegt im Pool und ist als Baustein eigener Karten **nicht** freigegeben (Absicht: bleibt
  einfach).
- **Neubau nach Zerstoerung nach Entfernung**, siehe `terr_rebuild_dist` oben.
- **Premove mit dem Finger** (Viewer): ist eine Figur gewaehlt, plant ein Tipp auf ein Feld, das sie nicht in einem Zug erreicht,
  einen Weg (`tapRoute`, die vorhandene Route wie Shift/Strg am PC); jedes weitere Feld verlaengert ihn, **dasselbe Feld nochmal
  tippen startet ihn** (ein Schritt je eigener Zug, von selbst), ein Tipp auf die Figur verwirft ihn oder **unterbricht** die
  laufende Route (auch ausser der Reihe). Ein Feld, das die Figur in einem Zug erreicht, bleibt das Zweimal-Tippen-Ziehen.
  **Eigene Fabrik antippen (Wunsch 18.11.)**: Ist eine Figur gewaehlt, plant der Tipp auf die eigene Fabrik **keinen Weg** und zieht
  auch nicht sofort hin, sondern waehlt die Fabrik: ihr Menue geht auf, die Figur bleibt gewaehlt. Ganz oben im Menue steht dann
  **Hierher ziehen** (`factoryGoto`/`facGoto`): ein direkt erlaubter Zug wird gespielt, sonst startet der Weg (ein schon angetippter
  Weg wird bis zur Fabrik verlaengert). Eine andere eigene Figur antippen wechselt die Auswahl und verwirft den angetippten Weg.
  Das gilt in der 2D-Ansicht mit Fingerbedienung (`touchUi`); am PC und in 3D zieht der Klick auf ein erreichbares Feld wie bisher.
- **Info-Fenster zu einem Feld (Wunsch 18.11.: "nervt, muss weg")**: Am **Handy** (Finger oder Stift) erscheint es nie mehr, auch nicht
  beim Halten. Am **PC** erscheint es nicht mehr beim blossen Darueberfahren, sondern nur, solange die **linke Maustaste gedrueckt** ist
  (`pointer()` prueft `pointerType === "mouse"` und `buttons & 1`). Die Hervorhebung des Feldes und die Ziehfelder unter der Maus
  bleiben. Der Hinweis auf den Tooltip im Umzugstext wurde zu "der Knopf nennt die Dauer vorab".
- **Koenig antippen**: die Blase zeigt **Fabrik hier bauen** (Neubau nach Zerstoerung oder Umzug auf das Feld des Koenigs, mit Dauer
  und Kosten) und, wenn moeglich, **Koenig verbessern**.

Noch nicht gebaut, bewusst offen:

- **Timer** (erst sinnvoll mit echtem Mehrspieler): Optionen sind feste Zeit je Zug (etwa 10 s), Gesamtzeit mit Zuschlag je Zug wie
  bei Lichess oder gleichzeitige Zuege nach 10 s. Premoves entschaerfen kurze Timer, weil man vorplanen kann.
- **Gelaendekarten** (Felder mit eigener Art): seit Version 3.4 umgesetzt, siehe dort (`terr_terrain`). Eine *Karte*, die Gelaende
  verteilt oder verwandelt, gibt es nicht; das Gelaende liegt von Anfang an fest auf dem Brett.

### Version 3.1 (vier Schalter, alle Standard aus)

Auf Wunsch zum Ausprobieren, nichts davon aendert Version 3: ohne Schalter spielt sich alles wie vorher (Tests
`squared_upkeep_needs_the_upkeep_switch_and_changes_nothing_by_default`, `the_delay_is_a_switch_that_is_off_by_default_and_changes_the_card_text`). Im Browser stehen sie in *Erweiterte Einstellungen* unter "Version 3.1" und als Paket
*Neue Regelideen* im Schnellstart.

| Schalter | Wirkung |
|---|---|
| `terr_delay` | **Verzoegerung**: *Sturmangriff* und *Doppelzug* wirken erst im **naechsten eigenen Zug**. Der Kauf legt sie "bereit" (`State::armed_charge`, `armed_extra`; der Viewer zeigt "Bereit bei A" oben im Lager). Bereitgestellt ist nur eines je Karte, solange es nicht gewirkt hat, kauft man dieselbe Karte nicht noch einmal. Der Kartentext nennt die Verzoegerung |
| `terr_upkeep_sq` | **Unterhalt Stufe im Quadrat**: Bauer 1, Krieger 4, Ritter 9, Held 16 Flaeche je Runde (Koenig 0). Braucht `terr_upkeep`. Die Aufloesung bei Minus waehlt nach dem Unterhalt der Figur (`Piece::upkeep(squared)`) |
| `terr_yield_lvl` | **Ertrag je Stufe**: eine Figur der Stufe n auf einem Vorkommen erntet n Stueck je Ernte statt 1 (`territory::yield_multiplier`). Braucht `terr_occupy` (die Figur steht auf dem Feld) |
| `terr_bread=N` | **Brot als Lager**: Kat 3 wird alle N Runden geerntet (wie Eisen, nur mit eigener Figur auf dem Feld, Ertrag je Stufe zaehlt mit) und liegt im Lager der Fabrik; der 2^k-Flaechenbonus entfaellt (`Config::kat3_bonus_on`). **Seit 3.3 Standard in Version 3 mit N = 3** (gleicher Takt wie Eisen, `set_v3`); `terr_bread=0` bringt den alten Stand. Die Kartenpreise sind dann ueber Eisen, Gold und Brot verteilt (`Card::v3_price_bread`, Tabelle oben); `price.<karte>` schlaegt das weiterhin. Braucht `terr_cycle`; geht nicht zusammen mit `terr_trade` |

Probe-Spiele: Die Schalter gehen in 4 ganzen Partien auf 4 Brettern gleichzeitig durch (Test `whole_games_with_all_new_switches_keep_every_invariant_on_every_board`). **Messung**
(Bots, nicht Menschen): *Brot als Lager* bremst das Hochstufen deutlich (weniger Gebaeudestufen bis Runde 100), weil Brot nur mit
einer Figur auf dem Feld und nur alle N Runden kommt. *Unterhalt Stufe²* beisst kaum, weil die Bots Flaeche horten (rund 109 / 319 /
1361 Flaeche in Runde 20 / 50 / 80): das Problem ist die eine Aktion je Zug, nicht das Geld. Die *Verzoegerung* laesst sich mit Bots
nicht messen, weil sie die beiden Karten nie kaufen; ob sie Doppelzug und Sturmangriff zaehmt, zeigt erst Spielen mit Menschen.

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
