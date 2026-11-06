//! Toy graph shared by the tests: it has every feature the domain graph will use.
//!
//! ```text
//! x[i] (input) ──► half[i] = x div 4            (early cut-off: many x give the same half)
//! sel (input) ──► pick[i] = sel even ? half[i] + half[i+1]   (dynamic dependencies)
//!                           : 2·x[i]
//! pick[i] ──► checked[i] = pick ≥ 0 ? pick : failure TEST_NEGATIVE   (errors as results)
//! checked[*] ──► sum                            (fan-in)
//! sum, sel ──► report = 10·sum + sel            (on-demand)
//! ```
#![allow(dead_code)]

use pv_graph::{serde_fingerprint, Ctx, Derived, Graph, GraphError, Input, Instance, Kind, Policy};
use pv_model::CommandError;

pub const N: u64 = 4;

pub fn i(n: u64) -> Instance {
    Instance::Index(n)
}

pub struct X;
impl Kind for X {
    type Output = i64;
    const NAME: &'static str = "x";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Input for X {}

pub struct Sel;
impl Kind for Sel {
    type Output = u64;
    const NAME: &'static str = "sel";
    fn fingerprint(v: &u64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Input for Sel {}

pub struct Half;
impl Kind for Half {
    type Output = i64;
    const NAME: &'static str = "half";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Half {
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<i64, GraphError> {
        Ok(ctx.get::<X>(instance)?.div_euclid(4))
    }
}

pub struct Pick;
impl Kind for Pick {
    type Output = i64;
    const NAME: &'static str = "pick";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Pick {
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<i64, GraphError> {
        let Instance::Index(n) = instance else {
            unreachable!()
        };
        if *ctx.get::<Sel>(Instance::Project)? % 2 == 0 {
            Ok(*ctx.get::<Half>(i(n))? + *ctx.get::<Half>(i((n + 1) % N))?)
        } else {
            Ok(2 * *ctx.get::<X>(i(n))?)
        }
    }
}

pub struct Checked;
impl Kind for Checked {
    type Output = i64;
    const NAME: &'static str = "checked";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Checked {
    fn compute(ctx: &mut Ctx<'_>, instance: Instance) -> Result<i64, GraphError> {
        let v = *ctx.get::<Pick>(instance)?;
        if v < 0 {
            return Err(CommandError::new("TEST_NEGATIVE")
                .with_number("value", v as f64)
                .into());
        }
        Ok(v)
    }
}

pub struct Sum;
impl Kind for Sum {
    type Output = i64;
    const NAME: &'static str = "sum";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Sum {
    fn compute(ctx: &mut Ctx<'_>, _: Instance) -> Result<i64, GraphError> {
        let mut s = 0;
        for n in 0..N {
            s += *ctx.get::<Checked>(i(n))?;
        }
        Ok(s)
    }
}

pub struct Report;
impl Kind for Report {
    type Output = i64;
    const NAME: &'static str = "report";
    fn fingerprint(v: &i64) -> u64 {
        serde_fingerprint(v)
    }
}
impl Derived for Report {
    const POLICY: Policy = Policy::OnDemand;
    fn compute(ctx: &mut Ctx<'_>, _: Instance) -> Result<i64, GraphError> {
        let sum = *ctx.get::<Sum>(Instance::Project)?;
        Ok(10 * sum + *ctx.get::<Sel>(Instance::Project)? as i64)
    }
}

pub fn toy_graph() -> Graph {
    let mut g = Graph::new();
    g.register_input::<X>().unwrap();
    g.register_input::<Sel>().unwrap();
    g.register::<Half>().unwrap();
    g.register::<Pick>().unwrap();
    g.register::<Checked>().unwrap();
    g.register::<Sum>().unwrap();
    g.register::<Report>().unwrap();
    g
}

/// Toy graph with every input set and `sum` watched.
pub fn toy_with(xs: [i64; N as usize], sel: u64) -> Graph {
    let mut g = toy_graph();
    for (n, x) in xs.into_iter().enumerate() {
        g.set_input::<X>(i(n as u64), x).unwrap();
    }
    g.set_input::<Sel>(Instance::Project, sel).unwrap();
    g.watch::<Sum>(Instance::Project).unwrap();
    g
}

pub fn value<K: Kind<Output = i64>>(g: &mut Graph, instance: Instance) -> Result<i64, GraphError> {
    g.get::<K>(instance).map(|v| *v)
}
