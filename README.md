# spacevitan (Arbeitstitel SpaceCivTan)

Rundenbasiertes Strategiespiel: eine Mischung aus Schach, Siedler (Catan) und Civ.
Das Regelwerk liegt als Dokument beim Projekt; dieses Repository enthaelt die Umsetzung.

## Aufbau

| Pfad | Inhalt |
|---|---|
| `crates/game_core` | Regel-Kern in Rust: Karte, Rohstoffe, Zugregeln, Bots. Keine Grafik, keine externen Pakete. |
| `crates/cli` | Text-Oberflaeche zum Zuschauen (Bot gegen Bot, Karten als Text). |
| `client/` | Godot-Client (Isometrie, Eingabe) – kommt, sobald der Kern steht. |
| `.github/workflows` | CI: Tests unter Linux, Windows-`.exe` als Download. |

## Tests

```
cargo test --workspace
```

## Stand

Meilenstein 1 ist in Arbeit (2 Spieler, Quadratkarte, ein vollstaendiger Spielverlauf Bot gegen Bot).
