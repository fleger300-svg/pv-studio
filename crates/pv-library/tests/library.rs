//! Library store: seed, CRUD, revisions, search, prices, datasheets, closure.

use pv_library::{ChangeAuthor, LibraryStore, SearchQuery};
use pv_model::command::keys;
use pv_model::component::{
    presets, resolve_mounting, ComponentFamily, ComponentSource, Currency, Descriptions, IsoDate,
    ModuleSpec, PartKind, PartSpec, PriceEntry,
};
use pv_model::{CommandError, Component, ComponentId, ComponentSpec, MountingSystem};

fn date(s: &str) -> IsoDate {
    IsoDate::parse(s).expect("date")
}

fn key(e: pv_library::LibraryError) -> String {
    CommandError::from(e).key
}

fn module(label: &str, pmax: f64) -> Component {
    let mut c = presets::lr7_54hvh_490m();
    c.id = ComponentId::new();
    c.label = label.into();
    c.manufacturer = "Test Solar".into();
    c.model = label.into();
    if let ComponentSpec::Module(ModuleSpec {
        electrical: Some(e),
        ..
    }) = &mut c.spec
    {
        e.pmax_w = pmax;
    }
    c
}

fn price(component: ComponentId, supplier: &str, cents: i64, from: &str) -> PriceEntry {
    PriceEntry {
        component,
        supplier: supplier.into(),
        list_price_cents: cents,
        currency: Currency::Chf,
        valid_from: date(from),
        valid_to: None,
    }
}

#[test]
fn base_catalogue_is_seeded_once_and_rebuilds_k2_single_rail() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("library.sqlite");
    let n = presets::all().len() + pv_library::catalog::components().expect("catalogue").len();
    {
        let lib = LibraryStore::open(&path).expect("open");
        let all = lib.search(&SearchQuery::default()).expect("search");
        assert_eq!(all.len(), n);
    }
    let mut lib = LibraryStore::open(&path).expect("reopen");
    assert_eq!(lib.seed().expect("seed"), 0, "seeding is idempotent");

    let snaps = lib
        .closure(&[presets::K2_SINGLE_RAIL], &date("2026-11-11"))
        .expect("closure");
    // 5 parts (the rail once), then the system.
    assert_eq!(snaps.len(), 6);
    assert_eq!(
        snaps.last().map(|s| s.component.id),
        Some(presets::K2_SINGLE_RAIL)
    );
    let module = lib
        .get(presets::LR7_54HVH_490M)
        .expect("get")
        .expect("module")
        .component;
    let system = &snaps.last().expect("system").component;
    let resolved = resolve_mounting(system, &module, snaps.as_slice()).expect("resolve");
    assert_eq!(resolved, MountingSystem::k2_single_rail());
    assert!(module.verified);
}

#[test]
fn updates_are_revisions_with_history() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    let mut m = lib
        .create(module("Test 400", 400.0), ChangeAuthor::User, "new")
        .expect("create");
    assert_eq!(m.revision, 1);
    // Unchanged: nothing stored.
    let same = lib
        .update(m.clone(), ChangeAuthor::User, "noop")
        .expect("noop");
    assert_eq!(same.revision, 1);

    m.descriptions = Descriptions {
        fr: Some("Module de test".into()),
        ..Descriptions::default()
    };
    let m2 = lib
        .update(m.clone(), ChangeAuthor::User, "description")
        .expect("update");
    assert_eq!(m2.revision, 2);

    // Editing the stale revision 1 again is refused.
    let err = lib
        .update(m.clone(), ChangeAuthor::User, "stale")
        .expect_err("stale");
    assert_eq!(key(err), keys::COMPONENT_STALE_REVISION);

    let history = lib.revisions(m.id).expect("history");
    assert_eq!(
        history.iter().map(|r| r.revision).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(history[1].note, "description");
    let first = lib.get_revision(m.id, 1).expect("rev").expect("rev 1");
    assert_eq!(first.descriptions, Descriptions::default());
    assert_eq!(lib.get(m.id).expect("get").expect("entry").component, m2);

    let err = lib
        .create(m2, ChangeAuthor::User, "dup")
        .expect_err("exists");
    assert_eq!(key(err), keys::COMPONENT_EXISTS);
}

