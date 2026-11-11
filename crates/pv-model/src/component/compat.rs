//! Compatibility of a field's module, mounting system and roof, checked when components are
//! assigned to a field.

use super::resolve::{find_part, module_spec, system_spec};
use super::{Component, ComponentLookup, PartKind};
use crate::command::{keys, CommandError};
use crate::layout::SupportGridKind;
use crate::mounting::Driver;

/// Every incompatibility between `module`, `system` and the roof's support grid (`support`,
/// `None` when the face has none), as translatable errors; empty when compatible.
#[must_use]
pub fn check_array_components<L: ComponentLookup + ?Sized>(
    module: &Component,
    system: &Component,
    support: Option<SupportGridKind>,
    lookup: &L,
) -> Vec<CommandError> {
    let (m, s) = match (module_spec(module), system_spec(system)) {
        (Ok(m), Ok(s)) => (m, s),
        (m, s) => return [m.err(), s.err()].into_iter().flatten().collect(),
    };
    let mut issues = Vec::new();
    let mm = |x: f64| (x * 1000.0).round();

    for rule in &s.parts {
        if !matches!(rule.driver, Driver::TopRailEnd | Driver::TopRailInnerJoint) {
            continue;
        }
        match find_part(lookup, rule.part) {
            Ok((c, p)) => {
                if let PartKind::Clamp { frame_height } = p.kind {
                    if !frame_height.contains(m.thickness_m) {
                        issues.push(
                            CommandError::new(keys::COMPAT_FRAME_HEIGHT)
                                .with_text("clamp", c.label.clone())
                                .with_number("min", mm(frame_height.min_m))
                                .with_number("max", mm(frame_height.max_m))
                                .with_number("height", mm(m.thickness_m)),
                        );
                    }
                }
            }
            Err(e) => issues.push(e),
        }
    }

    let c = &s.compatibility;
    if let Some(kind) = support {
        if !c.supports.is_empty() && !c.supports.contains(&kind) {
            let name = match kind {
                SupportGridKind::StandingSeam => "standingSeam",
                SupportGridKind::Rafters => "rafters",
            };
            issues.push(
                CommandError::new(keys::COMPAT_SUPPORT)
                    .with_text("system", system.label.clone())
                    .with_text("support", name),
            );
        }
    }
    let fits = |range: Option<super::LengthRange>, x| range.is_none_or(|r| r.contains(x));
    if !(fits(c.module_length, m.length_m) && fits(c.module_width, m.width_m)) {
        issues.push(
            CommandError::new(keys::COMPAT_MODULE_SIZE)
                .with_text("system", system.label.clone())
                .with_number("length", m.length_m)
                .with_number("width", m.width_m),
        );
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::resolve::tests::{k2_single_rail_catalogue, snapshots};
    use crate::component::{ComponentSpec, LengthRange};

    #[test]
    fn k2_single_rail_accepts_its_module_on_standing_seam() {
        let (system, module, parts) = k2_single_rail_catalogue();
        let snaps = snapshots(parts);
        let issues = check_array_components(
            &module,
            &system,
            Some(SupportGridKind::StandingSeam),
            snaps.as_slice(),
        );
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn incompatibilities_are_all_reported() {
        let (mut system, mut module, parts) = k2_single_rail_catalogue();
        let snaps = snapshots(parts);
        if let ComponentSpec::Module(m) = &mut module.spec {
            m.thickness_m = 0.045; // clamps 25–40 mm
        }
        if let ComponentSpec::MountingSystem(s) = &mut system.spec {
            s.compatibility.module_length = Some(LengthRange {
                min_m: 1.0,
                max_m: 1.75,
            });
        }
        let keys: Vec<String> = check_array_components(
            &module,
            &system,
            Some(SupportGridKind::Rafters),
            snaps.as_slice(),
        )
        .into_iter()
        .map(|e| e.key)
        .collect();
        assert_eq!(
            keys,
            [
                keys::COMPAT_FRAME_HEIGHT,
                keys::COMPAT_FRAME_HEIGHT,
                keys::COMPAT_SUPPORT,
                keys::COMPAT_MODULE_SIZE
            ]
        );
    }

    #[test]
    fn wrong_families_are_reported() {
        let (system, module, parts) = k2_single_rail_catalogue();
        let snaps = snapshots(parts);
        let issues = check_array_components(&system, &module, None, snaps.as_slice());
        assert_eq!(issues.len(), 2);
        assert!(issues.iter().all(|e| e.key == keys::COMPONENT_WRONG_FAMILY));
    }
}
