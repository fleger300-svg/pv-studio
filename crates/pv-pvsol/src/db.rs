//! `DB.json`: the PV*SOL database records of the components the project uses (modules,
//! inverters). They become PV Studio components frozen in the imported project, so that the
//! project computes with the very data PV*SOL used.
//!
//! Units in the records: dimensions [mm], temperature coefficients of Voc / Isc [mV/K, mA/K],
//! of Pmax [1/K]; inverter efficiencies `E5…E100` as fractions at 5…100 % load.

use pv_model::component::{
    Component, ComponentSource, Descriptions, EquipmentDetails, InverterAc, InverterDc,
    InverterKind, InverterSpec, ModuleDetails, ModuleElectrical, ModuleSpec, Tracker,
};
use pv_model::{ComponentId, ComponentSpec};
use serde_json::Value;

use crate::config::collapse;
use crate::guid;
use crate::ReadError;

/// Components found in `DB.json`, keyed by their normalised PV*SOL id.
#[derive(Debug, Clone, Default)]
pub struct PvsolDb {
    pub modules: Vec<(String, Component)>,
    pub inverters: Vec<(String, Component)>,
}

impl PvsolDb {
    #[must_use]
    pub fn module(&self, id: &str) -> Option<&Component> {
        self.modules.iter().find(|(k, _)| k == id).map(|(_, c)| c)
    }

    #[must_use]
    pub fn inverter(&self, id: &str) -> Option<&Component> {
        self.inverters.iter().find(|(k, _)| k == id).map(|(_, c)| c)
    }
}

fn num(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64).filter(|x| x.is_finite())
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(collapse)
        .unwrap_or_default()
}

fn note() -> String {
    "Importé de PV*SOL (fiche de la base de données du projet)".to_owned()
}

