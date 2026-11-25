//! Project location and weather: where the project is (geocoded address, terrain altitude)
//! and the typical meteorological year the yield simulation uses. Both are cached in the
//! project (ADR-003): a project recomputes offline, the same way, after it was fetched once.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::command::{keys, CommandError};

/// Geographic position of the project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GeoLocation {
    /// Address or place, as found (« Rue du Lac 1, 1003 Lausanne »).
    pub label: String,
    /// WGS84 [°].
    pub lat_deg: f64,
    pub lon_deg: f64,
    /// Swiss LV95 coordinates [m].
    pub east_m: f64,
    pub north_m: f64,
    /// Terrain altitude (swissALTI3D) [m a.s.l.].
    pub altitude_m: f64,
}

/// Hours of a typical (non-leap) year.
pub const HOURS_PER_YEAR: usize = 8760;

/// Typical meteorological year: hourly means, hour `h` = interval `[h, h + 1)` UTC from
/// 1 January 00:00.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WeatherYear {
    /// Data source, shown in reports (« MeteoSwiss Open Data, station BER Bern / Zollikofen,
    /// 2010–2024, typical months »).
    pub source: String,
    /// Station or grid point [°, m a.s.l.].
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub altitude_m: f64,
    /// Global horizontal irradiance [W/m²], 8760 values.
    pub ghi_w_m2: Vec<f32>,
    /// Diffuse horizontal irradiance [W/m²], 8760 values; empty when the source has none
    /// (the simulation then separates it with a decomposition model).
    pub dhi_w_m2: Vec<f32>,
    /// Air temperature 2 m above ground [°C], 8760 values.
    pub temp_c: Vec<f32>,
}

impl WeatherYear {
    /// Annual global horizontal irradiation [kWh/m²].
    #[must_use]
    pub fn annual_ghi_kwh_m2(&self) -> f64 {
        self.ghi_w_m2.iter().map(|&x| f64::from(x)).sum::<f64>() / 1000.0
    }

    pub(crate) fn validate(&self) -> Result<(), CommandError> {
        let invalid = |f: &str| CommandError::new(keys::ENTITY_INVALID).with_text("field", f);
        if self.ghi_w_m2.len() != HOURS_PER_YEAR || self.temp_c.len() != HOURS_PER_YEAR {
            return Err(invalid("weather"));
        }
        if !(self.dhi_w_m2.is_empty() || self.dhi_w_m2.len() == HOURS_PER_YEAR) {
            return Err(invalid("weather"));
        }
        let ok =
            |v: &[f32], lo: f32, hi: f32| v.iter().all(|x| x.is_finite() && (lo..=hi).contains(x));
        if !ok(&self.ghi_w_m2, 0.0, 1500.0)
            || !ok(&self.dhi_w_m2, 0.0, 1500.0)
            || !ok(&self.temp_c, -60.0, 60.0)
        {
            return Err(invalid("weather"));
        }
        Ok(())
    }
}

/// The surroundings of the project as a height grid (digital surface model: terrain,
/// neighbouring buildings, trees), for the shading of the modules. Regular grid in the
/// project's ENU frame, rows from north to south, columns from west to east.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Surroundings {
    /// Where the heights come from, shown in reports.
    pub source: String,
    /// ENU (east, north) of the north-west corner of the grid [m].
    pub origin_east_m: f64,
    pub origin_north_m: f64,
    /// Cell size [m].
    pub cell_m: f64,
    pub cols: u32,
    pub rows: u32,
    /// Height of each cell in the project frame (`z`, same as the buildings) [cm],
    /// `rows × cols`, row by row from the north-west corner.
    pub heights_cm: Vec<i32>,
}

impl Surroundings {
    /// Height of a cell removed from the survey (the project's own building, which the
    /// project's geometry replaces): casts no shadow.
    pub const CUT: i32 = i32::MIN;

    /// Height [m] of the cell holding ENU `(east, north)`; `None` outside the grid.
    #[must_use]
    pub fn height_at(&self, east: f64, north: f64) -> Option<f64> {
        let c = ((east - self.origin_east_m) / self.cell_m).floor();
        let r = ((self.origin_north_m - north) / self.cell_m).floor();
        if c < 0.0 || r < 0.0 || c >= f64::from(self.cols) || r >= f64::from(self.rows) {
            return None;
        }
        let i = r as usize * self.cols as usize + c as usize;
        self.heights_cm
            .get(i)
            .filter(|&&h| h != Self::CUT)
            .map(|&h| f64::from(h) / 100.0)
    }

