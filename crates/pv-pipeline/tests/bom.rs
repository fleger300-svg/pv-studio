//! B3: the bill of materials through the graph, on project A roof 1 (K2 tab « roof-1 » of
//! `testdata/k2/reference_bom.json`), and through real commands (removal, move + undo,
//! discount + undo).
//!
//! Rails (bars), climbers, middle and end clamps must match K2 exactly. Standing-seam clamps
//! are the documented gap: K2 does not print the seam positions, so our count (one clamp per
//! seam under the bottom rails) is checked against the hardware it comes from, and its
//! difference to K2 is printed, never asserted.

mod common;

use common::*;
use pv_graph::{CancelToken, Graph, Instance};
use pv_model::{Command, ModuleId, ModuleState, Pricing, Project};
use pv_pipeline::nodes::bom::ProjectBom;
use pv_pipeline::{sync_changes, sync_project, FieldStatics};
use pv_store::Author;
use serde::Deserialize;

const SEAM_CLAMP: &str = "2003024";

#[derive(Deserialize)]
struct Reference {
    articles: Vec<ArticleIn>,
    roofs: Vec<Tab>,
}

#[derive(Deserialize)]
struct ArticleIn {
    article_no: String,
    list_price: f64,
}

#[derive(Deserialize)]
struct Tab {
    id: String,
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct Line {
    article_no: String,
    quantity: u32,
}

fn reference() -> Reference {
    serde_json::from_str(include_str!("../../../testdata/k2/reference_bom.json")).unwrap()
}

/// Project A roof 1 with the reference (scaled) list prices on its parts.
fn priced_roof_1() -> Project {
    let r = reference();
    let mut p = ref_a_roof1();
    for a in &mut p.arrays {
        if let Some(m) = a.mounting.as_mut() {
            for part in &mut m.parts {
                part.article.list_price = r
                    .articles
                    .iter()
                    .find(|x| x.article_no == part.article.article_no)
                    .unwrap()
                    .list_price;
            }
        }
    }
    p
}

fn bom(g: &mut Graph) -> pv_bom::Bom {
    (*g.get::<ProjectBom>(Instance::Project).unwrap()).clone()
}

/// Deviations of the total against the K2 tab of roof 1 (seam clamps excluded).
fn k2_deviations(g: &mut Graph, p: &Project) -> Vec<String> {
    let r = reference();
    let tab = r.roofs.iter().find(|t| t.id == "roof-1").unwrap();
    let ours = bom(g);
    let mut failures = Vec::new();
    for l in &tab.lines {
        let got = ours
            .total
            .lines
            .iter()
            .find(|x| x.article_no == l.article_no)
            .map_or(0, |x| x.quantity);
        if l.article_no == SEAM_CLAMP {
            // Documented gap: one clamp per seam under the rails; the seams' phase is unknown.
            let fixations: u32 = (0..p.arrays.len())
                .map(|f| {
                    g.get::<FieldStatics>(array(f, p))
                        .unwrap()
                        .fixations
                        .unwrap_or(0)
                })
                .sum();
            if got != fixations {
                failures.push(format!("seam clamps {got} ≠ fixations {fixations}"));
            }
            eprintln!(
                "standing-seam clamps: engine {got}, K2 {} (documented gap, seam phase unknown)",
                l.quantity
            );
        } else if got != l.quantity {
            failures.push(format!("{}: engine {got}, K2 {}", l.article_no, l.quantity));
        }
    }
    failures
}

#[test]
fn roof_1_bom_through_the_graph_matches_k2() {
    let p = priced_roof_1();
    let mut g = graph_for(&p);
    let failures = k2_deviations(&mut g, &p);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let b = bom(&mut g);
    // One face, so its tab and the total agree; nothing saved by cutting across faces.
    assert_eq!(b.faces.len(), 1);
    assert_eq!(b.faces[0].lines, b.total.lines);
    assert!(b.saved_bars.is_empty());
    assert!(b.total.price > 0.0);
}

#[test]
fn k2_bom_through_commands_with_a_discount() {
    // Every module in place; K2's removed module comes by command.
    let mut base = priced_roof_1();
    let removed: Vec<ModuleId> = base
        .modules
        .iter_mut()
        .filter(|m| m.state == ModuleState::Removed)
        .map(|m| {
            m.state = ModuleState::Active;
            m.id
        })
        .collect();
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_for(&dir, "bom", &base);
    let mut g = pv_pipeline::new_graph();
    sync_project(&mut g, store.project()).unwrap();
    let step = |g: &mut Graph, store: &pv_store::ProjectStore, c: pv_model::ChangeSet| {
        sync_changes(g, store.project(), &c).unwrap();
        g.run_auto(&CancelToken::new()).unwrap();
        compare_with_full(g, store.project()).unwrap();
    };

    let c = store
        .apply(&Command::RemoveModules { ids: removed }, Author::User)
        .unwrap();
    step(&mut g, &store, c);
    let failures = k2_deviations(&mut g, store.project());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let list = bom(&mut g);

    // −35 % on K2: every line's unit price is the list price × 0.65 to the cent, and the
    // quantities do not move.
    let c = store
        .apply(
            &Command::SetDiscount {
                manufacturer: "K2 Systems".into(),
                rate: Some(0.35),
            },
            Author::User,
        )
        .unwrap();
    step(&mut g, &store, c);
    let discounted = bom(&mut g);
    for (l, d) in list.total.lines.iter().zip(&discounted.total.lines) {
        assert_eq!(l.quantity, d.quantity);
        assert_eq!(d.discount_rate, 0.35);
        let unit = (l.list_price * 100.0 * 0.65).round() / 100.0;
        assert!((d.unit_price - unit).abs() < 1e-9, "{}", d.article_no);
        assert!((d.price - f64::from(d.quantity) * unit).abs() < 0.005);
    }
    assert!(discounted.total.price < list.total.price);
    assert_eq!(store.project().pricing.rate_for("K2 Systems"), 0.35);

    // Undo: back to list prices, bit for bit.
    let c = store.undo().unwrap();
    step(&mut g, &store, c);
    assert_eq!(bom(&mut g), list);
    assert_eq!(store.project().pricing, Pricing::default());
}
