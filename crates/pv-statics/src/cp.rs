//! External pressure coefficients per roof zone.
//!
//! A [`CpSource`] gives, for one roof zone and loaded area, the most unfavourable pressure
//! (c_pe ≥ 0) and suction (c_pe ≤ 0) coefficient. Two kinds of source exist:
//!
//! - [`CpSource::Standard`]: normative tables (SN EN 1991-1-4 §7.2, which SIA 261 refers to
//!   for roof zones), interpolated on pitch and loaded area.
//! - [`CpSource::Manufacturer`]: explicit values from a manufacturer (wind-tunnel tests,
//!   e.g. K2 D-Dome / S-Dome). They are injected as data and take precedence when they exist.
//!
//! # Zones
//!
//! [`RoofZone`] is the position of a field on the roof face, as K2 Base names it. Each
//! position can be hit by several normative zones depending on wind direction (θ = 0° / 90°),
//! so a position's coefficient is the envelope over those zones. The mapping for hipped roofs
//! reproduces K2 Base 3.2.109 exactly (see `testdata/k2/reference_loads.json`, case `ref-a`).

use serde::{Deserialize, Serialize};

use crate::trace::{NormRef, Traced, Unit};
use crate::StaticsError;

pub const EN_1991_1_4_AREA: NormRef = NormRef::new("SN EN 1991-1-4", "§7.2.1, fig. 7.2");
pub const EN_1991_1_4_FLAT: NormRef = NormRef::new("SN EN 1991-1-4", "§7.2.3, tab. 7.2");
pub const EN_1991_1_4_HIPPED: NormRef = NormRef::new("SN EN 1991-1-4", "§7.2.6, tab. 7.5");

/// Roof shape, for the choice of the normative coefficient table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofShape {
    /// α < 5°.
    Flat,
    Monopitch,
    Duopitch,
    Hipped,
}

/// Position of a field on the roof face (K2 Base nomenclature).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoofZone {
    /// K2: « Zone de champ interne ».
    Interior,
    /// Strip along the ridge. K2: « Bord du faîte du toit ».
    Ridge,
    /// Strip along the verge, or along the hip line on a hipped roof.
    /// K2: « Rebord d'avant-toit ».
    Verge,
    /// Strip along the eave. K2: « Solin de gouttière ».
    Eave,
    /// Corner at the eave. K2: « Zone d'angle (gouttière) ».
    EaveCorner,
}

impl RoofZone {
    pub const ALL: [RoofZone; 5] = [
        RoofZone::Interior,
        RoofZone::Ridge,
        RoofZone::Verge,
        RoofZone::Eave,
        RoofZone::EaveCorner,
    ];

    /// Message key for the zone name (resolved by the i18n layer).
    #[must_use]
    pub fn label_key(self) -> &'static str {
        match self {
            RoofZone::Interior => "statics.zone.interior",
            RoofZone::Ridge => "statics.zone.ridge",
            RoofZone::Verge => "statics.zone.verge",
            RoofZone::Eave => "statics.zone.eave",
            RoofZone::EaveCorner => "statics.zone.eave_corner",
        }
    }
}

/// Coefficients of one zone for one loaded area.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneCp {
    pub zone: RoofZone,
    pub loaded_area_m2: f64,
    /// Most unfavourable pressure coefficient, ≥ 0.
    pub pressure: Traced,
    /// Most unfavourable suction coefficient, ≤ 0.
    pub suction: Traced,
}

/// Explicit coefficients from a manufacturer, for one roof zone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturerCpEntry {
    pub zone: RoofZone,
    pub cp_pressure: f64,
    pub cp_suction: f64,
}

/// A manufacturer's coefficient table (wind-tunnel values), loaded from the component
/// library. The provenance fields are shown in the report next to the values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManufacturerCpTable {
    pub manufacturer: String,
    pub product: String,
    /// Source document, e.g. a wind-tunnel report reference.
    pub document: String,
    /// Valid pitch range of the roof [°].
    pub pitch_range_deg: (f64, f64),
    pub entries: Vec<ManufacturerCpEntry>,
}

/// Where the pressure coefficients come from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CpSource {
    #[default]
    Standard,
    Manufacturer(ManufacturerCpTable),
}

