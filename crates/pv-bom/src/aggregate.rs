//! Fields → faces (exact quantities) → project (by article), with the rail cuts.

use std::collections::{BTreeMap, HashMap};

use pv_model::{Article, Driver, Pricing, RailLayerKind};

use crate::counts::{FaceRef, FieldCounts};
use crate::cutting::cut;
use crate::error::BomError;
use crate::pricing::{cents, sale_units, unit_price_cents};
use crate::rules::{counted, rail_pieces};
use crate::table::{Bom, BomLine, BomTable, SavedBars};

/// Quantities of one article.
struct Acc {
    article: Article,
    /// Counted pieces per face.
    counted: Vec<u64>,
    /// Bars per face (each face cut alone).
    bars_per_face: Vec<u64>,
    /// Bars of the project-wide cuts.
    bars_project: u64,
}

impl Acc {
    fn face_quantity(&self, f: usize) -> u64 {
        self.counted[f] + self.bars_per_face[f]
    }

    fn total(&self) -> u64 {
        self.counted.iter().sum::<u64>() + self.bars_project
    }
}

/// Pieces of one cut: an article, a layer, a bar length and a saw cut.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct CutKey {
    acc: usize,
    layer: RailLayerKind,
    stock_bits: u64,
    kerf_bits: u64,
}

fn to_u32(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn line(article: &Article, quantity: u64, pricing: &Pricing) -> Result<BomLine, BomError> {
    let rate = pricing.rate_for(&article.manufacturer);
    let unit = unit_price_cents(article, rate)?;
    let price = i64::try_from(quantity)
        .unwrap_or(i64::MAX)
        .saturating_mul(unit);
    Ok(BomLine {
        article_no: article.article_no.clone(),
        manufacturer: article.manufacturer.clone(),
        label: article.label.clone(),
        descriptions: article.descriptions.clone(),
        quantity: to_u32(quantity),
        // Masses to the milligram: no floating-point noise in reports.
        mass_kg: (quantity as f64 * article.unit_mass_kg * 1e6).round() / 1e6,
        sale_units: to_u32(sale_units(article, quantity)?),
        pack_qty: article.pack_qty,
        list_price: cents(article.list_price) as f64 / 100.0,
        discount_rate: rate,
        unit_price: unit as f64 / 100.0,
        price: price as f64 / 100.0,
    })
}

fn table(
    face: Option<&FaceRef>,
    title: String,
    lines: impl Iterator<Item = (Article, u64)>,
    pricing: &Pricing,
) -> Result<BomTable, BomError> {
    let lines = lines
        .filter(|(_, q)| *q > 0)
        .map(|(a, q)| line(&a, q, pricing))
        .collect::<Result<Vec<_>, _>>()?;
    let mass_kg = (lines.iter().map(|l| l.mass_kg).sum::<f64>() * 1e6).round() / 1e6;
    let price = lines.iter().map(|l| cents(l.price)).sum::<i64>() as f64 / 100.0;
    Ok(BomTable {
        face_id: face.map(|f| f.id.clone()),
        title,
        lines,
        mass_kg,
        price,
    })
}

/// The bill of materials of the given fields.
///
/// Faces and articles keep their order of first appearance (fields in input order, parts in
/// the order of their system's rules).
///
/// # Errors
/// A rail piece longer than its bar, an invalid bar, a pack of 0, an invalid discount.
pub fn compute(
    fields: &[FieldCounts],
    pricing: &Pricing,
    total_title: &str,
) -> Result<Bom, BomError> {
    // Faces, in order.
    let mut faces: Vec<&FaceRef> = Vec::new();
    let mut face_of: HashMap<&str, usize> = HashMap::new();
    for f in fields {
        if !face_of.contains_key(f.face.id.as_str()) {
            face_of.insert(&f.face.id, faces.len());
            faces.push(&f.face);
        }
    }
    let n_faces = faces.len();

    // Articles and counted quantities; rail pieces gathered per cut.
    let mut accs: Vec<Acc> = Vec::new();
    let mut acc_of: HashMap<(String, String), usize> = HashMap::new();
    let mut cuts: BTreeMap<CutKey, (Vec<Vec<f64>>, Vec<f64>)> = BTreeMap::new();
    for field in fields {
        let fi = face_of[field.face.id.as_str()];
        for rule in &field.parts {
            let key = (
                rule.article.manufacturer.clone(),
                rule.article.article_no.clone(),
            );
            let ai = *acc_of.entry(key).or_insert_with(|| {
                accs.push(Acc {
                    article: rule.article.clone(),
                    counted: vec![0; n_faces],
                    bars_per_face: vec![0; n_faces],
                    bars_project: 0,
                });
                accs.len() - 1
            });
            if let Some(q) = counted(rule, field) {
                accs[ai].counted[fi] += q;
            } else if let Driver::RailBars {
                layer,
                stock_length_m,
                kerf_m,
            } = rule.driver
            {
                let (per_face, all) = cuts
                    .entry(CutKey {
                        acc: ai,
                        layer,
                        stock_bits: stock_length_m.to_bits(),
                        kerf_bits: kerf_m.to_bits(),
                    })
                    .or_insert_with(|| (vec![Vec::new(); n_faces], Vec::new()));
                let pieces = rail_pieces(layer, field);
                per_face[fi].extend_from_slice(&pieces);
                all.extend(pieces);
            }
        }
    }

    // Cuts: each face alone (face tabs), and the whole project (Total).
    for (k, (per_face, all)) in &cuts {
        let (stock, kerf) = (f64::from_bits(k.stock_bits), f64::from_bits(k.kerf_bits));
        let acc = &mut accs[k.acc];
        let no = acc.article.article_no.clone();
        for (fi, pieces) in per_face.iter().enumerate() {
            acc.bars_per_face[fi] += cut(&no, pieces, stock, kerf)?.bar_count();
        }
        acc.bars_project += cut(&no, all, stock, kerf)?.bar_count();
    }

    let faces_tables = faces
        .iter()
        .enumerate()
        .map(|(fi, face)| {
            table(
                Some(face),
                face.name.clone(),
                accs.iter()
                    .map(|a| (a.article.clone(), a.face_quantity(fi))),
                pricing,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let total = table(
        None,
        total_title.to_owned(),
        accs.iter().map(|a| (a.article.clone(), a.total())),
        pricing,
    )?;
    let saved_bars = accs
        .iter()
        .filter(|a| a.bars_per_face.iter().sum::<u64>() > a.bars_project)
        .map(|a| SavedBars {
            article_no: a.article.article_no.clone(),
            per_faces: to_u32(a.bars_per_face.iter().sum()),
            project: to_u32(a.bars_project),
        })
        .collect();
    Ok(Bom {
        faces: faces_tables,
        total,
        saved_bars,
    })
}
