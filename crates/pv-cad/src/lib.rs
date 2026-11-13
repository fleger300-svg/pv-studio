//! Plan drawing of a project as DXF (AutoCAD R12 ASCII: readable by every CAD program).
//!
//! Plan view in metres (x east, y north, LV95-relative when the project is located): roof
//! points are projected with their face's frame. Layers have English names (decision of
//! 2026-11) and fixed colours (ACI):
//!
//! | Layer | Content | Colour |
//! |---|---|---|
//! | `BUILDING` | building footprints | 7 (white/black) |
//! | `ROOF_FACE` | roof face outlines | 8 (grey) |
//! | `PV_MODULE` | active modules | 5 (blue) |
//! | `PV_STRING` | string label of each module (`WR1.2.3`: inverter, MPPT, string) | 1 (red) |
//! | `RAIL` | rails (top and bottom) | 30 (orange) |
//! | `FIXATION` | roof fixations (circles Ø 6 cm) | 6 (magenta) |
//! | `TEXT` | names of buildings and faces | 7 |

use std::collections::HashMap;
use std::fmt::Write;

use pv_layout::model::ArrayHardware;
use pv_model::{ArrayId, ModuleRef, ModuleState, Project, RoofFace, Vec3};

/// Layers, with their colour.
pub const LAYERS: [(&str, u8); 7] = [
    ("BUILDING", 7),
    ("ROOF_FACE", 8),
    ("PV_MODULE", 5),
    ("PV_STRING", 1),
    ("RAIL", 30),
    ("FIXATION", 6),
    ("TEXT", 7),
];

struct Dxf {
    out: String,
}

impl Dxf {
    fn pair(&mut self, code: u16, value: impl std::fmt::Display) {
        let _ = write!(self.out, "{code}\n{value}\n");
    }

    fn num(&mut self, code: u16, v: f64) {
        self.pair(code, format!("{v:.4}"));
    }

    fn polyline(&mut self, layer: &str, points: &[[f64; 2]], closed: bool) {
        if points.len() < 2 {
            return;
        }
        self.pair(0, "POLYLINE");
        self.pair(8, layer);
        self.pair(66, 1);
        self.pair(70, u8::from(closed));
        for p in points {
            self.pair(0, "VERTEX");
            self.pair(8, layer);
            self.num(10, p[0]);
            self.num(20, p[1]);
        }
        self.pair(0, "SEQEND");
        self.pair(8, layer);
    }

    fn line(&mut self, layer: &str, a: [f64; 2], b: [f64; 2]) {
        self.pair(0, "LINE");
        self.pair(8, layer);
        self.num(10, a[0]);
        self.num(20, a[1]);
        self.num(11, b[0]);
        self.num(21, b[1]);
    }

    fn circle(&mut self, layer: &str, c: [f64; 2], r: f64) {
        self.pair(0, "CIRCLE");
        self.pair(8, layer);
        self.num(10, c[0]);
        self.num(20, c[1]);
        self.num(40, r);
    }

    fn text(&mut self, layer: &str, at: [f64; 2], height: f64, text: &str) {
        self.pair(0, "TEXT");
        self.pair(8, layer);
        self.num(10, at[0]);
        self.num(20, at[1]);
        self.num(40, height);
        // DXF R12 is not Unicode: keep ASCII.
        let ascii: String = text
            .chars()
            .map(|c| if c.is_ascii() { c } else { '?' })
            .collect();
        self.pair(1, ascii);
        self.pair(72, 1);
        self.num(11, at[0]);
        self.num(21, at[1]);
    }
}

fn plan(face: &RoofFace, u: f64, v: f64) -> [f64; 2] {
    let f = face.frame();
    let p: Vec3 = [
        f.origin[0] + u * f.axis_u[0] + v * f.axis_v[0],
        f.origin[1] + u * f.axis_u[1] + v * f.axis_v[1],
        0.0,
    ];
    [p[0], p[1]]
}

