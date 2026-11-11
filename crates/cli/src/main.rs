//! Text-Oberflaeche: Bots gegeneinander spielen lassen und zusehen oder Statistik sammeln.

use game_core::action::Event;
use game_core::bot::{play_out, Bot, GreedyBot, RandomBot};
use game_core::config::Config;
use game_core::game::Game;
use game_core::ids::PlayerId;
use game_core::render::{player_letter, render_game};
use game_core::replay::ReplayWriter;
use game_core::state::Outcome;
use std::io::BufRead;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

const HELP: &str = "\
spacevitan - Bots spielen gegeneinander (Textansicht)

Ohne Optionen (z. B. per Doppelklick): eine neue Partie Runde fuer Runde anzeigen.

Optionen:
  --players N    2 bis 8 (Standard 2)
  --seed N       Startwert fuer Karte und Bots; gleicher Wert = gleiche Partie
                 (Standard: jedes Mal neu, der Wert wird angezeigt)
  --side N       Kantenlaenge der Karte (Standard 10)
  --rounds N     Unentschieden nach N Runden (Standard 300)
  --stock N      Vorrat je Rohstoffvorkommen in Einheiten (Standard 0 = unerschoepflich)
  --kat1 N       Felder je Kat-1-Typ, also Bauplaetze fuer Fabriken je Ecke (Standard 2)
  --bots LISTE   pro Spieler: gierig oder zufall, z. B. gierig,zufall (Standard: gierig)
  --show         Partie Runde fuer Runde anzeigen
  --games N      N Partien mit Seeds seed..seed+N-1 spielen und Statistik zeigen
  --export DATEI Partie als JSON speichern (fuer die Browser-Ansicht), nur mit einer Partie
  --help         diese Hilfe

Legende: A* Koenig von A, a2 Figur von A mit Stufe 2, #A Fabrik von A,
         $ Beute, Ziffern = Rohstoffkategorien auf dem Feld, . leer";

struct Options {
    players: u8,
    seed: Option<u64>,
    side: u32,
    rounds: u32,
    stock: u32,
    kat1: u32,
    bots: Vec<String>,
    show: bool,
    games: u32,
    export: Option<String>,
}

fn parse(args: &[String]) -> Result<Option<Options>, String> {
    let mut o = Options {
        players: 2,
        seed: None,
        side: 10,
        rounds: 300,
        stock: 0,
        kat1: 0,
        bots: Vec::new(),
        show: false,
        games: 1,
        export: None,
    };
    let mut i = 0;
    let value = |i: usize| -> Result<&String, String> {
        args.get(i + 1)
            .ok_or_else(|| format!("{} braucht einen Wert", args[i]))
    };
    let number = |s: &String, name: &str| -> Result<u64, String> {
        s.parse::<u64>()
            .map_err(|_| format!("{name}: '{s}' ist keine Zahl"))
    };
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => return Ok(None),
            "--show" => o.show = true,
            "--players" => {
                o.players = number(value(i)?, "--players")? as u8;
                i += 1;
            }
            "--seed" => {
                o.seed = Some(number(value(i)?, "--seed")?);
                i += 1;
            }
            "--side" => {
                o.side = number(value(i)?, "--side")? as u32;
                i += 1;
            }
            "--rounds" => {
                o.rounds = number(value(i)?, "--rounds")? as u32;
                i += 1;
            }
            "--stock" => {
                o.stock = number(value(i)?, "--stock")? as u32;
                i += 1;
            }
            "--kat1" => {
                o.kat1 = number(value(i)?, "--kat1")? as u32;
                i += 1;
            }
            "--games" => {
                o.games = (number(value(i)?, "--games")? as u32).max(1);
                i += 1;
            }
            "--export" => {
                o.export = Some(value(i)?.clone());
                i += 1;
            }
            "--bots" => {
                o.bots = value(i)?.split(',').map(|s| s.trim().to_string()).collect();
                i += 1;
            }
            other => return Err(format!("unbekannte Option '{other}' (siehe --help)")),
        }
        i += 1;
    }
    Ok(Some(o))
}

