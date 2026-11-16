//! Calibrates PV Studio's reflection (ASHRAE `b0`, diffuse modifier `1 − b0`) and module
//! heat loss on roofs (`U`) on PV*SOL: for every completely imported project of the corpus
//! simulated with PV*SOL's weather, the median gap of the reflection and temperature steps.
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_calibrate -- <dir>`.

use std::path::{Path, PathBuf};

use pv_model::Project;
use pv_pvsol::{compare::compare_with, PvsolFile};
use pv_solar::SimulationAssumptions;

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

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}

/// (reflection gap, temperature gap, model gap) per project, in points / %.
fn gaps(cases: &[(PvsolFile, Project)], a: SimulationAssumptions) -> Vec<(f64, f64, f64, f64)> {
    let chunk = cases.len().div_ceil(16).max(1);
    std::thread::scope(|s| {
        let handles: Vec<_> = cases
            .chunks(chunk)
            .map(|part| {
                s.spawn(move || {
                    part.iter()
                        .filter_map(|(f, p)| {
                            let mut a = a;
                            let base = pv_pvsol::compare::assumptions_like(f);
                            a.albedo = base.albedo;
                            a.soiling_loss = base.soiling_loss;
                            a.mismatch_loss = base.mismatch_loss;
                            a.dc_cable_loss = base.dc_cable_loss;
                            let c = compare_with(f, p, a)?;
                            let r = |k: &str| {
                                let r = c.rows.iter().find(|r| r.key == k)?;
                                Some((r.pvsol, r.pvstudio))
                            };
                            let (rp, rs) = r("reflection")?;
                            let (tp, ts) = r("temperature")?;
                            let (lp, ls) = r("lowLight")?;
                            let (shp, shs) = r("shading")?;
                            let (wm, _) = r("wiringMismatch")?;
                            let gap = c.energy_gap()?;
                            let model = (1.0 + gap) * (1.0 - shp / 100.0) * (1.0 - wm / 100.0)
                                / (1.0 - shs / 100.0)
                                - 1.0;
                            Some((rs - rp, ts - tp, model * 100.0, ls - lp))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    })
}

fn report(label: &str, g: &[(f64, f64, f64, f64)]) {
    let within = g.iter().filter(|x| x.2.abs() <= 3.0).count();
    println!(
        "{label:<22} reflection {:+.2} pt  temperature {:+.2} pt  low light {:+.2} pt  energy {:+.2} %  within 3 %: {within}/{}",
        median(g.iter().map(|x| x.0).collect()),
        median(g.iter().map(|x| x.1).collect()),
        median(g.iter().map(|x| x.3).collect()),
        median(g.iter().map(|x| x.2).collect()),
        g.len()
    );
}

fn main() {
    let dir = std::env::args().nth(1).expect("directory");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    let mut cases = Vec::new();
    for path in files {
        let Ok(file) = pv_pvsol::read(&std::fs::read(&path).unwrap()) else {
            continue;
        };
        let Ok(out) = pv_pvsol::import(&file, "x") else {
            continue;
        };
        let p = out.project;
        let wired: usize = p.wiring.strings.iter().map(|s| s.modules.len()).sum();
        let complete = p.modules.len() == file.config.module_count() as usize
            && wired == p.modules.len()
            && file.results.is_some()
            && file.weather.is_some();
        if complete {
            cases.push((file, p));
        }
    }
    println!("{} complete projects", cases.len());
    let d = SimulationAssumptions::default();
    report("current defaults", &gaps(&cases, d));
    for k in [0.025, 0.03, 0.035, 0.04, 0.05] {
        let a = SimulationAssumptions {
            low_light_a: k,
            ..d
        };
        report(&format!("low light {k}"), &gaps(&cases, a));
    }
}
