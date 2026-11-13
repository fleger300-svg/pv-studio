//! Typical meteorological year from several years of one station: for each calendar month,
//! the year whose monthly global irradiation is closest to the median over the complete
//! years (a simplified Sandia TMY: it keeps real hour-to-hour variability instead of
//! averaging it away). Gaps of a few hours are interpolated; 29 February is dropped.

use std::collections::BTreeMap;

use pv_model::{WeatherYear, HOURS_PER_YEAR};

use crate::{days_in_month, HourlyRecord};

/// A month is usable when at most this share of its hours is missing (then interpolated).
const MAX_MISSING: f64 = 0.03;

/// Hours of `month` in a non-leap year.
fn hours(month: u32) -> usize {
    days_in_month(2001, month) as usize * 24
}

struct Month {
    ghi: Vec<Option<f32>>,
    dhi: Vec<Option<f32>>,
    temp: Vec<Option<f32>>,
}

/// A usable year of one month: year, global, temperature, diffuse (when complete enough).
type Candidate = (i32, Vec<f32>, Vec<f32>, Option<Vec<f32>>);

/// Fills gaps by linear interpolation (ends: nearest value); `None` when too many are missing.
fn filled(v: &[Option<f32>]) -> Option<Vec<f32>> {
    let missing = v.iter().filter(|x| x.is_none()).count();
    if v.is_empty() || missing as f64 > MAX_MISSING * v.len() as f64 {
        return None;
    }
    let known: Vec<(usize, f32)> = v
        .iter()
        .enumerate()
        .filter_map(|(i, x)| x.map(|x| (i, x)))
        .collect();
    let mut out = Vec::with_capacity(v.len());
    let mut k = 0usize;
    for (i, slot) in v.iter().enumerate() {
        if let Some(x) = *slot {
            out.push(x);
            continue;
        }
        while k + 1 < known.len() && known[k + 1].0 < i {
            k += 1;
        }
        let (a, b) = (
            known.get(k).copied(),
            known.iter().find(|(j, _)| *j > i).copied(),
        );
        out.push(match (a, b) {
            (Some((ia, xa)), Some((ib, xb))) if ia < i => {
                xa + (xb - xa) * (i - ia) as f32 / (ib - ia) as f32
            }
            (_, Some((_, xb))) => xb,
            (Some((_, xa)), None) => xa,
            (None, None) => 0.0,
        });
    }
    Some(out)
}

/// Builds the typical year; `None` when some calendar month has no usable year.
/// `station` describes the source for reports (name, position).
#[must_use]
pub fn typical_year(
    records: &[HourlyRecord],
    station_label: &str,
    lat_deg: f64,
    lon_deg: f64,
    altitude_m: f64,
) -> Option<WeatherYear> {
    // (year, month) → hourly slots.
    let mut months: BTreeMap<(i32, u32), Month> = BTreeMap::new();
    for r in records {
        if r.month == 2 && r.day == 29 {
            continue;
        }
        let n = hours(r.month);
        let m = months.entry((r.year, r.month)).or_insert_with(|| Month {
            ghi: vec![None; n],
            dhi: vec![None; n],
            temp: vec![None; n],
        });
        let i = ((r.day - 1) * 24 + r.hour) as usize;
        if i < n {
            m.ghi[i] = r.ghi.map(|x| x.max(0.0));
            m.dhi[i] = r.dhi.map(|x| x.max(0.0));
            m.temp[i] = r.temp;
        }
    }
    let mut ghi = Vec::with_capacity(HOURS_PER_YEAR);
    let mut dhi = Vec::with_capacity(HOURS_PER_YEAR);
    let mut temp = Vec::with_capacity(HOURS_PER_YEAR);
    let mut with_dhi = true;
    let mut years = Vec::new();
    for month in 1..=12u32 {
        // Usable years of this month and their irradiation.
        let mut usable: Vec<Candidate> = months
            .iter()
            .filter(|((_, m), _)| *m == month)
            .filter_map(|((y, _), m)| {
                let g = filled(&m.ghi)?;
                let t = filled(&m.temp)?;
                Some((*y, g, t, filled(&m.dhi)))
            })
            .collect();
        if usable.is_empty() {
            return None;
        }
        let mut sums: Vec<f64> = usable
            .iter()
            .map(|u| u.1.iter().map(|&x| f64::from(x)).sum())
            .collect();
        sums.sort_by(f64::total_cmp);
        let median = sums[sums.len() / 2];
        usable.sort_by(|a, b| {
            let sa: f64 = a.1.iter().map(|&x| f64::from(x)).sum();
            let sb: f64 = b.1.iter().map(|&x| f64::from(x)).sum();
            (sa - median)
                .abs()
                .total_cmp(&(sb - median).abs())
                .then(b.0.cmp(&a.0))
        });
        let (year, g, t, d) = usable.swap_remove(0);
        years.push(year);
        ghi.extend(g);
        temp.extend(t);
        match d {
            Some(d) if with_dhi => dhi.extend(
                d.into_iter()
                    .zip(&ghi[ghi.len() - hours(month)..])
                    .map(|(d, &g)| d.min(g)),
            ),
            _ => with_dhi = false,
        }
    }
    let (first, last) = (years.iter().min()?, years.iter().max()?);
    Some(WeatherYear {
        source: format!("MeteoSwiss Open Data, {station_label}, typical months {first}–{last}"),
        lat_deg,
        lon_deg,
        altitude_m,
        ghi_w_m2: ghi,
        dhi_w_m2: if with_dhi { dhi } else { Vec::new() },
        temp_c: temp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn year(y: i32, scale: f32) -> Vec<HourlyRecord> {
        let mut out = Vec::new();
        for month in 1..=12 {
            for day in 1..=days_in_month(y, month) {
                for hour in 0..24 {
                    let sun = if (8..16).contains(&hour) {
                        300.0 * scale
                    } else {
                        0.0
                    };
                    out.push(HourlyRecord {
                        year: y,
                        month,
                        day,
                        hour,
                        ghi: Some(sun),
                        dhi: Some(sun / 2.0),
                        temp: Some(10.0),
                    });
                }
            }
        }
        out
    }

    #[test]
    fn picks_the_median_year_of_each_month() {
        let mut r = year(2018, 0.8);
        r.extend(year(2019, 1.0));
        r.extend(year(2020, 1.3)); // leap year: 29 February dropped
                                   // A gap of 2 hours in 2019 is interpolated.
        r.retain(|x| {
            !(x.year == 2019 && x.month == 6 && x.day == 10 && (x.hour == 9 || x.hour == 10))
        });
        let w = typical_year(&r, "BER Bern", 46.99, 7.46, 552.0).expect("typical year");
        assert_eq!(w.ghi_w_m2.len(), HOURS_PER_YEAR);
        assert_eq!(w.dhi_w_m2.len(), HOURS_PER_YEAR);
        // Every month from 2019 (the median): 300 W/m² × 8 h × 365 d.
        assert!(
            (w.annual_ghi_kwh_m2() - 876.0).abs() < 1e-3,
            "{}",
            w.annual_ghi_kwh_m2()
        );
        assert!(w.source.contains("2019–2019"));
    }

    #[test]
    fn a_month_with_too_many_gaps_is_not_used() {
        let mut r = year(2019, 1.0);
        r.retain(|x| x.month != 7 || x.day > 5);
        assert!(typical_year(&r, "X", 0.0, 0.0, 0.0).is_none());
    }
}