#[test]
fn invalid_components_and_broken_systems_are_refused() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    let mut bad = module("Bad", 400.0);
    if let ComponentSpec::Module(m) = &mut bad.spec {
        m.width_m = -1.0;
    }
    let err = lib
        .create(bad, ChangeAuthor::User, "")
        .expect_err("invalid");
    assert_eq!(key(err), keys::COMPONENT_INVALID);

    // A system referring to a part that is not in the library.
    let mut system = presets::k2_single_rail();
    system.id = ComponentId::new();
    if let ComponentSpec::MountingSystem(s) = &mut system.spec {
        s.parts[0].part = ComponentId::new();
    }
    let err = lib
        .create(system, ChangeAuthor::User, "")
        .expect_err("unknown part");
    assert_eq!(key(err), keys::COMPONENT_UNKNOWN);
}

#[test]
fn search_filters_on_generated_columns() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    for (label, p) in [
        ("Alpha 410", 410.0),
        ("Beta 450", 450.0),
        ("Gamma 500", 500.0),
    ] {
        lib.create(module(label, p), ChangeAuthor::User, "")
            .expect("create");
    }
    let q = |lib: &LibraryStore, query: SearchQuery| -> Vec<String> {
        lib.search(&query)
            .expect("search")
            .into_iter()
            .map(|s| s.label)
            .collect()
    };
    let modules = SearchQuery {
        family: Some(ComponentFamily::Module),
        manufacturer: Some("Test Solar".into()),
        ..SearchQuery::default()
    };
    assert_eq!(q(&lib, modules.clone()).len(), 3);
    assert_eq!(
        q(
            &lib,
            SearchQuery {
                min_pmax_w: Some(440.0),
                max_pmax_w: Some(495.0),
                ..modules.clone()
            }
        ),
        ["Beta 450"]
    );
    assert_eq!(
        q(
            &lib,
            SearchQuery {
                text: Some("clamp 25-40".into()),
                ..SearchQuery::default()
            }
        ),
        ["K2 Clamp EC 25-40 Black", "K2 Clamp MC 25-40 Black"]
    );
    assert_eq!(
        q(
            &lib,
            SearchQuery {
                text: Some("2004393".into()),
                ..SearchQuery::default()
            }
        ),
        ["SingleRail 36; 4.80 m"]
    );
    assert_eq!(
        q(
            &lib,
            SearchQuery {
                part_kind: Some("rail".into()),
                ..SearchQuery::default()
            }
        ),
        ["SingleRail 36; 4.80 m"]
    );
    // `%` is a literal, not a wildcard.
    assert!(q(
        &lib,
        SearchQuery {
            text: Some("%".into()),
            ..SearchQuery::default()
        }
    )
    .is_empty());

    let alpha = lib.search(&SearchQuery {
        text: Some("alpha".into()),
        ..SearchQuery::default()
    });
    let id = alpha.expect("search")[0].id;
    lib.set_archived(id, true).expect("archive");
    assert_eq!(
        q(&lib, modules.clone()).len(),
        2,
        "archived components are hidden"
    );
    assert_eq!(
        q(
            &lib,
            SearchQuery {
                include_archived: true,
                ..modules
            }
        )
        .len(),
        3
    );
    assert!(lib.get(id).expect("get").expect("still readable").archived);
}

#[test]
fn price_periods_never_overlap_and_the_preferred_supplier_wins() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    let rail = presets::K2_SINGLE_RAIL_36;
    lib.add_price(&price(rail, "K2 Systems", 4_000, "2025-01-01"))
        .expect("p1");
    lib.add_price(&price(rail, "K2 Systems", 4_250, "2026-01-01"))
        .expect("p2");
    // Inserted between two periods: ends before the next one.
    lib.add_price(&price(rail, "K2 Systems", 4_100, "2025-07-01"))
        .expect("p3");
    lib.add_price(&price(rail, "Wholesaler", 3_900, "2026-01-01"))
        .expect("p4");

    let periods: Vec<(i64, String, Option<String>)> = lib
        .prices(rail)
        .expect("prices")
        .into_iter()
        .filter(|p| p.supplier == "K2 Systems")
        .map(|p| {
            (
                p.list_price_cents,
                p.valid_from.to_string(),
                p.valid_to.map(|d| d.to_string()),
            )
        })
        .collect();
    assert_eq!(
        periods,
        [
            (4_000, "2025-01-01".into(), Some("2025-06-30".into())),
            (4_100, "2025-07-01".into(), Some("2025-12-31".into())),
            (4_250, "2026-01-01".into(), None),
        ]
    );
    let on = |lib: &LibraryStore, d: &str| {
        lib.price_on(rail, &date(d))
            .expect("price")
            .map(|p| p.list_price_cents)
    };
    assert_eq!(on(&lib, "2025-08-15"), Some(4_100));
    // Same start date: sorted by supplier name without preference.
    assert_eq!(on(&lib, "2026-11-11"), Some(4_250));
    lib.set_preferred_supplier(Some("Wholesaler"))
        .expect("pref");
    assert_eq!(on(&lib, "2026-11-11"), Some(3_900));
    assert_eq!(
        lib.preferred_supplier().expect("pref").as_deref(),
        Some("Wholesaler")
    );
    lib.set_preferred_supplier(Some("K2 Systems"))
        .expect("pref");
    assert_eq!(on(&lib, "2026-11-11"), Some(4_250));

    // The closure freezes the price valid on the project date.
    let snaps = lib
        .closure(&[presets::K2_SINGLE_RAIL], &date("2025-08-15"))
        .expect("closure");
    let frozen = snaps.iter().find(|s| s.component.id == rail).expect("rail");
    assert_eq!(
        frozen.price.as_ref().map(|p| p.list_price_cents),
        Some(4_100)
    );

    let mut negative = price(rail, "K2 Systems", -1, "2027-01-01");
    assert_eq!(
        key(lib.add_price(&negative).expect_err("neg")),
        keys::COMPONENT_INVALID
    );
    negative.list_price_cents = 1;
    negative.component = ComponentId::new();
    assert_eq!(
        key(lib.add_price(&negative).expect_err("unknown")),
        keys::COMPONENT_UNKNOWN
    );
}

