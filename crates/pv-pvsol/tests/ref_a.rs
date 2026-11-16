//! Project A (anonymised PV*SOL premium 2026 R8 project, `testdata/pvsol/ref-a`): reading
//! every part, then importing it. Reference values from PV*SOL's own report and files.

use std::path::Path;

use pv_model::{ComponentSpec, ModuleRef};
use pv_pvsol::{read_archive, Archive};

fn fixture() -> Archive {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/pvsol/ref-a");
    let names = [
        "Project.xml",
        "SimResults.xml",
        "DB.json",
        "Visu3D/GeometrischeDaten.xml",
        "Visu3D/Modul_Verschaltung.xml",
        "Visu3D/ProjDaten.xml",
    ];
    Archive::from_entries(names.iter().map(|n| {
        (
            (*n).to_owned(),
            std::fs::read(root.join(n)).expect("fixture file"),
        )
    }))
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn reads_the_configuration() {
    let f = read_archive(&fixture()).unwrap();
    let c = &f.config;
    assert_eq!(f.title, "Projet A");
    assert!(c.program.starts_with("PV*SOL premium 2026"));
    assert_eq!(c.transposition_model, "HayDavies");
    assert!(close(c.spectral_loss, 0.01, 1e-12));
    assert_eq!(c.areas.len(), 2);
    assert_eq!(c.module_count(), 116);
    let south = &c.areas[0];
    assert_eq!(
        (south.module_count, south.tilt_deg, south.azimuth_deg),
        (68, 2.0, 190.0)
    );
    assert_eq!(south.module_name, "LR7-54HVH-490M 2026");
    // Global losses apply (the areas do not define their own).
    assert!(close(south.losses.soiling, 0.02, 1e-12));
    assert!(close(south.losses.mismatch, 0.02, 1e-12));
    assert!(close(south.losses.diodes, 0.005, 1e-12));
    // Inverter 1: 1×16, 1×10, 1×8, 2×16; inverter 2: 1×20, 2×15.
    let shape: Vec<Vec<(u32, u32)>> = c
        .inverters
        .iter()
        .map(|i| {
            i.trackers
                .iter()
                .flat_map(|t| t.iter().map(|s| (s.strings, s.in_series)))
                .collect()
        })
        .collect();
    assert_eq!(
        shape,
        vec![
            vec![(1, 16), (1, 10), (1, 8), (2, 16)],
            vec![(1, 20), (2, 15)]
        ]
    );
    let loc = (f.site.lat_deg.unwrap(), f.site.lon_deg.unwrap());
    assert!(
        close(loc.0, 46.43, 0.006) && close(loc.1, 6.91, 0.006),
        "{loc:?}"
    );
}

#[test]
fn reads_the_component_records() {
    let f = read_archive(&fixture()).unwrap();
    let m =
        f.db.module("185168d5-1dd3-4253-bab1-2b71adc66a72")
            .expect("module");
    let ComponentSpec::Module(spec) = &m.spec else {
        panic!("module spec")
    };
    let e = spec.electrical.unwrap();
    assert_eq!(e.pmax_w, 490.0);
    assert!(close(spec.length_m, 1.8, 1e-9) && close(spec.width_m, 1.134, 1e-9));
    // −81 mV/K on 40.52 V; 7.67 mA/K on 15.33 A; −0.256 %/K.
    assert!(close(e.temp_coeff_voc_pct_per_k, -0.1999, 1e-4));
    assert!(close(e.temp_coeff_isc_pct_per_k, 0.0500, 1e-4));
    assert!(close(e.temp_coeff_pmax_pct_per_k, -0.256, 1e-9));
    let inv =
        f.db.inverter("e9ee05cd-d678-46d8-8fd9-a51baf876ddf")
            .expect("30 kW");
    let ComponentSpec::Inverter(i) = &inv.spec else {
        panic!("inverter spec")
    };
    assert_eq!(i.ac.nominal_power_w, 30_000.0);
    assert_eq!(i.dc.trackers.len(), 4);
    assert!(i.dc.trackers.iter().all(|t| t.inputs == 2));
    // 3 % × η5 + 6 % × η10 + 13 % × η20 + 10 % × η30 + 48 % × η50 + 20 % × η100.
    assert!(close(i.euro_efficiency_pct.unwrap(), 98.447, 0.001));
    assert!(m.validate().is_ok() && inv.validate().is_ok());
}

#[test]
fn reads_pvsol_results() {
    let f = read_archive(&fixture()).unwrap();
    let r = f.results.expect("simulated");
    assert!(close(r.irradiance.global_horizontal, 1354.49, 0.01));
    assert_eq!(r.monthly_ghi_kwh_m2.len(), 12);
    assert!(close(r.dc_power_kwp, 56.84, 1e-9));
    assert!(close(r.grid_feed_in_kwh, 52_587.5, 0.5));
    assert!(close(r.specific_yield(), 925.2, 0.1));
    assert!(close(r.pv.nominal, 70_052.9, 0.1));
    assert!(close(r.pv.module_shading, -9_602.1, 0.1));
    assert!(close(r.inverter.fed_in, 52_588.3, 0.1));
    assert_eq!(r.areas.len(), 2);
}

#[test]
fn decodes_the_scene() {
    let f = read_archive(&fixture()).unwrap();
    let s = f.scene.expect("3-D scene");
    assert_eq!(s.formations.len(), 8);
    assert_eq!(s.module_count(), 116);
    assert_eq!(s.buildings.len(), 1);
    // Every formation of a face implies the same face origin (within 2 cm), at the eave.
    for f1 in &s.formations {
        for f2 in s.formations.iter().filter(|f2| f2.face == f1.face) {
            let (a, b) = (f1.face_origin(), f2.face_origin());
            assert!((0..3).all(|k| close(a[k], b[k], 0.02)), "{a:?} {b:?}");
        }
        let b = &s.buildings[0];
        assert!(close(
            f1.face_origin()[2],
            b.ground_m + b.eave_height_m,
            0.02
        ));
    }
    // Module centres match the cable points of Kabel.xml (face coordinates).
    let a1 = &s.formations[0];
    let c = a1.module_centre_uv(0, 0);
    assert!(
        close(c[0], 7.3754, 0.002) && close(c[1], 1.1546, 0.002),
        "{c:?}"
    );
}

#[test]
fn imports_project_a() {
    let f = read_archive(&fixture()).unwrap();
    let out = pv_pvsol::import(&f, "fallback").unwrap();
    let p = &out.project;
    p.validate().unwrap();
    assert_eq!(p.name, "Projet A");
    assert_eq!(p.buildings.len(), 1);
    assert_eq!(p.roof_faces.len(), 2);
    assert_eq!(p.arrays.len(), 8);
    assert_eq!(p.modules.len(), 116);
    assert_eq!(p.wiring.inverters.len(), 2);
    assert_eq!(p.wiring.strings.len(), 8);
    let wired: usize = p.wiring.strings.iter().map(|s| s.modules.len()).sum();
    assert_eq!(wired, 116);
    // Only the shading objects (3-D polygons) are left out, and the weather, which the
    // fixture does not carry (Meteonorm data).
    let mut keys: Vec<&str> = out.notes.iter().map(|n| n.key.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        ["pvsol.note.noWeather", "pvsol.note.objectsSkipped"],
        "{:?}",
        out.notes
    );
    // DC/AC ratios of PV*SOL's report: 107.8 % and 122.5 %.
    for (unit, expected) in p.wiring.inverters.iter().zip([1.078, 1.225]) {
        let modules: usize = p
            .wiring
            .strings
            .iter()
            .filter(|s| s.inverter == unit.id)
            .map(|s| s.modules.len())
            .sum();
        let Some(ComponentSpec::Inverter(i)) =
            p.component(unit.component).map(|c| &c.component.spec)
        else {
            panic!("inverter")
        };
        let ratio = modules as f64 * 490.0 / i.ac.nominal_power_w;
        assert!(close(ratio, expected, 0.0005), "{ratio}");
    }
    // The first module of the first string: formation 01, row 0, column 0, at the cable
    // point of Kabel.xml.
    let first = p.wiring.strings[0].modules[0];
    let m = p
        .modules
        .iter()
        .find(|m| {
            ModuleRef {
                array: m.array_id,
                row: m.row,
                col: m.col,
            } == first
        })
        .unwrap();
    assert_eq!((m.row, m.col), (0, 0));
    assert!(close(m.u, 7.3754, 0.002) && close(m.v, 1.1546, 0.002));
    // Faces: 2° gable, facing 190° and 10°, at the building's eave.
    let mut az: Vec<f64> = p.roof_faces.iter().map(|f| f.azimuth_deg).collect();
    az.sort_by(f64::total_cmp);
    // PV*SOL stores angles in single precision (190.00000003°).
    assert!(
        close(az[0], 10.0, 1e-4) && close(az[1], 190.0, 1e-4),
        "{az:?}"
    );
    assert_eq!(p.buildings[0].roof_kind, pv_model::RoofKind::Gable);
    // Without the weather file (not in the fixture), the location is still set.
    assert!(p.site.location.is_some() && p.site.weather.is_none());
}

/// Every `.pvprj` of the corpus imports (run with `--ignored` and `PVSOL_CORPUS` set to the
/// folder holding the client projects).
#[test]
#[ignore = "needs the PV*SOL corpus on this machine (PVSOL_CORPUS)"]
fn the_whole_corpus_imports() {
    let dir = std::env::var("PVSOL_CORPUS").expect("PVSOL_CORPUS");
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let path = e.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("pvprj"))
            {
                files.push(path);
            }
        }
    }
    assert!(!files.is_empty(), "no corpus");
    for path in files {
        let bytes = std::fs::read(&path).unwrap();
        let file = pv_pvsol::read(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let out = pv_pvsol::import(&file, "x")
            .unwrap_or_else(|e| panic!("{}: {} {:?}", path.display(), e.key, e.params));
        out.project.validate().unwrap();
    }
}
