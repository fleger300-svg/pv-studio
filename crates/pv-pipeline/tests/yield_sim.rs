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
            surroundings: None,
            horizon: None,
            objects: Vec::new(),
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
    let losses = b.horizon_kwh
        + b.shading_kwh
        + b.iam_kwh
        + b.soiling_kwh
        + b.low_light_kwh
        + b.temperature_kwh
        + b.mismatch_kwh
        + b.wiring_mismatch_kwh
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

/// Project A roof 1 with one inverter and two strings of `a` and `b` modules on tracker 0.
fn two_strings_on_one_tracker(a: usize, b: usize) -> Project {
    let mut p = located(ref_a_roof1());
    let inv = pv_library::catalog::components()
        .unwrap()
        .into_iter()
        .find(|c| c.model == "SUN2000-3KTL-M1")
        .expect("3 kW inverter");
    let component = inv.id;
    Command::AdoptComponents {
        snapshots: vec![ComponentSnapshot {
            component: inv,
            price: None,
        }],
    }
    .apply(&mut p)
    .unwrap();
    let modules: Vec<pv_model::ModuleRef> = p
        .modules
        .iter()
        .map(|m| pv_model::ModuleRef {
            array: m.array_id,
            row: m.row,
            col: m.col,
        })
        .collect();
    assert!(modules.len() >= a + b);
    let inverter = InverterId::new();
    let string = |m: &[pv_model::ModuleRef]| pv_model::PvString {
        id: pv_model::StringId::new(),
        inverter,
        tracker: 0,
        modules: m.to_vec(),
        locked: false,
    };
    p.wiring = Wiring {
        inverters: vec![InverterUnit {
            id: inverter,
            name: "WR1".into(),
            component,
        }],
        strings: vec![string(&modules[..a]), string(&modules[a..a + b])],
        settings: WiringSettings::default(),
    };
    p
}

#[test]
fn unequal_strings_on_one_tracker_lose_energy() {
    let a = SimulationAssumptions::default();
    let equal = simulate(&two_strings_on_one_tracker(8, 8), &a).unwrap();
    assert!(
        equal.balance.wiring_mismatch_kwh.abs() < 1e-9,
        "{:?}",
        equal.balance
    );
    let unequal = simulate(&two_strings_on_one_tracker(8, 6), &a).unwrap();
    let b = &unequal.balance;
    let before = b.nominal_kwh
        - b.horizon_kwh
        - b.shading_kwh
        - b.iam_kwh
        - b.soiling_kwh
        - b.low_light_kwh
        - b.temperature_kwh
        - b.mismatch_kwh;
    let share = b.wiring_mismatch_kwh / before;
    // PV*SOL books a few % for strings differing by a quarter of their length.
    assert!((0.01..0.15).contains(&share), "{share} {b:?}");
    let losses = b.horizon_kwh
        + b.shading_kwh
        + b.iam_kwh
        + b.soiling_kwh
        + b.low_light_kwh
        + b.temperature_kwh
        + b.mismatch_kwh
        + b.wiring_mismatch_kwh
        + b.dc_cable_kwh
        + b.inverter_kwh
        + b.clipping_kwh;
    assert!((b.nominal_kwh - losses - unequal.ac_energy_kwh).abs() < 1e-6 * b.nominal_kwh);
    println!("8 + 6 modules on one tracker: {:.2} %", share * 100.0);
}

