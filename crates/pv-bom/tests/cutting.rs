//! First-fit decreasing: properties on random cut lists, and the saw cut.

use pv_bom::cut;

/// SplitMix64.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[test]
fn every_piece_is_placed_once_and_no_bar_overflows() {
    for seed in 0..500u64 {
        let mut rng = Rng(seed);
        let n = 1 + (rng.next() % 80) as usize;
        let kerf = [0.0, 0.003, 0.005][(rng.next() % 3) as usize];
        // Pieces of 0.2 … 4.8 m, to the millimetre.
        let pieces: Vec<f64> = (0..n)
            .map(|_| (200 + rng.next() % 4601) as f64 / 1000.0)
            .collect();
        let plan = cut("R", &pieces, 4.8, kerf).unwrap();

        let mut placed: Vec<f64> = plan.bars.iter().flatten().copied().collect();
        let mut given = pieces.clone();
        placed.sort_by(f64::total_cmp);
        given.sort_by(f64::total_cmp);
        assert_eq!(placed, given, "seed {seed}: pieces lost or duplicated");
        for bar in &plan.bars {
            assert!(plan.used(bar) <= 4.8 + 1e-9, "seed {seed}: {bar:?}");
        }
        let lower = (pieces.iter().sum::<f64>() / 4.8 - 1e-9).ceil() as u64;
        assert!(plan.bar_count() >= lower, "seed {seed}");
        // FFD never uses more than about 11/9 of the optimum (+ 1); against the length
        // bound this is a loose sanity check.
        assert!(plan.bar_count() <= 2 * lower + 1, "seed {seed}");
        // Deterministic.
        assert_eq!(plan, cut("R", &pieces, 4.8, kerf).unwrap(), "seed {seed}");
    }
}

#[test]
fn the_saw_cut_is_taken_between_pieces() {
    // Two 2.40 m pieces fill a 4.80 m bar exactly, without saw cut only.
    assert_eq!(cut("R", &[2.4, 2.4], 4.8, 0.0).unwrap().bar_count(), 1);
    assert_eq!(cut("R", &[2.4, 2.4], 4.8, 0.005).unwrap().bar_count(), 2);
    // A piece of exactly the bar length is fine; a longer one is an error.
    assert_eq!(cut("R", &[4.8], 4.8, 0.005).unwrap().bar_count(), 1);
    let e = cut("R", &[4.81], 4.8, 0.0).unwrap_err();
    assert_eq!(e.key(), "errors.bom.pieceTooLong");
    assert_eq!(
        cut("R", &[1.0], 0.0, 0.0).unwrap_err().key(),
        "errors.bom.invalidStock"
    );
}
