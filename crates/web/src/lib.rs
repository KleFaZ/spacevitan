//! WebAssembly-Schnittstelle fuer den Browser.
//!
//! Hinein gehen nur Zahlen, heraus kommt JSON: Nach jedem Aufruf liegt das Ergebnis (oder
//! `{"error": "..."}`) im Ausgabepuffer, den JavaScript ueber `sv_out_ptr` und `sv_out_len`
//! aus dem Speicher liest. Rueckgabewert 0 = in Ordnung, 1 = Fehler.
//!
//! Platzarten: 0 = gierig, 1 = zufall, 2 = Mensch, 3 = stark (rechnet voraus), 4 = flaeche, 5 = sehr stark
//! (Gebietsmodus: Land und Rohstoffe).
//! `sv_new_game`: Spieler A ist ein Mensch, alle anderen haben die Art `opponent`
//! (2 = alle Spieler an einem Geraet). `sv_new_game_seats`: auch Spieler A frei waehlbar,
//! z. B. lauter Bots zum Zuschauen.
//!
//! Regeln (Nachwachsen, Wertung, Kartenzahl ...): `sv_in_reserve` + `sv_set_rules` stellen sie
//! als Text `name=wert,name=wert` fuer alle folgenden Partien ein (Namen wie bei `--rule` der
//! Kommandozeile, siehe `Config::set`).

use game_core::config::Config;
use game_core::session::{SeatKind, Session};
use std::cell::RefCell;

/// Version der Schnittstelle; die Seite prueft sie beim Laden.
pub const API_VERSION: u32 = 1;

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// Eingabepuffer fuer Text (Regeln): JavaScript holt sich mit `sv_in_reserve` Platz,
    /// schreibt die Bytes hinein und ruft dann die Funktion auf, die den Text liest.
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// Regeln fuer die naechsten Partien, als `name=wert,name=wert` (siehe `Config::set_all`).
    static RULES: RefCell<String> = const { RefCell::new(String::new()) };
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn finish(result: Result<String, String>) -> i32 {
    let (code, text) = match result {
        Ok(json) => (0, json),
        Err(e) => (1, format!("{{\"error\":{}}}", quote(&e))),
    };
    OUT.with(|o| *o.borrow_mut() = text.into_bytes());
    code
}

fn with_session(f: impl FnOnce(&mut Session) -> Result<String, String>) -> i32 {
    let result = SESSION.with(|cell| match cell.borrow_mut().as_mut() {
        Some(s) => f(s),
        None => Err("Es laeuft keine Partie.".into()),
    });
    finish(result)
}

fn seat_kind(code: u32) -> Result<SeatKind, String> {
    match code {
        0 => Ok(SeatKind::Greedy),
        1 => Ok(SeatKind::Random),
        2 => Ok(SeatKind::Human),
        3 => Ok(SeatKind::Strong),
        4 => Ok(SeatKind::Area),
        5 => Ok(SeatKind::VeryStrong),
        other => Err(format!("Unbekannte Gegnerart {other}")),
    }
}

fn new_game(
    players: u32,
    seed: u64,
    side: u32,
    seat_a: u32,
    opponent: u32,
) -> Result<String, String> {
    let players = u8::try_from(players).map_err(|_| format!("{players} Spieler sind zu viele"))?;
    if players == 0 {
        return Err("Mindestens ein Spieler".into());
    }
    let mut config = Config::new(players, seed);
    RULES.with(|r| config.set_all(&r.borrow()))?;
    if side != 0 {
        config.side = side;
    }
    let others = seat_kind(opponent)?;
    let mut seats = vec![others; players as usize];
    seats[0] = seat_kind(seat_a)?;
    let session = Session::new(config, &seats)?;
    let json = session.start_json();
    SESSION.with(|cell| *cell.borrow_mut() = Some(session));
    Ok(json)
}

#[no_mangle]
pub extern "C" fn sv_api_version() -> u32 {
    API_VERSION
}

/// Reserviert `len` Bytes im Eingabepuffer und gibt deren Adresse zurueck. JavaScript schreibt
/// den Text dorthin (UTF-8) und ruft danach z. B. `sv_set_rules(len)` auf.
#[no_mangle]
pub extern "C" fn sv_in_reserve(len: u32) -> *mut u8 {
    INPUT.with(|i| {
        let mut buf = i.borrow_mut();
        buf.clear();
        buf.resize(len as usize, 0);
        buf.as_mut_ptr()
    })
}