impl CpSource {
    /// Coefficients of `zone` on a roof of the given shape and pitch, for a loaded area [m²].
    pub fn zone_coefficients(
        &self,
        shape: RoofShape,
        pitch_deg: f64,
        zone: RoofZone,
        loaded_area_m2: f64,
    ) -> Result<ZoneCp, StaticsError> {
        StaticsError::check_range("loaded_area_m2", loaded_area_m2, 0.01, 10_000.0)?;
        match self {
            CpSource::Standard => standard_coefficients(shape, pitch_deg, zone, loaded_area_m2),
            CpSource::Manufacturer(table) => {
                manufacturer_coefficients(table, pitch_deg, zone, loaded_area_m2)
            }
        }
    }
}

fn manufacturer_coefficients(
    table: &ManufacturerCpTable,
    pitch_deg: f64,
    zone: RoofZone,
    loaded_area_m2: f64,
) -> Result<ZoneCp, StaticsError> {
    let (lo, hi) = table.pitch_range_deg;
    StaticsError::check_range("pitch_deg", pitch_deg, lo, hi)?;
    let entry = table
        .entries
        .iter()
        .find(|e| e.zone == zone)
        .ok_or(StaticsError::MissingCoefficient { zone })?;
    let reference = NormRef::new("Manufacturer data", "wind-tunnel test");
    let provenance = format!(
        "{} {} — {}",
        table.manufacturer, table.product, table.document
    );
    let make = |symbol: &str, value: f64| {
        Traced::given(symbol, value, Unit::Dimensionless)
            .reference(reference)
            .assume("statics.assumption.cp.manufacturer", provenance.clone())
    };
    Ok(ZoneCp {
        zone,
        loaded_area_m2,
        pressure: make("c_pe,+", entry.cp_pressure.max(0.0)),
        suction: make("c_pe,−", entry.cp_suction.min(0.0)),
    })
}

// ---------------------------------------------------------------------------------------------
// Normative tables
// ---------------------------------------------------------------------------------------------

/// One cell of an EN 1991-1-4 roof table: suction c_pe,10 and c_pe,1 (≤ 0) and pressure
/// (≥ 0, area-independent in the tables used here). A missing value in the standard is 0.
#[derive(Debug, Clone, Copy)]
struct Cell {
    suction_10: f64,
    suction_1: f64,
    pressure: f64,
}

const fn c(suction_10: f64, suction_1: f64, pressure: f64) -> Cell {
    Cell {
        suction_10,
        suction_1,
        pressure,
    }
}

/// Zones of SN EN 1991-1-4 fig. 7.9 / tab. 7.5 (hipped roofs), in table column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HipZone {
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
}

impl HipZone {
    fn name(self) -> &'static str {
        match self {
            HipZone::F => "F",
            HipZone::G => "G",
            HipZone::H => "H",
            HipZone::I => "I",
            HipZone::J => "J",
            HipZone::K => "K",
            HipZone::L => "L",
            HipZone::M => "M",
            HipZone::N => "N",
        }
    }
}

/// SN EN 1991-1-4 tab. 7.5, rows by pitch [°], columns F G H I J K L M N.
#[rustfmt::skip]
const HIPPED_TABLE: [(f64, [Cell; 9]); 6] = [
    (5.0,  [c(-1.7, -2.5, 0.0), c(-1.2, -2.0, 0.0), c(-0.6, -1.2, 0.0), c(-0.3, -0.3, 0.0), c(-0.6, -0.6, 0.0),
            c(-0.6, -0.6, 0.0), c(-1.2, -2.0, 0.0), c(-0.6, -1.2, 0.0), c(-0.4, -0.4, 0.0)]),
    (15.0, [c(-0.9, -2.0, 0.2), c(-0.8, -1.5, 0.2), c(-0.3, -0.3, 0.2), c(-0.5, -0.5, 0.0), c(-1.0, -1.5, 0.0),
            c(-1.2, -2.0, 0.0), c(-1.4, -2.0, 0.0), c(-0.6, -1.2, 0.0), c(-0.3, -0.3, 0.0)]),
    (30.0, [c(-0.5, -1.5, 0.5), c(-0.5, -1.5, 0.7), c(-0.2, -0.2, 0.4), c(-0.4, -0.4, 0.0), c(-0.7, -1.2, 0.0),
            c(-0.5, -0.5, 0.0), c(-1.4, -2.0, 0.0), c(-0.8, -1.2, 0.0), c(-0.2, -0.2, 0.0)]),
    (45.0, [c(0.0, 0.0, 0.7),   c(0.0, 0.0, 0.7),   c(0.0, 0.0, 0.6),   c(-0.3, -0.3, 0.0), c(-0.6, -0.6, 0.0),
            c(-0.3, -0.3, 0.0), c(-1.3, -2.0, 0.0), c(-0.8, -1.2, 0.0), c(-0.2, -0.2, 0.0)]),
    (60.0, [c(0.0, 0.0, 0.7),   c(0.0, 0.0, 0.7),   c(0.0, 0.0, 0.7),   c(-0.3, -0.3, 0.0), c(-0.6, -0.6, 0.0),
            c(-0.3, -0.3, 0.0), c(-1.2, -2.0, 0.0), c(-0.4, -0.4, 0.0), c(-0.2, -0.2, 0.0)]),
    (75.0, [c(0.0, 0.0, 0.8),   c(0.0, 0.0, 0.8),   c(0.0, 0.0, 0.8),   c(-0.3, -0.3, 0.0), c(-0.6, -0.6, 0.0),
            c(-0.3, -0.3, 0.0), c(-1.2, -2.0, 0.0), c(-0.4, -0.4, 0.0), c(-0.2, -0.2, 0.0)]),
];

