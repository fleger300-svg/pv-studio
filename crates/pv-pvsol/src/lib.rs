//! Reads PV*SOL premium projects (`.pvprj`) — configuration, 3-D scene, wiring, component
//! records, weather and PV*SOL's own results — and imports them as PV Studio projects
//! ([`import`]), so that existing projects can be migrated and PV Studio's computations
//! compared with PV*SOL's on the same input.
//!
//! The format is undocumented; what is decoded was established on the 224 projects of the
//! Younergy corpus (PV*SOL premium 2024 R6 to 2026 R8), see each module's notes.

pub mod archive;
pub mod compare;
pub mod config;
pub mod db;
pub mod geometry;
pub mod guid;
pub mod heightmap;
pub mod import;
pub mod results;
pub mod weather;
pub mod wiring;
pub mod xml;

pub use archive::Archive;
pub use compare::{compare, Comparison, ComparisonRow};
pub use config::PvsolConfig;
pub use db::PvsolDb;
pub use geometry::Scene;
pub use import::{import, ImportNote, ImportOutcome};
pub use results::PvsolResults;
pub use weather::PvsolWeather;
pub use wiring::WiredInverter;

/// Why a `.pvprj` could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    #[error("not a PV*SOL archive: {0}")]
    NotAnArchive(String),
    #[error("{0} is missing from the archive")]
    Missing(String),
    #[error("{file}: malformed XML ({message})")]
    Xml { file: String, message: String },
    #[error("{file}: unexpected content ({what})")]
    Format { file: String, what: String },
    #[error("weather file: {0}")]
    Weather(String),
}

impl ReadError {
    /// Stable key for the user interface's translations.
    #[must_use]
    pub fn key(&self) -> &'static str {
        match self {
            Self::NotAnArchive(_) => "errors.pvsol.notAnArchive",
            Self::Missing(_) => "errors.pvsol.missing",
            Self::Xml { .. } | Self::Format { .. } => "errors.pvsol.format",
            Self::Weather(_) => "errors.pvsol.weather",
        }
    }
}

/// Where the project is, as PV*SOL stores it.
#[derive(Debug, Clone, PartialEq)]
pub struct PvsolSite {
    /// Postal address typed in PV*SOL (may be empty).
    pub address: String,
    /// WGS84, east positive; `None` when the project has no 3-D data.
    pub lat_deg: Option<f64>,
    pub lon_deg: Option<f64>,
    /// Offset of the project's standard time from UTC [h] (PV*SOL `ZeitZone`, negative east;
    /// +1 when absent: Central European Time).
    pub utc_offset_h: i32,
}

/// Everything read from one `.pvprj`.
#[derive(Debug, Clone)]
pub struct PvsolFile {
    pub title: String,
    pub site: PvsolSite,
    pub config: PvsolConfig,
    pub db: PvsolDb,
    pub scene: Option<Scene>,
    pub wiring: Vec<WiredInverter>,
    /// Module area of every formation and half row (from the wiring file).
    pub form_areas: std::collections::HashMap<String, String>,
    /// Racks of the row formations (`Visu3D/MoSys.xml`).
    pub mounting_systems: Vec<geometry::MountingSystem>,
    /// Weather file name and content.
    pub weather: Option<(String, PvsolWeather)>,
    /// `None` when the project was never simulated.
    pub results: Option<PvsolResults>,
    /// PV*SOL's digital surface model of the map section, when the project has one.
    pub height_map: Option<heightmap::HeightMap>,
    /// Margin PV*SOL cuts around the project's buildings in the height map
    /// (`GelaendeModell/AddDist`, 1 m when absent) [m].
    pub cut_margin_m: f64,
    /// Far horizon profile (`Visu3D/BasisHorizont.txt`): `[azimuth°, elevation°]`, sorted.
    pub horizon: Option<Vec<[f64; 2]>>,
}

fn xml_entry(a: &Archive, name: &str) -> Result<Option<xml::Node>, ReadError> {
    a.get(name).map(|b| xml::parse(name, b)).transpose()
}

/// Reads a `.pvprj` from its bytes.
///
/// # Errors
/// [`ReadError`] when the archive or one of its required parts (`Project.xml`, `DB.json`)
/// cannot be read. Optional parts that are malformed are an error too: a half-read project
/// must not be imported silently.
pub fn read(bytes: &[u8]) -> Result<PvsolFile, ReadError> {
    read_archive(&Archive::from_bytes(bytes)?)
}

