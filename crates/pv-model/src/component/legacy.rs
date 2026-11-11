//! Converts a hard-coded [`MountingSystem`] (projects written before the component library,
//! schema ≤ v7) into components whose resolution gives back the same system.
//!
//! Ids are derived from the content (UUID v8 from two fingerprints): the same system, part or
//! module always gets the same id, so identical systems of several arrays are deduplicated
//! and migrating a file twice gives the same result. Data the old snapshot did not have is
//! filled neutrally and marked unverified: module frame height = [`MODULE_THICKNESS_M`],
//! clamps accept any frame, rails used only for statics get a 1 m bar.
//!
//! Exactness: the resolved system equals the original as long as list prices are given to
//! the cent and descriptions only use `fr`, `de`, `en` (true of every snapshot ever written).

use serde::Serialize;
use uuid::Uuid;

use super::{
    resolve_mounting, Component, ComponentSnapshot, ComponentSource, ComponentSpec, Currency,
    Descriptions, FixationCapacity, IsoDate, LengthRange, ModuleSpec, MountingStructure,
    MountingSystemSpec, PartKind, PartRuleRef, PartSpec, PriceEntry, SystemCompatibility,
    SystemStatics,
};
use crate::article::Article;
use crate::command::{keys, CommandError};
use crate::fingerprint::fingerprint;
use crate::ids::ComponentId;
use crate::layout::ArrayLayoutParams;
use crate::mounting::{Driver, MountingSystem, RailLayerKind, RailSectionSpec};
use crate::project::{Project, MODULE_THICKNESS_M};

/// Validity start of prices carried over from old snapshots.
const LEGACY_PRICE_DATE: &str = "2000-01-01";

/// Content-derived id.
fn derived_id<T: Serialize>(tag: &str, value: &T) -> ComponentId {
    let a = fingerprint(&(tag, "a", value));
    let b = fingerprint(&(tag, "b", value));
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&a.to_le_bytes());
    bytes[8..].copy_from_slice(&b.to_le_bytes());
    ComponentId(Uuid::new_v8(bytes))
}

fn invalid(field: &str) -> CommandError {
    CommandError::new(keys::COMPONENT_INVALID)
        .with_text("component", "legacy mounting")
        .with_text("field", field)
}

fn blank(manufacturer: &str, model: &str, no: &str, label: &str, spec: ComponentSpec) -> Component {
    Component {
        id: ComponentId(Uuid::nil()),
        revision: 1,
        manufacturer: manufacturer.into(),
        model: model.into(),
        article_no: no.into(),
        label: label.into(),
        descriptions: Descriptions::default(),
        source: ComponentSource::Import,
        verified: false,
        datasheet: None,
        spec,
    }
}

/// Gives `c` (and its price) its content-derived id.
fn identified(mut c: Component, price: Option<PriceEntry>) -> ComponentSnapshot {
    c.id = derived_id("component", &(&c, &price));
    let price = price.map(|p| PriceEntry {
        component: c.id,
        ..p
    });
    ComponentSnapshot {
        component: c,
        price,
    }
}

fn price_of(article: &Article) -> Result<Option<PriceEntry>, CommandError> {
    let currency = match article.currency.as_str() {
        "CHF" => Currency::Chf,
        "EUR" => Currency::Eur,
        _ => return Err(invalid("currency")),
    };
    if article.list_price == 0.0 && currency == Currency::default() {
        return Ok(None);
    }
    if !(article.list_price.is_finite() && article.list_price >= 0.0) {
        return Err(invalid("listPrice"));
    }
    #[allow(clippy::cast_possible_truncation)]
    let cents = (article.list_price * 100.0).round() as i64;
    Ok(Some(PriceEntry {
        component: ComponentId(Uuid::nil()),
        supplier: article.manufacturer.clone(),
        list_price_cents: cents,
        currency,
        valid_from: IsoDate::parse(LEGACY_PRICE_DATE)?,
        valid_to: None,
    }))
}

fn part_of(article: &Article, kind: PartKind) -> Result<ComponentSnapshot, CommandError> {
    let mut c = blank(
        &article.manufacturer,
        &article.label,
        &article.article_no,
        &article.label,
        ComponentSpec::MountingPart(PartSpec {
            unit_mass_kg: article.unit_mass_kg,
            pack_qty: article.pack_qty,
            kind,
        }),
    );
    let lang = |l: &str| article.descriptions.get(l).cloned();
    c.descriptions = Descriptions {
        fr: lang("fr"),
        de: lang("de"),
        en: lang("en"),
    };
    Ok(identified(c, price_of(article)?))
}

