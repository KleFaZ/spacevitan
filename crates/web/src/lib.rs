//! WebAssembly-Schnittstelle fuer den Browser.
//!
//! Hinein gehen nur Zahlen, heraus kommt JSON: Nach jedem Aufruf liegt das Ergebnis (oder
//! `{"error": "..."}`) im Ausgabepuffer, den JavaScript ueber `sv_out_ptr` und `sv_out_len`
//! aus dem Speicher liest. Rueckgabewert 0 = in Ordnung, 1 = Fehler.
//!
//! Platzarten: 0 = gierig, 1 = zufall, 2 = Mensch, 3 = stark (rechnet voraus).
//! `sv_new_game`: Spieler A ist ein Mensch, alle anderen haben die Art `opponent`
//! (2 = alle Spieler an einem Geraet). `sv_new_game_seats`: auch Spieler A frei waehlbar,
//! z. B. lauter Bots zum Zuschauen.

use game_core::config::Config;
use game_core::session::{SeatKind, Session};
use std::cell::RefCell;

/// Version der Schnittstelle; die Seite prueft sie beim Laden.
pub const API_VERSION: u32 = 1;

thread_local! {
    static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
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
    fn a_new_game_returns_the_start_as_json() {
        assert_eq!(sv_new_game(2, 7, 0, 0, 0), 0);
        let json = out();
        assert!(json.starts_with("{\"header\":{\"format\":\"spacevitan-replay\""));
        assert!(json.contains("\"seed\":7,"));
        assert!(json.contains("\"seats\":[\"mensch\",\"gierig\"]"));
        assert!(json.contains("\"human_to_move\":true"));
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

    #[test]
    fn errors_are_valid_json_even_with_quotes() {
        assert_eq!(finish(Err("a \"b\" \\ c\n".into())), 1);
        assert_eq!(out(), r#"{"error":"a \"b\" \\ c\u000a"}"#);
    }
}