#[test]
fn datasheets_are_deduplicated() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    let a = lib
        .put_blob(b"%PDF-1.7 datasheet", "application/pdf")
        .expect("a");
    let b = lib
        .put_blob(b"%PDF-1.7 datasheet", "application/pdf")
        .expect("b");
    let c = lib
        .put_blob(b"%PDF-1.7 other", "application/pdf")
        .expect("c");
    assert_eq!(a, b);
    assert_ne!(a, c);
    let blob = lib.blob(a).expect("blob").expect("exists");
    assert_eq!(blob.bytes, b"%PDF-1.7 datasheet");
    assert_eq!(blob.media_type, "application/pdf");
}

#[test]
fn new_parts_and_systems_can_be_created() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    let splice = Component {
        id: ComponentId::new(),
        revision: 1,
        manufacturer: "K2 Systems".into(),
        model: "SingleRail 36 Connector".into(),
        article_no: "2004394".into(),
        label: "SingleRail 36 Connector Set".into(),
        descriptions: Descriptions::default(),
        source: ComponentSource::Manual,
        verified: false,
        datasheet: None,
        spec: ComponentSpec::MountingPart(PartSpec {
            unit_mass_kg: 0.2,
            pack_qty: 10,
            kind: PartKind::Splice,
        }),
    };
    let splice = lib.create(splice, ChangeAuthor::User, "").expect("splice");
    let mut system = presets::k2_single_rail();
    system.id = ComponentId::new();
    system.label = "K2 SingleRail with connectors".into();
    if let ComponentSpec::MountingSystem(s) = &mut system.spec {
        s.parts.push(pv_model::component::PartRuleRef {
            part: splice.id,
            driver: pv_model::Driver::RailJoint {
                layer: pv_model::RailLayerKind::Top,
                stock_length_m: 4.8,
            },
            per: 1,
        });
    }
    lib.create(system.clone(), ChangeAuthor::User, "")
        .expect("system");
    let snaps = lib
        .closure(&[system.id], &date("2026-11-11"))
        .expect("closure");
    assert_eq!(snaps.len(), 7);
}

#[test]
fn the_copilot_never_verifies() {
    let mut lib = LibraryStore::open_in_memory().expect("open");
    let mut m = module("AI module", 450.0);
    m.source = ComponentSource::AiExtracted;
    m.verified = true;
    let created = lib
        .create(m, ChangeAuthor::Copilot, "datasheet")
        .expect("create");
    assert!(!created.verified, "rev 1 by the copilot stays unverified");
    let mut again = created.clone();
    again.verified = true;
    again.label = "AI module (checked)".into();
    let by_copilot = lib
        .update(again.clone(), ChangeAuthor::Copilot, "")
        .expect("rev 2");
    assert!(!by_copilot.verified);
    again.revision = by_copilot.revision;
    let by_user = lib
        .update(again, ChangeAuthor::User, "checked")
        .expect("rev 3");
    assert!(by_user.verified, "only the user verifies");
    let authors: Vec<ChangeAuthor> = lib
        .revisions(by_user.id)
        .expect("history")
        .into_iter()
        .map(|r| r.author)
        .collect();
    assert_eq!(
        authors,
        [
            ChangeAuthor::Copilot,
            ChangeAuthor::Copilot,
            ChangeAuthor::User
        ]
    );
}
