//! Near shading of PV Studio against PV*SOL's « 3-D module shading » over the completely
//! imported corpus projects that hold a height map (PV*SOL weather, PV*SOL-like losses).
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_near_shading -- <dir>`.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p
            .extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("pvprj"))
        {
            out.push(p);
        }
    }
}

fn main() {
    let dir = std::env::args().nth(1).expect("directory");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    let rows: Vec<(f64, f64, String)> = std::thread::scope(|s| {
        let chunk = files.len().div_ceil(16).max(1);
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .filter_map(|path| {
                            let file = pv_pvsol::read(&std::fs::read(path).ok()?).ok()?;
                            file.height_map.as_ref()?;
                            let p = pv_pvsol::import(&file, "x").ok()?.project;
                            let wired: usize =
                                p.wiring.strings.iter().map(|s| s.modules.len()).sum();
                            if p.modules.len() != file.config.module_count() as usize
                                || wired != p.modules.len()
                                || p.site.surroundings.is_none()
                            {
                                return None;
                            }
                            let c = pv_pvsol::compare(&file, &p)?;
                            let r = c.rows.iter().find(|r| r.key == "nearShading")?;
                            Some((
                                r.pvsol,
                                r.pvstudio,
                                path.file_name()?.to_string_lossy().into_owned(),
                            ))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let mut gaps: Vec<f64> = rows.iter().map(|r| r.1 - r.0).collect();
    gaps.sort_by(f64::total_cmp);
    let n = gaps.len();
    let within = gaps.iter().filter(|g| g.abs() <= 2.0).count();
    println!(
        "{n} projects; near shading gap (PV Studio − PV*SOL): median {:+.2} pt, p10 {:+.2}, p90 {:+.2}, within 2 pt: {within}",
        gaps[n / 2],
        gaps[n / 10],
        gaps[n * 9 / 10]
    );
    let mean_pvsol = rows.iter().map(|r| r.0).sum::<f64>() / n as f64;
    let mean_ours = rows.iter().map(|r| r.1).sum::<f64>() / n as f64;
    println!("mean near shading: PV*SOL {mean_pvsol:.2} %, PV Studio {mean_ours:.2} %");
    if std::env::var("LIST").is_ok() {
        for r in &rows {
            println!("  {:6.2} {:6.2}  {}", r.0, r.1, r.2);
        }
    }
}
