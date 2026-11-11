//! C3: the K2 bill of materials to the cent with prices from the component library (the
//! synthetic K2 list imported, frozen at adoption), and no silent change: a newer price list
//! reaches the project only through an accepted, undoable update proposal.

mod common;

use common::*;
use pv_graph::{CancelToken, Graph, Instance};
use pv_library::import::{ImportCell, ImportRequest, ImportSheet};
use pv_library::snapshot::accept_command;
use pv_library::LibraryStore;
use pv_model::component::{presets, Currency, IsoDate};
use pv_model::Project;
use pv_pipeline::nodes::bom::ProjectBom;
use pv_pipeline::sync_changes;
use pv_store::Author;
use serde::Deserialize;

const SEAM_CLAMP: &str = "2003024";

#[derive(Deserialize)]
struct Fixture {
    sheets: Vec<ImportSheet>,
}

#[derive(Deserialize)]
struct Reference {
    roofs: Vec<Tab>,
}

#[derive(Deserialize)]
struct Tab {
    id: String,
    lines: Vec<RefLine>,
}

#[derive(Deserialize)]
struct RefLine {
    article_no: String,
    quantity: u32,
    price: f64,
}

fn date(s: &str) -> IsoDate {
    IsoDate::parse(s).unwrap()
}

fn synthetic() -> Vec<ImportSheet> {
    let f: Fixture = serde_json::from_str(include_str!(
        "../../../testdata/library/k2_list_synthetic.json"
    ))
    .unwrap();
    f.sheets
}

fn import(lib: &mut LibraryStore, sheets: Vec<ImportSheet>, from: &str) {
    lib.import_prices(&ImportRequest {
        manufacturer: "K2 Systems".into(),
        supplier: "K2 Systems".into(),
        currency: Currency::Chf,
        valid_from: date(from),
        create_missing: false,
        sheets,
    })
    .unwrap();
}

fn bom(g: &mut Graph) -> pv_bom::Bom {
    (*g.get::<ProjectBom>(Instance::Project).unwrap()).clone()
}

/// Lines of roof 1 against K2: quantities, and prices to the cent (seam clamps excluded:
/// documented quantity gap).
fn deviations(g: &mut Graph) -> (usize, Vec<String>) {
    let r: Reference =
        serde_json::from_str(include_str!("../../../testdata/k2/reference_bom.json")).unwrap();
    let tab = r.roofs.iter().find(|t| t.id == "roof-1").unwrap();
    let ours = bom(g);
    let mut failures = Vec::new();
    let mut compared = 0;
    for l in tab.lines.iter().filter(|l| l.article_no != SEAM_CLAMP) {
        let line = ours
            .total
            .lines
            .iter()
            .find(|x| x.article_no == l.article_no)
            .unwrap();
        compared += 2;
        if line.quantity != l.quantity {
            failures.push(format!(
                "{}: {} pieces, K2 {}",
                l.article_no, line.quantity, l.quantity
            ));
        }
        if (line.price - l.price).abs() >= 0.005 {
            failures.push(format!(
                "{}: {:.2}, K2 {:.2}",
                l.article_no, line.price, l.price
            ));
        }
    }
    (compared, failures)
}

/// Roof 1 with its components frozen from a library priced by the synthetic K2 list.
fn roof_1_from_library(lib: &LibraryStore, on: &str) -> Project {
    let mut p = ref_a_roof1();
    let mut frozen = lib
        .closure(
            &[presets::LR7_54HVH_490M, presets::K2_SINGLE_RAIL],
            &date(on),
        )
        .unwrap();
    frozen.sort_by_key(|s| s.component.id);
    p.components = frozen;
    p
}

#[test]
fn k2_bom_to_the_cent_with_imported_prices() {
    let mut lib = LibraryStore::open_in_memory().unwrap();
    import(&mut lib, synthetic(), "2026-01-01");
    let p = roof_1_from_library(&lib, "2026-03-01");
    let mut g = graph_for(&p);
    let (compared, failures) = deviations(&mut g);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(compared, 8);
}

#[test]
fn a_new_price_list_changes_nothing_until_accepted() {
    let mut lib = LibraryStore::open_in_memory().unwrap();
    import(&mut lib, synthetic(), "2026-01-01");
    let p = roof_1_from_library(&lib, "2026-03-01");
    let dir = tempfile::tempdir().unwrap();
    let mut store = store_for(&dir, "prices", &p);
    let mut g = graph_for(store.project());
    let before = bom(&mut g);

    // +5 % from July.
    let mut sheets = synthetic();
    sheets.truncate(1);
    for row in &mut sheets[0].rows[1..] {
        if let ImportCell::Number(price) = &mut row[6] {
            *price = (*price * 1.05 * 100.0).round() / 100.0;
        }
    }
    import(&mut lib, sheets, "2026-07-01");

    // The project still computes with its frozen prices.
    pv_pipeline::sync_project(&mut g, store.project()).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(bom(&mut g), before, "no silent change");

    // Accepting the proposals: one command, prices updated; undo restores them exactly.
    let proposals = lib
        .update_proposals(&store.project().components, &date("2026-07-01"))
        .unwrap();
    assert_eq!(proposals.len(), 5);
    let c = store
        .apply(&accept_command(&proposals), Author::User)
        .unwrap();
    assert!(c.components);
    sync_changes(&mut g, store.project(), &c).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    compare_with_full(&mut g, store.project()).unwrap();
    let after = bom(&mut g);
    assert!(after.total.price > before.total.price * 1.04);
    assert_eq!(
        after
            .total
            .lines
            .iter()
            .map(|l| l.quantity)
            .collect::<Vec<_>>(),
        before
            .total
            .lines
            .iter()
            .map(|l| l.quantity)
            .collect::<Vec<_>>()
    );
    assert!(lib
        .update_proposals(&store.project().components, &date("2026-07-01"))
        .unwrap()
        .is_empty());

    let c = store.undo().unwrap();
    sync_changes(&mut g, store.project(), &c).unwrap();
    g.run_auto(&CancelToken::new()).unwrap();
    assert_eq!(bom(&mut g), before);
}
