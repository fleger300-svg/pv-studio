//! M1: DC design on project A roof 1 (LONGi LR7-54HVH-490M) with a Huawei SUN2000-30KTL-M3
//! (4 trackers × 2 inputs, 1100 V, MPP 200–1000 V, Isc 40 A per tracker): string limits,
//! automatic stringing, checks, and undo through the `SetWiring` command.

#![allow(dead_code)]

mod common;

use common::*;
use pv_electrical::{auto::auto_string, check, module_voltages, string_limits, Severity};
use pv_model::component::ComponentSnapshot;
use pv_model::{
    Command, ComponentSpec, InverterId, InverterUnit, Project, PvString, StringId, Wiring,
    WiringSettings,
};

fn with_inverter() -> (Project, pv_model::component::InverterSpec) {
    let mut p = ref_a_roof1();
    let inv = pv_library::catalog::components()
        .unwrap()
        .into_iter()
        .find(|c| c.model == "SUN2000-30KTL-M3")
        .expect("catalogue inverter");
    let ComponentSpec::Inverter(spec) = inv.spec.clone() else {
        panic!("inverter")
    };
    let id = inv.id;
    Command::AdoptComponents {
        snapshots: vec![ComponentSnapshot {
            component: inv,
            price: None,
        }],
    }
    .apply(&mut p)
    .unwrap();
    let wiring = Wiring {
        inverters: vec![InverterUnit {
            id: InverterId::new(),
            name: "WR1".into(),
            component: id,
        }],
        strings: vec![],
        settings: WiringSettings::default(),
    };
    Command::SetWiring {
        wiring: Box::new(wiring),
    }
    .apply(&mut p)
    .unwrap();
    (p, *spec)
}

fn lr7() -> pv_model::component::ModuleElectrical {
    let ComponentSpec::Module(m) = pv_model::component::presets::lr7_54hvh_490m().spec else {
        panic!()
    };
    m.electrical.unwrap()
}

#[test]
fn string_limits_follow_the_temperature_model() {
    let (_, spec) = with_inverter();
    let e = lr7();
    let s = WiringSettings::default();
    let v = module_voltages(&e, &s);
    // Voc(−10 °C) = 40.52 · (1 + 0.002 · 35) = 43.356 V.
    assert!((v.voc_cold - 43.3564).abs() < 1e-3, "{}", v.voc_cold);
    // Vmpp(70 °C) = 33.51 · (1 − 0.0031 · 45) = 28.835 V.
    assert!((v.vmpp_hot - 28.8354).abs() < 1e-3, "{}", v.vmpp_hot);
    // 1100 V / 43.356 V = 25.4 → 25; 200 V / 28.835 V = 6.9 → 7.
    assert_eq!(string_limits(&e, &spec, &s), Some((7, 25)));
}

#[test]
fn automatic_stringing_passes_its_own_checks() {
    let (mut p, _) = with_inverter();
    let before = p.clone();
    let wiring = auto_string(&p);
    assert!(!wiring.strings.is_empty());
    let cmd = Command::SetWiring {
        wiring: Box::new(wiring),
    };
    let undo = cmd.inverse(&p);
    let changes = cmd.apply(&mut p).unwrap();
    assert!(changes.wiring);

    let r = check(&p);
    assert!(
        r.worst.is_none_or(|w| w < Severity::Error),
        "{:#?}",
        r.inverters[0]
            .trackers
            .iter()
            .map(|t| &t.issues)
            .collect::<Vec<_>>()
    );
    let inv = &r.inverters[0];
    for t in &inv.trackers {
        let lengths: std::collections::BTreeSet<u32> =
            t.strings.iter().map(|s| s.modules).collect();
        assert!(lengths.len() <= 1, "equal strings per tracker");
        assert!(t.strings.len() <= 2);
        assert!(t.issues.iter().all(|i| i.severity < Severity::Error));
        for s in &t.strings {
            assert!((7..=25).contains(&s.modules));
            assert!(s.voc_cold_v <= 1100.0);
        }
    }
    let assigned: u32 = inv
        .trackers
        .iter()
        .flat_map(|t| &t.strings)
        .map(|s| s.modules)
        .sum();
    assert_eq!(assigned + r.unassigned_modules, r.active_modules);
    // 35 modules: two strings, 18 and 17, on two trackers (as PV*SOL would).
    assert_eq!(r.unassigned_modules, 0);
    let mut lengths: Vec<u32> = inv
        .trackers
        .iter()
        .flat_map(|t| &t.strings)
        .map(|s| s.modules)
        .collect();
    lengths.sort_unstable();
    assert_eq!(lengths, vec![17, 18]);
    println!(
        "{} modules: {} assigned, DC/AC {:.1} %, strings {:?}",
        r.active_modules,
        assigned,
        inv.dc_ac_ratio * 100.0,
        inv.trackers
            .iter()
            .map(|t| t.strings.iter().map(|s| s.modules).collect::<Vec<_>>())
            .collect::<Vec<_>>()
    );

    undo.apply(&mut p).unwrap();
    assert_eq!(p, before);
}

#[test]
fn a_too_long_string_is_an_error() {
    let (mut p, _) = with_inverter();
    let inverter = p.wiring.inverters[0].id;
    // 26 modules in a row → Voc(−10 °C) = 1127 V > 1100 V.
    let modules: Vec<pv_model::ModuleRef> = p
        .modules
        .iter()
        .filter(|m| m.state == pv_model::ModuleState::Active)
        .take(26)
        .map(|m| pv_model::ModuleRef {
            array: m.array_id,
            row: m.row,
            col: m.col,
        })
        .collect();
    assert_eq!(modules.len(), 26);
    let mut w = p.wiring.clone();
    w.strings.push(PvString {
        id: StringId::new(),
        inverter,
        tracker: 0,
        modules,
        locked: false,
    });
    Command::SetWiring {
        wiring: Box::new(w),
    }
    .apply(&mut p)
    .unwrap();
    let r = check(&p);
    let t = &r.inverters[0].trackers[0];
    assert!(
        t.issues.iter().any(|i| i.key == "wiring.issues.maxVoltage"),
        "{:?}",
        t.issues
    );
    assert_eq!(r.worst, Some(Severity::Error));
    // String plan table: 26 × 40.52 V = 1053.5 V; × 1.15 = 1211.5 V.
    let s = &t.strings[0];
    assert!((s.voc_stc_v - 1053.52).abs() < 1e-6 && (s.voc_plan_v - 1211.548).abs() < 1e-6);
}

#[test]
fn invalid_wiring_is_refused() {
    let (mut p, _) = with_inverter();
    let mut w = p.wiring.clone();
    w.strings.push(PvString {
        id: StringId::new(),
        inverter: w.inverters[0].id,
        tracker: 9,
        modules: vec![pv_model::ModuleRef {
            array: p.arrays[0].id,
            row: 0,
            col: 0,
        }],
        locked: false,
    });
    let before = p.clone();
    assert!(Command::SetWiring {
        wiring: Box::new(w)
    }
    .apply(&mut p)
    .is_err());
    assert_eq!(p, before);
}
