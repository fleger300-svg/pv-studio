//! The field chooses the bar length of its rails: the bars become the article of that length
//! (K2 SingleRail 36: 4.80 m by default, 2.40 m offered), cut and spliced on that length.

#![allow(dead_code)]

mod common;

use common::*;
use pv_graph::Instance;
use pv_pipeline::nodes::bom::ProjectBom;

fn qty(bom: &pv_bom::Bom, no: &str) -> u64 {
    bom.total
        .lines
        .iter()
        .filter(|l| l.article_no == no)
        .map(|l| u64::from(l.quantity))
        .sum()
}

#[test]
fn rails_in_the_chosen_bar_length() {
    let p = ref_a_roof1();
    let summary = pv_model::ProjectSummary::new(&p, String::new(), false, false);
    assert_eq!(summary.arrays[0].rail_lengths_m, vec![4.8, 2.4]);

    let mut g = graph_for(&p);
    let long = (*g.get::<ProjectBom>(Instance::Project).unwrap()).clone();
    let bars_480 = qty(&long, "2004393");
    assert!(bars_480 > 0);
    assert_eq!(qty(&long, "2003458"), 0);

    let mut short = p.clone();
    for a in &mut short.arrays {
        a.layout.rail_stock_length_m = Some(2.4);
    }
    let mut g = graph_for(&short);
    let bom = (*g.get::<ProjectBom>(Instance::Project).unwrap()).clone();
    assert_eq!(qty(&bom, "2004393"), 0);
    let bars_240 = qty(&bom, "2003458");
    // More bars, never less rail bought than needed: short pieces (2.388 m, 1.234 m) waste
    // less in 2.40 m bars, so the bought length may drop.
    assert!(
        bars_240 > bars_480,
        "{bars_240} × 2.40 m vs {bars_480} × 4.80 m"
    );
    // Climbers and clamps do not depend on the bar length.
    for no in ["2003145", "2004148", "2004545"] {
        assert_eq!(qty(&bom, no), qty(&long, no), "{no}");
    }
    println!("{bars_480} × 4.80 m → {bars_240} × 2.40 m");
}
