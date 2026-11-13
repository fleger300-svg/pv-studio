//! Rebuilds the engine types ([`MountingSystem`], [`Article`]) from components: the engines
//! (pv-layout, pv-statics, pv-bom) never see components.

use std::collections::BTreeMap;

use super::{
    Component, ComponentFamily, ComponentSnapshot, ComponentSpec, Currency, ModuleSpec,
    MountingStructure, MountingSystemSpec, PartKind, PartSpec, PriceEntry,
};
use crate::article::Article;
use crate::command::{keys, CommandError};
use crate::ids::ComponentId;
use crate::mounting::{Driver, FixationSpec, MountingSystem, PartRule, RailLayerKind, RailStatics};

/// Where resolution finds the components a system refers to, and their prices: a project's
/// snapshots, or the library at adoption time.
pub trait ComponentLookup {
    fn component(&self, id: ComponentId) -> Option<&Component>;
    fn price(&self, id: ComponentId) -> Option<&PriceEntry>;
}

impl ComponentLookup for [ComponentSnapshot] {
    fn component(&self, id: ComponentId) -> Option<&Component> {
        self.iter().map(|s| &s.component).find(|c| c.id == id)
    }

    fn price(&self, id: ComponentId) -> Option<&PriceEntry> {
        self.iter()
            .find(|s| s.component.id == id)
            .and_then(|s| s.price.as_ref())
    }
}

fn wrong_family(c: &Component, expected: ComponentFamily) -> CommandError {
    c.error(keys::COMPONENT_WRONG_FAMILY)
        .with_text("expected", expected.as_str())
}

pub(crate) fn find<L: ComponentLookup + ?Sized>(
    lookup: &L,
    id: ComponentId,
) -> Result<&Component, CommandError> {
    lookup
        .component(id)
        .ok_or_else(|| CommandError::new(keys::COMPONENT_UNKNOWN).with_text("id", id.to_string()))
}

pub(crate) fn find_part<L: ComponentLookup + ?Sized>(
    lookup: &L,
    id: ComponentId,
) -> Result<(&Component, &PartSpec), CommandError> {
    let c = find(lookup, id)?;
    match &c.spec {
        ComponentSpec::MountingPart(p) => Ok((c, p)),
        _ => Err(wrong_family(c, ComponentFamily::MountingPart)),
    }
}

pub(crate) fn module_spec(c: &Component) -> Result<&ModuleSpec, CommandError> {
    match &c.spec {
        ComponentSpec::Module(m) => Ok(m),
        _ => Err(wrong_family(c, ComponentFamily::Module)),
    }
}

pub(crate) fn system_spec(c: &Component) -> Result<&MountingSystemSpec, CommandError> {
    match &c.spec {
        ComponentSpec::MountingSystem(s) => Ok(s.as_ref()),
        _ => Err(wrong_family(c, ComponentFamily::MountingSystem)),
    }
}

/// The article of a part, at `price` (list price 0 in CHF when there is none).
#[must_use]
pub fn to_article(component: &Component, part: &PartSpec, price: Option<&PriceEntry>) -> Article {
    let d = &component.descriptions;
    let descriptions: BTreeMap<String, String> = [("fr", &d.fr), ("de", &d.de), ("en", &d.en)]
        .into_iter()
        .filter_map(|(lang, text)| text.clone().map(|t| (lang.to_owned(), t)))
        .collect();
    Article {
        article_no: component.article_no.clone(),
        manufacturer: component.manufacturer.clone(),
        label: component.label.clone(),
        descriptions,
        unit_mass_kg: part.unit_mass_kg,
        pack_qty: part.pack_qty,
        list_price: price.map_or(0.0, PriceEntry::list_price),
        currency: price
            .map_or(Currency::default(), |p| p.currency)
            .code()
            .to_owned(),
    }
}

/// camelCase name of a driver, for error parameters.
fn driver_name(driver: &Driver) -> &'static str {
    match driver {
        Driver::Fixation => "fixation",
        Driver::Climber => "climber",
        Driver::TopRailEnd => "topRailEnd",
        Driver::TopRailInnerJoint => "topRailInnerJoint",
        Driver::Module => "module",
        Driver::BallastBlock => "ballastBlock",
        Driver::PairRun { .. } => "pairRun",
        Driver::RailJoint { .. } => "railJoint",
        Driver::RailBars { .. } => "railBars",
    }
}

fn rail_stock_m(part: &PartSpec) -> Option<f64> {
    match part.kind {
        PartKind::Rail { stock_length_m, .. } => Some(stock_length_m),
        _ => None,
    }
}

