//! Cancellation: nothing partial is published, the next pass resumes the work.

mod common;

use std::cell::Cell;

use common::*;
use pv_graph::{
    serde_fingerprint, CancelToken, Ctx, Derived, Graph, GraphError, Instance, Kind, NodeStatus,
};

thread_local! {
    /// The `slow` node with this index cancels the pass while it computes.
    static TRIP: Cell<Option<u64>> = const { Cell::new(None) };
}

struct Slow;
impl Kind for Slow {
    type Output = i64;
    const NAME: &'static str = "slow";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Slow {
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<i64, GraphError> {
        let x = *ctx.get::<X>(instance)?;
        if TRIP.with(Cell::get)
            == Some(match instance {
                Instance::Index(n) => n,
                _ => unreachable!(),
            })
        {
            ctx.cancel_token().cancel(); // a newer command arrived meanwhile
        }
        Ok(x * x)
    }
}

struct Total;
impl Kind for Total {
    type Output = i64;
    const NAME: &'static str = "total";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Total {
    fn compute(ctx: &mut Ctx<'_>, _: Instance) -> Result<i64, GraphError> {
        let mut t = 0;
        for n in 0..N {
            t += *ctx.get::<Slow>(i(n))?;
        }
        Ok(t)
    }
}

fn graph() -> Graph {
    let mut g = toy_graph();
    g.register::<Slow>().unwrap();
    g.register::<Total>().unwrap();
    for n in 0..N {
        g.set_input::<X>(i(n), n as i64 + 1).unwrap();
    }
    g.watch::<Total>(Instance::Project).unwrap();
    g
}

#[test]
fn a_cancelled_token_does_nothing() {
    let mut g = graph();
    let token = CancelToken::new();
    token.cancel();
    assert_eq!(g.run_auto(&token), Err(GraphError::Cancelled));
    assert_eq!(g.stats().total_computed(), 0);
    assert_eq!(g.status_of::<Total>(Instance::Project), NodeStatus::Missing);
}

#[test]
fn cancellation_mid_pass_keeps_finished_nodes_and_discards_the_rest() {
    let mut g = graph();
    TRIP.with(|t| t.set(Some(2)));
    assert_eq!(g.run_auto(&CancelToken::new()), Err(GraphError::Cancelled));
    TRIP.with(|t| t.set(None));
    // slow[0], slow[1] finished; slow[2] was cancelled while computing: not stored.
    assert_eq!(g.stats().computed("slow"), 2);
    assert_eq!(g.status_of::<Slow>(i(1)), NodeStatus::Fresh);
    assert_eq!(g.status_of::<Slow>(i(2)), NodeStatus::Missing);
    assert_eq!(g.status_of::<Total>(Instance::Project), NodeStatus::Missing);

    // The next pass (new token) resumes: only slow[2], slow[3] and total compute.
    let ready = g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(g.stats().computed("slow"), 4);
    assert_eq!(g.stats().computed("total"), 1);
    assert_eq!(ready.len(), 1);
    assert_eq!(
        value::<Total>(&mut g, Instance::Project),
        Ok(1 + 4 + 9 + 16)
    );
}

#[test]
fn cancelling_a_stale_recompute_keeps_the_node_stale() {
    let mut g = graph();
    g.run_auto(&CancelToken::new()).unwrap();
    g.set_input::<X>(i(3), 10).unwrap();
    TRIP.with(|t| t.set(Some(3)));
    assert_eq!(g.run_auto(&CancelToken::new()), Err(GraphError::Cancelled));
    TRIP.with(|t| t.set(None));
    // The old value is still there but flagged stale, never presented as fresh.
    assert_eq!(g.status_of::<Total>(Instance::Project), NodeStatus::Stale);
    assert_eq!(g.status_of::<Slow>(i(3)), NodeStatus::Stale);
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(
        value::<Total>(&mut g, Instance::Project),
        Ok(1 + 4 + 9 + 100)
    );
}
