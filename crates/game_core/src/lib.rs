//! Regel-Kern des Spiels (Arbeitstitel SpaceCivTan).
//!
//! Der Kern kennt weder Fenster noch Grafik: Zustand plus Aktion ergibt neuen Zustand.
//! Godot (Darstellung, Eingabe) und die Bots sprechen spaeter dieselbe Schnittstelle an.

pub mod rng;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
