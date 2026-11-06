//! The engine's own cost (G1 acceptance: < 5 ms to invalidate and recompute 10 000 nodes
//! with trivial computations), so that the 100 ms budget of the architecture goes to the
//! domain computations.
//!
//! Graph: 100 inputs → 1 000 `mid` nodes (2 inputs each) → 9 000 watched `top` nodes
//! (2 `mid` each): 10 000 derived nodes, 20 000 edges.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use pv_graph::{CancelToken, Ctx, Derived, Graph, GraphError, Input, Instance, Kind};

const SRC: u64 = 100;
const MID: u64 = 1_000;
const TOP: u64 = 9_000;
const BUDGET: Duration = Duration::from_millis(5);

/// The measurements run one at a time: in parallel they measure each other.
static SERIAL: Mutex<()> = Mutex::new(());

/// Exact fingerprint of an integer: the computations are trivial, their fingerprint too
/// (domain kinds use `serde_fingerprint`, whose cost belongs to the domain node).
fn trivial_fingerprint(v: i64) -> u64 {
    pv_model::fingerprint::fingerprint_bytes(&v.to_le_bytes())
}

fn idx(instance: Instance) -> u64 {
    match instance {
        Instance::Index(n) => n,
        _ => unreachable!(),
    }
}

struct Src;
impl Kind for Src {
    type Output = i64;
    const NAME: &'static str = "src";
    fn fingerprint(v: &i64) -> u64 {
        trivial_fingerprint(*v)
    }
}
impl Input for Src {}

struct Mid;
impl Kind for Mid {
    type Output = i64;
    const NAME: &'static str = "mid";
    fn fingerprint(v: &i64) -> u64 {
        trivial_fingerprint(*v)
    }
}
impl Derived for Mid {
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<i64, GraphError> {
        let n = idx(instance);
        Ok(*ctx.get::<Src>(Instance::Index(n % SRC))?
            + *ctx.get::<Src>(Instance::Index((n / SRC + n) % SRC))?)
    }
}

struct Top;
impl Kind for Top {
    type Output = i64;
    const NAME: &'static str = "top";
    fn fingerprint(v: &i64) -> u64 {
        trivial_fingerprint(*v)
    }
}
impl Derived for Top {
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<i64, GraphError> {
        let n = idx(instance);
        Ok(*ctx.get::<Mid>(Instance::Index(n % MID))?
            - *ctx.get::<Mid>(Instance::Index((n * 7 + 3) % MID))?)
    }
}

fn graph() -> Graph {
    let mut g = Graph::new();
    g.register_input::<Src>().unwrap();
    g.register::<Mid>().unwrap();
    g.register::<Top>().unwrap();
    for n in 0..SRC {
        g.set_input::<Src>(Instance::Index(n), n as i64).unwrap();
    }
    for n in 0..TOP {
        g.watch::<Top>(Instance::Index(n)).unwrap();
    }
    g.run_auto(&CancelToken::new()).unwrap();
    g
}

/// Best of `rounds` (the machine may be busy; the minimum is the engine's cost).
fn best(rounds: usize, mut f: impl FnMut(usize) -> Duration) -> Duration {
    (0..rounds).map(&mut f).min().unwrap_or_default()
}

#[test]
fn full_invalidation_and_recompute_of_10_000_nodes() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut g = graph();
    assert_eq!(g.node_count() as u64, SRC + MID + TOP);
    let mut invalidation = Duration::MAX;
    let t = best(7, |round| {
        let start = Instant::now();
        // A scale factor, not an offset: an offset common to all sources would cancel out in
        // `top = mid − mid` and the cut-off would (rightly) skip every top.
        for n in 0..SRC {
            g.set_input::<Src>(Instance::Index(n), (n * (31 + round as u64) + 1) as i64)
                .unwrap();
        }
        invalidation = invalidation.min(start.elapsed());
        let ready = g.run_auto(&CancelToken::new()).unwrap();
        let t = start.elapsed();
        assert!(ready.len() > 8_000, "{}", ready.len());
        t
    });
    let computed = g.stats().computed("mid") + g.stats().computed("top");
    eprintln!("full recompute of 10 000 nodes: {t:?} (of which invalidation {invalidation:?})");
    assert!(computed >= 7 * 10_000);
    assert!(t < BUDGET, "{t:?} > {BUDGET:?}");
}

#[test]
fn cut_off_full_invalidation() {
    // Every input changes but the mids keep their values: 1 000 recomputations and 9 000
    // confirmations.
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut g = graph();
    let t = best(7, |round| {
        let d = if round % 2 == 0 { 1 } else { -1 };
        let start = Instant::now();
        // Every source changes and comes back: every mid is dirty and recomputes to the same
        // value, every top is confirmed without recomputation.
        for n in 0..SRC {
            g.set_input::<Src>(Instance::Index(n), n as i64 + d)
                .unwrap();
        }
        for n in 0..SRC {
            g.set_input::<Src>(Instance::Index(n), n as i64).unwrap();
        }
        g.reset_stats();
        g.run_auto(&CancelToken::new()).unwrap();
        let t = start.elapsed();
        assert_eq!(g.stats().computed("mid"), MID);
        assert_eq!(g.stats().computed("top"), 0);
        assert_eq!(g.stats().verified, TOP);
        t
    });
    eprintln!("cut-off pass over 10 000 stale nodes: {t:?}");
    assert!(t < BUDGET, "{t:?} > {BUDGET:?}");
}

#[test]
fn local_change() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut g = graph();
    let t = best(7, |round| {
        let start = Instant::now();
        g.set_input::<Src>(Instance::Index(42), 1_000 + round as i64)
            .unwrap();
        g.run_auto(&CancelToken::new()).unwrap();
        start.elapsed()
    });
    eprintln!("local change: {t:?}");
    assert!(t < Duration::from_millis(1), "{t:?}");
}