/// The project's plan as DXF text. `hardware` gives the rails and fixations of the rail
/// fields (from the pipeline, with the statics' fixation spacing).
#[must_use]
pub fn plan_dxf(project: &Project, hardware: &HashMap<ArrayId, ArrayHardware>) -> String {
    let mut d = Dxf { out: String::new() };
    // Header and layer table.
    d.pair(0, "SECTION");
    d.pair(2, "HEADER");
    d.pair(9, "$INSUNITS");
    d.pair(70, 6); // metres
    d.pair(0, "ENDSEC");
    d.pair(0, "SECTION");
    d.pair(2, "TABLES");
    d.pair(0, "TABLE");
    d.pair(2, "LAYER");
    d.pair(70, LAYERS.len());
    for (name, colour) in LAYERS {
        d.pair(0, "LAYER");
        d.pair(2, name);
        d.pair(70, 0);
        d.pair(62, colour);
        d.pair(6, "CONTINUOUS");
    }
    d.pair(0, "ENDTAB");
    d.pair(0, "ENDSEC");
    d.pair(0, "SECTION");
    d.pair(2, "ENTITIES");

    for b in &project.buildings {
        let pts: Vec<[f64; 2]> = b
            .footprint
            .iter()
            .map(|p| [b.origin[0] + p[0], b.origin[1] + p[1]])
            .collect();
        d.polyline("BUILDING", &pts, true);
        if let Some(c) = centroid(&pts) {
            d.text("TEXT", c, 0.5, &b.name);
        }
    }
    for f in &project.roof_faces {
        let uv: Vec<[f64; 2]> = f.outline.as_ref().map_or_else(
            || {
                vec![
                    [0.0, 0.0],
                    [f.width_m, 0.0],
                    [f.width_m, f.height_m],
                    [0.0, f.height_m],
                ]
            },
            |o| o.vertices_uv.clone(),
        );
        let pts: Vec<[f64; 2]> = uv.iter().map(|p| plan(f, p[0], p[1])).collect();
        d.polyline("ROOF_FACE", &pts, true);
        if let Some(c) = centroid(&pts) {
            d.text("TEXT", [c[0], c[1] - 0.6], 0.3, &f.name);
        }
    }

    // String label of each module position.
    let mut label: HashMap<ModuleRef, String> = HashMap::new();
    for (ii, inv) in project.wiring.inverters.iter().enumerate() {
        let mut ordinal: HashMap<u32, u32> = HashMap::new();
        for s in project
            .wiring
            .strings
            .iter()
            .filter(|s| s.inverter == inv.id)
        {
            let k = ordinal.entry(s.tracker).or_insert(0);
            *k += 1;
            for m in &s.modules {
                label.insert(*m, format!("WR{}.{}.{}", ii + 1, s.tracker + 1, k));
            }
        }
    }

    for a in &project.arrays {
        let Some(face) = project.roof_faces.iter().find(|f| f.id == a.roof_face_id) else {
            continue;
        };
        let (hu, hv) = a.half_footprint();
        for m in project
            .modules
            .iter()
            .filter(|m| m.array_id == a.id && m.state == ModuleState::Active)
        {
            let corners = [
                plan(face, m.u - hu, m.v - hv),
                plan(face, m.u + hu, m.v - hv),
                plan(face, m.u + hu, m.v + hv),
                plan(face, m.u - hu, m.v + hv),
            ];
            d.polyline("PV_MODULE", &corners, true);
            let r = ModuleRef {
                array: a.id,
                row: m.row,
                col: m.col,
            };
            if let Some(l) = label.get(&r) {
                d.text("PV_STRING", plan(face, m.u, m.v), 0.15, l);
            }
        }
        if let Some(hw) = hardware.get(&a.id) {
            for rail in hw.rails.top.iter().chain(
                hw.fixations
                    .as_ref()
                    .map_or(&hw.rails.bottom, |f| &f.bottom)
                    .iter(),
            ) {
                d.line(
                    "RAIL",
                    plan(face, rail.start.u, rail.start.v),
                    plan(face, rail.end.u, rail.end.v),
                );
            }
            if let Some(fx) = &hw.fixations {
                for (rail, us) in fx.bottom.iter().zip(&fx.positions) {
                    for &u in us {
                        d.circle("FIXATION", plan(face, u, rail.start.v), 0.03);
                    }
                }
            }
        }
    }
    d.pair(0, "ENDSEC");
    d.pair(0, "EOF");
    d.out
}

fn centroid(pts: &[[f64; 2]]) -> Option<[f64; 2]> {
    if pts.is_empty() {
        return None;
    }
    let n = pts.len() as f64;
    Some([
        pts.iter().map(|p| p[0]).sum::<f64>() / n,
        pts.iter().map(|p| p[1]).sum::<f64>() / n,
    ])
}
