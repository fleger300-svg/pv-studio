//! PV*SOL weather files (`*.wbv`, binary): the hourly year the PV*SOL simulation used
//! (typically Meteonorm 2001–2020 for the project's location).
//!
//! Layout (little endian, identical in every file of the corpus, PV*SOL 2024–2026):
//! - `0x000`, `0x100`, `0x200`: length-prefixed strings in 256-byte slots: location name,
//!   author, column list (`T;G;WGS`: air temperature °C, global horizontal W/m², wind m/s);
//! - `0x304` `i32`: number of hours (8760);
//! - `0x30D` `f64`: latitude [°]; `0x315` `f64`: longitude [°, **east negative**];
//! - `0x362` `f64`: altitude [m]; `0x398` `f64`: annual global horizontal [Wh/m²];
//! - the last `hours × columns × 4` bytes: `f32` values, hour by hour.
//!
//! Hours are **local standard time** (row `h` = interval `[h, h + 1)` of the time zone of the
//! project): over the Montreux year the irradiance-weighted mean hour is 12.5, i.e. solar
//! noon at 12:30 UTC+1. PV Studio's weather year is in UTC, hence the shift in
//! [`PvsolWeather::to_weather_year`].

use pv_model::{WeatherYear, HOURS_PER_YEAR};

use crate::ReadError;

/// A decoded weather file.
#[derive(Debug, Clone, PartialEq)]
pub struct PvsolWeather {
    pub location_name: String,
    pub lat_deg: f64,
    /// East positive (converted from the file's convention).
    pub lon_deg: f64,
    pub altitude_m: f64,
    pub ghi_w_m2: Vec<f32>,
    pub temp_c: Vec<f32>,
}

impl PvsolWeather {
    /// As the project's weather year, in UTC: the file's local hours are shifted by
    /// `utc_offset_h` (+1 for Switzerland: UTC hour `h` is local hour `h + 1`); the year wraps
    /// around. No measured diffuse: the simulation decomposes.
    #[must_use]
    pub fn to_weather_year(&self, file_name: &str, utc_offset_h: i32) -> WeatherYear {
        let shift =
            usize::try_from(i64::from(utc_offset_h).rem_euclid(HOURS_PER_YEAR as i64)).unwrap_or(0);
        let to_utc = |v: &[f32]| {
            let mut out = v.to_vec();
            out.rotate_left(shift);
            out
        };
        WeatherYear {
            source: format!("PV*SOL — {} ({file_name})", self.location_name),
            lat_deg: self.lat_deg,
            lon_deg: self.lon_deg,
            altitude_m: self.altitude_m,
            ghi_w_m2: to_utc(&self.ghi_w_m2),
            dhi_w_m2: Vec::new(),
            temp_c: to_utc(&self.temp_c),
        }
    }

    /// Annual global horizontal irradiation [kWh/m²].
    #[must_use]
    pub fn annual_ghi_kwh_m2(&self) -> f64 {
        self.ghi_w_m2.iter().map(|&g| f64::from(g)).sum::<f64>() / 1000.0
    }
}

fn pascal_string(b: &[u8], at: usize) -> Option<String> {
    let len = usize::from(*b.get(at)?);
    let s = b.get(at + 1..at + 1 + len)?;
    // Delphi short strings in the Windows code page: Latin-1 covers the location names.
    Some(s.iter().map(|&c| char::from(c)).collect())
}