/// A 3 m high, 2 m wide chimney just below (south of) the middle of the modules.
fn add_chimney(p: &mut Project) {
    let us: Vec<f64> = p.modules.iter().map(|m| m.u).collect();
    let v0 = p.modules.iter().map(|m| m.v).fold(f64::INFINITY, f64::min);
    let u_mid = (us.iter().copied().fold(f64::INFINITY, f64::min)
        + us.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        / 2.0;
    let face = &mut p.roof_faces[0];
    let outline = face.outline.as_mut().expect("outline");
    outline.obstacles.push(pv_model::RoofObstacle {
        name: "chimney".into(),
        kind: pv_model::ObstacleKind::Chimney,
        footprint_uv: vec![
            [u_mid - 1.0, v0 - 1.6],
            [u_mid + 1.0, v0 - 1.6],
            [u_mid + 1.0, v0 - 0.8],
            [u_mid - 1.0, v0 - 0.8],
        ],
        height_m: 3.0,
        clearance_m: None,
    });
}

#[test]
fn partial_shading_of_a_string_costs_more_than_the_shaded_area() {
    let a = SimulationAssumptions::default();
    // Every module in one string per tracker, so that no strings are in parallel.
    let mut p = two_strings_on_one_tracker(8, 8);
    p.wiring.strings[1].tracker = 1;
    let base = simulate(&p, &a).unwrap();
    add_chimney(&mut p);
    let r = simulate(&p, &a).unwrap();
    let b = &r.balance;
    assert!(b.shading_kwh > base.balance.shading_kwh, "{b:?}");
    assert!(
        b.wiring_mismatch_kwh > base.balance.wiring_mismatch_kwh + 0.001 * b.nominal_kwh,
        "{:?} → {b:?}",
        base.balance
    );
    let losses = b.horizon_kwh
        + b.shading_kwh
        + b.iam_kwh
        + b.soiling_kwh
        + b.low_light_kwh
        + b.temperature_kwh
        + b.mismatch_kwh
        + b.wiring_mismatch_kwh
        + b.dc_cable_kwh
        + b.inverter_kwh
        + b.clipping_kwh;
    assert!((b.nominal_kwh - losses - r.ac_energy_kwh).abs() < 1e-6 * b.nominal_kwh);
    println!(
        "chimney: shading {:.2} %, partial shading (diodes) {:.2} %",
        b.shading_kwh / b.nominal_kwh * 100.0,
        b.wiring_mismatch_kwh / b.nominal_kwh * 100.0
    );
}

#[test]
fn a_chimney_in_front_of_the_modules_shades_them() {
    let p = located(ref_a_roof1());
    let base = simulate(&p, &SimulationAssumptions::default()).unwrap();
    // Without obstacles, only the building itself may shade (little or nothing).
    assert!(
        base.balance.shading_kwh < 0.02 * base.balance.nominal_kwh,
        "{:?}",
        base.balance
    );

    let mut shaded = p.clone();
    add_chimney(&mut shaded);
    let r = simulate(&shaded, &SimulationAssumptions::default()).unwrap();
    assert!(
        r.balance.shading_kwh > base.balance.shading_kwh + 0.005 * r.balance.nominal_kwh,
        "{:?}",
        r.balance
    );
    assert!(r.ac_energy_kwh < base.ac_energy_kwh);
    println!(
        "shading: {:.1} % → {:.1} %",
        base.balance.shading_kwh / base.balance.nominal_kwh * 100.0,
        r.balance.shading_kwh / r.balance.nominal_kwh * 100.0
    );
}

#[test]
fn a_far_horizon_costs_energy_and_the_balance_still_closes() {
    let p = located(ref_a_roof1());
    let base = simulate(&p, &SimulationAssumptions::default()).unwrap();
    assert!(base.balance.horizon_kwh.abs() < 1e-6, "no horizon: no loss");
    let mut hilly = p.clone();
    hilly.site.horizon = Some(pv_model::Horizon {
        source: "test".into(),
        points: vec![[0.0, 20.0], [180.0, 20.0]],
    });
    let r = simulate(&hilly, &SimulationAssumptions::default()).unwrap();
    let b = &r.balance;
    assert!(b.horizon_kwh > 0.05 * b.nominal_kwh, "{b:?}");
    let losses = b.horizon_kwh
        + b.shading_kwh
        + b.iam_kwh
        + b.soiling_kwh
        + b.low_light_kwh
        + b.temperature_kwh
        + b.mismatch_kwh
        + b.wiring_mismatch_kwh
        + b.dc_cable_kwh
        + b.inverter_kwh
        + b.clipping_kwh;
    assert!((b.nominal_kwh - losses - r.ac_energy_kwh).abs() < 1e-6 * b.nominal_kwh);
    assert!(r.ac_energy_kwh < base.ac_energy_kwh);
}

#[test]
fn the_shading_of_each_module_adds_up_to_the_balance() {
    let mut p = located(ref_a_roof1());
    add_chimney(&mut p);
    p.site.horizon = Some(pv_model::Horizon {
        source: "test".into(),
        points: vec![[0.0, 10.0], [180.0, 10.0]],
    });
    let a = SimulationAssumptions::default();
    let r = simulate(&p, &a).unwrap();
    let map = pv_solar::module_shading::module_shading(&p, a.albedo).unwrap();
    let active = p
        .modules
        .iter()
        .filter(|m| m.state == pv_model::ModuleState::Active)
        .count();
    assert_eq!(map.modules.len(), active);
    // One module type on one face: the irradiation-weighted means are the balance's shares.
    let b = &r.balance;
    assert!(
        (map.near - b.shading_kwh / b.nominal_kwh).abs() < 1e-9,
        "{map:?} {b:?}"
    );
    assert!((map.horizon - b.horizon_kwh / b.nominal_kwh).abs() < 1e-9);
    // The chimney stands south of the middle of the lowest row: the most shaded module is
    // in that row, close to the middle.
    let worst = map
        .modules
        .iter()
        .max_by(|x, y| x.near.total_cmp(&y.near))
        .unwrap();
    let least = map
        .modules
        .iter()
        .map(|m| m.near)
        .fold(f64::INFINITY, f64::min);
    assert!(worst.near > least + 0.02, "{worst:?} vs {least}");
    let m = &p.modules[worst.module_id.0 as usize];
    let v0 = p.modules.iter().map(|m| m.v).fold(f64::INFINITY, f64::min);
    assert!(
        m.v - v0 < 1.5,
        "the worst module is in the lowest row: {m:?}"
    );
    // The horizon is the same for every module of the face.
    assert!(map
        .modules
        .iter()
        .all(|x| (x.horizon - map.horizon).abs() < 1e-12));
}

#[test]
fn the_roof_map_shows_the_chimney_shadow_and_the_horizon_everywhere() {
    let mut p = located(ref_a_roof1());
    p.site.horizon = Some(pv_model::Horizon {
        source: "test".into(),
        points: vec![[0.0, 10.0], [180.0, 10.0]],
    });
    let a = SimulationAssumptions::default();
    let open = pv_solar::roof_shading::roof_shading(&p, a.albedo).unwrap();
    let map = pv_solar::module_shading::module_shading(&p, a.albedo).unwrap();
    // Without obstacles, the roof loses about the horizon alone (a little more at its edges,
    // under the neighbouring face).
    let face = open
        .faces
        .iter()
        .find(|f| f.face_id == p.roof_faces[0].id)
        .unwrap();
    let cells: Vec<f64> = face
        .losses
        .iter()
        .filter(|&&x| x >= 0)
        .map(|&x| f64::from(x) / 10_000.0)
        .collect();
    assert!(!cells.is_empty());
    let mut sorted = cells.clone();
    sorted.sort_by(f64::total_cmp);
    let median = sorted[sorted.len() / 2];
    assert!(
        (median - map.horizon).abs() < 0.01,
        "median {median} vs horizon {}",
        map.horizon
    );

    // A chimney: the cells just north of it (in its shadow) lose much more.
    add_chimney(&mut p);
    let shaded = pv_solar::roof_shading::roof_shading(&p, a.albedo).unwrap();
    let face = shaded
        .faces
        .iter()
        .find(|f| f.face_id == p.roof_faces[0].id)
        .unwrap();
    let obstacle = &p.roof_faces[0].outline.as_ref().unwrap().obstacles[0].footprint_uv;
    let (u_mid, v_top) = ((obstacle[0][0] + obstacle[1][0]) / 2.0, obstacle[2][1]);
    let at = |u: f64, v: f64| {
        let c = ((u - face.origin_uv[0]) / face.cell_m) as usize;
        let r = ((v - face.origin_uv[1]) / face.cell_m) as usize;
        f64::from(face.losses[r * face.cols as usize + c]) / 10_000.0
    };
    let behind = at(u_mid, v_top + 0.3);
    assert!(
        behind > median + 0.1,
        "behind the chimney {behind} vs {median}"
    );
    // Under the chimney there is no roof to show.
    assert_eq!(
        face.losses[((obstacle[0][1] + 0.3 - face.origin_uv[1]) / face.cell_m) as usize
            * face.cols as usize
            + ((u_mid - face.origin_uv[0]) / face.cell_m) as usize],
        pv_solar::roof_shading::OUTSIDE
    );
}
