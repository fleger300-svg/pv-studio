//! Electrical mismatch on one MPP tracker: unequal strings in parallel, partial shading.
//!
//! The tracker holds all its strings at one voltage. When their MPP voltages differ (other
//! length, orientation, temperature or irradiance), each string works off its own MPP
//! ([`loss_fraction`]).
//!
//! Inside a string, the modules' bypass-diode substrings are in series and carry the same
//! current: a shaded substring either limits the current of the whole string or is bypassed
//! by its diode, losing its voltage ([`tracker_power`]). The tracker scans for the global
//! maximum, as current inverters do.
//!
//! Each string or substring follows the explicit I-V model
//! `I = Isc·(1 − C1·(exp(V/(C2·Voc)) − 1))`, which passes through `(0, Isc)`, `(Vmpp, Impp)`
//! and `(Voc, 0)`.

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

    /// Voltage at current `i`; above its short-circuit current a substring is bypassed by its
    /// diode.
    fn voltage(&self, i: f64) -> f64 {
        if i >= self.isc {
            return -BYPASS_V;
        }
        self.c2_voc * (1.0 + (1.0 - i.max(0.0) / self.isc) / self.c1).ln()
    }

    fn pmax(&self) -> f64 {
        golden_max(&|v| v * self.current(v), 0.0, self.voc)
    }
}

/// Forward voltage of a conducting bypass diode [V].
const BYPASS_V: f64 = 0.4;

/// `count` identical bypass-diode substrings in series (`point`: one substring's data).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Block {
    pub count: f64,
    pub point: StringPoint,
}

/// MPP power of one substring [W] (0 when its data is not a valid curve).
#[must_use]
pub fn block_pmax(point: &StringPoint) -> f64 {
    Curve::new(point).map_or(0.0, |c| c.pmax())
}

struct SeriesString {
    blocks: Vec<(f64, Curve)>,
    /// Highest short-circuit current of its substrings.
    i_max: f64,
}

impl SeriesString {
    fn new(blocks: &[Block]) -> Option<Self> {
        let blocks: Vec<(f64, Curve)> = blocks
            .iter()
            .filter(|b| b.count > 0.0)
            .filter_map(|b| Curve::new(&b.point).map(|c| (b.count, c)))
            .collect();
        let i_max = blocks.iter().map(|b| b.1.isc).fold(0.0, f64::max);
        (!blocks.is_empty()).then_some(Self { blocks, i_max })
    }

    fn voltage(&self, i: f64) -> f64 {
        self.blocks.iter().map(|(n, c)| n * c.voltage(i)).sum()
    }