fn f64_at(b: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?))
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Decodes a `.wbv` file.
///
/// # Errors
/// [`ReadError::Weather`] when the layout is not the known one or the values are implausible.
pub fn parse(bytes: &[u8]) -> Result<PvsolWeather, ReadError> {
    let bad = |why: &str| ReadError::Weather(why.to_owned());
    let columns = pascal_string(bytes, 0x200).ok_or_else(|| bad("columns"))?;
    let cols: Vec<&str> = columns.split(';').collect();
    let (Some(ti), Some(gi)) = (
        cols.iter().position(|c| *c == "T"),
        cols.iter().position(|c| *c == "G"),
    ) else {
        return Err(bad("columns"));
    };
    let hours = i32_at(bytes, 0x304).ok_or_else(|| bad("hours"))?;
    if usize::try_from(hours).ok() != Some(HOURS_PER_YEAR) {
        return Err(bad("hours"));
    }
    let data_len = HOURS_PER_YEAR * cols.len() * 4;
    let start = bytes
        .len()
        .checked_sub(data_len)
        .filter(|&s| s >= 0x3A8)
        .ok_or_else(|| bad("length"))?;
    let lat = f64_at(bytes, 0x30D).ok_or_else(|| bad("latitude"))?;
    let lon = -f64_at(bytes, 0x315).ok_or_else(|| bad("longitude"))?;
    let alt = f64_at(bytes, 0x362).ok_or_else(|| bad("altitude"))?;
    if !((-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon))
        || !(-500.0..=9000.0).contains(&alt)
    {
        return Err(bad("location"));
    }
    let mut ghi = Vec::with_capacity(HOURS_PER_YEAR);
    let mut temp = Vec::with_capacity(HOURS_PER_YEAR);
    for h in 0..HOURS_PER_YEAR {
        let row = start + h * cols.len() * 4;
        let g = f32_at(bytes, row + gi * 4).ok_or_else(|| bad("data"))?;
        let t = f32_at(bytes, row + ti * 4).ok_or_else(|| bad("data"))?;
        if !(g.is_finite()
            && (0.0..=1500.0).contains(&g)
            && t.is_finite()
            && (-60.0..=60.0).contains(&t))
        {
            return Err(bad("data"));
        }
        ghi.push(g);
        temp.push(t);
    }
    // Integrity: the header carries the annual sum (it differs from the f32 sum by rounding).
    let sum: f64 = ghi.iter().map(|&g| f64::from(g)).sum();
    if let Some(expected) = f64_at(bytes, 0x398).filter(|&s| s > 0.0) {
        if ((sum - expected) / expected).abs() > 1e-3 {
            return Err(bad("checksum"));
        }
    }
    Ok(PvsolWeather {
        location_name: pascal_string(bytes, 0).unwrap_or_default(),
        lat_deg: lat,
        lon_deg: lon,
        altitude_m: alt,
        ghi_w_m2: ghi,
        temp_c: temp,
    })
}

/// Writes a `.wbv` in the same layout (tests and anonymised fixtures).
#[must_use]
pub fn encode(w: &PvsolWeather) -> Vec<u8> {
    let mut b = vec![0u8; 2517];
    let put_str = |b: &mut Vec<u8>, at: usize, s: &str| {
        let bytes: Vec<u8> = s
            .chars()
            .map(|c| u8::try_from(c).unwrap_or(b'?'))
            .take(255)
            .collect();
        b[at] = u8::try_from(bytes.len()).unwrap_or(255);
        b[at + 1..at + 1 + bytes.len()].copy_from_slice(&bytes);
    };
    put_str(&mut b, 0, &w.location_name);
    put_str(&mut b, 0x200, "T;G;WGS");
    b[0x304..0x308].copy_from_slice(&8760i32.to_le_bytes());
    b[0x30D..0x315].copy_from_slice(&w.lat_deg.to_le_bytes());
    b[0x315..0x31D].copy_from_slice(&(-w.lon_deg).to_le_bytes());
    b[0x362..0x36A].copy_from_slice(&w.altitude_m.to_le_bytes());
    let sum: f64 = w.ghi_w_m2.iter().map(|&g| f64::from(g)).sum();
    b[0x398..0x3A0].copy_from_slice(&sum.to_le_bytes());
    for h in 0..HOURS_PER_YEAR {
        b.extend_from_slice(&w.temp_c[h].to_le_bytes());
        b.extend_from_slice(&w.ghi_w_m2[h].to_le_bytes());
        b.extend_from_slice(&1.0f32.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PvsolWeather {
        PvsolWeather {
            location_name: "Montreux".into(),
            lat_deg: 46.43,
            lon_deg: 6.91,
            altitude_m: 391.0,
            ghi_w_m2: (0..HOURS_PER_YEAR)
                .map(|h| {
                    if (8..16).contains(&(h % 24)) {
                        400.0
                    } else {
                        0.0
                    }
                })
                .collect(),
            temp_c: vec![10.5; HOURS_PER_YEAR],
        }
    }

    #[test]
    fn round_trips_through_the_binary_layout() {
        let w = sample();
        let bytes = encode(&w);
        assert_eq!(bytes.len(), 107_637, "same size as PV*SOL's files");
        let back = parse(&bytes).unwrap();
        assert_eq!(back, w);
        assert!((back.annual_ghi_kwh_m2() - 8.0 * 0.4 * 365.0).abs() < 1e-6);
        let y = back.to_weather_year("x.wbv", 1);
        // UTC hour 7 is local hour 8 (UTC+1): the first lit hour of the sample.
        assert_eq!((y.ghi_w_m2[6], y.ghi_w_m2[7]), (0.0, 400.0));
        assert_eq!(y.ghi_w_m2[HOURS_PER_YEAR - 1], 0.0);
        assert!(y.source.contains("Montreux"));
        assert!(y.dhi_w_m2.is_empty());
    }

    #[test]
    fn refuses_truncated_or_corrupted_files() {
        let bytes = encode(&sample());
        assert!(parse(&bytes[..2000]).is_err());
        let mut bad = bytes.clone();
        bad[0x398..0x3A0].copy_from_slice(&1.0f64.to_le_bytes());
        assert!(matches!(parse(&bad), Err(ReadError::Weather(w)) if w == "checksum"));
    }
}