/// Normative zones that can act on each roof position of a hipped roof, over all wind
/// directions. Calibrated against K2 Base 3.2.109 (case `ref-a`: all 5 positions, 10 m² and
/// module area, match to < 0.3 %).
fn hipped_envelope(zone: RoofZone) -> &'static [HipZone] {
    use HipZone::{F, G, H, I, J, K, L, M, N};
    match zone {
        RoofZone::Interior => &[H, I, M, N],
        RoofZone::Ridge => &[H, I, J, K],
        RoofZone::Verge => &[H, I, L, M],
        RoofZone::Eave => &[G, H],
        RoofZone::EaveCorner => &[F, G, L],
    }
}

/// SN EN 1991-1-4 tab. 7.2, flat roofs with sharp eaves, columns F G H I.
const FLAT_TABLE: [Cell; 4] = [
    c(-1.8, -2.5, 0.0),
    c(-1.2, -2.0, 0.0),
    c(-0.7, -1.2, 0.0),
    c(-0.2, -0.2, 0.2),
];
const FLAT_NAMES: [&str; 4] = ["F", "G", "H", "I"];

fn flat_envelope(zone: RoofZone) -> Result<&'static [usize], StaticsError> {
    match zone {
        RoofZone::EaveCorner => Ok(&[0]),
        RoofZone::Eave | RoofZone::Verge => Ok(&[1]),
        RoofZone::Interior => Ok(&[2, 3]),
        RoofZone::Ridge => Err(StaticsError::MissingCoefficient { zone }),
    }
}

/// c_pe(A) = c_pe,1 − (c_pe,1 − c_pe,10)·log₁₀ A, clamped to [1, 10] m².
fn area_interpolate(cp1: f64, cp10: f64, area_m2: f64) -> f64 {
    let a = area_m2.clamp(1.0, 10.0);
    cp1 - (cp1 - cp10) * a.log10()
}

fn interpolate_cell(a: Cell, b: Cell, t: f64) -> Cell {
    let lerp = |x: f64, y: f64| x + (y - x) * t;
    Cell {
        suction_10: lerp(a.suction_10, b.suction_10),
        suction_1: lerp(a.suction_1, b.suction_1),
        pressure: lerp(a.pressure, b.pressure),
    }
}

fn hipped_row(pitch_deg: f64) -> Result<[Cell; 9], StaticsError> {
    let (first, last) = (HIPPED_TABLE[0].0, HIPPED_TABLE[HIPPED_TABLE.len() - 1].0);
    StaticsError::check_range("pitch_deg", pitch_deg, first, last)?;
    for pair in HIPPED_TABLE.windows(2) {
        let ((a0, ra), (a1, rb)) = (pair[0], pair[1]);
        if pitch_deg <= a1 {
            let t = (pitch_deg - a0) / (a1 - a0);
            return Ok(std::array::from_fn(|i| interpolate_cell(ra[i], rb[i], t)));
        }
    }
    Ok(HIPPED_TABLE[HIPPED_TABLE.len() - 1].1)
}

