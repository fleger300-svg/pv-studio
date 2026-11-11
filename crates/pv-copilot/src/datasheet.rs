//! Datasheet extraction: request, response, conversion to an unverified component.

use pv_model::command::keys;
use pv_model::component::{
    ComponentSource, Descriptions, LengthRange, ModuleElectrical, ModuleSpec, PartKind, PartSpec,
};
use pv_model::{BlobId, CommandError, Component, ComponentId, ComponentSpec};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use specta::Type;

/// Model used for extraction.
pub const MODEL: &str = "claude-opus-5-5";
/// `anthropic-version` header.
pub const API_VERSION: &str = "2026-10-21";
/// `anthropic-beta` header enabling the server-side refusal fallback.
pub const BETA_FALLBACK: &str = "server-side-fallback-2026-07-01";
/// Largest PDF sent (the API limits a request to 32 MB, base64 included).
pub const MAX_PDF_BYTES: usize = 22 * 1024 * 1024;

const SYSTEM: &str = "You read datasheets of photovoltaic modules and mounting parts for \
PV Studio, a PV design tool used by installers. Your output fills a component library whose \
values feed structural and electrical calculations, so accuracy matters more than \
completeness.

Rules:
- Take every value exactly as printed in the datasheet. Never estimate, convert from another \
model or fill in typical values: use null when a value is not printed.
- Dimensions in millimetres, masses in kilograms, temperature coefficients in %/K (a value \
printed in %/°C is the same number).
- When the datasheet covers several variants (power classes, lengths, colours), extract the \
one matching the variant hint; without a hint, the first one listed. Say which variant you \
chose in a warning.
- `label` is the manufacturer and the model as a buyer would name it.
- Add a short warning for each value that was hard to read, ambiguous or printed for several \
variants at once. Write warnings in the user's language given in the request.";

/// Everything the shell needs to send the request.
#[derive(Debug, Clone, PartialEq)]
pub struct ApiCall {
    /// `(name, value)` headers besides `x-api-key` and the content type.
    pub headers: Vec<(&'static str, &'static str)>,
    pub body: Value,
}

fn nullable(t: &str) -> Value {
    json!({ "anyOf": [{ "type": t }, { "type": "null" }] })
}

/// JSON schema of the extraction (structured outputs subset: every object closed, no numeric
/// bounds — those are checked when converting).
#[must_use]
pub fn extraction_schema() -> Value {
    let num = || nullable("number");
    let module = json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "lengthMm", "widthMm", "thicknessMm", "massKg", "pmaxW", "vocV", "iscA", "vmppV",
            "imppA", "tempCoeffPmaxPctPerK", "tempCoeffVocPctPerK", "tempCoeffIscPctPerK",
            "maxSystemVoltageV", "bifaciality"
        ],
        "properties": {
            "lengthMm": num(), "widthMm": num(), "thicknessMm": num(), "massKg": num(),
            "pmaxW": num(), "vocV": num(), "iscA": num(), "vmppV": num(), "imppA": num(),
            "tempCoeffPmaxPctPerK": num(), "tempCoeffVocPctPerK": num(),
            "tempCoeffIscPctPerK": num(), "maxSystemVoltageV": num(),
            "bifaciality": { "description": "Rear/front power ratio (0..1); null for a monofacial module.", "anyOf": [{ "type": "number" }, { "type": "null" }] }
        }
    });
    let part = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["kind", "unitMassKg", "packQty", "stockLengthMm", "frameHeightMinMm", "frameHeightMaxMm"],
        "properties": {
            "kind": { "type": "string", "enum": ["rail", "splice", "clamp", "fixation", "climber", "ballastBlock", "other"] },
            "unitMassKg": num(),
            "packQty": nullable("integer"),
            "stockLengthMm": { "description": "Bar length of a rail.", "anyOf": [{ "type": "number" }, { "type": "null" }] },
            "frameHeightMinMm": { "description": "Smallest module frame height a clamp fits.", "anyOf": [{ "type": "number" }, { "type": "null" }] },
            "frameHeightMaxMm": { "description": "Largest module frame height a clamp fits.", "anyOf": [{ "type": "number" }, { "type": "null" }] }
        }
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["family", "manufacturer", "model", "articleNo", "label", "module", "part", "warnings"],
        "properties": {
            "family": { "type": "string", "enum": ["module", "mountingPart"] },
            "manufacturer": { "type": "string" },
            "model": { "type": "string" },
            "articleNo": nullable("string"),
            "label": { "type": "string" },
            "module": { "description": "Filled for a PV module, else null.", "anyOf": [module, { "type": "null" }] },
            "part": { "description": "Filled for a mounting part, else null.", "anyOf": [part, { "type": "null" }] },
            "warnings": { "type": "array", "items": { "type": "string" } }
        }
    })
}

