//! Price list import and update proposals.

use pv_library::import::{ImportCell, ImportRequest, ImportSheet};
use pv_library::snapshot::accept_command;
use pv_library::{ChangeAuthor, LibraryStore};
use pv_model::component::{presets, Currency, IsoDate};
use pv_model::ComponentSpec;
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    sheets: Vec<ImportSheet>,
}

fn synthetic() -> Vec<ImportSheet> {
    let f: Fixture = serde_json::from_str(include_str!(
        "../../../testdata/library/k2_list_synthetic.json"
    ))
    .expect("fixture");
    f.sheets
}

fn date(s: &str) -> IsoDate {
    IsoDate::parse(s).expect("date")
}

fn request(sheets: Vec<ImportSheet>, from: &str, create_missing: bool) -> ImportRequest {
    ImportRequest {
        manufacturer: "K2 Systems".into(),
        supplier: "K2 Systems".into(),
        currency: Currency::Chf,
        valid_from: date(from),
        create_missing,
        sheets,
    }
}

fn text(s: &str) -> ImportCell {
    ImportCell::Text(s.into())
}

#[test]
fn the_synthetic_k2_list_prices_the_base_catalogue() {
    let mut lib = LibraryStore::open_in_memory().expect("lib");
    let req = request(synthetic(), "2026-01-01", false);
    let preview = lib.preview_import(&req).expect("preview");
    assert_eq!(
        preview.priced,
        ["2003024", "2003145", "2004148", "2004393", "2004545"]
    );
    assert_eq!(preview.unknown, ["2004394"]);
    assert_eq!(preview.skipped_sheets, ["Notes"]);
    assert!(preview.conflicts.is_empty() && preview.differences.is_empty());
    // A preview changes nothing.
    assert_eq!(
        lib.price_on(presets::K2_SINGLE_RAIL_36, &date("2026-06-01"))
            .expect("p"),
        None
    );

    let report = lib.import_prices(&req).expect("import");
    assert_eq!(report, preview);
    let rail = lib
        .price_on(presets::K2_SINGLE_RAIL_36, &date("2026-06-01"))
        .expect("price")
        .expect("priced");
    assert_eq!(
        rail.list_price_cents, 5_709,
        "scaled reference price, to the cent"
    );
    assert_eq!(
        lib.price_on(presets::K2_SINGLE_RAIL_36, &date("2025-12-31"))
            .expect("p"),
        None
    );

    // Unknown articles become unverified parts when asked.
    let report = lib
        .import_prices(&request(synthetic(), "2026-01-01", true))
        .expect("import");
    assert_eq!(report.created, ["2004394"]);
    let created = lib
        .search(&pv_library::SearchQuery {
            text: Some("2004394".into()),
            ..Default::default()
        })
        .expect("search");
    assert_eq!(created.len(), 1);
    assert!(!created[0].verified);
}

#[test]
fn headers_in_german_text_numbers_conflicts_and_differences() {
    let mut lib = LibraryStore::open_in_memory().expect("lib");
    let header = vec![
        text("Art.-Nr."),
        text("Bezeichnung"),
        text("Menge pro Verkaufseinheit"),
        text("Listenpreis"),
    ];
    let sheet = |name: &str, rows: Vec<Vec<ImportCell>>| ImportSheet {
        name: name.into(),
        rows: [
            vec![vec![text("K2 Base"), ImportCell::Empty], header.clone()],
            rows,
        ]
        .concat(),
    };
    let sheets = vec![
        sheet(
            "Total",
            vec![
                // Article number stored as a number, price as text with a comma.
                vec![
                    ImportCell::Number(2_004_393.0),
                    text("SingleRail 36; 4.80 m"),
                    ImportCell::Number(25.0),
                    text("57,09"),
                ],
                vec![
                    text("2004148"),
                    text("K2 Clamp MC 25-40 Black"),
                    ImportCell::Number(20.0),
                    ImportCell::Number(3.1),
                ],
            ],
        ),
        sheet(
            "Dach 1",
            vec![vec![
                text("2004148"),
                text("K2 Clamp MC 25-40 Black"),
                ImportCell::Number(20.0),
                ImportCell::Number(3.2),
            ]],
        ),
    ];
    let report = lib
        .import_prices(&request(sheets, "2026-01-01", false))
        .expect("import");
    assert_eq!(report.priced, ["2004393"]);
    assert_eq!(report.conflicts, ["2004148"], "two prices for one article");
    assert_eq!(report.differences.len(), 1);
    assert_eq!(report.differences[0].field, "packQty");
    assert_eq!(report.differences[0].list, "25");
    // The difference is reported, not applied.
    let rail = lib
        .get(presets::K2_SINGLE_RAIL_36)
        .expect("get")
        .expect("rail");
    let ComponentSpec::MountingPart(p) = &rail.component.spec else {
        panic!("part")
    };
    assert_eq!(p.pack_qty, 20);
    assert_eq!(rail.component.revision, 1);
}

#[test]
fn proposals_list_new_prices_and_revisions_and_nothing_else() {
    let mut lib = LibraryStore::open_in_memory().expect("lib");
    lib.import_prices(&request(synthetic(), "2026-01-01", false))
        .expect("import");
    let today = date("2026-03-01");
    let frozen = lib
        .closure(&[presets::LR7_54HVH_490M, presets::K2_SINGLE_RAIL], &today)
        .expect("closure");
    assert!(lib
        .update_proposals(&frozen, &today)
        .expect("none")
        .is_empty());

    // A new list from July: a proposal for the price only, once the date is reached.
    let mut sheets = synthetic();
    for row in &mut sheets[0].rows[1..] {
        if let ImportCell::Number(p) = &mut row[6] {
            *p = (*p * 1.05 * 100.0).round() / 100.0;
        }
    }
    sheets.truncate(1);
    lib.import_prices(&request(sheets, "2026-07-01", false))
        .expect("new list");
    assert!(lib
        .update_proposals(&frozen, &today)
        .expect("before")
        .is_empty());
    let july = date("2026-07-01");
    let proposals = lib.update_proposals(&frozen, &july).expect("after");
    assert_eq!(proposals.len(), 5, "the five priced parts");
    let rail = proposals
        .iter()
        .find(|p| p.component == presets::K2_SINGLE_RAIL_36)
        .expect("rail");
    assert!(rail.changes.is_empty() && rail.price_changed());
    assert_eq!(
        rail.price_before.as_ref().map(|p| p.list_price_cents),
        Some(5_709)
    );
    assert_eq!(
        rail.price_after.as_ref().map(|p| p.list_price_cents),
        Some(5_994)
    );

    // A new revision of the module: its changed fields are listed.
    let mut module = lib
        .get(presets::LR7_54HVH_490M)
        .expect("get")
        .expect("module")
        .component;
    if let ComponentSpec::Module(m) = &mut module.spec {
        m.mass_kg = 22.0;
    }
    lib.update(module, ChangeAuthor::User, "datasheet v2")
        .expect("revision");
    let proposals = lib.update_proposals(&frozen, &today).expect("module");
    assert_eq!(proposals.len(), 1);
    let p = &proposals[0];
    assert_eq!((p.from_revision, p.to_revision), (1, 2));
    assert_eq!(p.changes.len(), 1);
    assert_eq!(p.changes[0].path, "spec.massKg");
    assert_eq!(p.changes[0].after.as_deref(), Some("22.0"));
    let pv_model::Command::UpdateComponents { snapshots } = accept_command(&proposals) else {
        panic!("update command")
    };
    assert_eq!(snapshots[0].component.revision, 2);
}
