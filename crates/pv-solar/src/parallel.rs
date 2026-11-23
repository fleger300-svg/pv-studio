//! Unequal strings in parallel on one MPP tracker.
//!
//! The tracker holds all its strings at one voltage. When their MPP voltages differ (other
//! length, orientation, temperature or irradiance), each string works off its own MPP; the
//! loss is the gap between the sum of the strings' maxima and the best common operating point.
//!
//! Each string follows the explicit I-V model `I = Isc·(1 − C1·(exp(V/(C2·Voc)) − 1))`, which
//! passes through `(0, Isc)`, `(Vmpp, Impp)` and `(Voc, 0)`.

/// The operating data of one string at a given hour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StringPoint {
    pub vmpp_v: f64,
    pub impp_a: f64,
    pub voc_v: f64,
    pub isc_a: f64,
}

struct Curve {
    isc: f64,
    c1: f64,
    c2_voc: f64,
    voc: f64,
}

impl Curve {
    fn new(s: &StringPoint) -> Option<Self> {
        let ok = s.vmpp_v > 0.0 && s.voc_v > s.vmpp_v && s.impp_a > 0.0 && s.isc_a > s.impp_a;
        if !ok {
            return None;
        }
        let c2 = (s.vmpp_v / s.voc_v - 1.0) / (1.0 - s.impp_a / s.isc_a).ln();
        let c1 = (1.0 - s.impp_a / s.isc_a) * (-s.vmpp_v / (c2 * s.voc_v)).exp();
        Some(Self {
            isc: s.isc_a,
            c1,
            c2_voc: c2 * s.voc_v,
            voc: s.voc_v,
        })
    }

    /// Current at `v`; a string above its open-circuit voltage delivers nothing.
    fn current(&self, v: f64) -> f64 {
        if v >= self.voc {
            return 0.0;
        }
        (self.isc * (1.0 - self.c1 * ((v / self.c2_voc).exp() - 1.0))).max(0.0)
    }
}

/// Maximum of `f` on `[lo, hi]` (golden section; `f` unimodal there).
fn golden_max(f: &dyn Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    const R: f64 = 0.618_033_988_749_895;
    let mut x1 = hi - R * (hi - lo);
    let mut x2 = lo + R * (hi - lo);
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..40 {
        if f1 < f2 {
            lo = x1;
            x1 = x2;
            f1 = f2;
            x2 = lo + R * (hi - lo);
            f2 = f(x2);
        } else {
            hi = x2;
            x2 = x1;
            f2 = f1;
            x1 = hi - R * (hi - lo);
            f1 = f(x1);
        }
    }
    f1.max(f2)
}

/// Share of the strings' MPP power lost by holding them at one common voltage, in `[0, 1]`.
#[must_use]
pub fn loss_fraction(strings: &[StringPoint]) -> f64 {
    let curves: Vec<Curve> = strings.iter().filter_map(Curve::new).collect();
    if curves.len() < 2 {
        return 0.0;
    }
    // Same voltages: same curve shapes, same MPP voltage, no loss.
    let same = |f: &dyn Fn(&StringPoint) -> f64| {
        let (lo, hi) = strings
            .iter()
            .map(f)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| {
                (a.min(x), b.max(x))
            });
        hi - lo <= 1e-9 * hi.abs()
    };
    if same(&|s| s.vmpp_v) && same(&|s| s.voc_v) && same(&|s| s.impp_a / s.isc_a) {
        return 0.0;
    }
    let own: f64 = curves
        .iter()
        .map(|c| golden_max(&|v| v * c.current(v), 0.0, c.voc))
        .sum();
    if own <= 0.0 {
        return 0.0;
    }
    let total = |v: f64| v * curves.iter().map(|c| c.current(v)).sum::<f64>();
    // The sum can have several maxima (strings of very different lengths): coarse scan first.
    let hi = curves.iter().map(|c| c.voc).fold(0.0, f64::max);
    const N: usize = 96;
    let step = hi / N as f64;
    let best = (1..N)
        .max_by(|&a, &b| total(a as f64 * step).total_cmp(&total(b as f64 * step)))
        .unwrap_or(1);
    let common = golden_max(
        &total,
        (best as f64 - 1.0) * step,
        (best as f64 + 1.0) * step,
    );
    (1.0 - common / own).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(n: f64, g: f64) -> StringPoint {
        StringPoint {
            vmpp_v: 34.0 * n,
            impp_a: 10.0 * g,
            voc_v: 41.0 * n,
            isc_a: 10.6 * g,
        }
    }

    #[test]
    fn equal_strings_lose_nothing() {
        assert_eq!(loss_fraction(&[string(10.0, 1.0), string(10.0, 0.5)]), 0.0);
        assert_eq!(loss_fraction(&[string(10.0, 1.0)]), 0.0);
    }

    #[test]
    fn unequal_lengths_lose_more_the_more_they_differ() {
        let small = loss_fraction(&[string(10.0, 1.0), string(9.0, 1.0)]);
        let large = loss_fraction(&[string(10.0, 1.0), string(7.0, 1.0)]);
        assert!(small > 0.001 && small < 0.03, "{small}");
        assert!(large > small, "{small} {large}");
        // Never more than the short string's whole share.
        let extreme = loss_fraction(&[string(20.0, 1.0), string(5.0, 1.0)]);
        assert!(extreme <= 0.2 + 1e-9, "{extreme}");
    }

    #[test]
    fn the_curve_passes_through_the_mpp() {
        let s = string(1.0, 1.0);
        let c = Curve::new(&s).unwrap();
        assert!((c.current(0.0) - s.isc_a).abs() < 1e-9);
        assert!((c.current(s.vmpp_v) - s.impp_a).abs() < 0.01);
        assert!(c.current(s.voc_v).abs() < 1e-9);
    }
}