/// Checks that a rule's bar length is the one of the rail it cuts or joins.
fn check_stock(
    article: &Component,
    expected: f64,
    rail: Option<&PartSpec>,
) -> Result<(), CommandError> {
    match rail.and_then(rail_stock_m) {
        Some(actual) if (actual - expected).abs() <= 1e-9 => Ok(()),
        actual => Err(CommandError::new(keys::COMPONENT_STOCK_MISMATCH)
            .with_text("article", article.article_no.clone())
            .with_number("expected", expected)
            .with_number("actual", actual.unwrap_or(f64::NAN))),
    }
}

/// The [`MountingSystem`] the engines use, rebuilt from the system component, the field's
/// module and the parts found in `lookup` (with their prices).
///
/// # Errors
/// A missing component ([`keys::COMPONENT_UNKNOWN`]), a reference to the wrong family, a part
/// that its driver cannot count, a bar length differing from the rail's, or an invalid value.
pub fn resolve_mounting<L: ComponentLookup + ?Sized>(
    system: &Component,
    module: &Component,
    lookup: &L,
) -> Result<MountingSystem, CommandError> {
    module.validate()?;
    let mass = module_spec(module)?.mass_kg;
    resolve_with_module_mass(system, mass, lookup)
}

/// Checks a system and every part it refers to, without a module (what the library checks
/// before storing a system).
///
/// # Errors
/// As [`resolve_mounting`].
pub fn check_system<L: ComponentLookup + ?Sized>(
    system: &Component,
    lookup: &L,
) -> Result<(), CommandError> {
    resolve_with_module_mass(system, 0.0, lookup).map(|_| ())
}

