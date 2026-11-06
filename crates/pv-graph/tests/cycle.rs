//! Cycles are detected and reported with their path, without hanging.

use pv_graph::{
    serde_fingerprint, CancelToken, Ctx, Derived, Graph, GraphError, Input, Instance, Kind,
    NodeStatus,
};

struct Flag;
impl Kind for Flag {
    type Output = bool;
    const NAME: &'static str = "flag";
    fn fingerprint(v: &bool) -> u64 {
        serde_fingerprint(v)
    }
}
impl Input for Flag {}

macro_rules! node {
    ($name:ident, $id:literal, |$ctx:ident| $body:expr) => {
        struct $name;
        impl Kind for $name {
            type Output = i64;
            const NAME: &'static str = $id;
            fn fingerprint(v: &i64) -> u64 {
                serde_fingerprint(v)
            }
        }
        impl Derived for $name {
            fn compute($ctx: &mut Ctx<'_>, _: Instance) -> Result<i64, GraphError> {
                $body
            }
        }
    };
}

// a reads b; b reads a only when the flag is set.
node!(A, "a", |ctx| Ok(*ctx.get::<B>(Instance::Project)? + 1));
node!(B, "b", |ctx| if *ctx.get::<Flag>(Instance::Project)? {
    Ok(*ctx.get::<A>(Instance::Project)? + 1)
} else {
    Ok(0)
});
node!(SelfLoop, "self", |ctx| Ok(
    *ctx.get::<SelfLoop>(Instance::Project)?
));

fn graph(flag: bool) -> Graph {
    let mut g = Graph::new();
    g.register_input::<Flag>().unwrap();
    g.register::<A>().unwrap();
    g.register::<B>().unwrap();
    g.register::<SelfLoop>().unwrap();
    g.set_input::<Flag>(Instance::Project, flag).unwrap();
    g
}

#[test]
fn cycle_is_reported_with_its_path() {
    let mut g = graph(true);
    let p = Instance::Project;
    let cycle = GraphError::Cycle {
        path: vec![A::key(p), B::key(p), A::key(p)],
    };
    assert_eq!(g.get::<A>(p).map(|v| *v), Err(cycle.clone()));
    // Stored as the result: reading again does not loop.
    assert_eq!(g.status(A::key(p)), NodeStatus::Failed(cycle.clone()));
    assert_eq!(g.get::<A>(p).map(|v| *v), Err(cycle.clone()));
    g.watch::<A>(p).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();

    // Breaking the cycle heals both nodes.
    g.set_input::<Flag>(p, false).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(g.get::<A>(p).map(|v| *v), Ok(1));
    assert_eq!(g.get::<B>(p).map(|v| *v), Ok(0));
}

#[test]
fn self_loop() {
    let mut g = graph(false);
    let p = Instance::Project;
    assert_eq!(
        g.get::<SelfLoop>(p).map(|v| *v),
        Err(GraphError::Cycle {
            path: vec![SelfLoop::key(p), SelfLoop::key(p)]
        })
    );
}
