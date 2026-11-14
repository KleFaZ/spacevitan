//! Partie-Export als JSON: Karte, Rohstoffe und nach jedem Zug Aktion, Ereignisse und Zustand.
//!
//! Das Format liest die Browser-Ansicht (`viewer/`) und spaeter der Godot-Client.
//! Frame 0 ist der Startzustand, Frame k der Zustand nach dem k-ten Zug.
//! Zellen, Figuren, Fabriken und Rohstoffe werden ueber ihre numerischen IDs referenziert.

use crate::action::{Action, Event, PaySource};
use crate::cards::Card;
use crate::game::Game;
use crate::ids::PlayerId;
use crate::state::{Inventory, Outcome, PieceKind};

pub const FORMAT: &str = "spacevitan-replay";
pub const VERSION: u32 = 1;

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn list<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    let parts: Vec<String> = items.into_iter().map(f).collect();
    format!("[{}]", parts.join(","))
}

/// Zahl mit hoechstens 4 Nachkommastellen, ohne ueberfluessige Nullen (`1.5`, `3`, `-0.25`).
fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

fn point(p: (f64, f64)) -> String {
    format!("[{},{}]", num(p.0), num(p.1))
}

fn source_json(from: &PaySource) -> String {
    match from {
        PaySource::King => r#""king""#.to_string(),
        PaySource::Factory(f) => f.0.to_string(),
    }
}

fn opt_piece(p: &Option<crate::ids::PieceId>) -> String {
    p.map_or("null".to_string(), |p| p.0.to_string())
}

/// Karte im Markt: Kurzname, Name, Wirkung, Kosten, Ziel.
fn card_json(card: &Card, pawn_cost: Option<u8>, territory: Option<[u32; 3]>) -> String {
    let mut c = card.cost_with(1, pawn_cost);
    if let Some([k1, k2, k3]) = territory {
        // Gebietsmodus: bezahlt wird nur nach Kategorie, ohne Mindestzahl an Farben.
        c = crate::cards::Cost {
            color: k1.min(255) as u8,
            foreign_color: 0,
            k2: k2.min(255) as u8,
            k2_kinds: 0,
            k3: k3.min(255) as u8,
            k3_kinds: 0,
        };
    }
    let target = match card.target() {
        crate::cards::CardTarget::Player => "player",
        crate::cards::CardTarget::Unit => "unit",
    };
    format!(
        r#"{{"key":{},"name":{},"text":{},"target":{},"set":{},"per_range":{},"cost":{{"color":{},"foreign_color":{},"k2":{},"k2_kinds":{},"k3":{},"k3_kinds":{}}}}}"#,
        esc(card.key()),
        esc(card.name()),
        esc(card.text()),
        esc(target),
        card.is_set_piece(),
        card.priced_by_range() && territory.is_none(),
        c.color,
        c.foreign_color,
        c.k2,
        c.k2_kinds,
        c.k3,
        c.k3_kinds
    )
}

/// Rohstoffmengen als `[[typ, menge], ...]`.
fn inv_json(inv: &Inventory) -> String {
    list(inv.iter(), |(r, n)| format!("[{},{}]", r.0, n))
}