/// The Messages API request for `pdf_base64` (`file_name` titles the document; `hint` names
/// the variant to extract; `language` is the user's UI language, for the warnings).
#[must_use]
pub fn build_request(
    pdf_base64: &str,
    file_name: &str,
    hint: Option<&str>,
    language: &str,
) -> ApiCall {
    let hint = hint
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .map_or_else(|| "none".to_owned(), str::to_owned);
    let body = json!({
        "model": MODEL,
        "max_tokens": 16000,
        // Datasheet values feed statics: depth over speed.
        "output_config": {
            "effort": "high",
            "format": { "type": "json_schema", "schema": extraction_schema() }
        },
        "fallbacks": "default",
        "system": SYSTEM,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "document",
                    "title": file_name,
                    "source": { "type": "base64", "media_type": "application/pdf", "data": pdf_base64 }
                },
                {
                    "type": "text",
                    "text": format!("Extract the component described by this datasheet.\nVariant hint: {hint}\nUser language for warnings: {language}")
                }
            ]
        }]
    });
    ApiCall {
        headers: vec![
            ("anthropic-version", API_VERSION),
            ("anthropic-beta", BETA_FALLBACK),
        ],
        body,
    }
}

/// Electrical and mechanical values of a module, as printed (`None`: not printed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedModule {
    pub length_mm: Option<f64>,
    pub width_mm: Option<f64>,
    pub thickness_mm: Option<f64>,
    pub mass_kg: Option<f64>,
    pub pmax_w: Option<f64>,
    pub voc_v: Option<f64>,
    pub isc_a: Option<f64>,
    pub vmpp_v: Option<f64>,
    pub impp_a: Option<f64>,
    pub temp_coeff_pmax_pct_per_k: Option<f64>,
    pub temp_coeff_voc_pct_per_k: Option<f64>,
    pub temp_coeff_isc_pct_per_k: Option<f64>,
    pub max_system_voltage_v: Option<f64>,
    pub bifaciality: Option<f64>,
}

/// A mounting part, as printed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedPart {
    pub kind: String,
    pub unit_mass_kg: Option<f64>,
    pub pack_qty: Option<u32>,
    pub stock_length_mm: Option<f64>,
    pub frame_height_min_mm: Option<f64>,
    pub frame_height_max_mm: Option<f64>,
}

/// The model's answer (matches [`extraction_schema`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Extraction {
    pub family: String,
    pub manufacturer: String,
    pub model: String,
    pub article_no: Option<String>,
    pub label: String,
    pub module: Option<ExtractedModule>,
    pub part: Option<ExtractedPart>,
    pub warnings: Vec<String>,
}

fn invalid(detail: impl Into<String>) -> CommandError {
    CommandError::new(keys::COPILOT_INVALID).with_text("detail", detail)
}

/// Reads the HTTP response (`status`, body text).
///
/// # Errors
/// [`keys::COPILOT_HTTP`] (API error, with its message), [`keys::COPILOT_REFUSED`],
/// [`keys::COPILOT_TRUNCATED`], or [`keys::COPILOT_INVALID`] (no JSON answer).
pub fn parse_response(status: u16, body: &str) -> Result<Extraction, CommandError> {
    let v: Value = serde_json::from_str(body).map_err(|e| invalid(e.to_string()))?;
    if !(200..300).contains(&status) {
        let message = v["error"]["message"].as_str().unwrap_or("").to_owned();
        return Err(CommandError::new(keys::COPILOT_HTTP)
            .with_number("status", f64::from(status))
            .with_text("detail", message));
    }
    match v["stop_reason"].as_str() {
        Some("refusal") => {
            let category = v["stop_details"]["category"].as_str().unwrap_or("other");
            return Err(CommandError::new(keys::COPILOT_REFUSED).with_text("category", category));
        }
        Some("max_tokens") => return Err(CommandError::new(keys::COPILOT_TRUNCATED)),
        _ => {}
    }
    // The JSON answer is the last text block (thinking and fallback blocks are skipped).
    let text = v["content"]
        .as_array()
        .and_then(|blocks| {
            blocks
                .iter()
                .rev()
                .find(|b| b["type"] == "text")
                .and_then(|b| b["text"].as_str())
        })
        .ok_or_else(|| invalid("no text block"))?;
    serde_json::from_str(text).map_err(|e| invalid(e.to_string()))
}

fn mm(v: Option<f64>) -> Option<f64> {
    v.filter(|x| x.is_finite() && *x > 0.0).map(|x| x / 1000.0)
}

