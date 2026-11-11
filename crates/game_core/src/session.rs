//! Spiel-Sitzung fuer Oberflaechen: Menschen und Bots an einem Tisch.
//!
//! Alle Ausgaben sind JSON im Format der Wiedergabe (`replay.rs`), damit dieselbe Darstellung
//! laufende Partien und Wiedergaben zeigt. Ein Mensch waehlt seine Aktion per Index aus der
//! zuletzt gelieferten Liste erlaubter Aktionen; die Schnittstelle muss deshalb kein JSON lesen.

use crate::action::{Action, Event};
use crate::bot::{Bot, GreedyBot, RandomBot};
use crate::config::Config;
use crate::game::Game;
use crate::ids::PlayerId;
use crate::replay::{action_json, event_json, header_json, state_json};
use crate::state::Outcome;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatKind {
    Human,
    Greedy,
    Random,
}

impl SeatKind {
    pub fn name(self) -> &'static str {
        match self {
            SeatKind::Human => "mensch",
            SeatKind::Greedy => "gierig",
            SeatKind::Random => "zufall",
        }
    }
}

pub struct Session {
    game: Game,
    seats: Vec<SeatKind>,
    bots: Vec<Option<Box<dyn Bot>>>,
    legal: Vec<Action>,
    turns: usize,
}

fn json_list<T>(items: impl IntoIterator<Item = T>, f: impl Fn(T) -> String) -> String {
    let parts: Vec<String> = items.into_iter().map(f).collect();
    format!("[{}]", parts.join(","))
}