/// Rail part used only by the statics (no bar rule refers to it).
fn statics_rail(manufacturer: &str, section: &RailSectionSpec) -> ComponentSnapshot {
    identified(
        blank(
            manufacturer,
            &section.name,
            "",
            &section.name,
            ComponentSpec::MountingPart(PartSpec {
                unit_mass_kg: 0.0,
                pack_qty: 1,
                kind: PartKind::Rail {
                    stock_length_m: 1.0,
                    section: section.clone(),
                },
            }),
        ),
        None,
    )
}

/// Components equivalent to a legacy snapshot: `(module, system, parts)`.
pub struct LegacyComponents {
    pub module: ComponentSnapshot,
    pub system: ComponentSnapshot,
    /// Parts, before the system that uses them.
    pub parts: Vec<ComponentSnapshot>,
}

impl LegacyComponents {
    /// Parts, module, system: the order to adopt them in.
    #[must_use]
    pub fn into_snapshots(self) -> Vec<ComponentSnapshot> {
        let mut all = self.parts;
        all.push(self.module);
        all.push(self.system);
        all
    }
}

/// Converts `m`, used by an array whose modules are `length_m` × `width_m`.
///
/// # Errors
/// A snapshot that cannot be expressed as components (no rails nor ballast, both, bar rules
/// without rails, unknown currency, non-positive module mass).
pub fn components_from_mounting(
    m: &MountingSystem,
    length_m: f64,
    width_m: f64,
) -> Result<LegacyComponents, CommandError> {
    let manufacturer = m
        .parts
        .first()
        .map(|p| p.article.manufacturer.clone())
        .or_else(|| m.ballast.as_ref().map(|b| b.manufacturer.clone()))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "—".to_owned());

    let section = |layer: RailLayerKind| {
        m.rails.as_ref().map(|r| match layer {
            RailLayerKind::Top => &r.top_rail,
            RailLayerKind::Bottom => &r.bottom_rail,
        })
    };
    let mut parts: Vec<ComponentSnapshot> = Vec::new();
    let mut add = |s: ComponentSnapshot| -> ComponentId {
        let id = s.component.id;
        if !parts.iter().any(|p| p.component.id == id) {
            parts.push(s);
        }
        id
    };
    let mut rules = Vec::with_capacity(m.parts.len());
    let mut bar_part = [None, None];
    for rule in &m.parts {
        let kind = match rule.driver {
            Driver::Fixation => PartKind::Fixation { capacity: None },
            Driver::Climber => PartKind::Climber,
            Driver::TopRailEnd | Driver::TopRailInnerJoint => PartKind::Clamp {
                frame_height: LengthRange {
                    min_m: 0.0,
                    max_m: 1.0,
                },
            },
            Driver::Module => PartKind::Other,
            Driver::BallastBlock => PartKind::BallastBlock,
            Driver::RailJoint { .. } => PartKind::Splice,
            Driver::RailBars {
                layer,
                stock_length_m,
                ..
            } => PartKind::Rail {
                stock_length_m,
                section: section(layer).ok_or_else(|| invalid("rails"))?.clone(),
            },
        };
        let id = add(part_of(&rule.article, kind)?);
        if let Driver::RailBars { layer, .. } = rule.driver {
            bar_part[usize::from(layer == RailLayerKind::Bottom)].get_or_insert(id);
        }
        rules.push(PartRuleRef {
            part: id,
            driver: rule.driver,
            per: rule.per,
        });
    }

    let structure = match (&m.rails, &m.ballast) {
        (Some(r), None) => {
            let mut rail = |layer: RailLayerKind, s: &RailSectionSpec| {
                bar_part[usize::from(layer == RailLayerKind::Bottom)]
                    .unwrap_or_else(|| add(statics_rail(&manufacturer, s)))
            };
            let top_rail = rail(RailLayerKind::Top, &r.top_rail);
            let bottom_rail = rail(RailLayerKind::Bottom, &r.bottom_rail);
            let checked_fixation = r.fixation.as_ref().map(|f| {
                add(identified(
                    blank(
                        &manufacturer,
                        &f.name,
                        "",
                        &f.name,
                        ComponentSpec::MountingPart(PartSpec {
                            unit_mass_kg: 0.0,
                            pack_qty: 1,
                            kind: PartKind::Fixation {
                                capacity: Some(FixationCapacity {
                                    uplift_kn: f.uplift_kn,
                                    pressure_kn: f.pressure_kn,
                                    parallel_kn: f.parallel_kn,
                                }),
                            },
                        }),
                    ),
                    None,
                ))
            });
            MountingStructure::CrossRail {
                top_rail,
                bottom_rail,
                checked_fixation,
            }
        }
        (None, Some(b)) => MountingStructure::Ballasted { ballast: b.clone() },
        _ => return Err(invalid("structure")),
    };

    let system = identified(
        blank(
            &manufacturer,
            &m.name,
            "",
            &m.name,
            ComponentSpec::MountingSystem(Box::new(MountingSystemSpec {
                structure,
                defaults: ArrayLayoutParams::default(),
                statics: SystemStatics {
                    system_mass_per_module_kg: m.system_mass_per_module_kg,
                    loaded_area_m2: m.loaded_area_m2,
                    psi0_snow: m.psi0_snow,
                    cp: m.cp.clone(),
                },
                parts: rules,
                compatibility: SystemCompatibility::default(),
                approval: None,
            })),
        ),
        None,
    );
    let module = identified(
        blank(
            "—",
            "",
            "",
            &format!(
                "Module {length_m:.3} × {width_m:.3} m, {} kg",
                m.module_mass_kg
            ),
            ComponentSpec::Module(ModuleSpec {
                length_m,
                width_m,
                thickness_m: MODULE_THICKNESS_M,
                mass_kg: m.module_mass_kg,
                electrical: None,
            }),
        ),
        None,
    );
    let out = LegacyComponents {
        module,
        system,
        parts,
    };
    // Never adopt components that would not give the same system back.
    let snaps = out.parts.as_slice();
    let resolved = resolve_mounting(&out.system.component, &out.module.component, snaps)?;
    if resolved != *m {
        return Err(invalid("roundTrip"));
    }
    Ok(out)
}