/// Reads an already opened archive.
///
/// # Errors
/// See [`read`].
pub fn read_archive(a: &Archive) -> Result<PvsolFile, ReadError> {
    let project =
        xml_entry(a, "Project.xml")?.ok_or_else(|| ReadError::Missing("Project.xml".into()))?;
    let config = config::parse(&project)?;
    let db = db::parse(
        a.get("DB.json")
            .ok_or_else(|| ReadError::Missing("DB.json".into()))?,
    )?;
    let scene = xml_entry(a, "Visu3D/GeometrischeDaten.xml")?
        .map(|d| geometry::parse(&d))
        .transpose()?;
    let wiring_doc = xml_entry(a, "Visu3D/Modul_Verschaltung.xml")?;
    let wiring = wiring_doc
        .as_ref()
        .map(wiring::parse)
        .transpose()?
        .unwrap_or_default();
    let form_areas = wiring_doc
        .as_ref()
        .map(wiring::form_areas)
        .unwrap_or_default();
    let mounting_systems = xml_entry(a, "Visu3D/MoSys.xml")?
        .as_ref()
        .map(geometry::mounting_systems)
        .unwrap_or_default();
    let results = xml_entry(a, "SimResults.xml")?
        .map(|d| results::parse(&d))
        .transpose()?
        .flatten();
    let height_map = a.get("HeightMap.bin").map(heightmap::parse).transpose()?;
    let weather = a
        .weather()
        .map(|(name, b)| weather::parse(b).map(|w| (name.to_owned(), w)))
        .transpose()?;

    // Site: the 3-D project data hold the coordinates in radians, longitude east negative.
    let proj = xml_entry(a, "Visu3D/ProjDaten.xml")?;
    let standort = proj.as_ref().and_then(|p| p.at("Proj_DatenAllg/Standort"));
    let coord = |k: &str| standort.and_then(|s| s.f64_at(k));
    let (lat, lon) = match (coord("BreitenGrad"), coord("LaengenGrad")) {
        (Some(la), Some(lo)) if la.abs() <= std::f64::consts::FRAC_PI_2 => {
            (Some(la.to_degrees()), Some(-lo.to_degrees()))
        }
        _ => (
            weather.as_ref().map(|(_, w)| w.lat_deg),
            weather.as_ref().map(|(_, w)| w.lon_deg),
        ),
    };
    let address = project
        .text_at("ProjektdatenV1/AnlagenStandort/Anschrift/Value")
        .map(config::collapse)
        .unwrap_or_default();
    let title = project
        .text_at("ProjektdatenV1/Projekttitel/Value")
        .map(config::collapse)
        .filter(|t| !t.is_empty())
        .or_else(|| {
            proj.as_ref()
                .and_then(|p| p.text_at("Proj_DatenAllg/Proj_Bezeichnung"))
                .map(config::collapse)
        })
        .unwrap_or_default();
    Ok(PvsolFile {
        title,
        site: PvsolSite {
            address,
            lat_deg: lat,
            lon_deg: lon,
            utc_offset_h: coord("ZeitZone").map_or(1, |z| -(z.round() as i32)),
        },
        config,
        db,
        scene,
        wiring,
        form_areas,
        mounting_systems,
        weather,
        results,
        height_map,
        horizon: a.get("Visu3D/BasisHorizont.txt").and_then(parse_horizon),
        cut_margin_m: proj
            .as_ref()
            .and_then(|p| p.f64_at("GelaendeModell/AddDist"))
            .filter(|m| (0.0..=10.0).contains(m))
            .unwrap_or(1.0),
    })
}

/// Parses PV*SOL's horizon profile: one `azimuth elevation` pair per line (compass degrees,
/// north = 0). `None` when no valid line is found.
fn parse_horizon(bytes: &[u8]) -> Option<Vec<[f64; 2]>> {
    let text = String::from_utf8_lossy(bytes);
    let mut points: Vec<[f64; 2]> = text
        .lines()
        .filter_map(|l| {
            let mut it = l
                .split_whitespace()
                .map(|x| x.replace(',', ".").parse::<f64>());
            let (az, el) = (it.next()?.ok()?, it.next()?.ok()?);
            (az.is_finite() && el.is_finite() && (-10.0..=90.0).contains(&el))
                .then_some([az.rem_euclid(360.0), el])
        })
        .collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    points.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9);
    (!points.is_empty()).then_some(points)
}

#[cfg(test)]
mod horizon_tests {
    #[test]
    fn reads_the_profile() {
        let h = super::parse_horizon(b"0 9.5\r\n8 11.8\n\n360 9.5\nbad line\n").unwrap();
        assert_eq!(h, vec![[0.0, 9.5], [8.0, 11.8]]);
        assert!(super::parse_horizon(b"").is_none());
    }
}
