//! Site data, pure parts (no network: the application downloads, this crate reads):
//!
//! - MeteoSwiss Open Data (`ch.meteoschweiz.ogd-smn`): station list
//!   (`ogd-smn_meta_stations.csv`) and hourly station files
//!   (`ogd-smn_<abbr>_h_historical_<decade>.csv`, `…_h_recent.csv`): `;`-separated,
//!   Windows-1252, `ReferenceTS` = `dd.mm.yyyy HH:MM` UTC, missing values empty. Hourly
//!   global radiation `gre000h0`, diffuse `ods000h0` [W/m²], air temperature `tre200h0` [°C].
//! - The typical meteorological year built from several years of one station ([`typical`]).
//! - geo.admin.ch answers: address search (`SearchServer`, `type=locations`, `sr=4326`) and
//!   terrain height (`height`, LV95).

pub mod typical;

use serde::Deserialize;

/// A MeteoSwiss station.
#[derive(Debug, Clone, PartialEq)]
pub struct Station {
    pub abbr: String,
    pub name: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub altitude_m: f64,
}

/// Decodes Windows-1252 (Latin-1 for the characters of Swiss station names).
#[must_use]
pub fn decode_cp1252(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

fn columns(header: &str) -> Vec<String> {
    header
        .trim_start_matches('\u{feff}')
        .split(';')
        .map(|c| c.trim().trim_matches('"').to_owned())
        .collect()
}

/// Stations of `ogd-smn_meta_stations.csv` (rows without coordinates are skipped).
#[must_use]
pub fn parse_stations(text: &str) -> Vec<Station> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols = columns(header);
    let at = |name: &str| cols.iter().position(|c| c == name);
    let (Some(abbr), Some(name), Some(lat), Some(lon), Some(alt)) = (
        at("station_abbr"),
        at("station_name"),
        at("station_coordinates_wgs84_lat"),
        at("station_coordinates_wgs84_lon"),
        at("station_height_masl"),
    ) else {
        return Vec::new();
    };
    lines
        .filter_map(|l| {
            let f: Vec<&str> = l.split(';').map(|x| x.trim().trim_matches('"')).collect();
            Some(Station {
                abbr: f.get(abbr)?.to_lowercase(),
                name: (*f.get(name)?).to_owned(),
                lat_deg: f.get(lat)?.parse().ok()?,
                lon_deg: f.get(lon)?.parse().ok()?,
                altitude_m: f.get(alt)?.parse().unwrap_or(0.0),
            })
        })
        .collect()
}

/// Great-circle distance [km].
#[must_use]
pub fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6371.0 * a.sqrt().asin()
}

/// Stations ordered by suitability for a site: distance, with 100 m of altitude difference
/// counting as 5 km (climate depends on altitude as much as on distance).
#[must_use]
pub fn nearest_stations(stations: &[Station], lat: f64, lon: f64, altitude_m: f64) -> Vec<Station> {
    let score = |s: &Station| {
        distance_km(lat, lon, s.lat_deg, s.lon_deg)
            + (s.altitude_m - altitude_m).abs() / 100.0 * 5.0
    };
    let mut out = stations.to_vec();
    out.sort_by(|a, b| score(a).total_cmp(&score(b)));
    out
}

/// One hour of a station file.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HourlyRecord {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Hour of the timestamp (0–23), UTC.
    pub hour: u32,
    pub ghi: Option<f32>,
    pub dhi: Option<f32>,
    pub temp: Option<f32>,
}

fn parse_ts(ts: &str) -> Option<(i32, u32, u32, u32)> {
    let (date, time) = ts.trim().split_once(' ')?;
    let mut d = date.split('.');
    let day = d.next()?.parse().ok()?;
    let month = d.next()?.parse().ok()?;
    let year = d.next()?.parse().ok()?;
    let hour = time.split(':').next()?.parse().ok()?;
    Some((year, month, day, hour))
}