/// Neuer Startwert aus der Uhrzeit, kurz genug zum Abtippen.
fn fresh_seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    game_core::rng::Rng::new(nanos).next_u64() % 1_000_000
}

fn make_bots(o: &Options, seed: u64) -> Result<Vec<Box<dyn Bot>>, String> {
    (0..o.players as usize)
        .map(|i| {
            let kind = o
                .bots
                .get(i)
                .or(o.bots.last())
                .map(String::as_str)
                .unwrap_or("gierig");
            let bot_seed = seed.wrapping_mul(1_000_003).wrapping_add(i as u64);
            match kind {
                "gierig" | "greedy" => Ok(Box::new(GreedyBot::new(bot_seed)) as Box<dyn Bot>),
                "zufall" | "random" => Ok(Box::new(RandomBot::new(bot_seed)) as Box<dyn Bot>),
                other => Err(format!("unbekannter Bot '{other}' (gierig oder zufall)")),
            }
        })
        .collect()
}

#[derive(Default)]
struct Stats {
    wins: Vec<u32>,
    draws: u32,
    rounds: u64,
    spawned: u64,
    factories: u64,
    captures: u64,
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(mut o) = parse(&args)? else {
        println!("{HELP}");
        return Ok(());
    };
    // Ohne Optionen (Doppelklick unter Windows): Partie zeigen und am Ende auf Enter warten,
    // damit sich das Fenster nicht sofort schliesst.
    let interactive = args.is_empty();
    if interactive {
        o.show = true;
    }
    let base_seed = o.seed.unwrap_or_else(fresh_seed);
    if o.export.is_some() && o.games > 1 {
        return Err("--export geht nur mit einer Partie (ohne --games)".into());
    }

    let mut stats = Stats {
        wins: vec![0; o.players as usize],
        ..Stats::default()
    };

    for g in 0..o.games {
        let seed = base_seed + g as u64;
        let mut config = Config::new(o.players, seed);
        config.side = o.side;
        config.max_rounds = o.rounds;
        config.deposit_stock = o.stock;
        if o.kat1 > 0 {
            config.cells_per_type[0] = o.kat1;
        }
        let mut game = Game::new(config).map_err(|e| e.to_string())?;
        let mut bots = make_bots(&o, seed)?;

        if o.show {
            let names: Vec<String> = bots
                .iter()
                .enumerate()
                .map(|(i, b)| format!("{}={}", player_letter(PlayerId(i as u8)), b.name()))
                .collect();
            println!(
                "Seed {seed}, {} Spieler ({})\n",
                o.players,
                names.join(", ")
            );
            println!("Karte:\n{}", game.map.render_ascii());
        }

        let show = o.show;
        if show {
            println!("Start:\n{}", render_game(&game));
        }
        let mut last_round = 1;
        let bot_names: Vec<String> = bots.iter().map(|b| b.name().to_string()).collect();
        let mut writer = o
            .export
            .as_ref()
            .map(|_| ReplayWriter::new(&game, &bot_names));
        let outcome = play_out(&mut game, &mut bots, |game, player, action, events| {
            if let Some(w) = writer.as_mut() {
                w.record(game, player, action, events);
            }
            for e in events {
                match e {
                    Event::Spawned { .. } => stats.spawned += 1,
                    Event::FactoryBuilt { .. } => stats.factories += 1,
                    Event::Captured { .. } => stats.captures += 1,
                    _ => {}
                }
                if show {
                    if let Event::PlayerEliminated { player } = e {
                        println!("Spieler {} ist ausgeschieden.", player_letter(*player));
                    }
                }
            }
            if show && game.outcome().is_none() && game.state.round != last_round {
                println!("Nach Runde {last_round}:\n{}", render_game(game));
                last_round = game.state.round;
            }
        });
        if show {
            println!("Endstand:\n{}", render_game(&game));
        }
        stats.rounds += game.state.round.min(o.rounds) as u64;
        if let (Some(path), Some(w)) = (o.export.as_ref(), writer) {
            let frames = w.frame_count();
            std::fs::write(path, w.finish(&game))
                .map_err(|e| format!("{path} konnte nicht geschrieben werden: {e}"))?;
            println!("Partie gespeichert: {path} ({frames} Zuege inklusive Start)");
        }

        match outcome {
            Outcome::Winner(p) => {
                stats.wins[p.idx()] += 1;
                if o.show || o.games == 1 {
                    println!(
                        "Ergebnis: Spieler {} gewinnt in Runde {}. (Seed {seed}, gleiche Partie mit --seed {seed})",
                        player_letter(p),
                        game.state.round
                    );
                }
            }
            Outcome::Draw => {
                stats.draws += 1;
                if o.show || o.games == 1 {
                    println!(
                        "Ergebnis: unentschieden nach {} Runden. (Seed {seed}, gleiche Partie mit --seed {seed})",
                        o.rounds
                    );
                }
            }
        }
    }

