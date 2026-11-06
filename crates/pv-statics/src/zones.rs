//! Wind zones on a roof face, assigned to fields by position.
//!
//! The zones are defined for the whole face (architecture: « Les zones de vent sont
//! calculées pour le pan entier, puis attribuées à chaque champ selon sa position »), from
//! SN EN 1991-1-4 §7.2: with `e = min(b, 2h)` (`b` the building dimension across the wind on
//! the face, `h` its height),
//!
//! - strips of width `e/10` along the eave, the ridge and the verges / hips;
//! - eave corners: in the eave strip and within `e/4` of a verge or hip;
//! - flat roofs (parapets): edge strips of `e/10`, corners within `e/10` of one edge and
//!   `e/4` of another;
//! - everything else is the interior.
//!
//! A module belongs to the zone its centre lies in; when several apply, the most severe one
//! (corner > verge > ridge > eave > interior). A field's zones are those of its modules.
//! On project A (roof-1) this gives K2's five zones for each of the two fields.

use std::collections::BTreeSet;

use pv_geo::Vec2;
use pv_layout::{EdgeKind, RoofOutline};
use serde::Serialize;

use crate::cp::RoofZone;
use crate::trace::{NormRef, Traced, Unit};
use crate::StaticsError;

pub const EN_1991_1_4_ZONES: NormRef = NormRef::new("SN EN 1991-1-4", "§7.2.2–7.2.6, fig. 7.6–7.9");

/// Zone geometry of a face.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneGeometry {
    /// e = min(b, 2h) [m].
    pub e: Traced,
}

impl ZoneGeometry {
    /// `crosswind_m`: building dimension across the wind blowing onto the face (the eave
    /// length of a pitched face); `height_m`: building height.
    pub fn new(crosswind_m: f64, height_m: f64) -> Result<Self, StaticsError> {
        StaticsError::check_range("crosswind_m", crosswind_m, 0.5, 1000.0)?;
        StaticsError::check_range("building_height_m", height_m, 0.5, 200.0)?;
        let e = crosswind_m.min(2.0 * height_m);
        Ok(Self {
            e: Traced::new("e", e, Unit::Metre)
                .formula("e = min(b, 2h)")
                .reference(EN_1991_1_4_ZONES)
                .input("b", crosswind_m, Unit::Metre)
                .input("h", height_m, Unit::Metre),
        })
    }

    #[must_use]
    pub fn strip(&self) -> f64 {
        self.e.value / 10.0
    }

    #[must_use]
    pub fn corner_length(&self) -> f64 {
        self.e.value / 4.0
    }
}

fn severity(z: RoofZone) -> u8 {
    match z {
        RoofZone::Interior => 0,
        RoofZone::Eave => 1,
        RoofZone::Ridge => 2,
        RoofZone::Verge => 3,
        RoofZone::EaveCorner => 4,
    }
}

/// Zone of a point of the face.
#[must_use]
pub fn zone_at(roof: &RoofOutline, geometry: &ZoneGeometry, p: Vec2) -> RoofZone {
    let (strip, corner) = (geometry.strip(), geometry.corner_length());
    let mut d_eave = f64::INFINITY;
    let mut d_ridge = f64::INFINITY;
    let mut d_side = f64::INFINITY;
    let mut parapets: Vec<f64> = Vec::new();
    for (i, edge) in roof.boundary().edges().enumerate() {
        let d = edge.distance_to_point(p);
        match roof.edge_kind(i) {
            EdgeKind::Eave => d_eave = d_eave.min(d),
            EdgeKind::Ridge => d_ridge = d_ridge.min(d),
            EdgeKind::Verge | EdgeKind::Hip => d_side = d_side.min(d),
            EdgeKind::Parapet => parapets.push(d),
            EdgeKind::Valley | EdgeKind::Abutment | EdgeKind::Other => {}
        }
    }
    let mut zones = vec![RoofZone::Interior];
    if !parapets.is_empty() {
        parapets.sort_by(f64::total_cmp);
        if parapets[0] < strip {
            zones.push(RoofZone::Eave);
            if parapets.get(1).is_some_and(|&d| d < corner) {
                zones.push(RoofZone::EaveCorner);
            }
        }
    }
    if d_eave < strip {
        zones.push(RoofZone::Eave);
        if d_side < corner {
            zones.push(RoofZone::EaveCorner);
        }
    }
    if d_ridge < strip {
        zones.push(RoofZone::Ridge);
    }
    if d_side < strip {
        zones.push(RoofZone::Verge);
    }
    zones
        .into_iter()
        .max_by_key(|&z| severity(z))
        .unwrap_or(RoofZone::Interior)
}

/// Zone of every module centre.
#[must_use]
pub fn assign_zones(
    roof: &RoofOutline,
    geometry: &ZoneGeometry,
    centres: &[Vec2],
) -> Vec<RoofZone> {
    centres
        .iter()
        .map(|&c| zone_at(roof, geometry, c))
        .collect()
}

/// Zones a field touches, in the order of [`RoofZone::ALL`].
#[must_use]
pub fn field_zones(roof: &RoofOutline, geometry: &ZoneGeometry, centres: &[Vec2]) -> Vec<RoofZone> {
    let set: BTreeSet<u8> = assign_zones(roof, geometry, centres)
        .into_iter()
        .map(severity)
        .collect();
    RoofZone::ALL
        .into_iter()
        .filter(|z| set.contains(&severity(*z)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_roof_corners_and_edges() {
        let roof = RoofOutline::flat(20.0, 12.0).unwrap();
        let g = ZoneGeometry::new(20.0, 9.0).unwrap(); // e = 18 → strip 1.8, corner 4.5
        assert_eq!(
            zone_at(&roof, &g, Vec2::new(1.0, 1.0)),
            RoofZone::EaveCorner
        );
        assert_eq!(zone_at(&roof, &g, Vec2::new(10.0, 1.0)), RoofZone::Eave);
        assert_eq!(zone_at(&roof, &g, Vec2::new(10.0, 6.0)), RoofZone::Interior);
        assert!((g.e.value - 18.0).abs() < 1e-12);
    }

    #[test]
    fn pitched_face_zones() {
        let roof = RoofOutline::rectangle(20.0, 8.0, EdgeKind::Verge).unwrap();
        let g = ZoneGeometry::new(20.0, 30.0).unwrap(); // e = 20 → strip 2, corner 5
        assert_eq!(
            zone_at(&roof, &g, Vec2::new(3.0, 1.0)),
            RoofZone::EaveCorner
        );
        assert_eq!(zone_at(&roof, &g, Vec2::new(10.0, 1.0)), RoofZone::Eave);
        assert_eq!(zone_at(&roof, &g, Vec2::new(10.0, 7.0)), RoofZone::Ridge);
        assert_eq!(zone_at(&roof, &g, Vec2::new(1.0, 4.0)), RoofZone::Verge);
        assert_eq!(zone_at(&roof, &g, Vec2::new(10.0, 4.0)), RoofZone::Interior);
    }
}
