//! `Project.xml`: the PV*SOL project configuration — module areas (orientation, module,
//! count, losses), inverters with their MPP trackers and strings, simulation settings.
//!
//! Numbers sit in `<Name><Value>…</Value></Name>` wrappers; GUIDs are lower case here and
//! upper case in braces in the 3-D files (see [`crate::guid`]).

use crate::guid;
use crate::xml::Node;
use crate::ReadError;

/// Losses PV*SOL applies to a module area (fractions).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AreaLosses {
    pub albedo: f64,
    pub soiling: f64,
    pub diodes: f64,
    pub mismatch: f64,
}

/// A module area (« Modulfeld »): one orientation, one module type.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigArea {
    pub id: String,
    pub name: String,
    pub tilt_deg: f64,
    /// Compass azimuth the modules face (180 = south), PV*SOL convention.
    pub azimuth_deg: f64,
    pub module_count: u32,
    /// PV*SOL database id of the module (key into `DB.json`).
    pub module_id: String,
    pub module_manufacturer: String,
    pub module_name: String,
    /// PV*SOL mounting situation code (0: roof parallel, 3: flat roof, …).
    pub installation: u32,
    pub losses: AreaLosses,
}

/// One series connection on an MPP tracker: `strings` strings of `in_series` modules of
/// module area `area`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigStrings {
    pub area: String,
    pub in_series: u32,
    pub strings: u32,
}

/// One inverter type of the configuration, present `count` times.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigInverter {
    /// PV*SOL database id (key into `DB.json`).
    pub id: String,
    pub manufacturer: String,
    pub name: String,
    pub count: u32,
    /// Per MPP tracker, the strings connected to it.
    pub trackers: Vec<Vec<ConfigStrings>>,
}

/// The parts of `Project.xml` PV Studio uses.
#[derive(Debug, Clone, PartialEq)]
pub struct PvsolConfig {
    /// Last PV*SOL release that saved the project (« PV*SOL premium 2026 (R8) »).
    pub program: String,
    pub system_type: String,
    pub transposition_model: String,
    pub diffuse_model: String,
    pub spectral_loss: f64,
    pub areas: Vec<ConfigArea>,
    pub inverters: Vec<ConfigInverter>,
}

impl PvsolConfig {
    /// Installed modules.
    #[must_use]
    pub fn module_count(&self) -> u32 {
        self.areas.iter().map(|a| a.module_count).sum()
    }
}

fn losses(n: &Node) -> AreaLosses {
    AreaLosses {
        albedo: n.value_at("AlbedoJahreswert").unwrap_or(0.2),
        soiling: n.value_at("VerschmutzungJahreswert").unwrap_or(0.0),
        diodes: n.value_at("VerlusteInDioden").unwrap_or(0.0),
        mismatch: n.value_at("MismatchHersteller").unwrap_or(0.0),
    }
}

fn u32_value(n: &Node, path: &str) -> u32 {
    n.value_at(path)
        .or_else(|| n.f64_at(path))
        .filter(|x| *x >= 0.0)
        .map_or(0, |x| x.round() as u32)
}

/// Reads `Project.xml`.
///
/// # Errors
/// [`ReadError::Format`] when the root or the module areas are missing.
pub fn parse(doc: &Node) -> Result<PvsolConfig, ReadError> {
    let bad = |what: &str| ReadError::Format {
        file: "Project.xml".into(),
        what: what.into(),
    };
    if doc.name != "PVData" {
        return Err(bad("PVData"));
    }
    let pv = doc.child("PVModule").ok_or_else(|| bad("PVModule"))?;
    // Global losses apply unless the areas define their own (`PVModule2`).
    let global = doc.child("PVModule2");
    let per_area = global
        .and_then(|g| g.text_at("VerlusteEinzelnDefinieren"))
        .is_some_and(|t| t.eq_ignore_ascii_case("true"));
    let global_losses = global
        .and_then(|g| g.child("VerlusteSimulation"))
        .map(losses);

    let mut areas = Vec::new();
    for a in pv.children("Modulfelder") {
        let own = a
            .at("Verluste/VerlusteSimulation")
            .map(losses)
            .unwrap_or_default();
        areas.push(ConfigArea {
            id: guid::normalise(a.text_at("ModulfeldID").unwrap_or_default()),
            name: a.text_at("Modulname").unwrap_or_default().to_owned(),
            tilt_deg: a.value_at("Modulneigung").unwrap_or(0.0),
            azimuth_deg: a.value_at("Modulausrichtung").unwrap_or(180.0),
            module_count: u32_value(a, "Modulanzahl"),
            module_id: guid::normalise(a.text_at("ModulID").unwrap_or_default()),
            module_manufacturer: a.text_at("ModulHersteller").unwrap_or_default().to_owned(),
            module_name: collapse(a.text_at("ModulName").unwrap_or_default()),
            installation: u32_value(a, "Einbausituation"),
            losses: if per_area {
                own
            } else {
                global_losses.unwrap_or(own)
            },
        });
    }
    let mut inverters = Vec::new();
    if let Some(v) = doc.at("Wechselrichter/Verschaltung") {
        for w in v.children("WRObj") {
            let trackers = w
                .children("MPPListe")
                .map(|mpp| {
                    mpp.descendants("Straenge")
                        .into_iter()
                        .map(|s| ConfigStrings {
                            area: guid::normalise(s.text_at("ModulfeldId").unwrap_or_default()),
                            in_series: u32_value(s, "AnzahlInReihe"),
                            strings: u32_value(s, "AnzahlStrings"),
                        })
                        .filter(|s| s.in_series > 0 && s.strings > 0)
                        .collect()
                })
                .collect();
            inverters.push(ConfigInverter {
                id: guid::normalise(w.text_at("InverterID").unwrap_or_default()),
                manufacturer: w
                    .text_at("InverterHersteller")
                    .unwrap_or_default()
                    .to_owned(),
                name: w.text_at("InverterName").unwrap_or_default().to_owned(),
                count: w
                    .f64_at("Haeufigkeit")
                    .map_or(1, |x| x.max(1.0).round() as u32),
                trackers,
            });
        }
    }

    let meteo = doc.child("Meteo");
    let meteo_text = |p: &str| {
        meteo
            .and_then(|m| m.at(p))
            .and_then(|n| n.text_at("Value"))
            .unwrap_or_default()
            .to_owned()
    };
    Ok(PvsolConfig {
        program: doc
            .at("ProgramsUsedToEditProject")
            .and_then(|p| p.text_at("Program"))
            .unwrap_or_default()
            .to_owned(),
        system_type: doc
            .text_at("AnlagenTyp/SystemType")
            .unwrap_or_default()
            .to_owned(),
        transposition_model: meteo_text("TranspositionModelType"),
        diffuse_model: meteo_text("DiffuseModelType"),
        spectral_loss: meteo
            .and_then(|m| m.value_at("VerlusteDurchSpektrumsAbweichung"))
            .unwrap_or(0.0),
        areas,
        inverters,
    })
}

/// PV*SOL pads names with runs of spaces (« LR7-54HVH-490M          2026 »).
#[must_use]
pub fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