    if o.games > 1 {
        let n = o.games as f64;
        println!(
            "{} Partien, {} Spieler, Seeds {}..{}",
            o.games,
            o.players,
            base_seed,
            base_seed + o.games as u64 - 1
        );
        for (i, w) in stats.wins.iter().enumerate() {
            println!(
                "  Siege {}: {w} ({:.0} %)",
                player_letter(PlayerId(i as u8)),
                *w as f64 / n * 100.0
            );
        }
        println!(
            "  Unentschieden: {} ({:.0} %)",
            stats.draws,
            stats.draws as f64 / n * 100.0
        );
        println!("  Runden im Schnitt: {:.1}", stats.rounds as f64 / n);
        println!(
            "  Pro Partie: {:.1} Fabriken, {:.1} Figuren erzeugt, {:.1} geschlagen",
            stats.factories as f64 / n,
            stats.spawned as f64 / n,
            stats.captures as f64 / n
        );
    }
    if interactive {
        println!("\nEnter druecken zum Beenden.");
        let _ = std::io::stdin().lock().lines().next();
    }
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Fehler: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn defaults() {
        let o = parse(&[]).unwrap().unwrap();
        assert_eq!(
            (o.players, o.seed, o.side, o.rounds, o.games, o.show),
            (2, None, 10, 300, 1, false)
        );
    }

    #[test]
    fn parses_all_options() {
        let o = parse(&args(
            "--players 4 --seed 9 --side 12 --rounds 50 --games 3 --show --bots gierig,zufall",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            (o.players, o.seed, o.side, o.rounds, o.games, o.show),
            (4, Some(9), 12, 50, 3, true)
        );
        assert_eq!(o.bots, vec!["gierig", "zufall"]);
    }

    #[test]
    fn fresh_seeds_are_short_enough_to_type() {
        for _ in 0..100 {
            assert!(fresh_seed() < 1_000_000);
        }
    }

    #[test]
    fn export_takes_a_file_name() {
        let o = parse(&args("--export partie.json")).unwrap().unwrap();
        assert_eq!(o.export.as_deref(), Some("partie.json"));
        assert!(parse(&args("--export")).is_err());
    }

    #[test]
    fn help_and_errors() {
        assert!(parse(&args("--help")).unwrap().is_none());
        assert!(parse(&args("--seed")).is_err());
        assert!(parse(&args("--seed abc")).is_err());
        assert!(parse(&args("--frobnicate")).is_err());
    }

    #[test]
    fn unknown_bot_names_are_rejected_and_the_last_bot_repeats() {
        let mut o = parse(&args("--players 4 --bots zufall")).unwrap().unwrap();
        assert_eq!(
            make_bots(&o, 1)
                .unwrap()
                .iter()
                .filter(|b| b.name() == "zufall")
                .count(),
            4
        );
        o.bots = vec!["schach".into()];
        assert!(make_bots(&o, 1).is_err());
    }
}
