//! Text-Oberflaeche: Bots gegeneinander spielen lassen und zusehen oder Statistik sammeln.

use game_core::action::{Action, Event};
use game_core::ai::{SearchBot, SearchParams, Weights};
use game_core::bot::{play_out_with, Bot, GreedyBot, RandomBot};
use game_core::cards::Card;
use game_core::cards::CardTarget;
use game_core::config::Config;
use game_core::game::Game;
use game_core::ids::PlayerId;
use game_core::render::{player_letter, render_game};
use game_core::replay::ReplayWriter;
use game_core::state::Outcome;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::BufRead;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const HELP: &str = "\
spacevitan - Bots spielen gegeneinander (Textansicht)

Ohne Optionen (z. B. per Doppelklick): eine neue Partie Runde fuer Runde anzeigen.

Optionen:
  --players N    2 bis 8 (Standard 2)
  --seed N       Startwert fuer Karte und Bots; gleicher Wert = gleiche Partie
                 (Standard: jedes Mal neu, der Wert wird angezeigt)
  --side N       Kantenlaenge der Karte (Standard 10)
  --rounds N     Unentschieden nach N Runden (Standard 300; vorher entscheidet meist die
                 Wertung nach score_after = 100 Runden)
  --stock N      Vorrat je Rohstoffvorkommen in Einheiten (Standard 0 = unerschoepflich)
  --kat1 N       eigene Kat-1-Vorkommen je Ecke, der Koenig erntet sie zum Bauen (Standard 1)
  --cards LISTE  Markt fest vorgeben, z. B. training,lance (leer = ohne Karten)
  --gift X       Messung: Spieler A bekommt die Karte X geschenkt (sobald sie wirken kann)
                 oder N Rohstoffeinheiten: units:N ab Runde 3, units@fig:N mit der
                 ersten Figur, units@fab:N mit der ersten Fabrik
  --rule LISTE   weitere Regeln, z. B. upgrade_step=6,regrow_cat2=4
                 (cat1_distance, center_colors, center_radius, center_cells, mid_cat1,
                 regrow_cat1..4, center_mix, score_after, upgrade_region_min, upgrade_step, max_upgrades, market_size,
                 card_target_near, min_factory_distance, kat_roles, pawn_cost)
  --bots LISTE   pro Spieler: stark, stark-pur (kauft nie Karten), gierig oder zufall,
                 z. B. stark,gierig
                 (Standard: gierig)
  --nodes N      Rechenbudget von \"stark\": Stellungen pro Zug (Standard 20000)
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
    rules: String,
    cards: Option<Vec<Card>>,
    gift: Option<Gift>,
    bots: Vec<String>,
    nodes: u64,
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
        rules: String::new(),
        cards: None,
        gift: None,
        bots: Vec::new(),
        nodes: SearchParams::DEFAULT.node_budget,
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
            "--gift" => {
                let spec = value(i)?;
                o.gift = Some(parse_gift(spec)?);
                i += 1;
            }
            "--cards" => {
                let list = value(i)?;
                let mut cards = Vec::new();
                for key in list.split(',').map(str::trim).filter(|k| !k.is_empty()) {
                    cards.push(
                        Card::from_key(key).ok_or_else(|| format!("unbekannte Karte '{key}'"))?,
                    );
                }
                o.cards = Some(cards);
                i += 1;
            }
            "--rule" | "--rules" => {
                let list = value(i)?;
                Config::new(2, 0).set_all(list)?;
                if !o.rules.is_empty() {
                    o.rules.push(',');
                }
                o.rules.push_str(list);
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
            "--nodes" => {
                o.nodes = number(value(i)?, "--nodes")?.max(1);
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
                "stark" | "strong" => {
                    // Zum Abstimmen: SPACEVITAN_WEIGHTS="trap=30,king_hunt=4"
                    let weights = match std::env::var("SPACEVITAN_WEIGHTS") {
                        Ok(list) => Weights::DEFAULT.parse_overrides(&list)?,
                        Err(_) => Weights::DEFAULT,
                    };
                    let bot = SearchBot::with_params(bot_seed, SearchParams::with_budget(o.nodes))
                        .with_weights(weights);
                    Ok(Box::new(bot) as Box<dyn Bot>)
                }
                "stark-pur" => {
                    let bot = SearchBot::with_params(bot_seed, SearchParams::with_budget(o.nodes))
                        .without_cards();
                    Ok(Box::new(bot) as Box<dyn Bot>)
                }
                "gierig" | "greedy" => Ok(Box::new(GreedyBot::new(bot_seed)) as Box<dyn Bot>),
                "zufall" | "random" => Ok(Box::new(RandomBot::new(bot_seed)) as Box<dyn Bot>),
                other => Err(format!(
                    "unbekannter Bot '{other}' (stark, gierig oder zufall)"
                )),
            }
        })
        .collect()
}

