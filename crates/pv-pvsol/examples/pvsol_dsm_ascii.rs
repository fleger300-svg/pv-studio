//! Prints the imported surroundings around the modules as a text map: one character per
//! `STEP` metres (default 2), north up; `#` cut out, `M` modules, digits the height above
//! the mean module height in steps of 3 m (`.` below it, `0` up to 3 m above…).
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_dsm_ascii -- <file.pvprj> [half-size m]`.

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let file = pv_pvsol::read(&std::fs::read(&args[1]).expect("read")).expect("parse");
    let half: f64 = args.get(2).and_then(|a| a.parse().ok()).unwrap_or(40.0);
    let step: f64 = std::env::var("STEP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2.0);
    let p = pv_pvsol::import(&file, "x").expect("import").project;
    let s = p.site.surroundings.as_ref().expect("surroundings");
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
    println!(
        "modules mean height {:.1} m, {} m per character",
        c[2], step
    );
    let k = (half / step) as i64;
    for r in -k..=k {
        let north = c[1] - r as f64 * step;
        let line: String = (-k..=k)
            .map(|q| {
                let east = c[0] + q as f64 * step;
                if centres
                    .iter()
                    .any(|m| (m[0] - east).abs() < step / 2.0 && (m[1] - north).abs() < step / 2.0)
                {
                    return 'M';
                }
                let col = ((east - s.origin_east_m) / s.cell_m).floor();
                let row = ((s.origin_north_m - north) / s.cell_m).floor();
                if col < 0.0 || row < 0.0 || col >= f64::from(s.cols) || row >= f64::from(s.rows) {
                    return ' ';
                }
                let v = s.heights_cm[row as usize * s.cols as usize + col as usize];
                if v == pv_model::Surroundings::CUT {
                    return '#';
                }
                let above = f64::from(v) / 100.0 - c[2];
                if above < 0.0 {
                    '.'
                } else {
                    char::from_digit(((above / 3.0) as u32).min(9), 10).unwrap_or('9')
                }
            })
            .collect();
        println!("{line}");
    }
}