    fn validate(&self) -> Result<(), CommandError> {
        let ok = self.origin_east_m.is_finite()
            && self.origin_north_m.is_finite()
            && self.cell_m.is_finite()
            && self.cell_m > 0.0
            && self.cols > 0
            && self.rows > 0
            && self.heights_cm.len() == self.cols as usize * self.rows as usize;
        if ok {
            Ok(())
        } else {
            Err(CommandError::new(keys::ENTITY_INVALID).with_text("field", "surroundings"))
        }
    }
}

/// The far horizon (relief) seen from the project: elevation by compass azimuth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Horizon {
    /// Where the profile comes from, shown in reports.
    pub source: String,
    /// `[azimuth°, elevation°]`, azimuths increasing in `[0, 360)`; linear in between,
    /// wrapping around north.
    pub points: Vec<[f64; 2]>,
}

impl Horizon {
    /// Elevation of the horizon towards compass `azimuth_deg` [°].
    #[must_use]
    pub fn elevation(&self, azimuth_deg: f64) -> f64 {
        let p = &self.points;
        let n = p.len();
        if n < 2 {
            return p.first().map_or(0.0, |q| q[1]);
        }
        let az = azimuth_deg.rem_euclid(360.0);
        let (a, b) = match p.iter().rposition(|q| q[0] <= az) {
            Some(i) if i + 1 < n => (p[i], p[i + 1]),
            Some(i) => (p[i], [p[0][0] + 360.0, p[0][1]]),
            None => ([p[n - 1][0] - 360.0, p[n - 1][1]], p[0]),
        };
        a[1] + (b[1] - a[1]) * (az - a[0]) / (b[0] - a[0])
    }

    fn validate(&self) -> Result<(), CommandError> {
        let ok = !self.points.is_empty()
            && self.points.iter().all(|q| {
                q[0].is_finite()
                    && (0.0..360.0).contains(&q[0])
                    && q[1].is_finite()
                    && (-10.0..=90.0).contains(&q[1])
            })
            && self.points.windows(2).all(|w| w[0][0] < w[1][0]);
        if ok {
            Ok(())
        } else {
            Err(CommandError::new(keys::ENTITY_INVALID).with_text("field", "horizon"))
        }
    }
}

/// What a [`ShadingObject`] is (shown in reports and the 3-D view).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ShadingObjectKind {
    Building,
    /// Chimney, vent pipe, dish: a fixture on a roof.
    RoofFixture,
    /// Party wall, parapet, free-standing wall.
    Wall,
    Tree,
    /// A flat polygon drawn in 3-D.
    Polygon,
    /// A dormer on a roof: its storey (a solid) and its roof faces (polygons).
    Dormer,
}

/// An object around or on the buildings that casts shadows: a neighbouring building, a
/// chimney, a parapet, a tree, a 3-D polygon. Coordinates in the project frame (ENU, same as
/// the buildings) [m].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ShadingObject {
    pub name: String,
    pub kind: ShadingObjectKind,
    /// Base outline (a solid) or the polygon itself (`height_m = 0`), `[east, north, up]`.
    pub outline: Vec<[f64; 3]>,
    /// A solid is the outline extruded upwards by this height [m]; 0 for a flat polygon.
    pub height_m: f64,
    /// Share of the light the object stops, January to December (a tree's crown); empty
    /// for an opaque object.
    #[serde(default)]
    pub opacity_by_month: Vec<f64>,
}

impl ShadingObject {
    /// Share of the light stopped in `month` (0 = January).
    #[must_use]
    pub fn opacity(&self, month: usize) -> f64 {
        self.opacity_by_month.get(month).copied().unwrap_or(1.0)
    }

    fn valid(&self) -> bool {
        let finite = |p: &[f64; 3]| p.iter().all(|x| x.is_finite());
        self.outline.len() >= 2
            && self.outline.iter().all(finite)
            && self.height_m.is_finite()
            && self.height_m >= 0.0
            && (self.height_m > 0.0 || self.outline.len() >= 3)
            && (self.opacity_by_month.is_empty() || self.opacity_by_month.len() == 12)
            && self
                .opacity_by_month
                .iter()
                .all(|o| (0.0..=1.0).contains(o))
    }
}

