//! L3: K2 D-Dome bill of materials on project B (east-west, concrete blocks): per continuous
//! east-west run of n pairs, Peak = SD = 2(n+1), EC = 8, MC = 4(n−1), MK2 = M8x20 = 2 × Peak,
//! one concrete screw per ballast block; counted once per east-west system.

#![allow(dead_code)]

mod common;

use common::*;
use pv_model::component::{presets, resolve_mounting, ComponentSnapshot};
use pv_pipeline::nodes::ballast::Ballast;
use pv_pipeline::nodes::bom::{FieldCountsNode, ProjectBom};

fn dome_rules() -> Vec<pv_model::PartRule> {
    let snaps: Vec<ComponentSnapshot> = presets::all()
        .into_iter()
        .map(|component| ComponentSnapshot {
            component,
            price: None,
        })
        .collect();
    resolve_mounting(
        &presets::k2_d_dome(),
        &presets::lr7_54hvh_490m(),
        snaps.as_slice(),
    )
    .expect("d-dome")
    .parts
}

fn qty(bom: &pv_bom::Bom, no: &str) -> u64 {
    bom.total
        .lines
        .iter()
        .filter(|l| l.article_no == no)
        .map(|l| u64::from(l.quantity))
        .sum()
}

#[test]
fn d_dome_quantities_follow_the_k2_rules() {
    let p = ref_b_east_west_with_parts(dome_rules());
    let mut g = graph_for(&p);
    let (a, b) = (array(0, &p), array(1, &p));
    let ca = g.get::<FieldCountsNode>(a).unwrap();
    let cb = g.get::<FieldCountsNode>(b).unwrap();
    // One half carries the runs, the other none.
    let runs = if ca.pair_runs.is_empty() {
        cb.pair_runs.clone()
    } else {
        assert!(cb.pair_runs.is_empty());
        ca.pair_runs.clone()
    };
    assert!(!runs.is_empty());
    let active = p
        .modules
        .iter()
        .filter(|m| m.state == pv_model::ModuleState::Active)
        .count() as u64;
    let pairs: u64 = runs.iter().map(|&n| u64::from(n)).sum();
    assert_eq!(2 * pairs, active, "every module is in one pair");

    let bom = (*g.get::<ProjectBom>(pv_graph::Instance::Project).unwrap()).clone();
    let r = runs.len() as u64;
    let peak = qty(&bom, "2004125");
    assert_eq!(peak, 2 * pairs + 2 * r);
    assert_eq!(qty(&bom, "2003243"), peak);
    let (mc, ec) = (qty(&bom, "2002609"), qty(&bom, "2004564"));
    assert_eq!(ec, 8 * r);
    assert_eq!(mc + ec, 2 * peak);
    assert_eq!(qty(&bom, "1001643"), 2 * peak);
    assert_eq!(qty(&bom, "2001729"), 2 * peak);
    let blocks: u64 = [a, b]
        .iter()
        .filter_map(|&i| {
            g.get::<Ballast>(i)
                .unwrap()
                .as_ref()
                .as_ref()
                .map(|x| u64::from(x.total_blocks))
        })
        .sum();
    assert!(blocks > 0);
    assert_eq!(qty(&bom, "2002570"), blocks);
    println!("runs {runs:?}: Peak {peak}, MC {mc}, EC {ec}, blocks {blocks}");
}
