//! Benchmark report file naming: `bench-<preset>-<platform>-<UTC timestamp>.json`.

/// Compact UTC timestamp `YYYYMMDDTHHMMSSZ` for a Unix time (no external date crate).
#[must_use]
pub fn utc_stamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days (H. Hinnant), valid for the proleptic Gregorian calendar.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}T{h:02}{m:02}{s:02}Z")
}

/// File name of a benchmark report. `preset` and `platform` are lower-cased slugs.
#[must_use]
pub fn report_file_name(preset: &str, platform: &str, unix_secs: u64) -> String {
    let slug = |s: &str| {
        s.chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
    };
    format!(
        "bench-{}-{}-{}.json",
        slug(preset),
        slug(platform),
        utc_stamp(unix_secs)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamps() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(946_684_800), "20000101T000000Z");
        assert_eq!(utc_stamp(951_782_400 + 3_723), "20000229T010203Z"); // leap day
        assert_eq!(utc_stamp(1_793_836_800), "20261105T000000Z");
    }

    #[test]
    fn file_name() {
        assert_eq!(
            report_file_name("Residential1k", "windows", 0),
            "bench-residential1k-windows-19700101T000000Z.json"
        );
    }
}
