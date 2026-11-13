//! Datasheet catalogue shipped with the application: public manufacturer data read from the
//! datasheets (`catalog/*.json`, in the `Component` JSON format). Every entry is unverified
//! until the user reviews it; ids are fixed, so a corrected datasheet value reaches projects as
//! an update proposal. No prices.

use pv_model::Component;
use serde::Deserialize;

use crate::LibraryError;

#[derive(Deserialize)]
struct CatalogFile {
    components: Vec<Component>,
}

/// The catalogue files, in seeding order (parts before the systems that use them).
const FILES: &[(&str, &str)] = &[
    ("modules", include_str!("../catalog/modules.json")),
    ("inverters", include_str!("../catalog/inverters.json")),
    ("optimizers", include_str!("../catalog/optimizers.json")),
    ("batteries", include_str!("../catalog/batteries.json")),
    ("equipment", include_str!("../catalog/equipment.json")),
    ("structures", include_str!("../catalog/structures.json")),
];

/// Every catalogue component.
///
/// # Errors
/// A catalogue file that does not parse (a bug: the files are tested).
pub fn components() -> Result<Vec<Component>, LibraryError> {
    let mut all = Vec::new();
    for (name, json) in FILES {
        let file: CatalogFile = serde_json::from_str(json)
            .map_err(|e| LibraryError::Corrupt(format!("catalogue {name}: {e}")))?;
        all.extend(file.components);
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use pv_model::component::presets;
    use pv_model::ComponentSpec;

    use super::*;

    #[test]
    fn the_catalogue_is_valid_unique_and_unverified() {
        let all = components().expect("parse");
        assert!(all.len() >= 250, "{} components", all.len());
        for family in ["module", "inverter", "optimizer", "battery", "equipment"] {
            assert!(
                all.iter().any(|c| c.spec.family().as_str() == family),
                "no {family} in the catalogue"
            );
        }
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        let preset_ids: BTreeSet<_> = presets::all().iter().map(|c| c.id).collect();
        for c in &all {
            c.validate()
                .unwrap_or_else(|e| panic!("{}: {e:?}", c.label));
            assert!(ids.insert(c.id), "duplicate id {}", c.label);
            assert!(
                names.insert((c.manufacturer.clone(), c.model.clone())),
                "duplicate {}",
                c.label
            );
            assert!(!preset_ids.contains(&c.id));
            assert!(!c.verified, "{} must be reviewed by a person", c.label);
        }
    }

    #[test]
    fn module_values_are_consistent() {
        for c in components().expect("parse") {
            let ComponentSpec::Module(m) = &c.spec else {
                continue;
            };
            let e = m
                .electrical
                .as_ref()
                .unwrap_or_else(|| panic!("{}: electrical", c.label));
            let ratio = e.vmpp_v * e.impp_a / e.pmax_w;
            assert!(
                (ratio - 1.0).abs() <= 0.02,
                "{}: V×I / Pmax = {ratio:.3}",
                c.label
            );
            assert!(m.length_m >= m.width_m);
            assert!(
                m.length_m < 3.0 && m.width_m > 0.1,
                "{}: dimensions in metres",
                c.label
            );
        }
    }

    #[test]
    fn k2_hook_systems_resolve_with_their_parts() {
        use pv_model::component::{resolve_mounting, ComponentSnapshot};
        use pv_model::RailLayers;
        let snaps: Vec<ComponentSnapshot> = presets::all()
            .into_iter()
            .chain(components().expect("parse"))
            .map(|component| ComponentSnapshot {
                component,
                price: None,
            })
            .collect();
        let module = presets::lr7_54hvh_490m();
        let mut layers = BTreeSet::new();
        let mut n = 0;
        for s in snaps.iter().map(|s| &s.component) {
            let ComponentSpec::MountingSystem(spec) = &s.spec else {
                continue;
            };
            let m = resolve_mounting(s, &module, snaps.as_slice())
                .unwrap_or_else(|e| panic!("{}: {e:?}", s.label));
            if s.model.starts_with("SingleRail 36 SingleHook") {
                n += 1;
                let rails = m.rails.as_ref().expect("rail system");
                assert!(rails.fixation.is_some(), "{}: hooks are checked", s.label);
                layers.insert(format!("{:?}", spec.defaults.rails.expect("rails").layers));
                // Two wood screws per hook.
                assert!(m.parts.iter().any(|p| p.per == 2));
            }
        }
        assert_eq!(n, 10);
        assert_eq!(layers.len(), 2, "{layers:?}");
        let _ = RailLayers::One;
    }
}