/// Geschenk an Spieler A fuer die Kartenwert-Messung (`--gift`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gift {
    Card(Card),
    Units(u32, GiftTime),
}

/// Wann ein Rohstoff-Geschenk kommt: ab Runde 3, mit der ersten Figur oder mit der ersten
/// Fabrik. So laesst sich eine Karte mit Rohstoffen zum selben Zeitpunkt vergleichen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GiftTime {
    Round3,
    FirstUnit,
    FirstFactory,
}

fn parse_gift(spec: &str) -> Result<Gift, String> {
    let units = |n: &str, when| {
        n.parse()
            .map(|n| Gift::Units(n, when))
            .map_err(|_| format!("--gift {spec}: keine Zahl"))
    };
    if let Some(n) = spec.strip_prefix("units:") {
        units(n, GiftTime::Round3)
    } else if let Some(n) = spec.strip_prefix("units@fig:") {
        units(n, GiftTime::FirstUnit)
    } else if let Some(n) = spec.strip_prefix("units@fab:") {
        units(n, GiftTime::FirstFactory)
    } else {
        Card::from_key(spec)
            .map(Gift::Card)
            .ok_or_else(|| format!("unbekannte Karte '{spec}'"))
    }
}

/// Gibt das Geschenk, sobald es wirken kann: Figurenkarten an A's staerkste Figur, sobald es
/// eine gibt; Fabrikkarten (Rekrut, Doppelschicht, Bergbau, Ackerbau) mit der ersten Fabrik;
/// Rohstoffe zum gewaehlten Zeitpunkt; sonst ab Runde 3. `true` = vergeben.
fn give_gift(g: &mut Game, gift: Gift) -> bool {
    let a = PlayerId(0);
    if g.current_player() != a || g.state.round < 3 || !g.state.alive[0] {
        return false;
    }
    let has_unit = g.pieces_of(a).any(|p| !p.is_king());
    let has_factory = g.factories_of(a).next().is_some();
    match gift {
        Gift::Units(n, when) => {
            let ready = match when {
                GiftTime::Round3 => true,
                GiftTime::FirstUnit => has_unit,
                GiftTime::FirstFactory => has_factory,
            };
            if ready {
                g.grant_resources(a, n);
            }
            ready
        }
        Gift::Card(Card::Recruit | Card::DoubleShift | Card::Mining | Card::Farming)
            if !has_factory =>
        {
            false
        }
        Gift::Card(card) if card.target() == CardTarget::Unit => {
            let best = g
                .pieces_of(a)
                .filter(|p| !p.is_king())
                .max_by_key(|p| match &p.kind {
                    game_core::state::PieceKind::Unit { level, .. } => *level,
                    game_core::state::PieceKind::King => 0,
                })
                .map(|p| p.id);
            best.is_some() && g.grant_card(a, card, best)
        }
        Gift::Card(card) => g.grant_card(a, card, None),
    }
}

/// Misst die Rechenzeit eines Bots.
struct Timed {
    inner: Box<dyn Bot>,
    clock: Rc<RefCell<Clock>>,
}

#[derive(Default)]
struct Clock {
    total: Duration,
    max: Duration,
    moves: u64,
}

