//! Unequal strings on one tracker, PV Studio vs PV*SOL: for every completely imported project
//! of the corpus simulated with PV*SOL's weather, both losses and the energy gap once only the
//! shading of both is taken out. Usage:
//! `cargo run -p pv-pvsol --release --example pvsol_wiring -- <dir>`.

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

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied().unwrap_or(f64::NAN)
}

/// (PV*SOL loss %, PV Studio loss %, energy gap without shading %, name, PV*SOL near shading
/// %, trackers with strings of unequal lengths, trackers with several strings).
type Case = (f64, f64, f64, String, f64, usize, usize);

fn case(path: &Path) -> Option<Case> {
    let file = pv_pvsol::read(&std::fs::read(path).ok()?).ok()?;
    let p = pv_pvsol::import(&file, "x").ok()?.project;
    let wired: usize = p.wiring.strings.iter().map(|s| s.modules.len()).sum();
    let complete = p.modules.len() == file.config.module_count() as usize
        && wired == p.modules.len()
        && file.results.is_some()
        && file.weather.is_some();
    if !complete {
        return None;
    }
    let c = pv_pvsol::compare(&file, &p)?;
    let r = |k: &str| {
        c.rows
            .iter()
            .find(|r| r.key == k)
            .map(|r| (r.pvsol, r.pvstudio))
    };
    let (wp, ws) = r("wiringMismatch")?;
    let (shp, shs) = r("shading")?;
    let gap = c.energy_gap()?;
    let model = (1.0 + gap) * (1.0 - shp / 100.0) / (1.0 - shs / 100.0) - 1.0;
    let name = path.file_name()?.to_string_lossy().into_owned();
    let (near, _) = r("nearShading")?;
    let mut lengths: std::collections::BTreeMap<(String, u32), Vec<usize>> = Default::default();
    for s in &p.wiring.strings {
        lengths
            .entry((format!("{:?}", s.inverter), s.tracker))
            .or_default()
            .push(s.modules.len());
    }
    let parallel = lengths.values().filter(|l| l.len() > 1).count();
    let unequal = lengths
        .values()
        .filter(|l| l.iter().any(|&n| n != l[0]))
        .count();
    Some((wp, ws, model * 100.0, name, near, unequal, parallel))
}

fn main() {
    let dir = std::env::args().nth(1).expect("directory");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();
    let chunk = files.len().div_ceil(16).max(1);
    let cases: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = files
            .chunks(chunk)
            .map(|part| s.spawn(move || part.iter().filter_map(|f| case(f)).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    for (wp, ws, gap, name, near, unequal, parallel) in
        cases.iter().filter(|c| c.0 > 0.05 || c.1 > 0.05)
    {
        println!(
            "PV*SOL {wp:5.2} %  PV Studio {ws:5.2} %  near shading {near:5.1} %  trackers unequal {unequal}/{parallel}  energy {gap:+5.1} %  {name}"
        );
    }
    let n = cases.len();
    let affected: Vec<_> = cases.iter().filter(|c| c.0 > 0.05 || c.1 > 0.05).collect();
    println!(
        "\n{n} complete projects, {} with unequal strings",
        affected.len()
    );
    println!(
        "loss: PV*SOL median {:.2} %, PV Studio median {:.2} % (affected); gap median {:+.2} pt, |gap| ≤ 0.5 pt: {}",
        median(affected.iter().map(|c| c.0).collect()),
        median(affected.iter().map(|c| c.1).collect()),
        median(affected.iter().map(|c| c.1 - c.0).collect()),
        affected.iter().filter(|c| (c.1 - c.0).abs() <= 0.5).count()
    );
    println!(
        "energy without shading: median {:+.2} %, within ±3 %: {}/{n}",
        median(cases.iter().map(|c| c.2).collect()),
        cases.iter().filter(|c| c.2.abs() <= 3.0).count()
    );
}
