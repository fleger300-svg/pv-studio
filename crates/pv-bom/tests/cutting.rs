//! First-fit decreasing: properties on random cut lists, and the saw cut.

use pv_bom::cut;
use pv_bom::cutting::joints;

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
        // Pieces of 0.2 … 4.8 m, to the millimetre; one in five longer (spliced rails, up to
        // 12 m).
        let pieces: Vec<f64> = (0..n)
            .map(|_| {
                let max_mm = if rng.next().is_multiple_of(5) { 12_000 } else { 4_800 };
                (200 + rng.next() % (max_mm - 199)) as f64 / 1000.0
            })
            .collect();
        let plan = cut("R", &pieces, 4.8, kerf).unwrap();

        // Short pieces are placed as they are, long ones as their segments: same total length.
        let placed: f64 = plan.bars.iter().flatten().sum();
        let given: f64 = pieces.iter().sum();
        assert!((placed - given).abs() < 1e-6, "seed {seed}: length lost");
        let short_given = pieces.iter().filter(|&&p| p <= 4.8).count();
        let segments: usize = plan.bars.iter().map(Vec::len).sum();
        let spliced: u64 = pieces.iter().map(|&p| joints(p, 4.8)).sum();
        assert_eq!(
            segments as u64,
            short_given as u64
                + pieces
                    .iter()
                    .filter(|&&p| p > 4.8)
                    .map(|&p| joints(p, 4.8) + 1)
                    .sum::<u64>(),
            "seed {seed}: segments"
        );
        assert!(spliced <= segments as u64);
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
    // A piece of exactly the bar length takes one bar, no splice.
    assert_eq!(cut("R", &[4.8], 4.8, 0.005).unwrap().bar_count(), 1);
    assert_eq!(joints(4.8, 4.8), 0);
    assert_eq!(
        cut("R", &[1.0], 0.0, 0.0).unwrap_err().key(),
        "errors.bom.invalidStock"
    );
    assert_eq!(
        cut("R", &[-1.0], 4.8, 0.0).unwrap_err().key(),
        "errors.bom.invalidPiece"
    );
}

#[test]
fn long_rails_are_spliced() {
    // 9.10 m: one whole bar and a 4.30 m rest — one splice. 9.60 m: two whole bars, one
    // splice. 10.00 m: two whole bars and 0.40 m — two splices.
    assert_eq!(joints(9.1, 4.8), 1);
    assert_eq!(joints(9.6, 4.8), 1);
    assert_eq!(joints(10.0, 4.8), 2);
    let plan = cut("R", &[9.1], 4.8, 0.0).unwrap();
    assert_eq!(plan.bar_count(), 2);
    assert_eq!(plan.bars[0], vec![4.8]);
    assert!((plan.bars[1][0] - 4.3).abs() < 1e-9);
    // The rest shares a bar with other pieces: 4.30 + 0.40 ≤ 4.80.
    let plan = cut("R", &[9.1, 0.4], 4.8, 0.0).unwrap();
    assert_eq!(plan.bar_count(), 2);
}
