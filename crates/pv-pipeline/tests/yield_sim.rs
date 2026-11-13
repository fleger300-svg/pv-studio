//! M3: yield simulation on project A roof 1 (LONGi 490 W, 19° south-ish) with a synthetic
//! weather year: the energy balance closes exactly, PR and specific yield are plausible, and
//! an undersized inverter clips.

#![allow(dead_code)]

mod common;

use common::*;
use pv_model::component::ComponentSnapshot;
use pv_model::{
    Command, ComponentSpec, GeoLocation, InverterId, InverterUnit, Project, ProjectSite,
    WeatherYear, Wiring, WiringSettings, HOURS_PER_YEAR,
};
use pv_solar::sun::{extraterrestrial, position};
use pv_solar::{simulate, NotSimulated, SimulationAssumptions};

/// A year of hazy clear days in Bern: GHI = 0.6 · E₀ · cos z, 12 °C.
fn synthetic_weather() -> WeatherYear {
    let mut ghi = Vec::with_capacity(HOURS_PER_YEAR);
    for h in 0..HOURS_PER_YEAR {
        let day = (h / 24) as u32 + 1;
        let sun = position(46.95, 7.44, day, (h % 24) as f64 + 0.5);
        let cz = sun.zenith_deg.to_radians().cos();
        ghi.push(if cz > 0.0 {
            (0.6 * extraterrestrial(day) * cz) as f32
        } else {
            0.0
        });
    }
    WeatherYear {
        source: "synthetic".into(),
        lat_deg: 46.95,
        lon_deg: 7.44,
        altitude_m: 550.0,
        ghi_w_m2: ghi,
        dhi_w_m2: Vec::new(),
        temp_c: vec![12.0; HOURS_PER_YEAR],
    }
}

fn located(mut p: Project) -> Project {
    Command::SetSite {
        site: Box::new(ProjectSite {
            location: Some(GeoLocation {
                label: "Bern".into(),
                lat_deg: 46.95,
                lon_deg: 7.44,
                east_m: 2_600_000.0,
                north_m: 1_200_000.0,
                altitude_m: 550.0,
            }),
            weather: Some(synthetic_weather()),
        }),
    }
    .apply(&mut p)
    .unwrap();
    p
}

#[test]
fn without_weather_there_is_no_simulation() {
    assert_eq!(
        simulate(&ref_a_roof1(), &SimulationAssumptions::default()).unwrap_err(),
        NotSimulated::NoWeather
    );
}

#[test]
fn the_energy_balance_closes_and_is_plausible() {
    let p = located(ref_a_roof1());
    let r = simulate(&p, &SimulationAssumptions::default()).unwrap();
    let b = &r.balance;
    let losses = b.iam_kwh
        + b.soiling_kwh
        + b.low_light_kwh
        + b.temperature_kwh
        + b.mismatch_kwh
        + b.dc_cable_kwh
        + b.inverter_kwh
        + b.clipping_kwh;
    assert!((b.nominal_kwh - losses - r.ac_energy_kwh).abs() < 1e-6 * b.nominal_kwh);
    assert!(
        (0.75..0.92).contains(&r.performance_ratio),
        "PR {}",
        r.performance_ratio
    );
    assert!(
        r.poa_kwh_m2 > r.ghi_kwh_m2,
        "a 19° south-facing roof gains over horizontal"
    );
    assert!((r.monthly_ac_kwh.iter().sum::<f64>() - r.ac_energy_kwh).abs() < 1e-6);
    assert!(r.monthly_ac_kwh[5] > r.monthly_ac_kwh[11]);
    assert_eq!(b.clipping_kwh, 0.0);
    println!(
        "{:.2} kWp: {:.0} kWh, {:.0} kWh/kWp, PR {:.1} %, GHI {:.0}, POA {:.0}",
        r.peak_power_kwp,
        r.ac_energy_kwh,
        r.specific_yield,
        r.performance_ratio * 100.0,
        r.ghi_kwh_m2,
        r.poa_kwh_m2
    );
}

#[test]
fn an_undersized_inverter_clips() {
    let mut p = located(ref_a_roof1());
    let inv = pv_library::catalog::components()
        .unwrap()
        .into_iter()
        .find(|c| c.model == "SUN2000-3KTL-M1")
        .expect("3 kW inverter");
    assert!(matches!(inv.spec, ComponentSpec::Inverter(_)));
    let id = inv.id;
    Command::AdoptComponents {
        snapshots: vec![ComponentSnapshot {
            component: inv,
            price: None,
        }],
    }
    .apply(&mut p)
    .unwrap();
    Command::SetWiring {
        wiring: Box::new(Wiring {
            inverters: vec![InverterUnit {
                id: InverterId::new(),
                name: "WR1".into(),
                component: id,
            }],
            strings: Vec::new(),
            settings: WiringSettings::default(),
        }),
    }
    .apply(&mut p)
    .unwrap();
    // Every module on the 3 kW inverter: one long list of strings is not needed for the
    // simulation, which only maps modules to inverters.
    let mut w = p.wiring.clone();
    let modules: Vec<pv_model::ModuleRef> = p
        .modules
        .iter()
        .map(|m| pv_model::ModuleRef {
            array: m.array_id,
            row: m.row,
            col: m.col,
        })
        .collect();
    for (k, chunk) in modules.chunks(8).enumerate() {
        w.strings.push(pv_model::PvString {
            id: pv_model::StringId::new(),
            inverter: w.inverters[0].id,
            tracker: (k % 2) as u32,
            modules: chunk.to_vec(),
            locked: false,
        });
    }
    p.wiring = w;
    let r = simulate(&p, &SimulationAssumptions::default()).unwrap();
    assert!(
        r.balance.clipping_kwh > 0.1 * r.ac_energy_kwh,
        "{:?}",
        r.balance
    );
    assert!(r.performance_ratio < 0.5);
}
