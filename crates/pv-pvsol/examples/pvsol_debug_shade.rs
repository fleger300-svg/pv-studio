//! Diagnostic: for one imported project, the highest surroundings cell seen from the mean
//! module centre in each direction (elevation angle, distance, height).
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_debug_shade -- <file.pvprj>`.

fn model_beam(m: &mut pv_solar::shade::ShadeModel, a: pv_model::ArrayId, az: f64, el: f64) -> f64 {
    m.beam_shade(a, pv_solar::shade::bin_of(az, el), 6)
}

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let file = pv_pvsol::read(&std::fs::read(path).unwrap()).unwrap();
    let out = pv_pvsol::import(&file, "x").unwrap();
    for n in &out.notes {
        println!("note {} {:?}", n.key, n.params);
    }
    let p = out.project;
    let s = p.site.surroundings.as_ref().expect("surroundings");
    let mut c = [0.0f64; 3];
    for m in &p.modules {
        let (a, f) = p.module_context(m).unwrap();
        let w = f.uv_to_world(m.u, m.v);
        let _ = a;
        for k in 0..3 {
            c[k] += w[k] / p.modules.len() as f64;
        }
    }
    println!("module centre ENU {c:?}");
    for b in &p.buildings {
        println!(
            "building {} origin {:?} eave {:.2} footprint {:?}",
            b.name, b.origin, b.eave_height_m, b.footprint
        );
    }
    let mut bare = p.clone();
    if std::env::var("BARE").is_ok() {
        bare.site.surroundings = None;
    }
    let mut model = pv_solar::shade::ShadeModel::new(&bare);
    for a in &p.arrays {
        let v: Vec<String> = [
            (180.0, 60.0),
            (180.0, 30.0),
            (180.0, 15.0),
            (120.0, 30.0),
            (240.0, 30.0),
        ]
        .iter()
        .map(|&(az, el)| format!("az{az} el{el}: {:.2}", model_beam(&mut model, a.id, az, el)))
        .collect();
        println!(
            "array {}: beam {} | sky view {:.2}",
            a.id,
            v.join("  "),
            model.diffuse_factor(a.id)
        );
    }
    for a in p.arrays.iter().take(4) {
        let m = p.modules.iter().find(|m| m.array_id == a.id).unwrap();
        let (_, face) = p.module_context(m).unwrap();
        let w = face.uv_to_world(m.u, m.v);
        println!(
            "array {} first module at {:.2?} (face tilt {:.1} az {:.1})",
            a.id, w, face.tilt_deg, face.azimuth_deg
        );
        for dn in [3.0, 1.5, 0.0, -1.5, -3.0] {
            let row: Vec<String> = [-3.0, -1.5, 0.0, 1.5, 3.0]
                .iter()
                .map(|de| {
                    s.height_at(w[0] + de, w[1] + dn)
                        .map_or("  cut".into(), |h| format!("{h:5.1}"))
                })
                .collect();
            println!("   {}", row.join(" "));
        }
    }
    let cut = s
        .heights_cm
        .iter()
        .filter(|&&h| h == pv_model::Surroundings::CUT)
        .count();
    println!("cells {} cut {cut}", s.heights_cm.len());
    for az in (60..=300).step_by(15) {
        let a = f64::from(az).to_radians();
        let (mut best, mut at, mut hh) = (-90.0f64, 0.0, 0.0);
        let mut d = 0.5;
        while d < 150.0 {
            if let Some(h) = s.height_at(c[0] + d * a.sin(), c[1] + d * a.cos()) {
                let el = (h - c[2]).atan2(d).to_degrees();
                if el > best {
                    (best, at, hh) = (el, d, h);
                }
            }
            d += 0.25;
        }
        println!(
            "az {az:3}: {best:5.1}° at {at:5.1} m, cell height {hh:6.2} (module {:.2})",
            c[2]
        );
    }
}