impl Bot for Timed {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn choose(&mut self, game: &Game) -> Action {
        let start = Instant::now();
        let a = self.inner.choose(game);
        let took = start.elapsed();
        let mut c = self.clock.borrow_mut();
        c.total += took;
        c.max = c.max.max(took);
        c.moves += 1;
        a
    }
}

#[derive(Default)]
struct Stats {
    wins: Vec<u32>,
    wins_by_bot: BTreeMap<String, u32>,
    clocks: BTreeMap<String, Rc<RefCell<Clock>>>,
    draws: u32,
    /// Wie viele der Siege erst durch die Wertung nach `score_after` Runden fielen.
    by_points: u32,
    rounds: u64,
    spawned: u64,
    factories: u64,
    captures: u64,
    cards: u64,
    cards_by_kind: BTreeMap<&'static str, u64>,
    /// Je Stufe 2 und 3: Summe der Runden, in denen die erste solche Figur stand, und in wie
    /// vielen Partien es ueberhaupt eine gab.
    first_level: [(u64, u32); 2],
}

/// Hoechste Stufe einer Figur auf dem Brett.
fn top_level(game: &Game) -> u8 {
    game.state
        .pieces
        .values()
        .filter_map(|p| match &p.kind {
            game_core::state::PieceKind::Unit { level, .. } => Some(*level),
            _ => None,
        })
        .max()
        .unwrap_or(0)
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
        config.set_all(&o.rules)?;
        let mut game = Game::new(config).map_err(|e| e.to_string())?;
        if let Some(cards) = &o.cards {
            game.state.market = cards.clone();
        }
        let mut bots: Vec<Box<dyn Bot>> = make_bots(&o, seed)?
            .into_iter()
            .map(|b| {
                let clock = stats
                    .clocks
                    .entry(b.name().to_string())
                    .or_default()
                    .clone();
                Box::new(Timed { inner: b, clock }) as Box<dyn Bot>
            })
            .collect();

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
        let mut gift = o.gift;
        let mut seen_level = [false; 2];
        let before = |g: &mut Game| {
            if let Some(gv) = gift {
                if give_gift(g, gv) {
                    gift = None;
                }
            }
        };
        let outcome = play_out_with(
            &mut game,
            &mut bots,
            before,
            |game, player, action, events| {
                if let Some(w) = writer.as_mut() {
                    w.record(game, player, action, events);
                }
                let top = top_level(game);
                for (i, seen) in seen_level.iter_mut().enumerate() {
                    if !*seen && top >= i as u8 + 2 {
                        *seen = true;
                        stats.first_level[i].0 += game.state.round as u64;
                        stats.first_level[i].1 += 1;
                    }
                }
                for e in events {
                    match e {
                        Event::Spawned { .. } => stats.spawned += 1,
                        Event::FactoryBuilt { .. } => stats.factories += 1,
                        Event::Captured { .. } => stats.captures += 1,
                        Event::CardBought { card, .. } => {
                            stats.cards += 1;
                            *stats.cards_by_kind.entry(card.name()).or_default() += 1;
                        }
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
            },
        );
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
            Outcome::Winner(p) | Outcome::Points(p) => {
                if matches!(outcome, Outcome::Points(_)) {
                    stats.by_points += 1;
                }
                stats.wins[p.idx()] += 1;
                *stats
                    .wins_by_bot
                    .entry(bot_names[p.idx()].clone())
                    .or_default() += 1;
                if o.show || o.games == 1 {
                    let how = if matches!(outcome, Outcome::Points(_)) {
                        format!(" nach Wertung {:?}", game.scores())
                    } else {
                        String::new()
                    };
                    println!(
                        "Ergebnis: Spieler {} gewinnt in Runde {}{how}. (Seed {seed}, gleiche Partie mit --seed {seed})",
                        player_letter(p),
                        game.state.round
                    );
                }
            }
            Outcome::Draw => {
                stats.draws += 1;
                if o.show || o.games == 1 {
                    println!(
                        "Ergebnis: unentschieden nach {} Runden, Wertung {:?}. (Seed {seed}, gleiche Partie mit --seed {seed})",
                        game.state.round,
                        game.scores()
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
        if stats.by_points > 0 {
            println!("  davon Siege erst nach Wertung: {}", stats.by_points);
        }
        if stats.clocks.len() > 1 {
            let by_bot: Vec<String> = stats
                .clocks
                .keys()
                .map(|name| format!("{name} {}", stats.wins_by_bot.get(name).unwrap_or(&0)))
                .collect();
            println!("  Siege nach Bot: {}", by_bot.join(", "));
        }
        println!("  Runden im Schnitt: {:.1}", stats.rounds as f64 / n);
        for (name, clock) in &stats.clocks {
            let c = clock.borrow();
            if c.moves > 0 {
                println!(
                    "  Rechenzeit {name}: {:.1} ms pro Zug, hoechstens {:.0} ms",
                    c.total.as_secs_f64() * 1000.0 / c.moves as f64,
                    c.max.as_secs_f64() * 1000.0
                );
            }
        }
        println!(
            "  Pro Partie: {:.1} Fabriken, {:.1} Figuren erzeugt, {:.1} geschlagen, {:.1} Karten gekauft",
            stats.factories as f64 / n,
            stats.spawned as f64 / n,
            stats.captures as f64 / n,
            stats.cards as f64 / n
        );
        let first = |(sum, games): (u64, u32)| match games {
            0 => "nie".to_string(),
            g => format!(
                "Runde {:.1} (in {g} von {} Partien)",
                sum as f64 / g as f64,
                o.games
            ),
        };
        println!(
            "  Erste Figur Stufe 2: {}; Stufe 3: {}",
            first(stats.first_level[0]),
            first(stats.first_level[1])
        );
        if !stats.cards_by_kind.is_empty() {
            let mut kinds: Vec<(&str, u64)> =
                stats.cards_by_kind.iter().map(|(k, v)| (*k, *v)).collect();
            kinds.sort_by_key(|k| std::cmp::Reverse(k.1));
            let text: Vec<String> = kinds.iter().map(|(k, v)| format!("{k} {v}")).collect();
            println!("  Karten: {}", text.join(", "));
        }
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

    #[test]
    fn gifts_and_fixed_markets_can_be_given() {
        let o = parse(&args("--gift units:8 --cards lance,horse"))
            .unwrap()
            .unwrap();
        assert_eq!(o.gift, Some(Gift::Units(8, GiftTime::Round3)));
        assert_eq!(o.cards, Some(vec![Card::Lance, Card::Horse]));
        let o = parse(&args("--gift units@fab:4")).unwrap().unwrap();
        assert_eq!(o.gift, Some(Gift::Units(4, GiftTime::FirstFactory)));
        let o = parse(&args("--gift units@fig:2")).unwrap().unwrap();
        assert_eq!(o.gift, Some(Gift::Units(2, GiftTime::FirstUnit)));
        assert!(parse(&args("--gift units@fig:")).is_err());
        let o = parse(&args("--gift training")).unwrap().unwrap();
        assert_eq!(o.gift, Some(Gift::Card(Card::Training)));
        assert!(parse(&args("--gift schild")).is_err());
        assert!(parse(&args("--gift units:x")).is_err());
        assert!(parse(&args("--cards lance,schild")).is_err());
        let names: Vec<&str> =
            make_bots(&parse(&args("--bots stark-pur,stark")).unwrap().unwrap(), 1)
                .unwrap()
                .iter()
                .map(|b| b.name())
                .collect();
        assert_eq!(names, vec!["stark-pur", "stark"]);
    }

    #[test]
    fn the_strong_bot_and_its_budget_can_be_chosen() {
        let o = parse(&args("--bots stark,gierig --nodes 500"))
            .unwrap()
            .unwrap();
        assert_eq!(o.nodes, 500);
        let names: Vec<&str> = make_bots(&o, 1).unwrap().iter().map(|b| b.name()).collect();
        assert_eq!(names, vec!["stark", "gierig"]);
        assert!(parse(&args("--nodes x")).is_err());
    }
}
