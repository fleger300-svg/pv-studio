//! Reads PV*SOL premium projects (`.pvprj`) — configuration, 3-D scene, wiring, component
//! records, weather and PV*SOL's own results — and imports them as PV Studio projects
//! ([`import`]), so that existing projects can be migrated and PV Studio's computations
//! compared with PV*SOL's on the same input.
//!
//! The format is undocumented; what is decoded was established on the 224 projects of the
//! Younergy corpus (PV*SOL premium 2024 R6 to 2026 R8), see each module's notes.

pub mod archive;
pub mod config;
pub mod db;
pub mod geometry;
pub mod guid;
pub mod import;
pub mod results;
pub mod weather;
pub mod wiring;
pub mod xml;

pub use archive::Archive;
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
            Self::NotAnArchive(_) => "pvsol.notAnArchive",
            Self::Missing(_) => "pvsol.missing",
            Self::Xml { .. } | Self::Format { .. } => "pvsol.format",
            Self::Weather(_) => "pvsol.weather",
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
    /// Weather file name and content.
    pub weather: Option<(String, PvsolWeather)>,
    /// `None` when the project was never simulated.
    pub results: Option<PvsolResults>,
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
    let wiring = xml_entry(a, "Visu3D/Modul_Verschaltung.xml")?
        .map(|d| wiring::parse(&d))
        .transpose()?
        .unwrap_or_default();
    let results = xml_entry(a, "SimResults.xml")?
        .map(|d| results::parse(&d))
        .transpose()?
        .flatten();
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
        },
        config,
        db,
        scene,
        wiring,
        weather,
        results,
    })
}
