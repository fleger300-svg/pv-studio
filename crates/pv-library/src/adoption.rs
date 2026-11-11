//! Completing a project command with the components it needs: a command that refers to
//! library components not yet frozen in the project gets an `AdoptComponents` of their
//! closure in front of it, in one batch (one undo step). Components already frozen keep their
//! frozen version: newer library revisions only arrive through update proposals.

use std::time::{SystemTime, UNIX_EPOCH};

use pv_model::component::IsoDate;
use pv_model::{Command, ComponentId, Project};

use crate::{LibraryError, LibraryStore};

/// Component ids a command refers to (arrays' module and mounting system).
#[must_use]
pub fn referenced_components(cmd: &Command) -> Vec<ComponentId> {
    fn walk(cmd: &Command, out: &mut Vec<ComponentId>) {
        match cmd {
            Command::UpsertArray { array, .. } => {
                out.extend(array.module.iter().chain(&array.mounting))
            }
            Command::SetArrayComponents {
                module, mounting, ..
            } => out.extend(module.iter().chain(mounting)),
            Command::Batch { commands } => commands.iter().for_each(|c| walk(c, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(cmd, &mut out);
    out.sort_unstable();
    out.dedup();
    out
}

/// `cmd`, preceded by the adoption of the components it refers to that `project` lacks.
///
/// # Errors
/// A component missing from the library, or a database failure.
pub fn complete_command(
    library: &LibraryStore,
    project: &Project,
    cmd: Command,
    date: &IsoDate,
) -> Result<Command, LibraryError> {
    let missing: Vec<ComponentId> = referenced_components(&cmd)
        .into_iter()
        .filter(|&id| project.component(id).is_none())
        .collect();
    if missing.is_empty() {
        return Ok(cmd);
    }
    let snapshots: Vec<_> = library
        .closure(&missing, date)?
        .into_iter()
        .filter(|s| project.component(s.component.id).is_none())
        .collect();
    Ok(Command::Batch {
        commands: vec![Command::AdoptComponents { snapshots }, cmd],
    })
}

/// Today's date (UTC).
#[must_use]
pub fn today() -> IsoDate {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of_days(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// Civil date of `days` since 1970-01-01 (H. Hinnant's algorithm).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn date_of_days(days: i64) -> IsoDate {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    IsoDate::parse(&format!("{y:04}-{m:02}-{d:02}")).expect("valid civil date")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pv_model::component::presets;
    use pv_model::{ArrayGrid, ArrayId, ArrayLayoutParams, Orientation, PanelArray, RoofFaceId};

    #[test]
    fn civil_dates() {
        assert_eq!(date_of_days(0).as_str(), "1970-01-01");
        assert_eq!(date_of_days(20_768).as_str(), "2026-11-11");
        assert_eq!(date_of_days(11_016).as_str(), "2000-02-29");
        assert_eq!(date_of_days(-1).as_str(), "1969-12-31");
    }

    #[test]
    fn missing_components_are_adopted_in_the_same_batch() {
        let lib = LibraryStore::open_in_memory().expect("lib");
        let project = Project::empty("x");
        let array = PanelArray {
            id: ArrayId::new(),
            roof_face_id: RoofFaceId::new(),
            module_length_m: 1.8,
            module_width_m: 1.134,
            orientation: Orientation::Landscape,
            tilt_on_roof_deg: 0.0,
            azimuth_offset_deg: 0.0,
            grid: ArrayGrid::regular(0, 0, 0.0, 0.0, 0.0, 0.0),
            layout: ArrayLayoutParams::default(),
            module: Some(presets::LR7_54HVH_490M),
            mounting: Some(presets::K2_SINGLE_RAIL),
        };
        let cmd = Command::UpsertArray {
            array: Box::new(array),
            at: None,
        };
        let date = IsoDate::parse("2026-11-11").expect("date");
        let done = complete_command(&lib, &project, cmd.clone(), &date).expect("complete");
        let Command::Batch { commands } = done else {
            panic!("batch expected")
        };
        let Command::AdoptComponents { snapshots } = &commands[0] else {
            panic!("adoption first")
        };
        assert_eq!(snapshots.len(), 7, "module, system and 5 parts");
        assert_eq!(commands[1], cmd);

        // Nothing missing: unchanged.
        let other = Command::SetDiscount {
            manufacturer: "K2 Systems".into(),
            rate: None,
        };
        assert_eq!(
            complete_command(&lib, &project, other.clone(), &date).expect("same"),
            other
        );
    }
}
