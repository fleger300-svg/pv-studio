//! Property test: over random edit sequences, the incremental graph always agrees with a
//! graph built from scratch on the same inputs, reports every change of a watched result
//! exactly once, and is deterministic.

mod common;

use common::*;
use pv_graph::{
    serde_fingerprint, CancelToken, Graph, GraphError, Instance, NodeStatus, ResultReady,
};

const SEQUENCES: u64 = 500;
const STEPS: usize = 30;

/// SplitMix64: small, deterministic, good enough for test sequences.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, Copy)]
struct Inputs {
    xs: [Option<i64>; N as usize],
    sel: Option<u64>,
}

fn scratch(inputs: &Inputs) -> Graph {
    let mut g = toy_graph();
    for (n, x) in inputs.xs.iter().enumerate() {
        if let Some(x) = x {
            g.set_input::<X>(i(n as u64), *x).unwrap();
        }
    }
    if let Some(s) = inputs.sel {
        g.set_input::<Sel>(Instance::Project, s).unwrap();
    }
    g
}

fn all_values(g: &mut Graph) -> Vec<Result<i64, GraphError>> {
    let mut out = Vec::new();
    for n in 0..N {
        out.push(value::<Half>(g, i(n)));
        out.push(value::<Pick>(g, i(n)));
        out.push(value::<Checked>(g, i(n)));
    }
    out.push(value::<Sum>(g, Instance::Project));
    out.push(value::<Report>(g, Instance::Project));
    out
}

fn result_fingerprint(r: &Result<i64, GraphError>) -> u64 {
    match r {
        Ok(v) => serde_fingerprint(v),
        Err(e) => serde_fingerprint(e),
    }
}

/// Runs one random sequence; returns a transcript (for the determinism check).
fn run_sequence(seed: u64) -> Vec<String> {
    let mut rng = Rng(seed);
    let mut inputs = Inputs {
        xs: [Some(0); N as usize],
        sel: Some(0),
    };
    let mut g = scratch(&inputs);
    g.watch::<Sum>(Instance::Project).unwrap();
    if rng.below(2) == 0 {
        g.watch::<Report>(Instance::Project).unwrap();
    }
    let mut reported: std::collections::BTreeMap<_, u64> = Default::default();
    let mut transcript = Vec::new();

    for step in 0..STEPS {
        match rng.below(10) {
            0..=3 => {
                let n = rng.below(N);
                let x = rng.below(25) as i64 - 12;
                inputs.xs[n as usize] = Some(x);
                g.set_input::<X>(i(n), x).unwrap();
            }
            4 => {
                let s = rng.below(4);
                inputs.sel = Some(s);
                g.set_input::<Sel>(Instance::Project, s).unwrap();
            }
            5 => {
                let n = rng.below(N);
                inputs.xs[n as usize] = None;
                g.remove_input::<X>(i(n)).unwrap();
            }
            6 => {
                // A pass cancelled before it starts.
                let t = CancelToken::new();
                t.cancel();
                assert_eq!(g.run_auto(&t), Err(GraphError::Cancelled));
            }
            7 => {
                // On-demand request.
                let r = value::<Report>(&mut g, Instance::Project);
                assert_eq!(r, value::<Report>(&mut scratch(&inputs), Instance::Project));
            }
            _ => {
                let ready: Vec<ResultReady> = g.run_auto(&CancelToken::new()).unwrap();
                for r in &ready {
                    // Reported only when the result actually changed.
                    assert_ne!(
                        reported.get(&r.key),
                        Some(&r.output_fingerprint),
                        "seed {seed} step {step}: {r:?} re-reported"
                    );
                    reported.insert(r.key, r.output_fingerprint);
                    transcript.push(format!("{step}:{}:{}", r.key, r.output_fingerprint));
                }
                // After a pass: auto nodes are up to date, and nothing was missed.
                let sum = value::<Sum>(&mut g, Instance::Project);
                assert!(
                    matches!(
                        g.status_of::<Sum>(Instance::Project),
                        NodeStatus::Fresh | NodeStatus::Failed(_)
                    ),
                    "seed {seed} step {step}"
                );
                assert_eq!(
                    reported.get(&Sum::key(Instance::Project)),
                    Some(&result_fingerprint(&sum)),
                    "seed {seed} step {step}: missed a change"
                );
            }
        }
        // Sometimes compare everything (this pulls every node, so not at every step:
        // the stale paths must be exercised too).
        if rng.below(3) == 0 {
            let ours = all_values(&mut g);
            let theirs = all_values(&mut scratch(&inputs));
            assert_eq!(ours, theirs, "seed {seed} step {step}");
        }
    }
    let ours = all_values(&mut g);
    assert_eq!(ours, all_values(&mut scratch(&inputs)), "seed {seed} end");
    transcript.push(format!("{:?}", g.stats()));
    transcript
}

use pv_graph::Kind;

#[test]
fn incremental_equals_from_scratch() {
    for seed in 0..SEQUENCES {
        run_sequence(seed);
    }
}

#[test]
fn passes_are_deterministic() {
    for seed in 0..50 {
        assert_eq!(run_sequence(seed), run_sequence(seed), "seed {seed}");
    }
}
