//! Lists the decoded 3-D scene of one project: shading objects (type, name, reference index,
//! object it stands on), parametric buildings, formations (on which reference object and
//! face) and the objects not decoded.
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_scene_list -- <file.pvprj>`.

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let file = pv_pvsol::read(&std::fs::read(path).expect("read")).expect("parse");
    let s = file.scene.expect("scene");
    for o in &s.objects {
        let base = match &o.shape {
            pv_pvsol::geometry::SceneShape::Prism {
                base_m, height_m, ..
            } => {
                format!(" | base {base_m:.2} height {height_m:.2}")
            }
            _ => String::new(),
        };
        println!(
            "object type {:3} ref {:?} on {} | {}{base}",
            o.kind, o.reference_index, o.on_object, o.name
        );
    }
    for b in &s.buildings {
        println!(
            "building ref {} | {} | ground {:.2} eave {:.2}",
            b.reference_index, b.name, b.ground_m, b.eave_height_m
        );
    }
    for f in &s.formations {
        println!("formation on {:?} | {} modules", f.face, f.modules.len());
    }
    for r in &s.row_formations {
        println!(
            "row formation on {:?} | {} modules",
            r.face,
            r.module_count()
        );
    }
    for o in &s.others {
        println!("other type {:3} | {}", o.kind, o.name);
    }
}