/// Stellt die Regeln fuer alle folgenden Partien ein: der Text im Eingabepuffer lautet
/// `name=wert,name=wert` (z. B. `regrow_cat1=0,market_size=6`). Leer = alle Voreinstellungen.
/// Namen und Zahlen werden gleich geprueft; ob die Werte zusammenpassen, zeigt erst die Partie.
#[no_mangle]
pub extern "C" fn sv_set_rules(len: u32) -> i32 {
    let result = (|| {
        let text = read_input(len)?;
        // Probelauf auf einer Wegwerf-Einstellung: so schlaegt ein Tippfehler hier fehl und
        // nicht erst beim Start.
        Config::new(2, 0).set_all(&text)?;
        RULES.with(|r| *r.borrow_mut() = text.clone());
        Ok(format!("{{\"rules\":{}}}", quote(&text)))
    })();
    finish(result)
}

fn read_input(len: u32) -> Result<String, String> {
    INPUT.with(|i| {
        let buf = i.borrow();
        let bytes = buf
            .get(..len as usize)
            .ok_or_else(|| "Text fehlt im Eingabepuffer".to_string())?;
        std::str::from_utf8(bytes)
            .map(|t| t.trim().to_string())
            .map_err(|_| "Text ist kein UTF-8".to_string())
    })
}

fn parse_numbers(text: &str, sep: char) -> Result<Vec<u64>, String> {
    text.split(sep)
        .map(|part| {
            part.trim()
                .parse::<u64>()
                .map_err(|_| format!("\"{}\" ist keine Zahl", part.trim()))
        })
        .collect()
}

/// Wie viele Spieler am Tisch sitzen muessen, damit niemand einen Vorteil hat. Der Text im
/// Eingabepuffer lautet `ecken,spieler,auffuellen` (z. B. `6,5,1`: Sechseck, 5 Spieler, mit
/// Auffuell-Bots); Antwort `{"players":m,"added":m-k,"fair":bool,"corners":[...]}`. Ohne
/// Auffuellen (`0`) bleibt es bei der verlangten Spielerzahl, die Ecken sind dann moeglichst
/// weit auseinander gewaehlt, aber nicht unbedingt fair.
#[no_mangle]
pub extern "C" fn sv_plan(len: u32) -> i32 {
    let result = (|| {
        let nums = parse_numbers(&read_input(len)?, ',')?;
        let [corners, players, fill] = nums[..] else {
            return Err("Erwartet: ecken,spieler,auffuellen".to_string());
        };
        let (n, k) = (corners as usize, players as usize);
        if !(3..=8).contains(&n) || k < 2 || k > n {
            return Err(format!(
                "{k} Spieler passen nicht auf ein Brett mit {n} Ecken"
            ));
        }
        let total = if fill != 0 {
            game_core::config::fair_player_count(n, k)
        } else {
            k
        };
        let ring = game_core::config::fair_corners(n, total);
        let list: Vec<String> = ring.iter().map(|c| c.to_string()).collect();
        Ok(format!(
            "{{\"players\":{total},\"added\":{},\"fair\":{},\"corners\":[{}]}}",
            total - k,
            game_core::config::corners_are_fair(n, &ring),
            list.join(",")
        ))
    })();
    finish(result)
}

/// Neue Partie mit frei gewaehlter Brettform und einzeln gewaehlten Plaetzen. Der Text im
/// Eingabepuffer lautet `seed|side|ecken|plaetze`: `side` 0 = Standard, `ecken` 0 = Brett nach
/// Spielerzahl, sonst 3 bis 8; `plaetze` sind die Platzarten (siehe oben) durch Kommas, ihre
/// Anzahl ist die Spielerzahl (Auffuell-Bots stehen schon darin, siehe `sv_plan`).
#[no_mangle]
pub extern "C" fn sv_new_game_list(len: u32) -> i32 {
    let result = (|| {
        let text = read_input(len)?;
        let parts: Vec<&str> = text.split('|').collect();
        let [seed, side, corners, seats] = parts[..] else {
            return Err("Erwartet: seed|side|ecken|plaetze".to_string());
        };
        let seed = parse_numbers(seed, ',')?[0];
        let side = parse_numbers(side, ',')?[0] as u32;
        let corners = parse_numbers(corners, ',')?[0];
        let seats = parse_numbers(seats, ',')?
            .into_iter()
            .map(|c| seat_kind(c as u32))
            .collect::<Result<Vec<_>, _>>()?;
        let players = u8::try_from(seats.len())
            .map_err(|_| format!("{} Spieler sind zu viele", seats.len()))?;
        let mut config = Config::new(players, seed);
        RULES.with(|r| config.set_all(&r.borrow()))?;
        if side != 0 {
            config.side = side;
        }
        config.board_corners =
            u8::try_from(corners).map_err(|_| "Eckenzahl zu gross".to_string())?;
        let session = Session::new(config, &seats)?;
        let json = session.start_json();
        SESSION.with(|cell| *cell.borrow_mut() = Some(session));
        Ok(json)
    })();
    finish(result)
}