/// Records of an hourly station file. MeteoSwiss stamps an hourly mean at the *end* of its
/// interval: the record is returned at the hour it *starts* (stamp − 1 h, `hour` 23 of the
/// previous day for a 00:00 stamp).
#[must_use]
pub fn parse_hourly(text: &str) -> Vec<HourlyRecord> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let cols = columns(header);
    let at = |name: &str| cols.iter().position(|c| c == name);
    let Some(ts) = at("reference_timestamp").or_else(|| at("ReferenceTS")) else {
        return Vec::new();
    };
    let (ghi, dhi, temp) = (at("gre000h0"), at("ods000h0"), at("tre200h0"));
    let num = |f: &[&str], i: Option<usize>| -> Option<f32> {
        f.get(i?).and_then(|x| x.trim().parse::<f32>().ok())
    };
    lines
        .filter_map(|l| {
            let f: Vec<&str> = l.split(';').collect();
            let (y, m, d, h) = parse_ts(f.get(ts)?)?;
            let (y, m, d, h) = previous_hour(y, m, d, h);
            Some(HourlyRecord {
                year: y,
                month: m,
                day: d,
                hour: h,
                ghi: num(&f, ghi),
                dhi: num(&f, dhi),
                temp: num(&f, temp),
            })
        })
        .collect()
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days in `month` of `year`.
#[must_use]
pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn previous_hour(y: i32, m: u32, d: u32, h: u32) -> (i32, u32, u32, u32) {
    if h > 0 {
        return (y, m, d, h - 1);
    }
    if d > 1 {
        return (y, m, d - 1, 23);
    }
    if m > 1 {
        return (y, m - 1, days_in_month(y, m - 1), 23);
    }
    (y - 1, 12, 31, 23)
}

/// A geocoded place (geo.admin.ch `SearchServer`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub label: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
}

/// Places of a `SearchServer?type=locations&sr=4326` answer (HTML tags removed).
#[must_use]
pub fn parse_search(json: &str) -> Vec<Place> {
    #[derive(Deserialize)]
    struct Answer {
        results: Vec<Item>,
    }
    #[derive(Deserialize)]
    struct Item {
        attrs: Attrs,
    }
    #[derive(Deserialize)]
    struct Attrs {
        label: String,
        lat: f64,
        lon: f64,
    }
    let Ok(a) = serde_json::from_str::<Answer>(json) else {
        return Vec::new();
    };
    a.results
        .into_iter()
        .map(|i| Place {
            label: strip_tags(&i.attrs.label),
            lat_deg: i.attrs.lat,
            lon_deg: i.attrs.lon,
        })
        .collect()
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Height of a `height?easting=…&northing=…&sr=2056` answer (`{"height":"553.4"}`).
#[must_use]
pub fn parse_height(json: &str) -> Option<f64> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    match &v["height"] {
        serde_json::Value::String(s) => s.parse().ok(),
        serde_json::Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stations_and_nearest() {
        let csv = "station_abbr;station_name;station_canton;station_height_masl;station_coordinates_wgs84_lat;station_coordinates_wgs84_lon\n\
                   BER;Bern / Zollikofen;BE;552.0;46.990744;7.464061\n\
                   PUY;Pully;VD;456.0;46.512417;6.667683\n\
                   JUN;Jungfraujoch;BE;3571.0;46.547556;7.985306\n";
        let s = parse_stations(&decode_cp1252(csv.as_bytes()));
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].abbr, "ber");
        // Lausanne: Pully first; Thun (560 m): Bern before the Jungfraujoch.
        assert_eq!(nearest_stations(&s, 46.52, 6.63, 500.0)[0].abbr, "puy");
        assert_eq!(nearest_stations(&s, 46.75, 7.63, 560.0)[0].abbr, "ber");
    }

    #[test]
    fn hourly_records_start_at_the_hour_before_their_stamp() {
        let csv = "station_abbr;reference_timestamp;gre000h0;ods000h0;tre200h0\n\
                   BER;01.01.2020 00:00;0;;-2.5\n\
                   BER;01.03.2020 13:00;512.3;180.1;8.2\n";
        let r = parse_hourly(csv);
        assert_eq!(r.len(), 2);
        assert_eq!(
            (r[0].year, r[0].month, r[0].day, r[0].hour),
            (2019, 12, 31, 23)
        );
        assert_eq!(r[0].dhi, None);
        assert_eq!((r[1].month, r[1].day, r[1].hour), (3, 1, 12));
        assert_eq!(r[1].ghi, Some(512.3));
    }

    #[test]
    fn geo_admin_answers() {
        let search = r#"{"results":[{"attrs":{"label":"<b>Bundesplatz 3</b> 3011 Bern","lat":46.9466,"lon":7.4441}}]}"#;
        let p = parse_search(search);
        assert_eq!(p[0].label, "Bundesplatz 3 3011 Bern");
        assert_eq!(parse_height(r#"{"height":"553.4"}"#), Some(553.4));
        assert_eq!(parse_height("{}"), None);
    }
}