fn component(v: &Value, spec: ComponentSpec) -> Option<(String, Component)> {
    let raw_id = v.get("Id").and_then(Value::as_str)?;
    let id = guid::uuid(raw_id)?;
    let manufacturer = v
        .get("Company")
        .map(|c| text(c, "Name"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "PV*SOL".to_owned());
    let model = text(v, "Name");
    let label = format!("{manufacturer} {model}");
    let c = Component {
        id: ComponentId(id),
        revision: u32::try_from(v.get("Version").and_then(Value::as_u64).unwrap_or(1))
            .unwrap_or(1)
            .max(1),
        manufacturer,
        model,
        article_no: String::new(),
        label,
        descriptions: Descriptions::default(),
        source: ComponentSource::Import,
        verified: false,
        datasheet: None,
        spec,
    };
    c.validate().ok()?;
    Some((guid::normalise(raw_id), c))
}

fn module(v: &Value) -> Option<(String, Component)> {
    let w = num(v, "Width")?;
    let h = num(v, "Height")?;
    let voc = num(v, "OpenCircuitVoltage")?;
    let isc = num(v, "ShortCircuitCurrent")?;
    let bifaciality = num(v, "BifacialityFactor").filter(|b| *b > 0.0);
    let electrical = ModuleElectrical {
        pmax_w: num(v, "SpecifiedOutputPower")?,
        voc_v: voc,
        isc_a: isc,
        vmpp_v: num(v, "MPP_Voltage")?,
        impp_a: num(v, "MPP_Current")?,
        temp_coeff_pmax_pct_per_k: num(v, "OutputpowerCoefficient")? * 100.0,
        temp_coeff_voc_pct_per_k: num(v, "VoltageCoefficient")? / (voc * 10.0),
        temp_coeff_isc_pct_per_k: num(v, "CurrentCoefficient")? / (isc * 10.0),
        max_system_voltage_v: num(v, "MaxSysVoltage").unwrap_or(1000.0),
        bifaciality,
    };
    let spec = ModuleSpec {
        length_m: w.max(h) / 1000.0,
        width_m: w.min(h) / 1000.0,
        thickness_m: num(v, "DepthOfPanel").unwrap_or(30.0) / 1000.0,
        mass_kg: num(v, "Weight").unwrap_or(0.0),
        electrical: Some(electrical),
        details: Box::new(ModuleDetails {
            cell_count: num(v, "CellCount").map(|c| c.round() as u32),
            bifacial: Some(bifaciality.is_some()),
            notes: vec![note()],
            ..ModuleDetails::default()
        }),
    };
    component(v, ComponentSpec::Module(spec))
}

/// European efficiency from PV*SOL's part-load points.
fn euro_efficiency(v: &Value) -> Option<f64> {
    let weights = [
        ("E5", 0.03),
        ("E10", 0.06),
        ("E20", 0.13),
        ("E30", 0.10),
        ("E50", 0.48),
        ("E100", 0.20),
    ];
    let mut sum = 0.0;
    for (k, w) in weights {
        sum += w * num(v, k).filter(|e| *e > 0.0)?;
    }
    Some(sum * 100.0)
}

fn inverter(v: &Value) -> Option<(String, Component)> {
    let ac_w = num(v, "AC_PowerRating")?;
    let mut trackers = Vec::new();
    let (mut u_min, mut u_max) = (f64::INFINITY, 0.0f64);
    for g in 1..=3 {
        let n = num(v, &format!("MPPT{g}_NrOf")).unwrap_or(0.0).round() as u32;
        if n == 0 {
            continue;
        }
        let pos = |k: &str| num(v, &format!("MPPT{g}_{k}")).filter(|x| *x > 0.0);
        if let (Some(lo), Some(hi)) = (pos("U_MPP_Min"), pos("U_MPP_Max")) {
            u_min = u_min.min(lo);
            u_max = u_max.max(hi);
        }
        for _ in 0..n {
            trackers.push(Tracker {
                inputs: 1,
                max_current_a: pos("MaxCurrent"),
                max_short_circuit_current_a: pos("MaxShortCircuitCurrent"),
                max_power_w: pos("MaxPower"),
            });
        }
    }
    if trackers.is_empty() || !u_min.is_finite() {
        return None;
    }
    // String inputs: PV*SOL gives the total; spread them over the trackers.
    let inputs = num(v, "DC_InputCount").unwrap_or(0.0).round() as u32;
    let n = u32::try_from(trackers.len()).unwrap_or(1);
    for (i, t) in trackers.iter_mut().enumerate() {
        let i = u32::try_from(i).unwrap_or(0);
        t.inputs = (inputs / n + u32::from(i < inputs % n)).max(1);
    }
    let max_eff = ["E5", "E10", "E20", "E30", "E50", "E75", "E100"]
        .iter()
        .filter_map(|k| num(v, k))
        .fold(0.0f64, f64::max);
    let spec = InverterSpec {
        kind: InverterKind::String,
        ac: InverterAc {
            phases: u8::try_from(num(v, "NumberOfPhases").unwrap_or(3.0).round() as u32)
                .unwrap_or(3)
                .clamp(1, 3),
            nominal_power_w: ac_w,
            max_apparent_power_va: None,
            max_active_power_w: num(v, "MaxAC_Power").filter(|p| *p >= ac_w),
            nominal_voltage_v: num(v, "VoltageAC"),
            max_current_a: None,
        },
        dc: InverterDc {
            max_voltage_v: num(v, "MaxInputVoltageDC")?.max(u_max),
            start_voltage_v: None,
            min_voltage_v: None,
            nominal_voltage_v: num(v, "NominalVoltageDC"),
            mppt_min_v: u_min,
            mppt_max_v: u_max,
            full_power_mppt_min_v: None,
            full_power_mppt_max_v: None,
            max_power_w: num(v, "MaxDC_Power"),
            max_power_basis: Some("PV*SOL MaxDC_Power".to_owned()),
            max_current_total_a: num(v, "MaxCurrentDC"),
            max_short_circuit_current_total_a: num(v, "MaxShortCircuitCurrentDC"),
            trackers,
        },
        battery: None,
        backup: None,
        optimizer_required: false,
        compatible_optimizers: Vec::new(),
        max_efficiency_pct: (max_eff > 0.0).then_some(max_eff * 100.0),
        euro_efficiency_pct: euro_efficiency(v),
        afci: None,
        spd_dc: None,
        spd_ac: None,
        details: EquipmentDetails {
            notes: vec![note()],
            ..EquipmentDetails::default()
        },
    };
    component(v, ComponentSpec::Inverter(Box::new(spec)))
}

/// Reads `DB.json`. Records that cannot be converted are skipped (the import reports the
/// areas or inverters left without a component).
///
/// # Errors
/// [`ReadError::Format`] when the file is not JSON.
pub fn parse(bytes: &[u8]) -> Result<PvsolDb, ReadError> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let doc: Value = serde_json::from_slice(bytes).map_err(|e| ReadError::Format {
        file: "DB.json".into(),
        what: e.to_string(),
    })?;
    let list = |key: &str| {
        doc.get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    let mut db = PvsolDb::default();
    for key in ["PVModules", "PVModulesFrom3D"] {
        for v in list(key) {
            if let Some((id, c)) = module(&v) {
                if db.module(&id).is_none() {
                    db.modules.push((id, c));
                }
            }
        }
    }
    for key in ["PVInverters", "PVInvertersFrom3D"] {
        for v in list(key) {
            if let Some((id, c)) = inverter(&v) {
                if db.inverter(&id).is_none() {
                    db.inverters.push((id, c));
                }
            }
        }
    }
    Ok(db)
}