fn outcome_json(outcome: Option<Outcome>) -> String {
    match outcome {
        None => "null".into(),
        Some(Outcome::Winner(p)) => format!(r#"{{"type":"winner","player":{}}}"#, p.0),
        Some(Outcome::Draw) => r#"{"type":"draw"}"#.into(),
    }
}

impl Session {
    /// Neue Partie. `seats[i]` legt fest, wer Spieler `i` steuert.
    pub fn new(config: Config, seats: &[SeatKind]) -> Result<Session, String> {
        if seats.len() != config.players as usize {
            return Err(format!(
                "{} Plaetze angegeben, aber {} Spieler eingestellt",
                seats.len(),
                config.players
            ));
        }
        let seed = config.seed;
        let game = Game::new(config).map_err(|e| e.to_string())?;
        let bots = seats
            .iter()
            .enumerate()
            .map(|(i, kind)| {
                let bot_seed = seed.wrapping_mul(7919).wrapping_add(i as u64);
                match kind {
                    SeatKind::Human => None,
                    SeatKind::Greedy => Some(Box::new(GreedyBot::new(bot_seed)) as Box<dyn Bot>),
                    SeatKind::Random => Some(Box::new(RandomBot::new(bot_seed)) as Box<dyn Bot>),
                }
            })
            .collect();
        let legal = game.legal_actions();
        Ok(Session {
            game,
            seats: seats.to_vec(),
            bots,
            legal,
            turns: 0,
        })
    }

    pub fn game(&self) -> &Game {
        &self.game
    }

    pub fn legal(&self) -> &[Action] {
        &self.legal
    }

    pub fn is_over(&self) -> bool {
        self.game.outcome().is_some()
    }

    /// Ist gerade ein Mensch am Zug (und die Partie nicht vorbei)?
    pub fn human_to_move(&self) -> bool {
        !self.is_over() && self.seats[self.game.current_player().idx()] == SeatKind::Human
    }

    fn frame_json(
        &self,
        player: Option<PlayerId>,
        action: Option<&Action>,
        events: &[Event],
    ) -> String {
        format!(
            r#"{{"turn":{},"player":{},"action":{},"events":{},{}}}"#,
            self.turns,
            player.map_or("null".to_string(), |p| p.0.to_string()),
            action.map_or("null".to_string(), action_json),
            json_list(events.iter(), event_json),
            state_json(&self.game)
        )
    }

    fn tail_json(&self) -> String {
        format!(
            r#""legal":{},"human_to_move":{},"outcome":{}"#,
            json_list(self.legal.iter(), action_json),
            self.human_to_move(),
            outcome_json(self.game.outcome())
        )
    }

    /// Alles fuer den Start: Kopf (wie in der Wiedergabe), Plaetze, Startzustand, erlaubte Aktionen.
    pub fn start_json(&self) -> String {
        let names: Vec<String> = self.seats.iter().map(|s| s.name().to_string()).collect();
        format!(
            r#"{{"header":{{{},"outcome":null,"frames":[]}},"seats":{},"frame":{},{}}}"#,
            header_json(&self.game, &names),
            json_list(names.iter(), |n| format!("\"{n}\"")),
            self.frame_json(None, None, &[]),
            self.tail_json()
        )
    }

    fn step(&mut self, action: Action) -> Result<String, String> {
        let player = self.game.current_player();
        let events = self.game.apply(action).map_err(|e| e.to_string())?;
        self.turns += 1;
        self.legal = self.game.legal_actions();
        Ok(format!(
            r#"{{"frame":{},{}}}"#,
            self.frame_json(Some(player), Some(&action), &events),
            self.tail_json()
        ))
    }

    /// Ein Mensch fuehrt die Aktion mit diesem Index aus der Liste `legal` aus.
    pub fn apply_index(&mut self, index: usize) -> Result<String, String> {
        if self.is_over() {
            return Err("Die Partie ist vorbei.".into());
        }
        if !self.human_to_move() {
            return Err("Gerade ist kein Mensch am Zug.".into());
        }
        let Some(&action) = self.legal.get(index) else {
            return Err(format!(
                "Aktion {index} gibt es nicht (erlaubt: 0 bis {}).",
                self.legal.len().saturating_sub(1)
            ));
        };
        self.step(action)
    }

    /// Der Bot, der gerade am Zug ist, zieht.
    pub fn bot_step(&mut self) -> Result<String, String> {
        if self.is_over() {
            return Err("Die Partie ist vorbei.".into());
        }
        if self.human_to_move() {
            return Err("Ein Mensch ist am Zug.".into());
        }
        let current = self.game.current_player().idx();
        let action = self.bots[current]
            .as_mut()
            .expect("an einem Bot-Platz sitzt ein Bot")
            .choose(&self.game);
        self.step(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_check::validate;
    use crate::rng::Rng;

    fn session(players: u8, seed: u64, seats: &[SeatKind]) -> Session {
        let mut config = Config::new(players, seed);
        config.max_rounds = 150;
        Session::new(config, seats).expect("Sitzung")
    }

    #[test]
    fn the_start_is_valid_json_and_the_human_moves_first() {
        let s = session(2, 1, &[SeatKind::Human, SeatKind::Greedy]);
        let json = s.start_json();
        validate(&json).unwrap();
        assert!(s.human_to_move());
        assert!(json.contains(r#""human_to_move":true"#));
        assert!(json.contains(r#""seats":["mensch","gierig"]"#));
        assert!(json.contains(r#""format":"spacevitan-replay""#));
        assert!(json.contains(r#""legal":[{"type":"move""#));
        assert_eq!(s.legal().len(), 2, "Koenig in der Ecke hat zwei Zuege");
    }

    #[test]
    fn the_number_of_seats_must_match_the_players() {
        let config = Config::new(4, 1);
        assert!(Session::new(config, &[SeatKind::Human, SeatKind::Greedy]).is_err());
        let config = Config::new(3, 1);
        assert!(
            Session::new(config, &[SeatKind::Human; 3]).is_err(),
            "3 Spieler noch nicht"
        );
    }

    #[test]
    fn bad_indices_and_wrong_turns_are_rejected_without_changing_anything() {
        let mut s = session(2, 1, &[SeatKind::Human, SeatKind::Greedy]);
        let before = s.game().state.clone();
        assert!(s.apply_index(99).unwrap_err().contains("gibt es nicht"));
        assert!(s.bot_step().unwrap_err().contains("Mensch"));
        assert_eq!(s.game().state, before);

        let out = s.apply_index(0).unwrap();
        validate(&out).unwrap();
        assert!(!s.human_to_move());
        assert!(out.contains(r#""human_to_move":false"#));
        assert!(s.apply_index(0).unwrap_err().contains("kein Mensch"));
        let out = s.bot_step().unwrap();
        validate(&out).unwrap();
        assert!(s.human_to_move());
    }

    #[test]
    fn a_whole_game_with_a_human_can_be_played_through_the_interface() {
        for (players, seed) in [(2u8, 3u64), (4, 5)] {
            let mut seats = vec![SeatKind::Greedy; players as usize];
            seats[0] = SeatKind::Human;
            let mut s = session(players, seed, &seats);
            let mut rng = Rng::new(seed);
            let mut turns = 0;
            while !s.is_over() {
                let out = if s.human_to_move() {
                    let i = rng.below(s.legal().len());
                    s.apply_index(i).unwrap()
                } else {
                    s.bot_step().unwrap()
                };
                turns += 1;
                validate(&out).unwrap_or_else(|e| panic!("Zug {turns}: {e}"));
                assert!(out.contains(&format!(r#""turn":{turns},"#)));
                s.game().check_invariants().unwrap();
            }
            assert!(s.legal().is_empty());
            assert!(s.apply_index(0).unwrap_err().contains("vorbei"));
            assert!(s.bot_step().unwrap_err().contains("vorbei"));
        }
    }

    #[test]
    fn a_table_of_bots_plays_on_its_own() {
        let mut s = session(2, 9, &[SeatKind::Greedy, SeatKind::Random]);
        assert!(!s.human_to_move());
        while !s.is_over() {
            s.bot_step().unwrap();
        }
        assert!(s.game().outcome().is_some());
    }

    #[test]
    fn the_last_step_reports_the_outcome() {
        let mut s = session(2, 2, &[SeatKind::Greedy, SeatKind::Random]);
        let mut last = String::new();
        while !s.is_over() {
            last = s.bot_step().unwrap();
        }
        assert!(last.contains(r#""outcome":{"type":"#), "{last}");
        assert!(last.contains(r#""legal":[]"#));
    }
}