/// Neue Partie. Der Seed wird in zwei 32-Bit-Haelften uebergeben; `side` 0 = Standard.
#[no_mangle]
pub extern "C" fn sv_new_game(
    players: u32,
    seed_lo: u32,
    seed_hi: u32,
    side: u32,
    opponent: u32,
) -> i32 {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    finish(new_game(players, seed, side, 2, opponent))
}

/// Wie `sv_new_game`, aber auch Spieler A frei waehlbar (`seat_a`), z. B. Bot gegen Bot.
#[no_mangle]
pub extern "C" fn sv_new_game_seats(
    players: u32,
    seed_lo: u32,
    seed_hi: u32,
    side: u32,
    seat_a: u32,
    others: u32,
) -> i32 {
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    finish(new_game(players, seed, side, seat_a, others))
}

/// Der Mensch am Zug fuehrt die Aktion mit diesem Index aus der Liste `legal` aus.
#[no_mangle]
pub extern "C" fn sv_apply(index: u32) -> i32 {
    with_session(|s| s.apply_index(index as usize))
}

/// Die Ziehfelder einer beliebigen Figur (auch einer gegnerischen): `{"piece":id,"cells":[...]}`.
/// Fuer die Anzeige "Ziehfelder zeigen"; aendert nichts am Spiel.
#[no_mangle]
pub extern "C" fn sv_destinations(piece: u32) -> i32 {
    with_session(|s| s.destinations_json(piece))
}

/// Der Bot am Zug zieht.
#[no_mangle]
pub extern "C" fn sv_bot_step() -> i32 {
    with_session(|s| s.bot_step())
}

