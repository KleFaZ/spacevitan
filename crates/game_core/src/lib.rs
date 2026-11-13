//! Regel-Kern des Spiels (Arbeitstitel SpaceCivTan).
//!
//! Der Kern kennt weder Fenster noch Grafik: Zustand plus Aktion ergibt neuen Zustand.
//! Godot (Darstellung, Eingabe) und die Bots sprechen spaeter dieselbe Schnittstelle an.

pub mod action;
pub mod ai;
pub mod board;
pub mod bot;
pub mod cards;
pub mod config;
pub mod game;
pub mod ids;
pub mod map;
pub mod render;
pub mod replay;
pub mod rng;
pub mod state;
mod territory;

#[cfg(test)]
mod ai_tests;
#[cfg(test)]
mod cards_tests;
#[cfg(test)]
mod game_tests;
#[cfg(test)]
mod json_check;
#[cfg(test)]
mod roles_tests;
pub mod session;
#[cfg(test)]
mod territory_tests;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
