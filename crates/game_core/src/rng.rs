//! Deterministischer Zufallsgenerator (SplitMix64).
//!
//! Gleicher Seed ergibt auf jeder Plattform dieselbe Folge. Das ist die Grundlage fuer
//! reproduzierbare Karten, Server-seitige Zugpruefung und Tests.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Gleichverteilt in `0..n`, ohne Modulo-Verzerrung. `n` muss groesser als 0 sein.
    pub fn below(&mut self, n: usize) -> usize {
        assert!(n > 0, "Rng::below(0) ist nicht definiert");
        let n = n as u64;
        let zone = u64::MAX - (u64::MAX % n);
        loop {
            let v = self.next_u64();
            if v < zone {
                return (v % n) as usize;
            }
        }
    }

    /// Wahr mit Wahrscheinlichkeit `p` (0.0 = nie, 1.0 = immer).
    pub fn chance(&mut self, p: f64) -> bool {
        let unit = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        unit < p
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }

    /// Unabhaengiger Teilstrom, z. B. je Generator-Versuch. Verbraucht einen Wert dieses Stroms.
    pub fn fork(&mut self, label: u64) -> Rng {
        let base = self.next_u64();
        Rng::new(base ^ label.wrapping_mul(0xD6E8_FEB8_6659_FD93))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_splitmix64_reference_vector_for_seed_zero() {
        let mut rng = Rng::new(0);
        assert_eq!(rng.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(rng.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(rng.next_u64(), 0x06C4_5D18_8009_454F);
    }

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_give_different_sequences() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_stays_in_range_and_hits_every_value() {
        let mut rng = Rng::new(7);
        let mut seen = [0u32; 10];
        for _ in 0..10_000 {
            let v = rng.below(10);
            assert!(v < 10);
            seen[v] += 1;
        }
        assert!(seen.iter().all(|&c| c > 800), "Verteilung auffaellig: {seen:?}");
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut rng = Rng::new(3);
        for _ in 0..50 {
            assert_eq!(rng.below(1), 0);
        }
    }

    #[test]
    #[should_panic]
    fn below_zero_panics() {
        Rng::new(1).below(0);
    }

    #[test]
    fn chance_respects_the_extremes() {
        let mut rng = Rng::new(5);
        assert!((0..1000).all(|_| !rng.chance(0.0)));
        assert!((0..1000).all(|_| rng.chance(1.0)));
    }

    #[test]
    fn chance_is_roughly_calibrated() {
        let mut rng = Rng::new(11);
        let hits = (0..20_000).filter(|_| rng.chance(0.25)).count();
        assert!((4_500..5_500).contains(&hits), "hits = {hits}");
    }

    #[test]
    fn shuffle_keeps_all_elements() {
        let mut rng = Rng::new(9);
        let mut v: Vec<u32> = (0..50).collect();
        rng.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<u32>>());
        assert_ne!(v, (0..50).collect::<Vec<u32>>(), "Mischen sollte etwas aendern");
    }

    #[test]
    fn pick_on_empty_is_none() {
        let mut rng = Rng::new(1);
        let empty: [u8; 0] = [];
        assert!(rng.pick(&empty).is_none());
        assert_eq!(rng.pick(&[5u8]), Some(&5));
    }

    #[test]
    fn forks_with_different_labels_diverge() {
        let mut a = Rng::new(100);
        let mut b = Rng::new(100);
        let mut fa = a.fork(1);
        let mut fb = b.fork(2);
        assert_ne!(fa.next_u64(), fb.next_u64());
    }

    #[test]
    fn forks_are_reproducible() {
        let mut a = Rng::new(100);
        let mut b = Rng::new(100);
        assert_eq!(a.fork(3), b.fork(3));
    }
}