#[no_mangle]
pub extern "C" fn sv_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn sv_out_len() -> u32 {
    OUT.with(|o| o.borrow().len() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out() -> String {
        let len = sv_out_len() as usize;
        let ptr = sv_out_ptr();
        // In den Tests liegt der Puffer im selben Prozess; im Browser liest JavaScript ihn.
        let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
        String::from_utf8(bytes.to_vec()).expect("UTF-8")
    }

    #[test]
    fn without_a_game_every_call_reports_an_error() {
        assert_eq!(sv_apply(0), 1);
        assert!(out().contains("keine Partie"));
        assert_eq!(sv_bot_step(), 1);
    }

    #[test]
    fn destinations_work_for_any_piece_and_do_not_change_the_game() {
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0);
        let start = out();
        // Figur 0 ist der Koenig von A; ein Koenig zieht in alle Nachbarfelder.
        assert_eq!(sv_destinations(0), 0, "{}", out());
        let mine = out();
        assert!(mine.starts_with("{\"piece\":0,\"cells\":["), "{mine}");
        assert_eq!(
            sv_destinations(1),
            0,
            "auch der gegnerische Koenig: {}",
            out()
        );
        assert_eq!(sv_destinations(999), 1);
        assert!(out().contains("gibt es nicht"));
        // Die Partie ist unveraendert: derselbe Mensch am Zug.
        assert_eq!(sv_apply(0), 0, "{start}");
    }

    #[test]
    fn a_new_game_returns_the_start_as_json() {
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0);
        let json = out();
        assert!(json.starts_with("{\"header\":{\"format\":\"spacevitan-replay\""));
        assert!(json.contains("\"seed\":7,"));
        assert!(json.contains("\"seats\":[\"mensch\",\"gierig\"]"));
        assert!(json.contains("\"human_to_move\":true"));
    }

    fn put(text: &str) -> u32 {
        let ptr = sv_in_reserve(text.len() as u32);
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), ptr, text.len()) };
        text.len() as u32
    }

    #[test]
    fn the_plan_fills_the_table_with_bots_until_every_corner_is_fair() {
        // Quadrat, 3 Spieler: ein Bot kommt dazu.
        assert_eq!(sv_plan(put("4,3,1")), 0);
        let json = out();
        assert!(
            json.contains("\"players\":4,\"added\":1,\"fair\":true"),
            "{json}"
        );
        // Ohne Auffuellen bleibt es bei 3 und ist nicht fair.
        assert_eq!(sv_plan(put("4,3,0")), 0);
        let json = out();
        assert!(
            json.contains("\"players\":3,\"added\":0,\"fair\":false"),
            "{json}"
        );
        // Sechseck: 3 und 4 gehen, 5 wird 6.
        for (k, want) in [(3, 3), (4, 4), (5, 6)] {
            assert_eq!(sv_plan(put(&format!("6,{k},1"))), 0);
            assert!(out().contains(&format!("\"players\":{want},")), "{k}");
        }
        assert_eq!(sv_plan(put("4,5,1")), 1);
        assert_eq!(sv_plan(put("6,3")), 1);
    }

    #[test]
    fn a_game_can_start_on_any_board_with_a_seat_per_player() {
        // Sechseck, drei Plaetze: Mensch, stark, flaeche.
        assert_eq!(sv_new_game_list(put("5|0|6|2,3,4")), 0, "{}", out());
        let json = out();
        assert!(
            json.contains("\"seats\":[\"mensch\",\"stark\",\"flaeche\"]"),
            "{json}"
        );
        assert!(json.contains("\"player_corners\":[0,2,4]"), "{json}");
        // Quadrat, vier Plaetze, das vierte ein Auffuell-Bot.
        assert_eq!(sv_new_game_list(put("5|0|4|2,0,0,0")), 0);
        assert_eq!(
            sv_new_game_list(put("5|0|4|2,0,0,0,0")),
            1,
            "5 Spieler auf 4 Ecken"
        );
        assert_eq!(sv_new_game_list(put("5|0|4")), 1);
    }

    #[test]
    fn the_seed_uses_both_halves() {
        assert_eq!(sv_new_game(2, 1, 1, 0, 0), 0);
        assert!(out().contains(&format!("\"seed\":{},", (1u64 << 32) + 1)));
    }

    #[test]
    fn invalid_settings_are_reported() {
        assert_eq!(sv_new_game(9, 1, 0, 0, 0), 1);
        assert!(out().contains("error"));
        assert_eq!(sv_new_game(2, 1, 0, 3, 0), 1, "Seitenlaenge 3 ist zu klein");
        assert_eq!(sv_new_game(2, 1, 0, 0, 9), 1);
        assert!(out().contains("Gegnerart"));
        assert_eq!(sv_new_game(0, 1, 0, 0, 0), 1);
        assert_eq!(sv_new_game(300, 1, 0, 0, 0), 1);
    }

    #[test]
    fn every_player_count_from_two_to_eight_starts() {
        for (players, kind) in [
            (2, "square"),
            (3, "triangle"),
            (4, "square"),
            (5, "hex"),
            (8, "hex"),
        ] {
            assert_eq!(sv_new_game(players, 3, 0, 0, 0), 0, "{players}: {}", out());
            assert!(out().contains(&format!("\"kind\":\"{kind}\"")));
        }
    }

    #[test]
    fn a_game_against_the_bot_runs_to_the_end() {
        assert_eq!(sv_new_game(2, 11, 0, 0, 0), 0);
        let mut steps = 0;
        loop {
            let last = out();
            if !last.contains("\"outcome\":null") {
                break;
            }
            let code = if last.contains("\"human_to_move\":true") {
                sv_apply(0)
            } else {
                sv_bot_step()
            };
            assert_eq!(code, 0, "{}", out());
            steps += 1;
            assert!(steps < 5000, "Partie endet nicht");
        }
        assert_eq!(sv_apply(0), 1);
        assert!(out().contains("vorbei"));
    }

    #[test]
    fn bots_can_play_against_each_other_to_the_end() {
        assert_eq!(sv_new_game_seats(2, 9, 0, 0, 0, 0), 0, "{}", out());
        assert!(out().contains("\"seats\":[\"gierig\",\"gierig\"]"));
        assert!(out().contains("\"human_to_move\":false"));
        assert_eq!(sv_apply(0), 1, "kein Mensch am Zug");
        let mut steps = 0;
        while !out().contains("\"outcome\":{") {
            assert_eq!(sv_bot_step(), 0, "{}", out());
            steps += 1;
            assert!(steps < 5000, "Partie endet nicht");
        }
        assert_eq!(sv_bot_step(), 1, "nach dem Ende zieht niemand mehr");
        assert_eq!(sv_new_game_seats(2, 9, 0, 0, 7, 0), 1);
        assert!(out().contains("Unbekannte Gegnerart 7"));
    }

    #[test]
    fn the_strong_bot_answers_a_human_move() {
        assert_eq!(sv_new_game(2, 5, 0, 0, 3), 0);
        assert!(out().contains("\"seats\":[\"mensch\",\"stark\"]"));
        assert_eq!(sv_apply(0), 0);
        assert!(out().contains("\"human_to_move\":false"));
        assert_eq!(sv_bot_step(), 0, "{}", out());
        assert!(out().contains("\"human_to_move\":true"));
    }

    #[test]
    fn the_very_strong_bot_takes_a_seat_with_code_five() {
        assert_eq!(sv_new_game(2, 5, 0, 0, 5), 0, "{}", out());
        assert!(
            out().contains("\"seats\":[\"mensch\",\"sehr-stark\"]"),
            "{}",
            out()
        );
        assert_eq!(sv_apply(0), 0);
        assert_eq!(sv_bot_step(), 0, "{}", out());
        assert!(out().contains("\"human_to_move\":true"));
        assert_eq!(sv_new_game(2, 5, 0, 0, 6), 1, "Code 6 gibt es nicht");
    }

    #[test]
    fn the_area_bot_can_take_a_seat_and_version_2_rules_reach_the_game() {
        assert_eq!(set_rules("territory=1,terr_v2=1"), 0, "{}", out());
        assert_eq!(sv_new_game_seats(2, 5, 0, 0, 4, 4), 0, "{}", out());
        let json = out();
        assert!(
            json.contains("\"seats\":[\"flaeche\",\"flaeche\"]"),
            "{json}"
        );
        assert!(json.contains("\"terr_ripen\":[3,10,0,0]"), "{json}");
        assert!(json.contains("\"terr_exact\":true"));
        let mut steps = 0;
        while !out().contains("\"outcome\":{") {
            assert_eq!(sv_bot_step(), 0, "{}", out());
            steps += 1;
            assert!(steps < 5000, "Partie endet nicht");
        }
        assert_eq!(set_rules(""), 0);
    }

    #[test]
    fn hotseat_lets_every_player_move_by_index() {
        assert_eq!(sv_new_game(2, 4, 0, 0, 2), 0);
        assert!(out().contains("\"seats\":[\"mensch\",\"mensch\"]"));
        assert_eq!(sv_apply(0), 0);
        assert!(
            out().contains("\"human_to_move\":true"),
            "B ist auch ein Mensch"
        );
        assert_eq!(sv_bot_step(), 1, "kein Bot am Tisch");
    }

    fn set_rules(text: &str) -> i32 {
        let ptr = sv_in_reserve(text.len() as u32);
        // Wie im Browser: erst Platz holen, dann die Bytes hineinschreiben.
        unsafe { std::ptr::copy_nonoverlapping(text.as_ptr(), ptr, text.len()) };
        sv_set_rules(text.len() as u32)
    }

    #[test]
    fn rules_reach_the_next_game() {
        assert_eq!(set_rules("regrow_cat1=0, market_size=6,score_after=40"), 0);
        assert!(out().starts_with("{\"rules\":\"regrow_cat1=0"), "{}", out());
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0, "{}", out());
        let json = out();
        assert!(json.contains("\"regrow_rounds\":[0,5,3,2]"), "{json}");
        assert!(json.contains("\"market_size\":6"));
        assert!(json.contains("\"score_after\":40"));
        // Die Regeln gelten weiter, bis sie ersetzt werden.
        assert_eq!(sv_new_game(2, 8, 0, 0, 0), 0);
        assert!(out().contains("\"market_size\":6"));
        assert_eq!(set_rules(""), 0);
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0);
        let json = out();
        assert!(json.contains("\"regrow_rounds\":[20,5,3,2]"));
        assert!(json.contains("\"market_size\":4"));
        assert!(json.contains("\"score_after\":100"));
    }

    #[test]
    fn rule_cards_can_be_switched_off() {
        assert_eq!(set_rules("market_size=0"), 0);
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0, "{}", out());
        assert!(out().contains("\"market\":[]"), "{}", out());
        assert_eq!(set_rules(""), 0);
    }

    #[test]
    fn wrong_rules_are_reported_and_keep_the_old_ones() {
        assert_eq!(set_rules("market_size=6"), 0);
        assert_eq!(set_rules("schach=1"), 1);
        assert!(out().contains("schach"));
        assert_eq!(set_rules("market_size=x"), 1);
        assert_eq!(set_rules("market_size=-2"), 1);
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0);
        assert!(
            out().contains("\"market_size\":6"),
            "die alten Regeln bleiben"
        );
        // Unpassende Werte meldet erst der Start der Partie.
        assert_eq!(set_rules("side=3"), 0);
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 1);
        assert_eq!(set_rules(""), 0);
    }

    #[test]
    fn errors_are_valid_json_even_with_quotes() {
        assert_eq!(finish(Err("a \"b\" \\ c\n".into())), 1);
        assert_eq!(out(), r#"{"error":"a \"b\" \\ c\u000a"}"#);
    }
}
