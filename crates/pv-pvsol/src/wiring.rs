//! `Visu3D/Modul_Verschaltung.xml`: which module of the 3-D scene is on which string, MPP
//! tracker and inverter.
//!
//! `Verschaltung / TeilAnlage / WechselRichter / MPP_Tracker / Strang / Modul`: an inverter
//! names its PV*SOL database record in `AllgDaten/GUID_XML`, a string its module area, a
//! module its scene object (`Zugehoerigkeit/GUID_Modul`), in wiring order.

use crate::guid;
use crate::xml::Node;
use crate::ReadError;

/// A string: modules in wiring order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WiredString {
    /// Module area (normalised GUID).
    pub area: String,
    /// Scene module ids (normalised GUIDs).
    pub modules: Vec<String>,
}

/// One MPP tracker of an inverter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WiredTracker {
    /// 0-based tracker index (from PV*SOL's 1-based `Nummer`).
    pub index: u32,
    pub strings: Vec<WiredString>,
}

/// One inverter of the wiring.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WiredInverter {
    /// PV*SOL label (« OND 1: Huawei Technologies | SUN2000-30K-MC0 »).
    pub label: String,
    /// Database record (normalised GUID, key into `DB.json`).
    pub component: String,
    pub trackers: Vec<WiredTracker>,
}

/// Reads the wiring file.
///
/// # Errors
/// [`ReadError::Format`] when the root element is not the wiring.
pub fn parse(doc: &Node) -> Result<Vec<WiredInverter>, ReadError> {
    if doc.name != "Modulverschaltung_PVSol" {
        return Err(ReadError::Format {
            file: "Visu3D/Modul_Verschaltung.xml".into(),
            what: "Modulverschaltung_PVSol".into(),
        });
    }
    let mut out = Vec::new();
    for inv in doc.descendants("WechselRichter") {
        let mut trackers = Vec::new();
        for (k, t) in inv.children("MPP_Tracker").enumerate() {
            let index = t
                .f64_at("AllgDaten/Nummer")
                .filter(|n| *n >= 1.0)
                .map_or(u32::try_from(k).unwrap_or(0), |n| n.round() as u32 - 1);
            // A string spanning several module areas (« poly-string ») holds its modules in
            // `SubStrang` parts, in wiring order: take every module below the string.
            let strings = t
                .children("Strang")
                .map(|s| WiredString {
                    area: guid::normalise(
                        s.child("SubStrang")
                            .unwrap_or(s)
                            .text_at("AllgDaten/GUID_XML")
                            .unwrap_or_default(),
                    ),
                    modules: s
                        .descendants("Modul")
                        .into_iter()
                        .filter_map(|m| m.text_at("Zugehoerigkeit/GUID_Modul"))
                        .map(guid::normalise)
                        .collect(),
                })
                .filter(|s| !s.modules.is_empty())
                .collect::<Vec<_>>();
            trackers.push(WiredTracker { index, strings });
        }
        out.push(WiredInverter {
            label: inv
                .text_at("AllgDaten/Bezeichnung_XML")
                .unwrap_or_default()
                .to_owned(),
            component: guid::normalise(inv.text_at("AllgDaten/GUID_XML").unwrap_or_default()),
            trackers,
        });
    }
    Ok(out)
}

/// Module area of every formation (and of every half row of a row formation): the
/// `ReferenzModul` entries list them under their area (`FormsList`). Keys and values are
/// normalised GUIDs.
#[must_use]
pub fn form_areas(doc: &Node) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    for r in doc.children("ReferenzModul") {
        let area = guid::normalise(r.text_at("GUID_XML").unwrap_or_default());
        for f in r.children("FormsList") {
            out.insert(guid::normalise(&f.text), area.clone());
        }
    }
    out
}
