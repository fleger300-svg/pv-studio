//! The 3-D scene objects PV Studio does not import, over a corpus: by PV*SOL object type, how
//! many objects in how many projects, with example names. Usage:
//! `cargo run -p pv-pvsol --release --example pvsol_objects -- <dir>`.

use std::collections::BTreeMap;
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
    // kind → (objects, projects, names)
    let mut by_kind: BTreeMap<u32, (usize, usize, BTreeMap<String, usize>)> = BTreeMap::new();
    for f in &files {
        let Ok(file) = pv_pvsol::read(&std::fs::read(f).unwrap_or_default()) else {
            continue;
        };
        let Some(scene) = &file.scene else {
            continue;
        };
        let mut seen = std::collections::BTreeSet::new();
        for o in &scene.others {
            let e = by_kind.entry(o.kind).or_default();
            e.0 += 1;
            if seen.insert(o.kind) {
                e.1 += 1;
            }
            *e.2.entry(o.name.clone()).or_default() += 1;
        }
    }
    println!("{} files", files.len());
    for (k, (n, projects, names)) in &by_kind {
        let mut top: Vec<_> = names.iter().collect();
        top.sort_by(|a, b| b.1.cmp(a.1));
        let ex: Vec<String> = top
            .iter()
            .take(6)
            .map(|(n, c)| format!("{n} ×{c}"))
            .collect();
        println!(
            "type {k:3}: {n:5} objects in {projects:3} projects | {}",
            ex.join(", ")
        );
    }
}
