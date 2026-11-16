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