pub fn action_json(action: &Action) -> String {
    match action {
        Action::Move { piece, to } => {
            format!(r#"{{"type":"move","piece":{},"to":{}}}"#, piece.0, to.0)
        }
        Action::BuildFactory { pay } => format!(r#"{{"type":"build","pay":{}}}"#, pay.0),
        Action::SpawnPiece { factory } => {
            format!(r#"{{"type":"spawn","factory":{}}}"#, factory.0)
        }
        Action::BuildUnit { factory, level } => {
            format!(
                r#"{{"type":"build_unit","factory":{},"level":{}}}"#,
                factory.0, level
            )
        }
        Action::Pass => r#"{"type":"pass"}"#.to_string(),
        Action::BuyCard { card, from, target } => format!(
            r#"{{"type":"buy","card":{},"from":{},"target":{}}}"#,
            esc(card.key()),
            source_json(from),
            opt_piece(target)
        ),
    }
}

fn outcome_json(outcome: Outcome) -> String {
    match outcome {
        Outcome::Winner(p) => format!(r#"{{"type":"winner","player":{}}}"#, p.0),
        Outcome::Points(p) => format!(r#"{{"type":"points","player":{}}}"#, p.0),
        Outcome::Draw => r#"{"type":"draw"}"#.to_string(),
    }
}

pub fn event_json(event: &Event) -> String {
    match event {
        Event::Moved { piece, from, to } => format!(
            r#"{{"type":"moved","piece":{},"from":{},"to":{}}}"#,
            piece.0, from.0, to.0
        ),
        Event::Captured {
            piece,
            owner,
            at,
            by,
        } => format!(
            r#"{{"type":"captured","piece":{},"owner":{},"at":{},"by":{}}}"#,
            piece.0, owner.0, at.0, by.0
        ),
        Event::FactoryBuilt {
            factory,
            owner,
            cell,
        } => format!(
            r#"{{"type":"factory_built","factory":{},"owner":{},"cell":{}}}"#,
            factory.0, owner.0, cell.0
        ),
        Event::FactoryDestroyed {
            factory,
            owner,
            cell,
            by,
            loot,
        } => format!(
            r#"{{"type":"factory_destroyed","factory":{},"owner":{},"cell":{},"by":{},"loot":{}}}"#,
            factory.0,
            owner.0,
            cell.0,
            by.0,
            inv_json(loot)
        ),
        Event::Spawned {
            piece,
            owner,
            factory,
            cell,
            level,
            range,
        } => format!(
            r#"{{"type":"spawned","piece":{},"owner":{},"factory":{},"cell":{},"level":{},"range":{}}}"#,
            piece.0, owner.0, factory.0, cell.0, level, range
        ),
        Event::Collected {
            player,
            cell,
            gained,
        } => format!(
            r#"{{"type":"collected","player":{},"cell":{},"gained":{}}}"#,
            player.0,
            cell.0,
            inv_json(gained)
        ),
        Event::Unloaded {
            player,
            factory,
            amount,
        } => format!(
            r#"{{"type":"unloaded","player":{},"factory":{},"amount":{}}}"#,
            player.0,
            factory.0,
            inv_json(amount)
        ),
        Event::FactoryYield { factory, gained } => format!(
            r#"{{"type":"factory_yield","factory":{},"gained":{}}}"#,
            factory.0,
            inv_json(gained)
        ),
        Event::PlayerEliminated { player } => {
            format!(r#"{{"type":"eliminated","player":{}}}"#, player.0)
        }
        Event::Passed { player } => format!(r#"{{"type":"passed","player":{}}}"#, player.0),
        Event::Scored { scores } => {
            let list: Vec<String> = scores.iter().map(u32::to_string).collect();
            format!(r#"{{"type":"scored","scores":[{}]}}"#, list.join(","))
        }
        Event::GameOver { outcome } => format!(
            r#"{{"type":"game_over","outcome":{}}}"#,
            outcome_json(*outcome)
        ),
        Event::CardBought {
            player,
            card,
            from,
            target,
            paid,
        } => format!(
            r#"{{"type":"card_bought","player":{},"card":{},"from":{},"target":{},"paid":{}}}"#,
            player.0,
            esc(card.key()),
            source_json(from),
            opt_piece(target),
            inv_json(paid)
        ),
        Event::Recruited {
            piece,
            owner,
            factory,
            cell,
        } => format!(
            r#"{{"type":"recruited","piece":{},"owner":{},"factory":{},"cell":{}}}"#,
            piece.0, owner.0, factory.0, cell.0
        ),
        Event::DepositCreated { cell, resource } => format!(
            r#"{{"type":"deposit_created","cell":{},"resource":{}}}"#,
            cell.0, resource.0
        ),
        Event::Painted { player, cells } => format!(
            r#"{{"type":"painted","player":{},"cells":{}}}"#,
            player.0,
            list(cells.iter(), |c| c.0.to_string())
        ),
        Event::Enclosed { player, cells } => format!(
            r#"{{"type":"enclosed","player":{},"cells":{}}}"#,
            player.0,
            list(cells.iter(), |c| c.0.to_string())
        ),
        Event::Reconnected { piece, owner, cell } => format!(
            r#"{{"type":"reconnected","piece":{},"owner":{},"cell":{}}}"#,
            piece.0, owner.0, cell.0
        ),
        Event::CutOff { from, to, cells } => format!(
            r#"{{"type":"cut_off","from":{},"to":{},"cells":{}}}"#,
            from.0,
            to.map_or("null".to_string(), |p| p.0.to_string()),
            list(cells.iter(), |c| c.0.to_string())
        ),
        Event::Income {
            player,
            area,
            gained,
        } => format!(
            r#"{{"type":"income","player":{},"area":{},"gained":{}}}"#,
            player.0,
            area,
            inv_json(gained)
        ),
    }
}

/// Der veraenderliche Teil des Spielstands (ohne Karte).
pub fn state_json(game: &Game) -> String {
    let st = &game.state;
    let perks = |pk: &crate::state::Perks| {
        format!(
            r#""perks":{{"diagonal":{},"extra_range":{},"card_levels":{},"lance":{},"spring":{},"horse":{}}}"#,
            pk.diagonal, pk.extra_range, pk.card_levels, pk.lance, pk.spring, pk.horse
        )
    };
    let pieces = list(st.pieces.values(), |p| match &p.kind {
        PieceKind::King => format!(
            r#"{{"id":{},"owner":{},"cell":{},"king":true,{}}}"#,
            p.id.0,
            p.owner.0,
            p.cell.0,
            perks(&p.perks)
        ),
        PieceKind::Unit {
            level,
            range,
            colors,
        } => format!(
            r#"{{"id":{},"owner":{},"cell":{},"king":false,"level":{},"range":{},"colors":{},{}}}"#,
            p.id.0,
            p.owner.0,
            p.cell.0,
            level,
            range,
            list(colors.iter(), |c| c.0.to_string()),
            perks(&p.perks)
        ),
    });
    let player_perks = list(st.player_perks.iter(), |pp| {
        format!(
            r#"{{"mining":{},"farming":{},"runes":{},"shards":{},"cards_bought":{}}}"#,
            pp.mining, pp.farming, pp.runes, pp.shards, pp.cards_bought
        )
    });
    let factories = list(st.factories.values(), |f| {
        // Stufe der Figur, die beim Aufloesen jetzt entstuende (0 = keine Farbe), und der
        // Ausbau-Fortschritt: Einheiten und Typen ab Kat 2.
        let level = game.spawn_preview(f).map_or(0, |(l, _, _)| l);
        let (units, kinds) = game.upgrade_stock(&f.storage);
        format!(
            r#"{{"id":{},"owner":{},"cell":{},"storage":{},"level":{},"upgrade":{},"upgrade_units":{},"upgrade_kinds":{}}}"#,
            f.id.0,
            f.owner.0,
            f.cell.0,
            inv_json(&f.storage),
            level,
            game.upgrade_level(&f.storage),
            units,
            kinds
        )
    });
    let loot = list(st.loot.iter(), |(cell, inv)| {
        format!(r#"{{"cell":{},"items":{}}}"#, cell.0, inv_json(inv))
    });
    let territory = if game.config.territory {
        let connected = (0..game.config.players).map(|p| game.connected_count(PlayerId(p)));
        format!(
            r#","land":{},"area":{},"stored_card":{},"connected":{}"#,
            list(st.land.iter(), |o| o
                .map_or("-1".to_string(), |p| p.0.to_string())),
            list(st.area.iter(), |n| n.to_string()),
            list(st.stored_card.iter(), |c| c
                .map_or("null".to_string(), |c| esc(c.key()))),
            list(connected, |n| n.to_string())
        )
    } else {
        String::new()
    };
    format!(
        r#""round":{},"current":{},"alive":{},"pieces":{},"factories":{},"loot":{},"carried":{},"depleted":{},"regrow":{},"harvested":{},"player_perks":{},"bought_this_turn":{},"extra_actions":{},"charge":{},"extra_deposits":{}{}"#,
        st.round,
        st.current.0,
        list(st.alive.iter(), |a| a.to_string()),
        pieces,
        factories,
        loot,
        list(st.carried.iter(), inv_json),
        list(st.depleted.iter(), |(c, r)| format!("[{},{}]", c.0, r.0)),
        list(st.regrow.iter(), |((c, r), back)| format!(
            "[{},{},{}]",
            c.0, r.0, back
        )),
        list(st.harvested.iter(), |((c, r), n)| format!(
            "[{},{},{}]",
            c.0, r.0, n
        )),
        player_perks,
        st.bought_this_turn,
        st.extra_actions,
        st.charge,
        list(st.extra_deposits.iter(), |(c, rs)| format!(
            "[{},{}]",
            c.0,
            list(rs.iter(), |r| r.0.to_string())
        )),
        territory
    )
}

/// Kopf der Partie ohne umschliessende Klammern: Format, Karte, Rohstoffe, Einstellungen.
pub fn header_json(game: &Game, bots: &[String]) -> String {
    let c = &game.config;
    let m = &game.map;
    let board = format!(
        r#"{{"kind":{},"side":{},"cells":{},"corners":{},"shapes":{},"outline":{}}}"#,
        esc(m.board.kind().name()),
        m.board.side(),
        list(m.board.cells(), |cell| point(m.board.pos(cell))),
        list(m.board.corners().iter(), |cell| cell.0.to_string()),
        list(m.board.cells(), |cell| list(
            m.board.shape(cell).iter(),
            |&p| point(p)
        )),
        list(m.board.outline().iter(), |&p| point(p))
    );
    let types = list(m.resource_types.iter(), |t| {
        format!(
            r#"{{"id":{},"category":{},"home":{}}}"#,
            t.id.0,
            t.category,
            t.home_corner.map_or("null".to_string(), |c| c.to_string())
        )
    });
    let config = format!(
        r#"{{"side":{},"max_rounds":{},"score_after":{},"min_factory_distance":{},"deposit_stock":{},"yield_per_round":{},"cat1_distance":{},"regrow_rounds":{},"upgrade_region_min":{},"upgrade_step":{},"max_upgrades":{},"market_size":{},"kat_roles":{},"pawn_cost":{},"territory":{},"terr_pawn":{},"terr_area":{},"terr_area_step":{},"terr_k1":{},"terr_k2":{},"terr_k3":{},"terr_yield":{},"terr_cut_flips":{},"terr_enclose":{},"terr_enclose_edge":{},"terr_reconnect":{},"terr_card_mult":{}}}"#,
        c.side,
        c.max_rounds,
        c.score_after,
        c.min_factory_distance,
        c.deposit_stock,
        c.yield_per_round,
        c.cat1_distance,
        list(c.regrow_rounds.iter(), |n| n.to_string()),
        c.upgrade_region_min,
        c.upgrade_step,
        c.max_upgrades,
        c.market_size,
        c.kat_roles,
        c.pawn_cost,
        c.territory,
        c.terr_pawn,
        c.terr_area,
        c.terr_area_step,
        c.terr_k1,
        c.terr_k2,
        c.terr_k3,
        c.terr_yield,
        c.terr_cut_flips,
        c.terr_enclose,
        c.terr_enclose_edge,
        c.terr_reconnect,
        c.terr_card_mult
    );
    format!(
        r#""format":{},"version":{},"seed":{},"players":{},"bots":{},"config":{},"board":{},"resource_types":{},"deposits":{},"territories":{},"player_corners":{},"market":{}"#,
        esc(FORMAT),
        VERSION,
        c.seed,
        c.players,
        list(bots.iter(), |b| esc(b)),
        config,
        board,
        types,
        list(m.deposits.iter(), |d| list(d.iter(), |r| r.0.to_string())),
        list(m.territories.iter(), |t| t
            .map_or("-1".to_string(), |i| i.to_string())),
        list(m.player_corners.iter(), |i| i.to_string()),
        list(game.state.market.iter(), |c| card_json(
            c,
            game.roles_pawn_cost(),
            game.config.territory.then(|| game.territory_card_price(*c))
        ))
    )
}

/// Zeichnet eine Partie Zug fuer Zug auf.
pub struct ReplayWriter {
    head: String,
    frames: Vec<String>,
}

impl ReplayWriter {
    /// Beginnt die Aufzeichnung mit dem aktuellen Zustand als Frame 0.
    pub fn new(game: &Game, bots: &[String]) -> Self {
        let first = format!(
            r#"{{"turn":0,"player":null,"action":null,"events":[],{}}}"#,
            state_json(game)
        );
        ReplayWriter {
            head: header_json(game, bots),
            frames: vec![first],
        }
    }

    /// Haengt den Zustand nach einem Zug an.
    pub fn record(&mut self, game: &Game, player: PlayerId, action: &Action, events: &[Event]) {
        let frame = format!(
            r#"{{"turn":{},"player":{},"action":{},"events":{},{}}}"#,
            self.frames.len(),
            player.0,
            action_json(action),
            list(events.iter(), event_json),
            state_json(game)
        );
        self.frames.push(frame);
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Fertiges JSON-Dokument.
    pub fn finish(self, game: &Game) -> String {
        let outcome = game.outcome().map_or("null".to_string(), outcome_json);
        format!(
            "{{{},\"outcome\":{},\"frames\":[\n{}\n]}}\n",
            self.head,
            outcome,
            self.frames.join(",\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot::{play_out, Bot, GreedyBot, RandomBot};
    use crate::config::Config;

    use crate::json_check::validate;

    fn record(players: u8, seed: u64, kinds: &[&str]) -> (Game, String, usize) {
        let mut config = Config::new(players, seed);
        config.max_rounds = 120;
        let mut game = Game::new(config).unwrap();
        let names: Vec<String> = kinds.iter().map(|k| k.to_string()).collect();
        let mut writer = ReplayWriter::new(&game, &names);
        let mut bots: Vec<Box<dyn Bot>> = kinds
            .iter()
            .enumerate()
            .map(|(i, k)| match *k {
                "zufall" => Box::new(RandomBot::new(seed + i as u64)) as Box<dyn Bot>,
                _ => Box::new(GreedyBot::new(seed + i as u64)) as Box<dyn Bot>,
            })
            .collect();
        play_out(&mut game, &mut bots, |g, p, a, e| writer.record(g, p, a, e));
        let frames = writer.frame_count();
        let json = writer.finish(&game);
        (game, json, frames)
    }

    #[test]
    fn the_validator_itself_accepts_good_and_rejects_bad_json() {
        assert!(validate(r#"{"a":[1,-2,3.5,true,false,null,"x\"y"],"b":{}}"#).is_ok());
        assert!(validate("[]").is_ok());
        assert!(validate(r#"{"a":1,}"#).is_err());
        assert!(validate(r#"{"a" 1}"#).is_err());
        assert!(validate("[1,2").is_err());
        assert!(validate("{} x").is_err());
        assert!(validate("\"offen").is_err());
    }

    #[test]
    fn numbers_are_short_and_valid() {
        assert_eq!(num(3.0), "3");
        assert_eq!(num(1.5), "1.5");
        assert_eq!(num(-0.25), "-0.25");
        assert_eq!(num(-0.00001), "0");
        assert_eq!(num(0.866_025_4), "0.866");
        assert_eq!(point((2.0, 0.5)), "[2,0.5]");
    }

    #[test]
    fn hex_and_triangle_games_export_their_shapes() {
        for (players, kind) in [(3u8, "triangle"), (6, "hex")] {
            let (game, json, _) = record(players, 4, &vec!["gierig"; players as usize]);
            validate(&json).unwrap();
            assert!(json.contains(&format!(r#""kind":"{kind}""#)));
            assert!(json.contains(r#""shapes":[[["#));
            assert!(json.contains(r#""outline":[["#));
            assert!(game.map.board.len() > 50);
        }
    }

    #[test]
    fn strings_are_escaped() {
        assert_eq!(esc(r#"a"b\c"#), r#""a\"b\\c""#);
        assert_eq!(esc("x\ny"), r#""x\ny""#);
        assert_eq!(esc("\u{1}"), r#""\u0001""#);
        assert!(validate(&esc("\"\\\n\t\u{2}")).is_ok());
    }

    #[test]
    fn a_recorded_game_is_valid_json_with_one_frame_per_turn_plus_the_start() {
        for (players, kinds) in [(2u8, vec!["gierig", "zufall"]), (4, vec!["gierig"; 4])] {
            let (game, json, frames) = record(players, 5, &kinds);
            validate(&json).unwrap_or_else(|e| panic!("{players} Spieler: {e}"));
            assert_eq!(frames, game.state.turns_played as usize + 1);
            assert_eq!(json.matches("\"turn\":").count(), frames);
            assert!(json.starts_with("{\"format\":\"spacevitan-replay\",\"version\":1,"));
            assert!(json.contains("\"outcome\":{\"type\":"));
        }
    }

    #[test]
    fn the_start_frame_shows_only_kings_in_their_corners() {
        let config = Config::new(2, 3);
        let game = Game::new(config).unwrap();
        let writer = ReplayWriter::new(&game, &["a".into(), "b".into()]);
        let json = writer.finish(&game);
        validate(&json).unwrap();
        let start0 = game.map.start_cell(PlayerId(0)).0;
        let start1 = game.map.start_cell(PlayerId(1)).0;
        assert!(json.contains(&format!(
            r#"{{"id":0,"owner":0,"cell":{start0},"king":true,"perks":{{"diagonal":false"#
        )));
        assert!(json.contains(&format!(
            r#"{{"id":1,"owner":1,"cell":{start1},"king":true,"perks":"#
        )));
        assert!(json.contains("\"outcome\":null"));
        assert!(json.contains(r#""factories":[]"#));
    }

    #[test]
    fn the_header_describes_board_types_and_deposits_completely() {
        let (game, json, _) = record(2, 8, &["gierig", "gierig"]);
        let n = game.map.board.len();
        assert!(json.contains(&r#""kind":"square","side":10"#.to_string()));
        assert!(json.matches("],[").count() >= n - 1);
        assert!(json.contains(r#""player_corners":[0,2]"#));
        assert!(json.contains(&r#""resource_types":[{"id":0,"category":1,"home":0}"#.to_string()));
    }

    #[test]
    fn actions_and_events_have_a_type_field() {
        use crate::ids::{CellId, FactoryId, PieceId, ResourceId};
        let actions = [
            Action::Move {
                piece: PieceId(1),
                to: CellId(2),
            },
            Action::BuildFactory { pay: ResourceId(0) },
            Action::SpawnPiece {
                factory: FactoryId(3),
            },
            Action::Pass,
        ];
        for a in actions {
            let j = action_json(&a);
            validate(&j).unwrap();
            assert!(j.starts_with("{\"type\":"));
        }
        let mut inv = Inventory::new();
        inv.add(ResourceId(4), 2);
        let events = [
            Event::Moved {
                piece: PieceId(1),
                from: CellId(1),
                to: CellId(2),
            },
            Event::Captured {
                piece: PieceId(1),
                owner: PlayerId(1),
                at: CellId(2),
                by: PlayerId(0),
            },
            Event::FactoryBuilt {
                factory: FactoryId(1),
                owner: PlayerId(0),
                cell: CellId(3),
            },
            Event::FactoryDestroyed {
                factory: FactoryId(1),
                owner: PlayerId(0),
                cell: CellId(3),
                by: PlayerId(1),
                loot: inv.clone(),
            },
            Event::Spawned {
                piece: PieceId(4),
                owner: PlayerId(0),
                factory: FactoryId(1),
                cell: CellId(3),
                level: 2,
                range: 3,
            },
            Event::Collected {
                player: PlayerId(0),
                cell: CellId(3),
                gained: inv.clone(),
            },
            Event::Unloaded {
                player: PlayerId(0),
                factory: FactoryId(1),
                amount: inv.clone(),
            },
            Event::FactoryYield {
                factory: FactoryId(1),
                gained: inv,
            },
            Event::PlayerEliminated {
                player: PlayerId(1),
            },
            Event::Passed {
                player: PlayerId(0),
            },
            Event::GameOver {
                outcome: Outcome::Winner(PlayerId(0)),
            },
            Event::GameOver {
                outcome: Outcome::Draw,
            },
        ];
        for e in &events {
            let j = event_json(e);
            validate(&j).unwrap_or_else(|err| panic!("{j}: {err}"));
            assert!(j.starts_with("{\"type\":"), "{j}");
        }
        assert!(event_json(&events[3]).contains(r#""loot":[[4,2]]"#));
    }
}
