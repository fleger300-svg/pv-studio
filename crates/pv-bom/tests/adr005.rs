//! ADR-005 and pricing: sale units on the total, prices on pieces, discounts per
//! manufacturer, cents; bars saved by the project-wide cut.

use std::collections::BTreeMap;

use pv_bom::{compute, FaceRef, FieldCounts, TopRail};
use pv_model::{Article, Driver, ManufacturerDiscount, PartRule, Pricing, RailLayerKind};

fn article(no: &str, manufacturer: &str, pack: u32, price: f64) -> Article {
    Article {
        article_no: no.into(),
        manufacturer: manufacturer.into(),
        label: format!("article {no}"),
        descriptions: BTreeMap::new(),
        unit_mass_kg: 0.5,
        pack_qty: pack,
        list_price: price,
        currency: "CHF".into(),
    }
}

fn field(face: &str, fixations: u32, parts: Vec<PartRule>) -> FieldCounts {
    FieldCounts {
        face: FaceRef {
            id: face.into(),
            name: format!("Face {face}"),
        },
        modules: 0,
        top_rails: vec![],
        bottom_rails: vec![],
        climbers: 0,
        fixations,
        ballast_blocks: 0,
        parts,
    }
}

fn per_fixation(a: &Article) -> Vec<PartRule> {
    vec![PartRule {
        article: a.clone(),
        driver: Driver::Fixation,
        per: 1,
    }]
}

fn quantities(fields: &[FieldCounts], pricing: &Pricing) -> (Vec<(u32, u32)>, (u32, u32)) {
    let bom = compute(fields, pricing, "Total").unwrap();
    let faces = bom
        .faces
        .iter()
        .map(|t| (t.lines[0].quantity, t.lines[0].sale_units))
        .collect();
    let total = (bom.total.lines[0].quantity, bom.total.lines[0].sale_units);
    (faces, total)
}

#[test]
fn sale_units_are_computed_once_on_the_total() {
    // Architecture example: 128 rails in packs of 20 → 7 packs, not 2 + 2 + 2 + 2 = 8.
    let rail = article("R", "K2 Systems", 20, 10.0);
    let fields: Vec<_> = [40, 28, 28, 32]
        .iter()
        .enumerate()
        .map(|(i, &n)| field(&i.to_string(), n, per_fixation(&rail)))
        .collect();
    let (faces, total) = quantities(&fields, &Pricing::default());
    assert_eq!(faces, vec![(40, 2), (28, 2), (28, 2), (32, 2)]);
    assert_eq!(total, (128, 7));

    // K2 standing-seam clamps of the reference: 3 + 3 + 2 + 3 = 11 packs per face, 10 in total.
    let clamp = article("C", "K2 Systems", 50, 1.0);
    let fields: Vec<_> = [136, 105, 99, 126]
        .iter()
        .enumerate()
        .map(|(i, &n)| field(&i.to_string(), n, per_fixation(&clamp)))
        .collect();
    let (faces, total) = quantities(&fields, &Pricing::default());
    assert_eq!(faces.iter().map(|f| f.1).sum::<u32>(), 11);
    assert_eq!(total, (466, 10));
}

#[test]
fn prices_are_pieces_times_the_discounted_unit_price() {
    let k2 = article("C", "K2 Systems", 50, 9.29);
    let other = article("X", "Other", 10, 9.29);
    let mut parts = per_fixation(&k2);
    parts.extend(per_fixation(&other));
    let fields = vec![field("a", 466, parts)];

    let bom = compute(&fields, &Pricing::default(), "Total").unwrap();
    let l = &bom.total.lines[0];
    assert_eq!((l.unit_price, l.price), (9.29, 4329.14)); // on pieces, not on 10 packs

    // −35 % on K2 only: 9.29 × 0.65 = 6.0385 → 6.04 per piece; 466 × 6.04 = 2 814.64.
    let pricing = Pricing {
        discounts: vec![ManufacturerDiscount {
            manufacturer: "K2 Systems".into(),
            rate: 0.35,
        }],
    };
    let bom = compute(&fields, &pricing, "Total").unwrap();
    let (k, o) = (&bom.total.lines[0], &bom.total.lines[1]);
    assert_eq!(
        (k.discount_rate, k.unit_price, k.price),
        (0.35, 6.04, 2814.64)
    );
    assert_eq!(
        (o.discount_rate, o.unit_price, o.price),
        (0.0, 9.29, 4329.14)
    );
    assert_eq!(bom.total.price, 7143.78);
    assert!((bom.total.mass_kg - 466.0).abs() < 1e-9);
}

#[test]
fn invalid_data_is_reported_with_its_key() {
    let fields = vec![field("a", 3, per_fixation(&article("Z", "K2", 0, 1.0)))];
    let e = compute(&fields, &Pricing::default(), "Total").unwrap_err();
    assert_eq!(e.key(), "errors.bom.zeroPack");

    let fields = vec![field("a", 3, per_fixation(&article("C", "K2", 10, 1.0)))];
    let pricing = Pricing {
        discounts: vec![ManufacturerDiscount {
            manufacturer: "K2".into(),
            rate: 1.0,
        }],
    };
    let e = compute(&fields, &pricing, "Total").unwrap_err();
    assert_eq!(e.key(), "errors.bom.invalidDiscount");
}

#[test]
fn offcuts_reused_across_faces_are_reported() {
    let rail = article("R", "K2 Systems", 20, 50.0);
    let bars = vec![PartRule {
        article: rail,
        driver: Driver::RailBars {
            layer: RailLayerKind::Top,
            stock_length_m: 4.8,
            kerf_m: 0.0,
        },
        per: 1,
    }];
    let face = |id: &str, len: f64| FieldCounts {
        top_rails: vec![TopRail {
            length_m: len,
            modules: 1,
        }],
        ..field(id, 0, bars.clone())
    };
    // Each face alone needs a bar; on the project, 3.0 m + 1.7 m fit in one 4.80 m bar.
    let bom = compute(
        &[face("a", 3.0), face("b", 1.7)],
        &Pricing::default(),
        "Total",
    )
    .unwrap();
    assert_eq!(bom.faces[0].lines[0].quantity, 1);
    assert_eq!(bom.faces[1].lines[0].quantity, 1);
    assert_eq!(bom.total.lines[0].quantity, 1);
    assert_eq!(bom.saved_bars.len(), 1);
    assert_eq!(
        (bom.saved_bars[0].per_faces, bom.saved_bars[0].project),
        (2, 1)
    );
}
