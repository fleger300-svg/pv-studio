//! Unit helpers. Internally everything is SI (metres, radians in maths). Degrees are used
//! for stored angles because they are the user-facing unit; convert at the boundary.

/// Degrees → radians.
#[must_use]
pub fn deg_to_rad(deg: f64) -> f64 {
    deg.to_radians()
}

/// Radians → degrees.
#[must_use]
pub fn rad_to_deg(rad: f64) -> f64 {
    rad.to_degrees()
}

/// Peak power in kWp for `count` modules of `wp_per_module` watts-peak.
#[must_use]
pub fn peak_power_kwp(count: usize, wp_per_module: f64) -> f64 {
    count as f64 * wp_per_module / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_round_trip() {
        assert!((rad_to_deg(deg_to_rad(37.5)) - 37.5).abs() < 1e-12);
    }

    #[test]
    fn kwp() {
        assert!((peak_power_kwp(1000, 430.0) - 430.0).abs() < 1e-9);
    }
}
