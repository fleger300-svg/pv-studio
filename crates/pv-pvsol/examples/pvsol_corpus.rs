//! Reads and imports every `.pvprj` under a directory and prints one line per project:
//! modules in PV*SOL's configuration, in its 3-D scene, imported, and wired, then the
//! import notes. Usage: `cargo run -p pv-pvsol --example pvsol_corpus -- <dir>`.

use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
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
    files.sort();
    let (mut ok, mut full) = (0, 0);
    let mut notes: std::collections::BTreeMap<String, usize> = Default::default();
    for (i, f) in files.iter().enumerate() {
        let bytes = std::fs::read(f).expect("read");
        let file = match pv_pvsol::read(&bytes) {
            Ok(x) => x,
            Err(e) => {
                println!("{i:3} READ ERROR {e}");
                continue;
            }
        };
        let cfg = file.config.module_count();
        let scene = file.scene.as_ref().map_or(0, pv_pvsol::Scene::module_count);
        match pv_pvsol::import(&file, "x") {
            Ok(o) => {
                ok += 1;
                if let Some(c) = pv_pvsol::compare(&file, &o.project) {
                    let r = |k: &str| {
                        c.rows
                            .iter()
                            .find(|r| r.key == k)
                            .map_or((0.0, 0.0), |r| (r.pvsol, r.pvstudio))
                    };
                    let pick = [
                        "ghi",
                        "poa",
                        "shading",
                        "reflection",
                        "temperature",
                        "inverter",
                        "clipping",
                        "wiringMismatch",
                        "specificYield",
                    ];
                    let cells: Vec<String> = pick
                        .iter()
                        .map(|k| {
                            let (a, b) = r(k);
                            format!("{k} {a:.1}/{b:.1}")
                        })
                        .collect();
                    println!(
                        "{i:3} COMPARE gap {:+.1} % same_weather {} | {}",
                        c.energy_gap().unwrap_or(f64::NAN) * 100.0,
                        c.same_weather,
                        cells.join(" ")
                    );
                    // DETAIL=<text>: everything about the projects whose path contains it.
                    if std::env::var("DETAIL")
                        .is_ok_and(|d| f.display().to_string().contains(d.as_str()))
                    {
                        for a in &file.config.areas {
                            println!("    area {a:?}");
                        }
                        println!("    pvsol {:#?}", file.results);
                        println!("    assumptions {:?}", c.assumptions);
                        for r in &c.rows {
                            println!(
                                "    {} {:.2} / {:.2} {}",
                                r.key, r.pvsol, r.pvstudio, r.unit
                            );
                        }
                        for a in &o.project.arrays {
                            let face = o.project.roof_face(a.roof_face_id).unwrap();
                            println!(
                                "    array tilt {:.1} az {:.1} on-roof {:.1}",
                                face.tilt_deg, face.azimuth_deg, a.tilt_on_roof_deg
                            );
                        }
                    }
                }
                let imported = o.project.modules.len();
                let wired: usize = o
                    .project
                    .wiring
                    .strings
                    .iter()
                    .map(|s| s.modules.len())
                    .sum();
                if imported == cfg as usize && wired == imported {
                    full += 1;
                }
                for n in &o.notes {
                    *notes.entry(n.key.clone()).or_default() += 1;
                }
                let keys: Vec<&str> = o.notes.iter().map(|n| n.key.as_str()).collect();
                println!(
                    "{i:3} cfg {cfg:4} scene {scene:4} imported {imported:4} wired {wired:4} inv {} | {} | {}",
                    o.project.wiring.inverters.len(),
                    keys.join(" "),
                    f.display()
                );
            }
            Err(e) => println!("{i:3} IMPORT ERROR {} {:?}", e.key, e.params),
        }
    }
    println!(
        "\n{} files, {ok} imported, {full} complete (all modules, all wired)",
        files.len()
    );
    for (k, n) in notes {
        println!("  {k}: {n}");
    }
}