fn standard_coefficients(
    shape: RoofShape,
    pitch_deg: f64,
    zone: RoofZone,
    area_m2: f64,
) -> Result<ZoneCp, StaticsError> {
    // (name, cell) of every normative zone in the envelope.
    let (members, reference): (Vec<(&'static str, Cell)>, NormRef) = match shape {
        RoofShape::Hipped => {
            let row = hipped_row(pitch_deg)?;
            let members = hipped_envelope(zone)
                .iter()
                .map(|&z| (z.name(), row[z as usize]))
                .collect();
            (members, EN_1991_1_4_HIPPED)
        }
        RoofShape::Flat => {
            StaticsError::check_range("pitch_deg", pitch_deg, 0.0, 5.0)?;
            let members = flat_envelope(zone)?
                .iter()
                .map(|&i| (FLAT_NAMES[i], FLAT_TABLE[i]))
                .collect();
            (members, EN_1991_1_4_FLAT)
        }
        RoofShape::Monopitch | RoofShape::Duopitch => {
            return Err(StaticsError::Unsupported {
                what: format!("normative c_pe table for {shape:?} roofs"),
            });
        }
    };

    let envelope_detail =
        format!(
        "Envelope over zones {} (all wind directions), α = {pitch_deg:.1}°, A = {area_m2:.2} m²",
        members.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
    );
    let mut pressure = Traced::new("c_pe,+", 0.0, Unit::Dimensionless)
        .formula("c_pe,+ = max over zones of c_pe (pressure)")
        .reference(reference)
        .input("A", area_m2, Unit::SquareMetre)
        .input("α", pitch_deg, Unit::Degree)
        .assume("statics.assumption.cp.envelope", envelope_detail.clone());
    let mut suction = Traced::new("c_pe,−", 0.0, Unit::Dimensionless)
        .formula("c_pe,− = min over zones of c_pe,1 − (c_pe,1 − c_pe,10)·log₁₀ A")
        .reference(reference)
        .input("A", area_m2, Unit::SquareMetre)
        .input("α", pitch_deg, Unit::Degree)
        .assume("statics.assumption.cp.envelope", envelope_detail);
    for (name, cell) in members {
        let s = area_interpolate(cell.suction_1, cell.suction_10, area_m2);
        pressure = pressure.input(
            format!("c_pe,+({name})"),
            cell.pressure,
            Unit::Dimensionless,
        );
        suction = suction.input(format!("c_pe,−({name})"), s, Unit::Dimensionless);
        pressure.value = pressure.value.max(cell.pressure);
        suction.value = suction.value.min(s);
    }
    Ok(ZoneCp {
        zone,
        loaded_area_m2: area_m2,
        pressure,
        suction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_interpolation_endpoints() {
        assert_eq!(area_interpolate(-2.0, -1.4, 0.5), -2.0);
        assert_eq!(area_interpolate(-2.0, -1.4, 1.0), -2.0);
        assert!((area_interpolate(-2.0, -1.4, 10.0) + 1.4).abs() < 1e-12);
        assert!((area_interpolate(-2.0, -1.4, 50.0) + 1.4).abs() < 1e-12);
    }

    #[test]
    fn hipped_table_rows_are_exact_at_grid_points() {
        let row = hipped_row(15.0).unwrap();
        assert_eq!(row[HipZone::K as usize].suction_10, -1.2);
        let row = hipped_row(75.0).unwrap();
        assert_eq!(row[HipZone::H as usize].pressure, 0.8);
        assert!(hipped_row(4.0).is_err());
    }

    #[test]
    fn manufacturer_table_takes_precedence() {
        let table = ManufacturerCpTable {
            manufacturer: "Test".into(),
            product: "Dome".into(),
            document: "WT-1".into(),
            pitch_range_deg: (0.0, 5.0),
            entries: vec![ManufacturerCpEntry {
                zone: RoofZone::Interior,
                cp_pressure: 0.3,
                cp_suction: -0.4,
            }],
        };
        let src = CpSource::Manufacturer(table);
        let cp = src
            .zone_coefficients(RoofShape::Flat, 2.0, RoofZone::Interior, 2.0)
            .unwrap();
        assert_eq!((cp.pressure.value, cp.suction.value), (0.3, -0.4));
        assert!(matches!(
            src.zone_coefficients(RoofShape::Flat, 2.0, RoofZone::EaveCorner, 2.0),
            Err(StaticsError::MissingCoefficient { .. })
        ));
    }

    #[test]
    fn duopitch_is_reported_unsupported() {
        let err = CpSource::Standard.zone_coefficients(
            RoofShape::Duopitch,
            20.0,
            RoofZone::Interior,
            10.0,
        );
        assert!(matches!(err, Err(StaticsError::Unsupported { .. })));
    }
}