    /// Current at string voltage `v` (the voltage falls as the current rises).
    fn current(&self, v: f64) -> f64 {
        if v >= self.voltage(0.0) {
            return 0.0;
        }
        let (mut lo, mut hi) = (0.0, self.i_max);
        if self.voltage(hi) >= v {
            return hi;
        }
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if self.voltage(mid) > v {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Global MPP of the string alone: the power is smooth between the substrings'
    /// short-circuit currents, where diodes switch on.
    fn pmax(&self) -> f64 {
        let mut cuts: Vec<f64> = self.blocks.iter().map(|b| b.1.isc).collect();
        cuts.push(0.0);
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-12);
        cuts.windows(2)
            .map(|w| {
                let span = w[1] - w[0];
                golden_max(&|i| i * self.voltage(i), w[0], w[1] - 1e-9 * span.max(1e-9))
            })
            .fold(0.0, f64::max)
    }
}

/// Best power [W] of strings of substrings in parallel on one tracker: the tracker finds the
/// global maximum (a scan), every string at the same voltage.
#[must_use]
pub fn tracker_power(strings: &[Vec<Block>]) -> f64 {
    // Identical strings share one curve.
    if let [first, rest @ ..] = strings {
        if !rest.is_empty() && rest.iter().all(|s| s == first) {
            return strings.len() as f64 * tracker_power(std::slice::from_ref(first));
        }
    }
    let strings: Vec<SeriesString> = strings
        .iter()
        .filter_map(|s| SeriesString::new(s))
        .collect();
    match strings.as_slice() {
        [] => 0.0,
        [one] => one.pmax(),
        _ => {
            let total = |v: f64| v * strings.iter().map(|s| s.current(v)).sum::<f64>();
            let hi = strings.iter().map(|s| s.voltage(0.0)).fold(0.0, f64::max);
            if hi <= 0.0 {
                return 0.0;
            }
            const N: usize = 96;
            let step = hi / N as f64;
            let best = (1..N)
                .max_by(|&a, &b| total(a as f64 * step).total_cmp(&total(b as f64 * step)))
                .unwrap_or(1);
            golden_max(
                &total,
                (best as f64 - 1.0) * step,
                (best as f64 + 1.0) * step,
            )
            .max(0.0)
        }
    }
}

/// Maximum of `f` on `[lo, hi]` (golden section; `f` unimodal there).
fn golden_max(f: &dyn Fn(f64) -> f64, mut lo: f64, mut hi: f64) -> f64 {
    const R: f64 = 0.618_033_988_749_895;
    let mut x1 = hi - R * (hi - lo);
    let mut x2 = lo + R * (hi - lo);
    let (mut f1, mut f2) = (f(x1), f(x2));
    for _ in 0..32 {
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

    /// One substring (a third of a 34 V / 10 A module) at relative irradiance `c`.
    fn sub(count: f64, c: f64) -> Block {
        Block {
            count,
            point: StringPoint {
                vmpp_v: 34.0 / 3.0,
                impp_a: 10.0 * c,
                voc_v: 41.0 / 3.0,
                isc_a: 10.6 * c,
            },
        }
    }

    #[test]
    fn an_unshaded_string_gives_its_substrings_power() {
        let p = tracker_power(&[vec![sub(30.0, 1.0)]]);
        let one = block_pmax(&sub(1.0, 1.0).point);
        assert!((p / (30.0 * one) - 1.0).abs() < 1e-6, "{p} {one}");
        // Two equal strings: twice as much.
        let two = tracker_power(&[vec![sub(30.0, 1.0)], vec![sub(30.0, 1.0)]]);
        assert!((two / (60.0 * one) - 1.0).abs() < 1e-3, "{two}");
    }

    #[test]
    fn a_shaded_substring_is_bypassed() {
        // 10 modules, one substring in the shade (20 % of the light): its diode bypasses it,
        // and the string keeps about 29/30 of its power — more than the linear 29.2/30 minus
        // the limit of the whole string to 20 %, less than linear.
        let one = block_pmax(&sub(1.0, 1.0).point);
        let p = tracker_power(&[vec![sub(29.0, 1.0), sub(1.0, 0.2)]]) / (30.0 * one);
        assert!(p > 0.95 && p < 29.2 / 30.0, "{p}");
        // Half of the string in deep shade: better to carry the low current through all.
        let q = tracker_power(&[vec![sub(3.0, 1.0), sub(27.0, 0.9)]]) / (30.0 * one);
        assert!(q > 0.88 && q < 0.91 + 0.1 * 0.1, "{q}");
    }

    #[test]
    fn substrings_agree_with_whole_strings_when_unshaded() {
        // 10 and 8 modules on one tracker, as substrings and as whole strings.
        let p = tracker_power(&[vec![sub(30.0, 1.0)], vec![sub(24.0, 1.0)]]);
        let one = block_pmax(&sub(1.0, 1.0).point);
        let by_substrings = 1.0 - p / (54.0 * one);
        let by_strings = loss_fraction(&[string(10.0, 1.0), string(8.0, 1.0)]);
        assert!(
            (by_substrings - by_strings).abs() < 2e-3,
            "{by_substrings} {by_strings}"
        );
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
