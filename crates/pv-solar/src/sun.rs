//! Solar geometry (NOAA formulas, ≈ 0.01° over 1950–2050) and irradiance components.

use std::f64::consts::PI;

/// Solar constant [W/m²].
pub const SOLAR_CONSTANT: f64 = 1361.0;

/// Sun position: zenith and azimuth (from north, clockwise) [°].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunPosition {
    pub zenith_deg: f64,
    pub azimuth_deg: f64,
}

/// Position at `hour_utc` (fractional) of day `day_of_year` (1–365) of a non-leap year.
#[must_use]
pub fn position(lat_deg: f64, lon_deg: f64, day_of_year: u32, hour_utc: f64) -> SunPosition {
    let gamma = 2.0 * PI / 365.0 * (f64::from(day_of_year) - 1.0 + (hour_utc - 12.0) / 24.0);
    let eqtime = 229.18
        * (0.000_075 + 0.001_868 * gamma.cos()
            - 0.032_077 * gamma.sin()
            - 0.014_615 * (2.0 * gamma).cos()
            - 0.040_849 * (2.0 * gamma).sin());
    let decl = 0.006_918 - 0.399_912 * gamma.cos() + 0.070_257 * gamma.sin()
        - 0.006_758 * (2.0 * gamma).cos()
        + 0.000_907 * (2.0 * gamma).sin()
        - 0.002_697 * (3.0 * gamma).cos()
        + 0.001_48 * (3.0 * gamma).sin();
    let true_solar_min = hour_utc * 60.0 + eqtime + 4.0 * lon_deg;
    let ha = (true_solar_min / 4.0 - 180.0).to_radians();
    let lat = lat_deg.to_radians();
    let cos_z = (lat.sin() * decl.sin() + lat.cos() * decl.cos() * ha.cos()).clamp(-1.0, 1.0);
    let zenith = cos_z.acos();
    let sin_z = zenith.sin().max(1e-9);
    let cos_az = ((lat.sin() * cos_z - decl.sin()) / (lat.cos() * sin_z)).clamp(-1.0, 1.0);
    let az = cos_az.acos().to_degrees();
    // Morning: east (azimuth < 180°); afternoon: west.
    let azimuth = if ha > 0.0 { 180.0 + az } else { 180.0 - az };
    SunPosition {
        zenith_deg: zenith.to_degrees(),
        azimuth_deg: azimuth.rem_euclid(360.0),
    }
}

/// Extraterrestrial normal irradiance [W/m²].
#[must_use]
pub fn extraterrestrial(day_of_year: u32) -> f64 {
    let b = 2.0 * PI * f64::from(day_of_year) / 365.0;
    SOLAR_CONSTANT
        * (1.000_11
            + 0.034_221 * b.cos()
            + 0.001_28 * b.sin()
            + 0.000_719 * (2.0 * b).cos()
            + 0.000_077 * (2.0 * b).sin())
}

/// Diffuse fraction of global horizontal irradiance (Erbs et al. 1982) from the clearness
/// index `kt`.
#[must_use]
pub fn erbs_diffuse_fraction(kt: f64) -> f64 {
    if kt <= 0.22 {
        1.0 - 0.09 * kt
    } else if kt <= 0.8 {
        0.9511 - 0.1604 * kt + 4.388 * kt.powi(2) - 16.638 * kt.powi(3) + 12.336 * kt.powi(4)
    } else {
        0.165
    }
}

/// Angle of incidence on a plane of tilt `tilt_deg` facing `azimuth_deg` (from north) [°].
#[must_use]
pub fn incidence_deg(sun: &SunPosition, tilt_deg: f64, azimuth_deg: f64) -> f64 {
    let (z, a) = (sun.zenith_deg.to_radians(), sun.azimuth_deg.to_radians());
    let (t, p) = (tilt_deg.to_radians(), azimuth_deg.to_radians());
    let c = z.cos() * t.cos() + z.sin() * t.sin() * (a - p).cos();
    c.clamp(-1.0, 1.0).acos().to_degrees()
}

/// Plane-of-array irradiance components [W/m²].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Poa {
    pub beam: f64,
    /// Sky diffuse, circumsolar part included.
    pub sky_diffuse: f64,
    /// Circumsolar part of `sky_diffuse` (comes from around the sun: shaded like the beam).
    pub circumsolar: f64,
    pub ground: f64,
}

impl Poa {
    #[must_use]
    pub fn total(&self) -> f64 {
        self.beam + self.sky_diffuse + self.ground
    }
}

/// Hay & Davies transposition with isotropic ground reflection.
#[must_use]
pub fn hay_davies(
    sun: &SunPosition,
    ghi: f64,
    dhi: f64,
    dni_extra: f64,
    tilt_deg: f64,
    azimuth_deg: f64,
    albedo: f64,
) -> Poa {
    let cos_z = sun.zenith_deg.to_radians().cos();
    if ghi <= 0.0 {
        return Poa::default();
    }
    let dni = if cos_z > 0.0174 {
        ((ghi - dhi) / cos_z).max(0.0)
    } else {
        0.0
    };
    let cos_i = incidence_deg(sun, tilt_deg, azimuth_deg)
        .to_radians()
        .cos()
        .max(0.0);
    let tilt = tilt_deg.to_radians();
    let ai = (dni / dni_extra).clamp(0.0, 1.0);
    let rb = if cos_z > 0.0174 { cos_i / cos_z } else { 0.0 };
    let circumsolar = (dhi * ai * rb).max(0.0);
    let sky = circumsolar + dhi * (1.0 - ai) * (1.0 + tilt.cos()) / 2.0;
    Poa {
        beam: dni * cos_i,
        sky_diffuse: sky.max(0.0),
        circumsolar,
        ground: ghi * albedo * (1.0 - tilt.cos()) / 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_in_bern_at_the_june_solstice() {
        // 21 June (day 172), solar noon in Bern (7.44° E) ≈ 11:28 UTC: zenith ≈ 46.95 − 23.44.
        let s = position(46.95, 7.44, 172, 11.0 + 28.0 / 60.0);
        assert!((s.zenith_deg - 23.5).abs() < 0.3, "{s:?}");
        assert!((s.azimuth_deg - 180.0).abs() < 3.0, "{s:?}");
        let morning = position(46.95, 7.44, 172, 6.0);
        assert!(
            morning.azimuth_deg < 120.0 && morning.zenith_deg < 90.0,
            "{morning:?}"
        );
    }

    #[test]
    fn transposition_of_beam_on_a_sun_facing_plane() {
        let sun = SunPosition {
            zenith_deg: 30.0,
            azimuth_deg: 180.0,
        };
        // Clear sky, plane facing the sun: beam = DNI.
        let ghi = 800.0;
        let dhi = 100.0;
        let p = hay_davies(&sun, ghi, dhi, 1361.0, 30.0, 180.0, 0.2);
        let dni = (ghi - dhi) / 30f64.to_radians().cos();
        assert!((p.beam - dni).abs() < 1e-6);
        assert!(p.sky_diffuse > 0.0 && p.ground > 0.0);
        assert!((erbs_diffuse_fraction(0.1) - 0.991).abs() < 1e-9);
    }
}
