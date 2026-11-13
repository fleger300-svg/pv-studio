//! DC design of a project: string voltages at the design temperatures, checks of every
//! string, tracker (MPPT) and inverter against the inverter datasheet, DC/AC ratio, the string
//! plan table (`Voc × kT`) and the automatic stringing ([`auto`]).
//!
//! Temperature model (as PV*SOL and the usual Swiss practice):
//! - `Voc(T) = Voc_STC · (1 + β_Voc · (T − 25 °C))`, checked at the lowest cell temperature
//!   against the inverter's maximum DC voltage;
//! - `Vmpp(T) = Vmpp_STC · (1 + β_Vmpp · (T − 25 °C))` with `β_Vmpp ≈ γ_Pmax − α_Isc` (from
//!   `P = V·I`), checked at the highest cell temperature against the lower MPP limit and at
//!   the lowest against the upper one;
//! - currents at STC (`Isc`, `Impp`), summed over the parallel strings of a tracker.
//!
//! Every finding is an i18n key with parameters (`wiring.issues.*`), never prose.

pub mod auto;

use std::collections::{BTreeMap, HashMap};

use pv_model::component::{InverterSpec, ModuleElectrical};
use pv_model::{
    ComponentSpec, ErrorParam, InverterId, ModuleId, ModuleRef, ModuleState, Project, StringId,
    WiringSettings,
};
use serde::Serialize;
use specta::Type;

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A finding: i18n key (`wiring.issues.<name>`) and its parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Issue {
    pub severity: Severity,
    pub key: String,
    pub params: BTreeMap<String, ErrorParam>,
}

impl Issue {
    fn new(severity: Severity, name: &str) -> Self {
        Self {
            severity,
            key: format!("wiring.issues.{name}"),
            params: BTreeMap::new(),
        }
    }
    fn num(mut self, name: &str, v: f64) -> Self {
        self.params
            .insert(name.into(), ErrorParam::Number(round(v, 2)));
        self
    }
    fn text(mut self, name: &str, v: impl Into<String>) -> Self {
        self.params.insert(name.into(), ErrorParam::Text(v.into()));
        self
    }
}

fn round(v: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (v * f).round() / f
}

/// Orientation class of a module: azimuth and tilt rounded to the degree (strings of one
/// tracker should not mix them).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Orientation {
    pub azimuth_deg: i32,
    pub tilt_deg: i32,
}

/// What the checks need to know of one module position.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleSite {
    pub id: ModuleId,
    pub active: bool,
    pub orientation: Orientation,
    /// Electrical data of the array's module (`None`: generic module, not checkable).
    pub electrical: Option<ModuleElectrical>,
    /// Roof position, for the stringing order.
    pub u: f64,
    pub v: f64,
}

/// Every module position of the project.
#[must_use]
pub fn module_sites(project: &Project) -> HashMap<ModuleRef, ModuleSite> {
    let mut out = HashMap::with_capacity(project.modules.len());
    for a in &project.arrays {
        let face = project.roof_faces.iter().find(|f| f.id == a.roof_face_id);
        let (azimuth, tilt) = face.map_or((180.0, 0.0), |f| {
            (
                f.azimuth_deg + a.azimuth_offset_deg,
                if f.tilt_deg.abs() < 1e-9 {
                    a.tilt_on_roof_deg
                } else {
                    f.tilt_deg + a.tilt_on_roof_deg
                },
            )
        });
        let orientation = Orientation {
            azimuth_deg: (azimuth.rem_euclid(360.0)).round() as i32 % 360,
            tilt_deg: tilt.round() as i32,
        };
        let electrical =
            a.module
                .and_then(|id| project.component(id))
                .and_then(|s| match &s.component.spec {
                    ComponentSpec::Module(m) => m.electrical,
                    _ => None,
                });
        for m in project.modules.iter().filter(|m| m.array_id == a.id) {
            out.insert(
                ModuleRef {
                    array: a.id,
                    row: m.row,
                    col: m.col,
                },
                ModuleSite {
                    id: m.id,
                    active: m.state == ModuleState::Active,
                    orientation,
                    electrical,
                    u: m.u,
                    v: m.v,
                },
            );
        }
    }
    out
}

/// Voltages of one module at the design temperatures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModuleVoltages {
    pub voc_stc: f64,
    pub voc_cold: f64,
    pub vmpp_hot: f64,
    pub vmpp_cold: f64,
}

