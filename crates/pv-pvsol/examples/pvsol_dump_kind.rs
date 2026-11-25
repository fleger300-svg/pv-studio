//! Prints the raw XML tree of the 3-D scene objects of one PV*SOL type, to decode a layout.
//! Usage: `cargo run -p pv-pvsol --example pvsol_dump_kind -- <dir or .pvprj> <type> [max]`.

use std::path::{Path, PathBuf};

use pv_pvsol::xml::Node;

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

fn dump(n: &Node, depth: usize) {
    let text = n.text.trim();
    println!(
        "{}{}{}",
        "  ".repeat(depth),
        n.name,
        if text.is_empty() {
            String::new()
        } else {
            format!(" = {text}")
        }
    );
    for c in &n.children {
        dump(c, depth + 1);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).expect("directory");
    let kind: f64 = args.get(2).expect("type").parse().expect("type");
    let max: usize = args.get(3).and_then(|m| m.parse().ok()).unwrap_or(1);
    let mut files = Vec::new();
    walk(Path::new(dir), &mut files);
    let mut shown = 0;
    for f in &files {
        let Ok(a) = pv_pvsol::Archive::from_bytes(&std::fs::read(f).unwrap_or_default()) else {
            continue;
        };
        let name = "Visu3D/GeometrischeDaten.xml";
        let Some(Ok(doc)) = a.get(name).map(|b| pv_pvsol::xml::parse(name, b)) else {
            continue;
        };
        let Some(szene) = doc.child("Szene") else {
            continue;
        };
        for o in szene.children("ZeichenObjekt") {
            if o.f64_at("StandardDaten/AnwObjTyp") == Some(kind) {
                println!("==== {}", f.display());
                dump(o, 0);
                shown += 1;
                if shown >= max {
                    return;
                }
            }
        }
    }
}
