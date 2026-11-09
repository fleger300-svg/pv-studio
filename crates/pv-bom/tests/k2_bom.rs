//! The K2 Base material list of project A (`testdata/k2/reference_bom.json`), rebuilt from
//! the counts of the 4 roofs (`reference_layout.json`: rail cut lists, climbers, clamps) and
//! the parts of the K2 SingleRail preset: every tab, every line — quantities and sale units
//! exact, prices to the cent, masses to the gram.

use std::collections::HashMap;

use pv_bom::{compute, Bom, BomTable, FaceRef, FieldCounts, TopRail};
use pv_model::{MountingSystem, Pricing};
use serde::Deserialize;

const LAYOUT: &str = include_str!("../../../testdata/k2/reference_layout.json");
const BOM: &str = include_str!("../../../testdata/k2/reference_bom.json");

/// Top rails carry modules along v: length = n·1.134 + (n − 1)·0.02 + 2·0.05 (overhangs).
const MODULE_V_M: f64 = 1.134;
const GAP_V_M: f64 = 0.02;
const OVERHANG_M: f64 = 0.05;

#[derive(Deserialize)]
struct Layout {
    rails: Rails,
}

#[derive(Deserialize)]
struct Rails {
    per_roof: HashMap<String, RoofRails>,
}

#[derive(Deserialize)]
struct RoofRails {
    top: Vec<(f64, usize)>,
    bottom_long: Vec<(f64, usize)>,
    bottom_short: Vec<(f64, usize)>,
    climbers: u32,
    clamps: u32,
}

#[derive(Deserialize)]
struct Reference {
    articles: Vec<ArticleIn>,
    roofs: Vec<Tab>,
    total: TabLines,
}

#[derive(Deserialize)]
struct ArticleIn {
    article_no: String,
    unit_mass_kg: f64,
    pack_qty: u32,
    list_price: f64,
}

#[derive(Deserialize)]
struct Tab {
    id: String,
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct TabLines {
    lines: Vec<Line>,
}

#[derive(Deserialize)]
struct Line {
    article_no: String,
    quantity: u32,
    mass_kg: f64,
    sale_units: u32,
    price: f64,
}

fn expand(list: &[(f64, usize)]) -> Vec<f64> {
    list.iter()
        .flat_map(|&(len, n)| std::iter::repeat_n(len, n))
        .collect()
}

fn modules_on(length_m: f64) -> u32 {
    ((length_m - 2.0 * OVERHANG_M + GAP_V_M) / (MODULE_V_M + GAP_V_M)).round() as u32
}

fn reference_bom() -> (Bom, Reference) {
    let layout: Layout = serde_json::from_str(LAYOUT).unwrap();
    let reference: Reference = serde_json::from_str(BOM).unwrap();
    // The preset's articles, with the (scaled) reference prices; mass and pack must already
    // match K2.
    let mut parts = MountingSystem::k2_single_rail().parts;
    for p in &mut parts {
        let r = reference
            .articles
            .iter()
            .find(|a| a.article_no == p.article.article_no)
            .unwrap();
        assert_eq!(p.article.pack_qty, r.pack_qty, "{}", r.article_no);
        assert!((p.article.unit_mass_kg - r.unit_mass_kg).abs() < 1e-9);
        p.article.list_price = r.list_price;
    }
    let fields: Vec<FieldCounts> = ["roof-1", "roof-2", "roof-3", "roof-4"]
        .iter()
        .map(|id| {
            let r = &layout.rails.per_roof[*id];
            let top_rails: Vec<TopRail> = expand(&r.top)
                .into_iter()
                .map(|length_m| TopRail {
                    length_m,
                    modules: modules_on(length_m),
                })
                .collect();
            FieldCounts {
                face: FaceRef {
                    id: (*id).into(),
                    name: (*id).into(),
                },
                // Two top rails per module column.
                modules: top_rails.iter().map(|t| t.modules).sum::<u32>() / 2,
                top_rails,
                bottom_rails: [expand(&r.bottom_long), expand(&r.bottom_short)].concat(),
                climbers: r.climbers,
                fixations: r.clamps,
                ballast_blocks: 0,
                parts: parts.clone(),
            }
        })
        .collect();
    (
        compute(&fields, &Pricing::default(), "Total").unwrap(),
        reference,
    )
}

fn check(tab: &str, ours: &BomTable, theirs: &[Line], failures: &mut Vec<String>) {
    if ours.lines.len() != theirs.len() {
        failures.push(format!(
            "{tab}: {} lines, K2 {}",
            ours.lines.len(),
            theirs.len()
        ));
    }
    for t in theirs {
        let Some(o) = ours.lines.iter().find(|l| l.article_no == t.article_no) else {
            failures.push(format!("{tab} {}: missing", t.article_no));
            continue;
        };
        let what = format!("{tab} {}", t.article_no);
        if o.quantity != t.quantity {
            failures.push(format!("{what} quantity {} K2 {}", o.quantity, t.quantity));
        }
        if o.sale_units != t.sale_units {
            failures.push(format!(
                "{what} sale units {} K2 {}",
                o.sale_units, t.sale_units
            ));
        }
        if (o.price - t.price).abs() > 0.005 {
            failures.push(format!("{what} price {} K2 {}", o.price, t.price));
        }
        if (o.mass_kg - t.mass_kg).abs() > 0.001 {
            failures.push(format!("{what} mass {} K2 {}", o.mass_kg, t.mass_kg));
        }
    }
}

#[test]
fn k2_material_list_is_reproduced() {
    let (bom, reference) = reference_bom();
    let mut failures = Vec::new();
    for tab in &reference.roofs {
        let ours = bom
            .faces
            .iter()
            .find(|f| f.face_id.as_deref() == Some(tab.id.as_str()))
            .unwrap();
        check(&tab.id, ours, &tab.lines, &mut failures);
    }
    check("total", &bom.total, &reference.total.lines, &mut failures);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Here the project-wide cut saves nothing: the face tabs add up to the total (128 bars).
    assert!(bom.saved_bars.is_empty(), "{:?}", bom.saved_bars);
}

#[test]
fn top_rail_lengths_give_the_module_counts() {
    assert_eq!(modules_on(2.388), 2);
    assert_eq!(modules_on(1.234), 1);
}