#[must_use]
pub fn module_voltages(e: &ModuleElectrical, s: &WiringSettings) -> ModuleVoltages {
    let beta_voc = e.temp_coeff_voc_pct_per_k / 100.0;
    let beta_vmpp = (e.temp_coeff_pmax_pct_per_k - e.temp_coeff_isc_pct_per_k) / 100.0;
    ModuleVoltages {
        voc_stc: e.voc_v,
        voc_cold: e.voc_v * (1.0 + beta_voc * (s.min_cell_temp_c - 25.0)),
        vmpp_hot: e.vmpp_v * (1.0 + beta_vmpp * (s.max_cell_temp_c - 25.0)),
        vmpp_cold: e.vmpp_v * (1.0 + beta_vmpp * (s.min_cell_temp_c - 25.0)),
    }
}

/// Shortest and longest string of `e` modules an inverter accepts (`None` when no length
/// fits): longest by the maximum DC voltage (and the upper MPP limit at cold), shortest by
/// the lower MPP limit at the highest temperature.
#[must_use]
pub fn string_limits(
    e: &ModuleElectrical,
    inv: &InverterSpec,
    s: &WiringSettings,
) -> Option<(u32, u32)> {
    let v = module_voltages(e, s);
    let max_by_voc = (inv.dc.max_voltage_v / v.voc_cold + 1e-9).floor();
    let max_by_mpp = (inv.dc.mppt_max_v / v.vmpp_cold + 1e-9).floor();
    let n_max = max_by_voc.min(max_by_mpp);
    let n_min = (inv.dc.mppt_min_v / v.vmpp_hot - 1e-9).ceil().max(1.0);
    (n_min <= n_max && n_max >= 1.0).then_some((n_min as u32, n_max as u32))
}

/// One string of the report.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct StringReport {
    pub id: StringId,
    pub modules: u32,
    /// Module ids in wiring order (for the 3D colouring); missing positions are skipped.
    pub module_ids: Vec<ModuleId>,
    pub power_wp: f64,
    /// `n × Voc_STC` and `n × Voc_STC × kT` (string plan table).
    pub voc_stc_v: f64,
    pub voc_plan_v: f64,
    pub voc_cold_v: f64,
    pub vmpp_hot_v: f64,
    pub vmpp_cold_v: f64,
    pub isc_a: f64,
    pub impp_a: f64,
    pub issues: Vec<Issue>,
}

/// One tracker (MPPT) of an inverter.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrackerReport {
    pub index: u32,
    pub strings: Vec<StringReport>,
    pub power_wp: f64,
    pub isc_a: f64,
    pub impp_a: f64,
    /// Accepted string lengths for the module of its strings.
    #[specta(optional)]
    pub limits: Option<(u32, u32)>,
    pub issues: Vec<Issue>,
}

/// One inverter.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct InverterReport {
    pub id: InverterId,
    pub name: String,
    pub model: String,
    pub ac_power_w: f64,
    pub dc_power_wp: f64,
    /// DC Wp / AC W.
    pub dc_ac_ratio: f64,
    pub trackers: Vec<TrackerReport>,
    pub issues: Vec<Issue>,
}

/// The project's DC design.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct WiringReport {
    pub inverters: Vec<InverterReport>,
    pub dc_power_wp: f64,
    pub ac_power_w: f64,
    pub active_modules: u32,
    pub unassigned_modules: u32,
    /// Worst severity over everything (`None`: no finding).
    #[specta(optional)]
    pub worst: Option<Severity>,
    pub issues: Vec<Issue>,
}

fn inverter_spec(project: &Project, id: pv_model::ComponentId) -> Option<(&str, &InverterSpec)> {
    project.component(id).and_then(|s| match &s.component.spec {
        ComponentSpec::Inverter(i) => Some((s.component.label.as_str(), i.as_ref())),
        _ => None,
    })
}

