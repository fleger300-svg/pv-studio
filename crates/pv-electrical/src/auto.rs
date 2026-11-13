//! Automatic stringing: fills the trackers of the project's inverters with strings of equal
//! length, one orientation class per tracker, modules in a serpentine order (row by row,
//! alternating direction) so that consecutive modules of a string are neighbours.
//!
//! Locked strings are kept as they are; their modules and their trackers are left alone.
//!
//! For each orientation class of `M` free modules (in project order), the modules are split
//! into the fewest strings the inverter's voltage limits allow, of lengths `q + 1` or `q`
//! (all modules used); strings of one length go to the free trackers in order, as many per
//! tracker as its inputs and its short-circuit current allow, never two lengths on one
//! tracker. Modules left when the trackers run out stay unassigned (reported by the checks).

use std::collections::{BTreeMap, HashSet};

use pv_model::{ComponentSpec, ModuleRef, Project, PvString, StringId, Wiring};

use crate::{module_sites, string_limits, Orientation};

/// Free tracker slot: inverter index, tracker index, inputs, strings it can take by current.
struct Slot {
    inverter: usize,
    tracker: u32,
    capacity: u32,
}

/// A new wiring for `project`: locked strings kept, every other string rebuilt.
#[must_use]
pub fn auto_string(project: &Project) -> Wiring {
    let w = &project.wiring;
    let s = &w.settings;
    let locked: Vec<PvString> = w.strings.iter().filter(|x| x.locked).cloned().collect();
    let taken: HashSet<ModuleRef> = locked
        .iter()
        .flat_map(|x| x.modules.iter().copied())
        .collect();
    let busy: HashSet<(usize, u32)> = locked
        .iter()
        .filter_map(|x| {
            let i = w.inverters.iter().position(|u| u.id == x.inverter)?;
            Some((i, x.tracker))
        })
        .collect();

    // Free modules by orientation class, serpentine order (array, row, alternating column).
    let sites = module_sites(project);
    let mut classes: BTreeMap<Orientation, Vec<(ModuleRef, &crate::ModuleSite)>> = BTreeMap::new();
    for (r, m) in &sites {
        if m.active && !taken.contains(r) && m.electrical.is_some() {
            classes.entry(m.orientation).or_default().push((*r, m));
        }
    }
    for modules in classes.values_mut() {
        let order: Vec<_> = project.arrays.iter().map(|a| a.id).collect();
        modules.sort_by_key(|(r, _)| {
            let array = order
                .iter()
                .position(|&a| a == r.array)
                .unwrap_or(usize::MAX);
            let col = if r.row % 2 == 0 {
                i64::from(r.col)
            } else {
                -i64::from(r.col)
            };
            (array, r.row, col)
        });
    }

    // Free tracker slots, in inverter then tracker order.
    let mut slots: Vec<Slot> = Vec::new();
    for (ii, unit) in w.inverters.iter().enumerate() {
        let Some(ComponentSpec::Inverter(spec)) =
            project.component(unit.component).map(|c| &c.component.spec)
        else {
            continue;
        };
        for (ti, t) in spec.dc.trackers.iter().enumerate() {
            let ti = ti as u32;
            if !busy.contains(&(ii, ti)) {
                slots.push(Slot {
                    inverter: ii,
                    tracker: ti,
                    capacity: t.inputs,
                });
            }
        }
    }

    let mut strings = locked;
    let mut next_slot = 0usize;
    for modules in classes.values() {
        let Some(e) = modules.first().and_then(|(_, m)| m.electrical) else {
            continue;
        };
        let Some(first) = slots.get(next_slot) else {
            break;
        };
        let Some(ComponentSpec::Inverter(spec)) = project
            .component(w.inverters[first.inverter].component)
            .map(|c| &c.component.spec)
        else {
            continue;
        };
        let Some((n_min, n_max)) = string_limits(&e, spec, s) else {
            continue;
        };
        // Fewest strings, lengths as equal as possible (q + 1 or q modules).
        let Some(lengths) = split(modules.len(), n_min, n_max) else {
            continue;
        };
        let mut pos = 0usize;
        let mut lengths = lengths.into_iter().peekable();
        while let Some(&n) = lengths.peek() {
            let Some(slot) = slots.get(next_slot) else {
                break;
            };
            let unit = &w.inverters[slot.inverter];
            let Some(ComponentSpec::Inverter(spec)) =
                project.component(unit.component).map(|c| &c.component.spec)
            else {
                next_slot += 1;
                continue;
            };
            let tracker = &spec.dc.trackers[slot.tracker as usize];
            let by_current = tracker
                .max_short_circuit_current_a
                .map_or(u32::MAX, |max| ((max + 1e-9) / e.isc_a).floor() as u32);
            let per_tracker = slot.capacity.min(by_current).max(1) as usize;
            // Strings of one length on this tracker, up to its capacity.
            let mut placed = 0usize;
            while placed < per_tracker && lengths.peek() == Some(&n) {
                lengths.next();
                strings.push(PvString {
                    id: StringId::new(),
                    inverter: unit.id,
                    tracker: slot.tracker,
                    modules: modules[pos..pos + n].iter().map(|(r, _)| *r).collect(),
                    locked: false,
                });
                pos += n;
                placed += 1;
            }
            next_slot += 1;
        }
    }
    Wiring {
        inverters: w.inverters.clone(),
        strings,
        settings: w.settings,
    }
}

/// Lengths of the fewest strings of at most `n_max` modules covering `m` modules, as equal
/// as possible (longer ones first); `None` when they would be shorter than `n_min`.
fn split(m: usize, n_min: u32, n_max: u32) -> Option<Vec<usize>> {
    if m < n_min as usize || n_max == 0 {
        return None;
    }
    let k = m.div_ceil(n_max as usize);
    let q = m / k;
    if q < n_min as usize {
        return None;
    }
    let long = m % k;
    Some((0..k).map(|i| if i < long { q + 1 } else { q }).collect())
}

#[cfg(test)]
mod tests {
    use super::split;

    #[test]
    fn split_uses_the_fewest_equal_strings() {
        assert_eq!(split(35, 7, 25), Some(vec![18, 17]));
        assert_eq!(split(32, 7, 25), Some(vec![16, 16]));
        assert_eq!(split(76, 7, 25), Some(vec![19, 19, 19, 19]));
        assert_eq!(split(25, 7, 25), Some(vec![25]));
        assert_eq!(split(6, 7, 25), None);
    }
}
