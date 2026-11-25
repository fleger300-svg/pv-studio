//! Prints PV Studio against PV*SOL, row by row, for one project, with the import notes.
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_compare_one -- <file.pvprj>`.

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let file = pv_pvsol::read(&std::fs::read(path).expect("read")).expect("parse");
    let out = pv_pvsol::import(&file, "x").expect("import");
    for n in &out.notes {
        println!("note {} {:?}", n.key, n.params);
    }
    let p = &out.project;
    println!(
        "modules {} | horizon {} points | objects {}",
        p.modules.len(),
        p.site.horizon.as_ref().map_or(0, |h| h.points.len()),
        p.site.objects.len()
    );
    let mut planes: std::collections::BTreeMap<(i64, i64), usize> = Default::default();
    for m in &p.modules {
        if let Some((_, rot)) = p.module_transform(m) {
            let n = rot.rotate([0.0, 0.0, 1.0]);
            let tilt = n[2].clamp(-1.0, 1.0).acos().to_degrees();
            let az = n[0].atan2(n[1]).to_degrees().rem_euclid(360.0);
            *planes
                .entry((tilt.round() as i64, az.round() as i64))
                .or_default() += 1;
        }
    }
    if std::env::var("OUTLINES").is_ok() {
        for m in &p.modules {
            if let Some((pos, rot)) = p.module_transform(m) {
                if rot.rotate([0.0, 0.0, 1.0])[2].abs() < 0.2 {
                    println!(
                        "    facade module at [{:.2}, {:.2}, {:.2}]",
                        pos[0], pos[1], pos[2]
                    );
                }
            }
        }
    }
    for ((tilt, az), n) in &planes {
        println!("  modules tilt {tilt}° azimuth {az}°: {n}");
    }
    for o in &p.site.objects {
        let z = o.outline.iter().map(|q| q[2]).fold(f64::MIN, f64::max) + o.height_m;
        println!(
            "  object {:?} {} | {} corners, top {z:.1} m",
            o.kind,
            o.name,
            o.outline.len()
        );
        if std::env::var("OUTLINES").is_ok() {
            for q in &o.outline {
                println!("      [{:.2}, {:.2}, {:.2}]", q[0], q[1], q[2]);
            }
        }
    }
    if let Some(r) = &file.results {
        for a in &r.areas {
            println!(
                "  PV*SOL area {} | PR {:.1} % | {:.0} kWh",
                a.area, a.performance_ratio, a.ac_energy_kwh
            );
        }
    }
    if let Some(c) = pv_pvsol::compare(&file, p) {
        for r in &c.rows {
            println!(
                "{:24} {:>12.2} {:>12.2} {}",
                r.key, r.pvsol, r.pvstudio, r.unit
            );
        }
    }
}