/// Replaces the legacy snapshot of array `index` by components: adds the missing ones to the
/// project and sets the array's references.
///
/// # Errors
/// See [`components_from_mounting`]; an index out of range.
pub fn adopt_legacy_mounting(
    project: &mut Project,
    index: usize,
    m: &MountingSystem,
) -> Result<(), CommandError> {
    let array = project
        .arrays
        .get(index)
        .ok_or_else(|| CommandError::new(keys::ENTITY_UNKNOWN).with_text("kind", "array"))?;
    let legacy = components_from_mounting(m, array.module_length_m, array.module_width_m)?;
    let (module, system) = (legacy.module.component.id, legacy.system.component.id);
    for s in legacy.into_snapshots() {
        if project.component(s.component.id).is_none() {
            project.components.push(s);
        }
    }
    project.components.sort_by_key(|s| s.component.id);
    let array = &mut project.arrays[index];
    array.module = Some(module);
    array.mounting = Some(system);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mounting::{FixationSpec, PartRule};

    fn round_trip(m: &MountingSystem) -> LegacyComponents {
        let out = components_from_mounting(m, 1.8, 1.134).expect("convert");
        let again = components_from_mounting(m, 1.8, 1.134).expect("convert");
        assert_eq!(
            out.system.component.id, again.system.component.id,
            "deterministic"
        );
        out
    }

    #[test]
    fn presets_round_trip() {
        let single = round_trip(&MountingSystem::k2_single_rail());
        // The same rail serves both layers: 5 distinct parts.
        assert_eq!(single.parts.len(), 5);
        round_trip(&crate::stress::d_dome());
    }

    #[test]
    fn checked_fixations_prices_and_splices_round_trip() {
        let mut m = MountingSystem::k2_single_rail();
        if let Some(r) = m.rails.as_mut() {
            r.fixation = Some(FixationSpec {
                name: "Hanger bolt M10".into(),
                uplift_kn: 1.2,
                pressure_kn: 2.0,
                parallel_kn: 0.8,
            });
            r.bottom_rail.name = "Other rail".into();
        }
        m.parts[1].article.list_price = 42.5;
        m.parts[0]
            .article
            .descriptions
            .insert("fr".into(), "Pince".into());
        m.parts.push(PartRule {
            article: Article {
                list_price: 3.15,
                ..m.parts[3].article.clone()
            },
            driver: Driver::RailJoint {
                layer: RailLayerKind::Top,
                stock_length_m: 4.8,
            },
            per: 2,
        });
        let out = round_trip(&m);
        assert!(out.parts.iter().any(|p| p
            .price
            .as_ref()
            .is_some_and(|e| e.list_price_cents == 4_250)));
    }

    #[test]
    fn inexpressible_snapshots_are_refused() {
        let mut m = MountingSystem::k2_single_rail();
        m.ballast = crate::stress::d_dome().ballast;
        assert!(
            components_from_mounting(&m, 1.8, 1.134).is_err(),
            "rails and ballast"
        );
        let mut m = MountingSystem::k2_single_rail();
        m.parts[0].article.currency = "USD".into();
        assert!(components_from_mounting(&m, 1.8, 1.134).is_err());
    }
}
