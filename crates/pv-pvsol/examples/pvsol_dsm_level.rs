//! Vertical consistency of the imported surroundings over a corpus: the raw surface model
//! under the modules (the project's own roof) against the modules' height, in the frame the
//! import gives the surroundings. A positive gap: the roof in the survey stands higher than
//! the modules (the surroundings are set too low). Pitched-roof modules stand ~0.2 m above
//! their roof; facade modules are left out.
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_dsm_level -- <dir>`.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    if dir.is_file() {
        out.push(dir.to_path_buf());
        return;
    }
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
    let rows: Vec<(f64, String)> = std::thread::scope(|s| {
        let chunk = files.len().div_ceil(16).max(1);
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .filter_map(|path| {
                            let file = pv_pvsol::read(&std::fs::read(path).ok()?).ok()?;
                            let h = file.height_map.as_ref()?;
                            let map = file.scene.as_ref()?.map.as_ref()?;
                            let p = pv_pvsol::import(&file, "x").ok()?.project;
                            let sur = p.site.surroundings.as_ref()?;
                            let cell = map.width_m / h.cols as f64;
                            let west = map.corner[0];
                            let north = map.corner[1] + map.depth_m;
                            let raw = |e: f64, n: f64| -> Option<f64> {
                                let c = ((e - west) / cell).floor();
                                let r = ((north - n) / cell).floor();
                                (c >= 0.0
                                    && r >= 0.0
                                    && (c as usize) < h.cols
                                    && (r as usize) < h.rows)
                                    .then(|| h.altitude(r as usize, c as usize))
                            };
                            // Zero of the surroundings: from a kept cell, raw − imported.
                            let k = (sur.cell_m / cell).round().max(1.0) as usize;
                            let zero = (0..sur.heights_cm.len()).find_map(|i| {
                                let v = sur.heights_cm[i];
                                if v == pv_model::Surroundings::CUT {
                                    return None;
                                }
                                let (r, c) = (i / sur.cols as usize, i % sur.cols as usize);
                                // The maximum over the fine cells of that coarse cell.
                                let mut top = f64::MIN;
                                for rr in r * k..((r + 1) * k).min(h.rows) {
                                    for cc in c * k..((c + 1) * k).min(h.cols) {
                                        top = top.max(h.altitude(rr, cc));
                                    }
                                }
                                Some(top - f64::from(v) / 100.0)
                            })?;
                            let mut gaps: Vec<f64> = p
                                .modules
                                .iter()
                                .filter_map(|m| {
                                    let (_, f) = p.module_context(m)?;
                                    if f.tilt_deg > 60.0 {
                                        return None;
                                    }
                                    let w = f.uv_to_world(m.u, m.v);
                                    Some(raw(w[0], w[1])? - zero - w[2])
                                })
                                .collect();
                            if gaps.is_empty() {
                                return None;
                            }
                            gaps.sort_by(f64::total_cmp);
                            let median = gaps[gaps.len() / 2];
                            // Zero implied by the roof (the survey shows the modules' roof).
                            let z_roof = zero + median;
                            let min = h.cm.iter().copied().min().map_or(h.base_m, |m| h.base_m + f64::from(m) / 100.0);
                            Some((
                                median,
                                format!(
                                    "{} | zero − base {:+.2} | roof zero − base {:+.2} | roof zero − min {:+.2}",
                                    path.file_name()?.to_string_lossy(),
                                    zero - h.base_m,
                                    z_roof - h.base_m,
                                    z_roof - min
                                ),
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
    let mut gaps: Vec<f64> = rows.iter().map(|r| r.0).collect();
    gaps.sort_by(f64::total_cmp);
    let n = gaps.len();
    if n == 0 {
        println!("no project");
        return;
    }
    println!(
        "{n} projects; survey roof − modules: median {:+.2} m, p10 {:+.2}, p90 {:+.2}",
        gaps[n / 2],
        gaps[n / 10],
        gaps[n * 9 / 10]
    );
    for (g, name) in &rows {
        println!("  {g:+6.2}  {name}");
    }
}
