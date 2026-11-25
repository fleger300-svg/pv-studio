//! Checks the imported dormers over a corpus: whether each one stands on a roof face of the
//! project (its storey's front corners on the face's plane) and how high it rises above it.
//! Usage: `cargo run -p pv-pvsol --release --example pvsol_dormers -- <dir>`.

use std::path::{Path, PathBuf};

use pv_model::{ShadingObjectKind, Vec3};

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

fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + n - 1) % n]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            c = !c;
        }
    }
    c
}

fn main() {
    let dir = std::env::args().nth(1).expect("directory");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    let (mut undecoded, mut storeys, mut on_face, mut faces) = (0, 0, 0, 0);
    for path in &files {
        let Ok(file) = pv_pvsol::read(&std::fs::read(path).unwrap_or_default()) else {
            continue;
        };
        let Some(scene) = &file.scene else { continue };
        undecoded += scene
            .others
            .iter()
            .filter(|o| matches!(o.kind, 40 | 42 | 43))
            .count();
        if !scene.objects.iter().any(|o| matches!(o.kind, 40 | 42 | 43)) {
            continue;
        }
        let Ok(out) = pv_pvsol::import(&file, "x") else {
            continue;
        };
        let p = out.project;
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let dormers: Vec<_> = p
            .site
            .objects
            .iter()
            .filter(|o| o.kind == ShadingObjectKind::Dormer)
            .collect();
        faces += dormers.iter().filter(|o| o.height_m == 0.0).count();
        for o in dormers.iter().filter(|o| o.height_m > 0.0) {
            storeys += 1;
            // Face under the storey's front edge (its first two corners).
            let front: Vec<Vec3> = o.outline[..2].to_vec();
            let mid = [
                (front[0][0] + front[1][0]) / 2.0,
                (front[0][1] + front[1][1]) / 2.0,
                (front[0][2] + front[1][2]) / 2.0,
            ];
            let best = p
                .roof_faces
                .iter()
                .filter_map(|f| {
                    let fr = f.frame();
                    let (u, v, w) = fr.to_local(mid);
                    let poly: Vec<[f64; 2]> = f.outline.as_ref().map_or_else(
                        || {
                            vec![
                                [0.0, 0.0],
                                [f.width_m, 0.0],
                                [f.width_m, f.height_m],
                                [0.0, f.height_m],
                            ]
                        },
                        |s| s.vertices_uv.clone(),
                    );
                    inside(&poly, [u, v]).then_some((w, f))
                })
                .min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
            let Some((w_front, f)) = best else {
                println!("{name} | {}: no roof face under it", o.name);
                continue;
            };
            let fr = f.frame();
            let top = p
                .site
                .objects
                .iter()
                .filter(|q| q.name == o.name && q.kind == ShadingObjectKind::Dormer)
                .flat_map(|q| {
                    let h = q.height_m;
                    q.outline.iter().map(move |c| [c[0], c[1], c[2] + h])
                })
                .map(|c| fr.to_local(c).2)
                .fold(f64::MIN, f64::max);
            if w_front.abs() < 0.3 {
                on_face += 1;
            }
            println!(
                "{name} | {}: front {w_front:+.2} m off the face (tilt {:.0}°, az {:.0}°), top {top:.2} m above",
                o.name, f.tilt_deg, f.azimuth_deg
            );
        }
    }
    println!(
        "storeys {storeys}, front on its face (±0.3 m) {on_face}, roof faces {faces}, undecoded dormers {undecoded}"
    );
}