/// Location, weather and surroundings of a project.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSite {
    #[specta(optional)]
    pub location: Option<GeoLocation>,
    #[specta(optional)]
    pub weather: Option<WeatherYear>,
    /// Neighbouring buildings and terrain (imported from PV*SOL's 3-D map); tied to the
    /// location: a new location drops them.
    #[serde(default)]
    #[specta(optional)]
    pub surroundings: Option<Surroundings>,
    /// Far horizon (relief); tied to the location like the surroundings.
    #[serde(default)]
    #[specta(optional)]
    pub horizon: Option<Horizon>,
    /// Shading objects (trees, neighbouring buildings, roof fixtures); tied to the location
    /// like the surroundings.
    #[serde(default)]
    pub objects: Vec<ShadingObject>,
}

impl ProjectSite {
    /// # Errors
    /// [`keys::ENTITY_INVALID`] with the offending field.
    pub fn validate(&self) -> Result<(), CommandError> {
        if let Some(l) = &self.location {
            if !(l.lat_deg.is_finite()
                && (-90.0..=90.0).contains(&l.lat_deg)
                && l.lon_deg.is_finite()
                && (-180.0..=180.0).contains(&l.lon_deg)
                && l.altitude_m.is_finite()
                && l.east_m.is_finite()
                && l.north_m.is_finite())
            {
                return Err(CommandError::new(keys::ENTITY_INVALID).with_text("field", "location"));
            }
        }
        if let Some(w) = &self.weather {
            w.validate()?;
        }
        if let Some(s) = &self.surroundings {
            s.validate()?;
        }
        if let Some(h) = &self.horizon {
            h.validate()?;
        }
        if !self.objects.iter().all(ShadingObject::valid) {
            return Err(CommandError::new(keys::ENTITY_INVALID).with_text("field", "objects"));
        }
        Ok(())
    }
}

/// Swiss LV95 coordinates (E, N) [m] of a WGS84 position (swisstopo approximate formulas,
/// accuracy ≈ 1 m in Switzerland).
#[must_use]
pub fn wgs84_to_lv95(lat_deg: f64, lon_deg: f64) -> (f64, f64) {
    let phi = (lat_deg * 3600.0 - 169_028.66) / 10_000.0;
    let lambda = (lon_deg * 3600.0 - 26_782.5) / 10_000.0;
    let e = 2_600_072.37 + 211_455.93 * lambda
        - 10_938.51 * lambda * phi
        - 0.36 * lambda * phi * phi
        - 44.54 * lambda.powi(3);
    let n = 1_200_147.07 + 308_807.95 * phi + 3_745.25 * lambda * lambda + 76.63 * phi * phi
        - 194.56 * lambda * lambda * phi
        + 119.79 * phi.powi(3);
    (e, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lv95_of_the_swisstopo_example() {
        // swisstopo, approximate formulas: 46°02′38.87″ N, 8°43′49.79″ E → E 2 699 999.76,
        // N 1 099 999.97.
        let (e, n) = wgs84_to_lv95(
            46.0 + 2.0 / 60.0 + 38.87 / 3600.0,
            8.0 + 43.0 / 60.0 + 49.79 / 3600.0,
        );
        assert!((e - 2_699_999.76).abs() < 1.0, "{e}");
        assert!((n - 1_099_999.97).abs() < 1.0, "{n}");
    }
}

#[cfg(test)]
mod horizon_tests {
    use super::*;

    #[test]
    fn interpolates_and_wraps_around_north() {
        let h = Horizon {
            source: "t".into(),
            points: vec![[0.0, 10.0], [90.0, 20.0], [180.0, 0.0], [270.0, 4.0]],
        };
        assert!((h.elevation(45.0) - 15.0).abs() < 1e-12);
        assert!((h.elevation(180.0) - 0.0).abs() < 1e-12);
        // Between 270° (4°) and 360° (= 0°, 10°).
        assert!((h.elevation(315.0) - 7.0).abs() < 1e-12);
        assert!((h.elevation(-45.0) - 7.0).abs() < 1e-12);
        let late = Horizon {
            source: "t".into(),
            points: vec![[10.0, 2.0], [350.0, 6.0]],
        };
        // Before the first point: between 350° and 370°.
        assert!((late.elevation(0.0) - 4.0).abs() < 1e-12);
    }
}