/// Checks the wiring of `project`.
#[must_use]
pub fn check(project: &Project) -> WiringReport {
    let w = &project.wiring;
    let s = &w.settings;
    let sites = module_sites(project);
    let mut assigned = std::collections::HashSet::new();
    let mut inverters = Vec::with_capacity(w.inverters.len());
    for unit in &w.inverters {
        let Some((model, spec)) = inverter_spec(project, unit.component) else {
            continue;
        };
        let mut trackers = Vec::with_capacity(spec.dc.trackers.len());
        for (ti, tracker) in spec.dc.trackers.iter().enumerate() {
            let ti = ti as u32;
            let mut strings = Vec::new();
            for st in w
                .strings
                .iter()
                .filter(|x| x.inverter == unit.id && x.tracker == ti)
            {
                strings.push(check_string(st, &sites, s, &mut assigned));
            }
            trackers.push(check_tracker(ti, tracker, spec, strings, &sites, w, s));
        }
        let dc: f64 = trackers.iter().map(|t| t.power_wp).sum();
        let ac = spec.ac.nominal_power_w;
        let ratio = if ac > 0.0 { dc / ac } else { 0.0 };
        let mut issues = Vec::new();
        if dc > 0.0 && (ratio < s.dc_ac_min || ratio > s.dc_ac_max) {
            issues.push(
                Issue::new(Severity::Warning, "dcAcRatio")
                    .num("ratio", ratio * 100.0)
                    .num("min", s.dc_ac_min * 100.0)
                    .num("max", s.dc_ac_max * 100.0),
            );
        }
        if let Some(max) = spec.dc.max_power_w {
            if dc > max {
                issues.push(
                    Issue::new(Severity::Warning, "dcPower")
                        .num("power", dc)
                        .num("max", max),
                );
            }
        }
        let isc: f64 = trackers.iter().map(|t| t.isc_a).sum();
        if let Some(max) = spec.dc.max_short_circuit_current_total_a {
            if isc > max + 1e-9 {
                issues.push(
                    Issue::new(Severity::Error, "inverterIsc")
                        .num("current", isc)
                        .num("max", max),
                );
            }
        }
        inverters.push(InverterReport {
            id: unit.id,
            name: unit.name.clone(),
            model: model.to_owned(),
            ac_power_w: ac,
            dc_power_wp: dc,
            dc_ac_ratio: ratio,
            trackers,
            issues,
        });
    }
    let active: Vec<&ModuleSite> = sites.values().filter(|m| m.active).collect();
    let unassigned = active.iter().filter(|m| !assigned.contains(&m.id)).count() as u32;
    let mut issues = Vec::new();
    if unassigned > 0 && !w.inverters.is_empty() {
        issues
            .push(Issue::new(Severity::Warning, "unassigned").num("count", f64::from(unassigned)));
    }
    for unit in &w.inverters {
        if inverter_spec(project, unit.component).is_none() {
            issues.push(
                Issue::new(Severity::Error, "inverterMissing").text("name", unit.name.clone()),
            );
        }
    }
    let worst = inverters
        .iter()
        .flat_map(|i| {
            i.issues.iter().chain(i.trackers.iter().flat_map(|t| {
                t.issues
                    .iter()
                    .chain(t.strings.iter().flat_map(|s| s.issues.iter()))
            }))
        })
        .chain(&issues)
        .map(|i| i.severity)
        .max();
    WiringReport {
        dc_power_wp: inverters.iter().map(|i| i.dc_power_wp).sum(),
        ac_power_w: inverters.iter().map(|i| i.ac_power_w).sum(),
        inverters,
        active_modules: active.len() as u32,
        unassigned_modules: unassigned,
        worst,
        issues,
    }
}

fn check_string(
    st: &pv_model::PvString,
    sites: &HashMap<ModuleRef, ModuleSite>,
    s: &WiringSettings,
    assigned: &mut std::collections::HashSet<ModuleId>,
) -> StringReport {
    let mut issues = Vec::new();
    let present: Vec<&ModuleSite> = st
        .modules
        .iter()
        .filter_map(|r| sites.get(r))
        .filter(|m| m.active)
        .collect();
    let missing = st.modules.len() - present.len();
    if missing > 0 {
        issues.push(Issue::new(Severity::Error, "missingModules").num("count", missing as f64));
    }
    for m in &present {
        assigned.insert(m.id);
    }
    let n = present.len() as f64;
    let first = present.iter().find_map(|m| m.electrical.as_ref());
    let mixed_modules = present.iter().any(|m| m.electrical.as_ref() != first);
    if mixed_modules {
        issues.push(Issue::new(Severity::Error, "mixedModules"));
    }
    let orientations: std::collections::BTreeSet<Orientation> =
        present.iter().map(|m| m.orientation).collect();
    if orientations.len() > 1 {
        issues.push(Issue::new(Severity::Warning, "mixedOrientation"));
    }
    let mut r = StringReport {
        id: st.id,
        modules: present.len() as u32,
        module_ids: present.iter().map(|m| m.id).collect(),
        power_wp: 0.0,
        voc_stc_v: 0.0,
        voc_plan_v: 0.0,
        voc_cold_v: 0.0,
        vmpp_hot_v: 0.0,
        vmpp_cold_v: 0.0,
        isc_a: 0.0,
        impp_a: 0.0,
        issues,
    };
    match first {
        Some(e) => {
            let v = module_voltages(e, s);
            r.power_wp = n * e.pmax_w;
            r.voc_stc_v = n * v.voc_stc;
            r.voc_plan_v = r.voc_stc_v * s.voc_factor;
            r.voc_cold_v = n * v.voc_cold;
            r.vmpp_hot_v = n * v.vmpp_hot;
            r.vmpp_cold_v = n * v.vmpp_cold;
            r.isc_a = e.isc_a;
            r.impp_a = e.impp_a;
        }
        None if !present.is_empty() => {
            r.issues.push(Issue::new(Severity::Warning, "noModuleData"));
        }
        None => {}
    }
    r
}

