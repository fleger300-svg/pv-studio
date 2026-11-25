//! Raw surface model of one project around its modules: altitude percentiles under the
//! modules and within growing radii, and a text map of the altitude above the lowest
//! point of the window (one character per 2 m, digits in steps of 3 m).
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_dsm_raw -- <file.pvprj>`.

fn pct(mut v: Vec<f64>, p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p).round() as usize]
}

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let file = pv_pvsol::read(&std::fs::read(path).expect("read")).expect("parse");
    let h = file.height_map.as_ref().expect("height map");
    let map = file.scene.as_ref().and_then(|s| s.map).expect("map");
    let p = pv_pvsol::import(&file, "x").expect("import").project;
    let cell = map.width_m / h.cols as f64;
    let west = map.corner[0];
    let north = map.corner[1] + map.depth_m;
    let raw = |e: f64, n: f64| -> Option<f64> {
        let c = ((e - west) / cell).floor();
        let r = ((north - n) / cell).floor();
        (c >= 0.0 && r >= 0.0 && (c as usize) < h.cols && (r as usize) < h.rows)
            .then(|| h.altitude(r as usize, c as usize))
    };
    let centres: Vec<[f64; 3]> = p
        .modules
        .iter()
        .filter_map(|m| {
            let (_, f) = p.module_context(m)?;
            Some(f.uv_to_world(m.u, m.v))
        })
        .collect();
    let n = centres.len() as f64;
    let c = [0, 1, 2].map(|k| centres.iter().map(|q| q[k]).sum::<f64>() / n);
    let under: Vec<f64> = centres.iter().filter_map(|m| raw(m[0], m[1])).collect();
    println!(
        "cell {cell:.2} m | modules z {:.1} | survey under modules: p10 {:.1} median {:.1} p90 {:.1}",
        c[2],
        pct(under.clone(), 0.1),
        pct(under.clone(), 0.5),
        pct(under, 0.9)
    );
    for radius in [10.0f64, 20.0, 30.0, 50.0] {
        let mut v = Vec::new();
        let mut e = -radius;
        while e <= radius {
            let mut nn = -radius;
            while nn <= radius {
                if e.hypot(nn) <= radius {
                    if let Some(a) = raw(c[0] + e, c[1] + nn) {
                        v.push(a);
                    }
                }
                nn += 1.0;
            }
            e += 1.0;
        }
        println!(
            "within {radius:>2} m: p5 {:.1} p10 {:.1} median {:.1} p90 {:.1} max {:.1}",
            pct(v.clone(), 0.05),
            pct(v.clone(), 0.1),
            pct(v.clone(), 0.5),
            pct(v.clone(), 0.9),
            pct(v, 1.0)
        );
    }
    let low = {
        let mut m = f64::MAX;
        for r in -25..=25 {
            for q in -25..=25 {
                if let Some(a) = raw(c[0] + q as f64 * 2.0, c[1] - r as f64 * 2.0) {
                    m = m.min(a);
                }
            }
        }
        m
    };
    println!("text map: altitude above {low:.1} m, 3 m per digit, M modules");
    for r in -25..=25 {
        let line: String = (-25..=25)
            .map(|q| {
                let (e, nn) = (c[0] + q as f64 * 2.0, c[1] - r as f64 * 2.0);
                if centres
                    .iter()
                    .any(|m| (m[0] - e).abs() < 1.0 && (m[1] - nn).abs() < 1.0)
                {
                    return 'M';
                }
                raw(e, nn).map_or(' ', |a| {
                    char::from_digit((((a - low) / 3.0) as u32).min(9), 10).unwrap_or('9')
                })
            })
            .collect();
        println!("{line}");
    }
}
