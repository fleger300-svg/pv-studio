//! Roof commands for the 2D editor: a pure preview (called on every frame while a
//! footprint is dragged, ~1 ms) and the roof presets of a newly drawn footprint. Applying a
//! roof goes through `plan_roof` + `apply_command` (one journal entry).

use pv_model::{Building, CommandError, RoofEdgeKind, RoofKind, RoofSpec};
use pv_roof::{classify, generate, presets};
use serde::{Deserialize, Serialize};
use specta::Type;

/// One generated face, projected on the ground (world east, north; m).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFace {
    pub polygon: Vec<[f64; 2]>,
    /// One per polygon edge.
    pub edge_kinds: Vec<RoofEdgeKind>,
}

/// What the roof of a building would be (nothing is changed).
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofPreview {
    pub faces: Vec<PreviewFace>,
    pub roof_kind: RoofKind,
    pub ridge_height_m: f64,
}

/// The roof `building` would get. Errors are the ones applying it would give (invalid
/// footprint, re-entrant corner with the edges involved…), so the editor can show them
/// while the user is still dragging.
#[tauri::command]
#[specta::specta]
pub async fn preview_roof(building: Building) -> Result<RoofPreview, CommandError> {
    let roof = generate(&building)?;
    let kinds = classify::edge_kinds(&roof.input, &roof.plan_faces);
    let [ox, oy, _] = building.origin;
    Ok(RoofPreview {
        faces: roof
            .plan_faces
            .iter()
            .zip(kinds)
            .map(|(f, edge_kinds)| PreviewFace {
                polygon: f.polygon.iter().map(|p| [ox + p.u, oy + p.v]).collect(),
                edge_kinds,
            })
            .collect(),
        roof_kind: roof.roof_kind,
        ridge_height_m: roof.ridge_height_m,
    })
}

/// Roof types offered for a footprint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RoofPreset {
    Gable,
    Hipped,
    Monopitch,
    Flat,
}

/// The roof specification of a preset on a footprint (the main axis is the long side of
/// its minimum-area rectangle; a monopitch drains over the longest edge facing south).
#[tauri::command]
#[specta::specta]
pub async fn roof_preset(
    footprint: Vec<[f64; 2]>,
    preset: RoofPreset,
    pitch_deg: f64,
    overhang_m: f64,
) -> Result<RoofSpec, CommandError> {
    Ok(match preset {
        RoofPreset::Gable => presets::gable(&footprint, pitch_deg, overhang_m),
        RoofPreset::Hipped => presets::hipped(&footprint, pitch_deg, pitch_deg, overhang_m),
        RoofPreset::Flat => presets::flat(&footprint, overhang_m),
        RoofPreset::Monopitch => presets::monopitch(
            &footprint,
            south_low_edge(&footprint),
            pitch_deg,
            overhang_m,
        ),
    })
}

/// The edge a monopitch roof drains over: the longest among those whose outward normal
/// points south (else the longest).
fn south_low_edge(fp: &[[f64; 2]]) -> usize {
    let n = fp.len();
    let edge = |i: usize| {
        let (a, b) = (fp[i], fp[(i + 1) % n]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        // Counter-clockwise footprint: outward normal (dy, −dx).
        (dx.hypot(dy), -dx)
    };
    let longest = |filter: &dyn Fn(usize) -> bool| {
        (0..n)
            .filter(|&i| filter(i))
            .max_by(|&i, &j| edge(i).0.total_cmp(&edge(j).0))
    };
    longest(&|i| edge(i).1 < -1e-9 * edge(i).0)
        .or_else(|| longest(&|_| true))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monopitch_drains_south() {
        let fp = vec![[0.0, 0.0], [10.0, 0.0], [10.0, 6.0], [0.0, 6.0]];
        assert_eq!(south_low_edge(&fp), 0);
        let rotated = vec![[0.0, 6.0], [0.0, 0.0], [10.0, 0.0], [10.0, 6.0]];
        assert_eq!(south_low_edge(&rotated), 1);
    }
}

/// The roof of a building as the form shows it (type and a few numbers, or « custom »).
#[tauri::command]
#[specta::specta]
pub async fn roof_form(
    footprint: Vec<[f64; 2]>,
    spec: RoofSpec,
    eave_height_m: f64,
) -> Result<pv_roof::form::RoofForm, CommandError> {
    Ok(pv_roof::form::form_of(&footprint, &spec, eave_height_m))
}

/// The per-edge specification of a form (`None` for a custom roof: keep the edges).
#[tauri::command]
#[specta::specta]
pub async fn roof_from_form(
    footprint: Vec<[f64; 2]>,
    form: pv_roof::form::RoofForm,
) -> Result<Option<RoofSpec>, CommandError> {
    Ok(pv_roof::form::spec_of(&footprint, &form))
}
