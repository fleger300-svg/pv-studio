//! The roof as a form shows it: a type (gable, hipped, half-hipped, monopitch, flat) with
//! a few numbers, inferred from a per-edge specification — or « custom » when the edges
//! were set one by one. `spec_of` is the inverse: the per-edge specification of a form.

use serde::{Deserialize, Serialize};
use specta::Type;

use pv_model::{EdgeRoof, RoofSpec};

use crate::presets::{flat, gable, half_hipped, hipped, long_edges, monopitch};

/// Roof types of the building form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RoofShape {
    Gable,
    Hipped,
    HalfHipped,
    Monopitch,
    Flat,
    /// Edges set one by one (the per-edge table).
    Custom,
}

/// A roof specification as the form shows it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofForm {
    pub shape: RoofShape,
    /// Pitch of the long sides (the single slope of a monopitch roof), degrees.
    pub pitch_deg: f64,
    /// Pitch of the hipped ends, degrees.
    pub end_pitch_deg: f64,
    /// Eave height of the half-hipped ends above the ground, m.
    pub end_eave_m: f64,
    /// Footprint edge a monopitch roof drains over.
    pub low_edge: u32,
    pub overhang_m: f64,
}

const TOL: f64 = 1e-9;

fn all_equal(values: impl IntoIterator<Item = f64>) -> Option<f64> {
    let mut it = values.into_iter();
    let first = it.next()?;
    it.all(|v| (v - first).abs() <= TOL).then_some(first)
}

/// The form of a specification on its footprint (`eave_m`: the building's eave height).
#[must_use]
pub fn form_of(footprint: &[[f64; 2]], spec: &RoofSpec, eave_m: f64) -> RoofForm {
    let long = long_edges(footprint);
    let sloped: Vec<usize> = (0..spec.edges.len())
        .filter(|&i| spec.edges[i].is_sloped())
        .collect();
    // Shown when the form is not one of the types: the steepest slope, or 30°.
    let steepest = sloped
        .iter()
        .map(|&i| spec.edges[i].pitch_deg)
        .fold(None, |m: Option<f64>, p| Some(m.map_or(p, |m| m.max(p))))
        .unwrap_or(30.0);
    let raised = |e: &EdgeRoof| e.eave_height_m.is_some_and(|h| (h - eave_m).abs() > TOL);
    let mut form = RoofForm {
        shape: RoofShape::Custom,
        pitch_deg: steepest,
        end_pitch_deg: steepest,
        end_eave_m: eave_m,
        low_edge: 0,
        overhang_m: spec.overhang_m,
    };
    if spec.edges.iter().any(raised) && !spec.edges.iter().all(|e| e.is_sloped()) {
        return form;
    }
    if sloped.is_empty() {
        form.shape = RoofShape::Flat;
        return form;
    }
    let n = spec.edges.len();
    let long_idx: Vec<usize> = (0..n).filter(|&i| long[i]).collect();
    let end_idx: Vec<usize> = (0..n).filter(|&i| !long[i]).collect();
    if sloped.len() == 1 && !spec.edges.iter().any(raised) {
        form.shape = RoofShape::Monopitch;
        form.low_edge = sloped[0] as u32;
        return form;
    }
    let long_pitch = all_equal(long_idx.iter().map(|&i| spec.edges[i].pitch_deg));
    let (Some(p), true) = (
        long_pitch,
        long_idx.iter().all(|&i| !raised(&spec.edges[i])),
    ) else {
        return form;
    };
    form.pitch_deg = p;
    if p >= 90.0 {
        return form;
    }
    if end_idx.iter().all(|&i| !spec.edges[i].is_sloped()) {
        form.shape = RoofShape::Gable;
        return form;
    }
    let Some(q) = all_equal(end_idx.iter().map(|&i| spec.edges[i].pitch_deg)) else {
        return form;
    };
    if q >= 90.0 {
        return form;
    }
    form.end_pitch_deg = q;
    if end_idx.iter().all(|&i| !raised(&spec.edges[i])) {
        form.shape = RoofShape::Hipped;
    } else if let Some(h) = all_equal(
        end_idx
            .iter()
            .map(|&i| spec.edges[i].eave_height_m.unwrap_or(eave_m)),
    ) {
        form.shape = RoofShape::HalfHipped;
        form.end_eave_m = h;
    }
    form
}

/// The per-edge specification of a form; `None` for `Custom` (nothing to derive).
#[must_use]
pub fn spec_of(footprint: &[[f64; 2]], form: &RoofForm) -> Option<RoofSpec> {
    let o = form.overhang_m;
    Some(match form.shape {
        RoofShape::Gable => gable(footprint, form.pitch_deg, o),
        RoofShape::Hipped => hipped(footprint, form.pitch_deg, form.end_pitch_deg, o),
        RoofShape::HalfHipped => half_hipped(
            footprint,
            form.pitch_deg,
            form.end_pitch_deg,
            form.end_eave_m,
            o,
        ),
        RoofShape::Monopitch => {
            let low = (form.low_edge as usize).min(footprint.len().saturating_sub(1));
            monopitch(footprint, low, form.pitch_deg, o)
        }
        RoofShape::Flat => flat(footprint, o),
        RoofShape::Custom => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Vec<[f64; 2]> {
        pv_model::rectangle_footprint([14.0, 9.0])
    }

    fn round_trip(form: RoofForm) {
        let spec = spec_of(&rect(), &form).expect("spec");
        let back = form_of(&rect(), &spec, 6.0);
        assert_eq!(back.shape, form.shape, "{form:?}");
        assert_eq!(spec_of(&rect(), &back), Some(spec));
    }

    fn form(shape: RoofShape) -> RoofForm {
        RoofForm {
            shape,
            pitch_deg: 35.0,
            end_pitch_deg: 50.0,
            end_eave_m: 8.0,
            low_edge: 0,
            overhang_m: 0.4,
        }
    }

    #[test]
    fn every_shape_round_trips() {
        for s in [
            RoofShape::Gable,
            RoofShape::Hipped,
            RoofShape::HalfHipped,
            RoofShape::Monopitch,
            RoofShape::Flat,
        ] {
            round_trip(form(s));
        }
        let half = form_of(
            &rect(),
            &spec_of(&rect(), &form(RoofShape::HalfHipped)).unwrap(),
            6.0,
        );
        assert_eq!(
            (half.pitch_deg, half.end_pitch_deg, half.end_eave_m),
            (35.0, 50.0, 8.0)
        );
    }

    #[test]
    fn edges_set_one_by_one_are_custom() {
        let mut spec = spec_of(&rect(), &form(RoofShape::Gable)).unwrap();
        spec.edges[0].pitch_deg = 20.0;
        assert_eq!(form_of(&rect(), &spec, 6.0).shape, RoofShape::Custom);
        assert_eq!(spec_of(&rect(), &form(RoofShape::Custom)), None);
    }
}