fn check_tracker(
    index: u32,
    t: &pv_model::component::Tracker,
    inv: &InverterSpec,
    strings: Vec<StringReport>,
    sites: &HashMap<ModuleRef, ModuleSite>,
    w: &pv_model::Wiring,
    s: &WiringSettings,
) -> TrackerReport {
    let mut issues = Vec::new();
    let tag = |i: Issue| i.num("tracker", f64::from(index + 1));
    if strings.len() > t.inputs as usize {
        issues.push(tag(Issue::new(Severity::Error, "trackerInputs")
            .num("strings", strings.len() as f64)
            .num("inputs", f64::from(t.inputs))));
    }
    let lengths: std::collections::BTreeSet<u32> = strings.iter().map(|x| x.modules).collect();
    if lengths.len() > 1 {
        issues.push(tag(Issue::new(Severity::Error, "unequalStrings")));
    }
    // Strings of one tracker with different orientations.
    let orientations: std::collections::BTreeSet<Orientation> = w
        .strings
        .iter()
        .filter(|x| strings.iter().any(|r| r.id == x.id))
        .flat_map(|x| {
            x.modules
                .iter()
                .filter_map(|m| sites.get(m))
                .map(|m| m.orientation)
        })
        .collect();
    if orientations.len() > 1 {
        issues.push(tag(Issue::new(Severity::Warning, "trackerOrientation")));
    }
    let voc_cold = strings.iter().map(|x| x.voc_cold_v).fold(0.0, f64::max);
    let vmpp_hot = strings
        .iter()
        .filter(|x| x.modules > 0)
        .map(|x| x.vmpp_hot_v)
        .fold(f64::INFINITY, f64::min);
    let vmpp_cold = strings.iter().map(|x| x.vmpp_cold_v).fold(0.0, f64::max);
    if voc_cold > inv.dc.max_voltage_v {
        issues.push(tag(Issue::new(Severity::Error, "maxVoltage")
            .num("voltage", voc_cold)
            .num("max", inv.dc.max_voltage_v)
            .num("temp", s.min_cell_temp_c)));
    }
    if vmpp_hot.is_finite() && vmpp_hot > 0.0 && vmpp_hot < inv.dc.mppt_min_v {
        issues.push(tag(Issue::new(Severity::Warning, "belowMpp")
            .num("voltage", vmpp_hot)
            .num("min", inv.dc.mppt_min_v)
            .num("temp", s.max_cell_temp_c)));
    }
    if vmpp_cold > inv.dc.mppt_max_v {
        issues.push(tag(Issue::new(Severity::Warning, "aboveMpp")
            .num("voltage", vmpp_cold)
            .num("max", inv.dc.mppt_max_v)
            .num("temp", s.min_cell_temp_c)));
    }
    let isc: f64 = strings.iter().map(|x| x.isc_a).sum();
    let impp: f64 = strings.iter().map(|x| x.impp_a).sum();
    let power: f64 = strings.iter().map(|x| x.power_wp).sum();
    if let Some(max) = t.max_short_circuit_current_a {
        if isc > max + 1e-9 {
            issues.push(tag(Issue::new(Severity::Error, "trackerIsc")
                .num("current", isc)
                .num("max", max)));
        }
    }
    if let Some(max) = t.max_current_a {
        if impp > max + 1e-9 {
            issues.push(tag(Issue::new(Severity::Warning, "trackerCurrent")
                .num("current", impp)
                .num("max", max)));
        }
    }
    if let Some(max) = t.max_power_w {
        if power > max + 1e-9 {
            issues.push(tag(Issue::new(Severity::Warning, "trackerPower")
                .num("power", power)
                .num("max", max)));
        }
    }
    let limits = w
        .strings
        .iter()
        .find(|x| strings.iter().any(|r| r.id == x.id))
        .and_then(|x| {
            x.modules
                .iter()
                .find_map(|m| sites.get(m)?.electrical.as_ref())
        })
        .and_then(|e| string_limits(e, inv, s));
    TrackerReport {
        index,
        strings,
        power_wp: power,
        isc_a: isc,
        impp_a: impp,
        limits,
        issues,
    }
}