/// The unverified library component an extraction describes (`datasheet`: the stored PDF).
///
/// # Errors
/// [`keys::COPILOT_INCOMPLETE`] listing the values a component cannot do without (module
/// dimensions and mass), or [`keys::COPILOT_INVALID`] for a family without its data.
pub fn to_component(
    e: &Extraction,
    datasheet: Option<BlobId>,
) -> Result<(Component, Vec<String>), CommandError> {
    let mut warnings = e.warnings.clone();
    let spec = match e.family.as_str() {
        "module" => {
            let m = e
                .module
                .as_ref()
                .ok_or_else(|| invalid("module data missing"))?;
            let (l, w, t, mass) = (
                mm(m.length_mm),
                mm(m.width_mm),
                mm(m.thickness_mm),
                m.mass_kg,
            );
            let missing: Vec<&str> = [
                ("lengthMm", l.is_none()),
                ("widthMm", w.is_none()),
                ("thicknessMm", t.is_none()),
                ("massKg", !mass.is_some_and(|x| x > 0.0)),
            ]
            .into_iter()
            .filter_map(|(f, m)| m.then_some(f))
            .collect();
            if !missing.is_empty() {
                return Err(CommandError::new(keys::COPILOT_INCOMPLETE)
                    .with_text("fields", missing.join(", ")));
            }
            let (l, w) = (l.unwrap_or_default(), w.unwrap_or_default());
            let electrical = match (
                m.pmax_w,
                m.voc_v,
                m.isc_a,
                m.vmpp_v,
                m.impp_a,
                m.temp_coeff_pmax_pct_per_k,
                m.temp_coeff_voc_pct_per_k,
                m.temp_coeff_isc_pct_per_k,
                m.max_system_voltage_v,
            ) {
                (
                    Some(p),
                    Some(voc),
                    Some(isc),
                    Some(vmpp),
                    Some(impp),
                    Some(cp),
                    Some(cv),
                    Some(ci),
                    Some(vmax),
                ) => Some(ModuleElectrical {
                    pmax_w: p,
                    voc_v: voc,
                    isc_a: isc,
                    vmpp_v: vmpp,
                    impp_a: impp,
                    temp_coeff_pmax_pct_per_k: cp,
                    temp_coeff_voc_pct_per_k: cv,
                    temp_coeff_isc_pct_per_k: ci,
                    max_system_voltage_v: vmax,
                    bifaciality: m.bifaciality,
                }),
                _ => {
                    warnings.push("electrical data incomplete: left empty".to_owned());
                    None
                }
            };
            ComponentSpec::Module(ModuleSpec {
                // The long side is the length, whatever order the datasheet uses.
                length_m: l.max(w),
                width_m: l.min(w),
                thickness_m: t.unwrap_or_default(),
                mass_kg: mass.unwrap_or_default(),
                electrical,
            })
        }
        "mountingPart" => {
            let p = e
                .part
                .as_ref()
                .ok_or_else(|| invalid("part data missing"))?;
            let kind = match p.kind.as_str() {
                "splice" => PartKind::Splice,
                "climber" => PartKind::Climber,
                "ballastBlock" => PartKind::BallastBlock,
                "fixation" => PartKind::Fixation { capacity: None },
                "clamp" => match (mm(p.frame_height_min_mm), mm(p.frame_height_max_mm)) {
                    (Some(min_m), Some(max_m)) if min_m <= max_m => PartKind::Clamp {
                        frame_height: LengthRange { min_m, max_m },
                    },
                    _ => {
                        warnings
                            .push("clamp frame heights not printed: kind set to other".to_owned());
                        PartKind::Other
                    }
                },
                // A rail needs its cross-section values for the statics, which datasheets
                // rarely print: never invented, the user completes them.
                "rail" => {
                    warnings.push(
                        "rail: cross-section values must be entered: kind set to other".to_owned(),
                    );
                    PartKind::Other
                }
                _ => PartKind::Other,
            };
            ComponentSpec::MountingPart(PartSpec {
                unit_mass_kg: p.unit_mass_kg.filter(|m| *m >= 0.0).unwrap_or(0.0),
                pack_qty: p.pack_qty.filter(|q| *q >= 1).unwrap_or(1),
                kind,
            })
        }
        other => return Err(invalid(format!("unknown family {other}"))),
    };
    let label = if e.label.trim().is_empty() {
        format!("{} {}", e.manufacturer, e.model)
    } else {
        e.label.trim().to_owned()
    };
    let component = Component {
        id: ComponentId::new(),
        revision: 1,
        manufacturer: e.manufacturer.trim().to_owned(),
        model: e.model.trim().to_owned(),
        article_no: e.article_no.clone().unwrap_or_default(),
        label,
        descriptions: Descriptions::default(),
        source: ComponentSource::AiExtracted,
        verified: false,
        datasheet,
        spec,
    };
    component.validate()?;
    Ok((component, warnings))
}

/// Outcome of a key check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum KeyCheck {
    /// The API accepted the key.
    Valid,
    /// The API refused it (invalid, revoked, or without permission).
    Invalid,
    /// The API answered something else (rate limit, overload…): the key may be fine.
    Inconclusive,
}

/// Headers of the key check: `GET /v1/models?limit=1` (lists models, costs nothing). The
/// shell adds `x-api-key`.
#[must_use]
pub fn key_check_request() -> (&'static str, Vec<(&'static str, &'static str)>) {
    (
        "/v1/models?limit=1",
        vec![("anthropic-version", API_VERSION)],
    )
}

/// Reads the key check's HTTP status.
#[must_use]
pub fn key_check_outcome(status: u16) -> KeyCheck {
    match status {
        200..=299 => KeyCheck::Valid,
        401 | 403 => KeyCheck::Invalid,
        _ => KeyCheck::Inconclusive,
    }
}