fn resolve_with_module_mass<L: ComponentLookup + ?Sized>(
    system: &Component,
    module_mass_kg: f64,
    lookup: &L,
) -> Result<MountingSystem, CommandError> {
    system.validate()?;
    let spec = system_spec(system)?;

    let (rails, ballast, layer_rail) = match &spec.structure {
        MountingStructure::CrossRail {
            top_rail,
            bottom_rail,
            checked_fixation,
        } => {
            let section = |id| -> Result<_, CommandError> {
                let (c, p) = find_part(lookup, id)?;
                match &p.kind {
                    PartKind::Rail { section, .. } => Ok((section.clone(), p)),
                    _ => Err(c
                        .error(keys::COMPONENT_DRIVER_MISMATCH)
                        .with_text("article", c.article_no.clone())
                        .with_text("driver", "railBars")),
                }
            };
            let (top, top_part) = section(*top_rail)?;
            let (bottom, bottom_part) = section(*bottom_rail)?;
            let fixation = checked_fixation
                .map(|id| -> Result<_, CommandError> {
                    let (c, p) = find_part(lookup, id)?;
                    match p.kind {
                        PartKind::Fixation {
                            capacity: Some(cap),
                        } => Ok(FixationSpec {
                            name: c.label.clone(),
                            uplift_kn: cap.uplift_kn,
                            pressure_kn: cap.pressure_kn,
                            parallel_kn: cap.parallel_kn,
                        }),
                        _ => Err(c
                            .error(keys::COMPONENT_INVALID)
                            .with_text("field", "capacity")),
                    }
                })
                .transpose()?;
            let rails = RailStatics {
                top_rail: top,
                bottom_rail: bottom,
                fixation,
            };
            (Some(rails), None, Some((top_part, bottom_part)))
        }
        MountingStructure::Ballasted { ballast } => (None, Some(ballast.clone()), None),
    };

    let parts = spec
        .parts
        .iter()
        .map(|rule| -> Result<PartRule, CommandError> {
            let (c, p) = find_part(lookup, rule.part)?;
            c.validate()?;
            if !p.accepts(&rule.driver) {
                return Err(CommandError::new(keys::COMPONENT_DRIVER_MISMATCH)
                    .with_text("article", c.article_no.clone())
                    .with_text("driver", driver_name(&rule.driver)));
            }
            let layer = |l: RailLayerKind| {
                layer_rail.map(|(top, bottom)| match l {
                    RailLayerKind::Top => top,
                    RailLayerKind::Bottom => bottom,
                })
            };
            match rule.driver {
                Driver::RailBars { stock_length_m, .. } => check_stock(c, stock_length_m, Some(p))?,
                Driver::RailJoint {
                    layer: l,
                    stock_length_m,
                } => check_stock(c, stock_length_m, layer(l))?,
                _ => {}
            }
            Ok(PartRule {
                article: to_article(c, p, lookup.price(c.id)),
                driver: rule.driver,
                per: rule.per,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MountingSystem {
        name: system.label.clone(),
        module_mass_kg,
        system_mass_per_module_kg: spec.statics.system_mass_per_module_kg,
        loaded_area_m2: spec.statics.loaded_area_m2,
        psi0_snow: spec.statics.psi0_snow,
        cp: spec.statics.cp.clone(),
        rails,
        ballast,
        parts,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::component::presets;

    /// K2 SingleRail catalogue: `(system, module, parts)`.
    pub(crate) fn k2_single_rail_catalogue() -> (Component, Component, Vec<Component>) {
        (
            presets::k2_single_rail(),
            presets::lr7_54hvh_490m(),
            presets::k2_single_rail_parts(),
        )
    }

    pub(crate) fn snapshots(parts: Vec<Component>) -> Vec<ComponentSnapshot> {
        parts
            .into_iter()
            .map(|component| ComponentSnapshot {
                component,
                price: None,
            })
            .collect()
    }

    #[test]
    fn k2_single_rail_catalogue_rebuilds_the_preset() {
        let (system, module, parts) = k2_single_rail_catalogue();
        let snaps = snapshots(parts);
        let resolved = resolve_mounting(&system, &module, snaps.as_slice()).expect("resolve");
        assert_eq!(resolved, MountingSystem::k2_single_rail());
    }

    #[test]
    fn component_json_round_trip() {
        let (system, module, parts) = k2_single_rail_catalogue();
        for c in parts.iter().chain([&system, &module]) {
            let json = serde_json::to_string(c).expect("serialise");
            let back: Component = serde_json::from_str(&json).expect("deserialise");
            assert_eq!(&back, c);
        }
        let json = serde_json::to_value(&module.spec).expect("serialise");
        assert_eq!(json["family"], "module");
        assert_eq!(json["lengthM"], 1.8);
    }

    #[test]
    fn prices_reach_the_articles() {
        let (system, module, parts) = k2_single_rail_catalogue();
        let mut snaps = snapshots(parts);
        let rail = &mut snaps[1];
        rail.price = Some(PriceEntry {
            component: rail.component.id,
            supplier: "K2 Systems".into(),
            list_price_cents: 4_250,
            currency: Currency::Chf,
            valid_from: crate::component::IsoDate::parse("2026-01-01").expect("date"),
            valid_to: None,
        });
        let resolved = resolve_mounting(&system, &module, snaps.as_slice()).expect("resolve");
        assert!((resolved.parts[1].article.list_price - 42.5).abs() < 1e-12);
        assert!((resolved.parts[0].article.list_price).abs() < 1e-12);
    }

    #[test]
    fn broken_references_are_refused() {
        let (system, module, parts) = k2_single_rail_catalogue();
        let key = |r: Result<MountingSystem, CommandError>| r.expect_err("must fail").key;

        // Missing part.
        let snaps = snapshots(parts[1..].to_vec());
        assert_eq!(
            key(resolve_mounting(&system, &module, snaps.as_slice())),
            keys::COMPONENT_UNKNOWN
        );

        // Module and system swapped.
        let snaps = snapshots(parts.clone());
        assert_eq!(
            key(resolve_mounting(&module, &system, snaps.as_slice())),
            keys::COMPONENT_WRONG_FAMILY
        );

        // Climber counted per fixation.
        let mut bad = system.clone();
        if let ComponentSpec::MountingSystem(s) = &mut bad.spec {
            s.parts[3].driver = Driver::Fixation;
        }
        assert_eq!(
            key(resolve_mounting(&bad, &module, snaps.as_slice())),
            keys::COMPONENT_DRIVER_MISMATCH
        );

        // Bars of 6 m cut from a 4.80 m rail.
        let mut bad = system.clone();
        if let ComponentSpec::MountingSystem(s) = &mut bad.spec {
            s.parts[1].driver = Driver::RailBars {
                layer: RailLayerKind::Top,
                stock_length_m: 6.0,
                kerf_m: 0.0,
            };
        }
        assert_eq!(
            key(resolve_mounting(&bad, &module, snaps.as_slice())),
            keys::COMPONENT_STOCK_MISMATCH
        );

        // Invalid pack quantity.
        let mut bad_parts = parts;
        if let ComponentSpec::MountingPart(p) = &mut bad_parts[2].spec {
            p.pack_qty = 0;
        }
        let snaps = snapshots(bad_parts);
        let err = resolve_mounting(&system, &module, snaps.as_slice()).expect_err("pack 0");
        assert_eq!(err.key, keys::COMPONENT_INVALID);
    }
}
