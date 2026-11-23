//! Near shading by the project's own geometry — roof faces, walls of every building and roof
//! obstacles (prisms) — and by its surroundings (height grid of neighbouring buildings and
//! terrain, [`pv_model::Surroundings`]) cast shadows on the modules.
//!
//! - Beam: for each sun direction bin (5° × 5°), every module is sampled at 12 points on its
//!   surface — 3 strips across its short side (one per bypass diode) × 4 bands along its long
//!   side (two per half of a half-cell module) — giving a shade mask per module
//!   ([`ShadeModel::module_masks`]) and the shaded fraction of the array (share of blocked
//!   rays). Bins are computed lazily, only for the directions the sun actually takes.
//! - Diffuse: the sky view factor of each array (cosine-weighted hemisphere sampling around
//!   the module normal), averaged over its modules.
//!
//! Tilted modules (rows on flat roofs) shade each other. The fraction is linear in the
//! shaded area; the electrical effect of partial shading (bypass diodes) is computed from the
//! masks by the simulation ([`crate::parallel`]).

use std::collections::HashMap;

use pv_model::{ArrayId, BuildingId, ModuleRef, ModuleState, Project, RoofFaceId};

/// Sample points per module: bit `strip * BANDS + band` of a shade mask.
pub const STRIPS: usize = 3;
pub const BANDS: usize = 4;
pub const POINTS: usize = STRIPS * BANDS;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// A triangle that casts shadows, with the face it belongs to (a face never shades itself).
struct Tri {
    a: V3,
    e1: V3,
    e2: V3,
    face: Option<RoofFaceId>,
    /// Walls: their building (a building's walls never shade its own roof: they stand
    /// under it).
    wall_of: Option<BuildingId>,
    /// Tilted modules: index of their footprint (a module never shades a point under it).
    module: Option<usize>,
    /// A tree's crown: lets part of the light through (see [`ShadeModel::crown_opacity`]).
    crown: bool,
}

impl Tri {
    fn new(a: V3, b: V3, c: V3) -> Self {
        Self {
            a,
            e1: sub(b, a),
            e2: sub(c, a),
            face: None,
            wall_of: None,
            module: None,
            crown: false,
        }
    }
}

/// What stops a ray.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Clear,
    /// Only trees' crowns (partly transparent).
    Crown,
    Opaque,
}

/// Is the outline (projected on the horizontal plane) convex?
fn convex(points: &[V3]) -> bool {
    let n = points.len();
    let mut sign = 0.0;
    for i in 0..n {
        let (a, b, c) = (points[i], points[(i + 1) % n], points[(i + 2) % n]);
        let z = (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]);
        if z.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = z.signum();
        } else if z.signum() != sign {
            return false;
        }
    }
    true
}

/// Triangles of a flat, simple polygon (ear clipping in its own plane).
fn triangulate(points: &[V3], out: &mut Vec<Tri>) {
    let n = points.len();
    if n < 3 {
        return;
    }
    // Normal (Newell), then a 2-D frame in the plane.
    let mut nrm = [0.0; 3];
    for i in 0..n {
        let (a, b) = (points[i], points[(i + 1) % n]);
        nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    let len = dot(nrm, nrm).sqrt();
    if len < 1e-12 {
        return;
    }
    let nrm = mul(nrm, 1.0 / len);
    let helper = if nrm[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let ex = cross(helper, nrm);
    let ex = mul(ex, 1.0 / dot(ex, ex).sqrt());
    let ey = cross(nrm, ex);
    let p2: Vec<[f64; 2]> = points.iter().map(|p| [dot(*p, ex), dot(*p, ey)]).collect();
    let area: f64 = (0..n)
        .map(|i| {
            let (a, b) = (p2[i], p2[(i + 1) % n]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    let ccw = area > 0.0;
    let cross2 = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut idx: Vec<usize> = (0..n).collect();
    let mut guard = 0;
    while idx.len() > 3 && guard < n * n {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for k in 0..m {
            let (i, j, l) = (idx[(k + m - 1) % m], idx[k], idx[(k + 1) % m]);
            let turn = cross2(p2[i], p2[j], p2[l]);
            if (turn > 0.0) != ccw || turn.abs() < 1e-12 {
                continue;
            }
            let inside = idx.iter().any(|&q| {
                q != i && q != j && q != l && {
                    let (a, b, c) = (
                        cross2(p2[i], p2[j], p2[q]),
                        cross2(p2[j], p2[l], p2[q]),
                        cross2(p2[l], p2[i], p2[q]),
                    );
                    (a >= 0.0 && b >= 0.0 && c >= 0.0) || (a <= 0.0 && b <= 0.0 && c <= 0.0)
                }
            });
            if !inside {
                out.push(Tri::new(points[i], points[j], points[l]));
                idx.remove(k);
                clipped = true;
                break;
            }
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        out.push(Tri::new(points[idx[0]], points[idx[1]], points[idx[2]]));
    }
}

/// Occluder triangles of a shading object: the polygon itself, or the walls of the extruded
/// outline (with top and bottom when it is convex: a crown or a box seen from below); a
/// tree's crown is the spheroid inscribed in its extruded outline.
fn object_tris(o: &pv_model::ShadingObject, out: &mut Vec<Tri>) {
    let base = &o.outline;
    if o.height_m <= 0.0 {
        triangulate(base, out);
        return;
    }
    let up = [0.0, 0.0, o.height_m];
    if !o.opacity_by_month.is_empty() {
        let n = base.len() as f64;
        let c = [
            base.iter().map(|p| p[0]).sum::<f64>() / n,
            base.iter().map(|p| p[1]).sum::<f64>() / n,
            base.iter().map(|p| p[2]).sum::<f64>() / n,
        ];
        let r = base
            .iter()
            .map(|p| (p[0] - c[0]).hypot(p[1] - c[1]))
            .sum::<f64>()
            / n;
        const SIDES: usize = 8;
        const RINGS: usize = 4;
        let ring = |k: usize| -> Vec<V3> {
            let f = k as f64 / RINGS as f64;
            let z = c[2] + f * o.height_m;
            let rr = r * (1.0 - (2.0 * f - 1.0).powi(2)).max(0.0).sqrt();
            (0..SIDES)
                .map(|s| {
                    let a = std::f64::consts::TAU * s as f64 / SIDES as f64;
                    [c[0] + rr * a.cos(), c[1] + rr * a.sin(), z]
                })
                .collect()
        };
        let start = out.len();
        for k in 0..RINGS {
            let (lo, hi) = (ring(k), ring(k + 1));
            for s in 0..SIDES {
                let t = (s + 1) % SIDES;
                out.push(Tri::new(lo[s], lo[t], hi[t]));
                out.push(Tri::new(lo[s], hi[t], hi[s]));
            }
        }
        for t in &mut out[start..] {
            t.crown = true;
        }
        return;
    }
    let top: Vec<V3> = base.iter().map(|&p| add(p, up)).collect();
    let n = base.len();
    for i in 0..n {
        let j = (i + 1) % n;
        out.push(Tri::new(base[i], base[j], top[j]));
        out.push(Tri::new(base[i], top[j], top[i]));
    }
    if n >= 3 && convex(base) {
        triangulate(base, out);
        triangulate(&top, out);
    }
}

/// Möller–Trumbore: does the ray `o + t·d` (t > ε) hit the triangle?
fn hits(t: &Tri, o: V3, d: V3) -> bool {
    let p = cross(d, t.e2);
    let det = dot(t.e1, p);
    if det.abs() < 1e-12 {
        return false;
    }
    let inv = 1.0 / det;
    let s = sub(o, t.a);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, t.e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    dot(t.e2, q) * inv > 1e-3
}

/// A run of triangles with their bounding box: rays that miss the box skip them all.
struct Cluster {
    lo: V3,
    hi: V3,
    start: usize,
    end: usize,
}

/// Closes the cluster of the triangles pushed since `start`.
fn close(tris: &[Tri], start: usize, clusters: &mut Vec<Cluster>) {
    if start >= tris.len() {
        return;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for t in &tris[start..] {
        for p in [t.a, add(t.a, t.e1), add(t.a, t.e2)] {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k] - 1e-6);
                hi[k] = hi[k].max(p[k] + 1e-6);
            }
        }
    }
    clusters.push(Cluster {
        lo,
        hi,
        start,
        end: tris.len(),
    });
}

/// Does the ray `o + t·d` (t > 0) cross the box?
fn crosses(c: &Cluster, o: V3, d: V3) -> bool {
    let (mut t0, mut t1) = (0.0_f64, f64::INFINITY);
    for k in 0..3 {
        if d[k].abs() < 1e-12 {
            if o[k] < c.lo[k] || o[k] > c.hi[k] {
                return false;
            }
        } else {
            let inv = 1.0 / d[k];
            let (a, b) = ((c.lo[k] - o[k]) * inv, (c.hi[k] - o[k]) * inv);
            let (a, b) = if a < b { (a, b) } else { (b, a) };
            t0 = t0.max(a);
            t1 = t1.min(b);
            if t0 > t1 {
                return false;
            }
        }
    }
    true
}

fn fan(points: &[V3], face: Option<RoofFaceId>, wall_of: Option<BuildingId>, out: &mut Vec<Tri>) {
    for i in 1..points.len().saturating_sub(1) {
        out.push(Tri {
            a: points[0],
            e1: sub(points[i], points[0]),
            e2: sub(points[i + 1], points[0]),
            face,
            wall_of,
            module: None,
            crown: false,
        });
    }
}

/// Unit vector towards the sun (x east, y north, z up).
#[must_use]
pub fn sun_vector(azimuth_deg: f64, elevation_deg: f64) -> V3 {
    let (az, el) = (azimuth_deg.to_radians(), elevation_deg.to_radians());
    [az.sin() * el.cos(), az.cos() * el.cos(), el.sin()]
}

/// Index of the 5° × 5° bin of a sun direction.
#[must_use]
pub fn bin_of(azimuth_deg: f64, elevation_deg: f64) -> (u16, u16) {
    (
        (azimuth_deg.rem_euclid(360.0) / 5.0) as u16 % 72,
        (elevation_deg.clamp(0.0, 89.999) / 5.0) as u16,
    )
}

/// The surroundings height grid as an occluder. Cells under the project's own buildings (and
/// within [`OWN_MARGIN_M`] of their outline) are ignored: the survey holds the project's own
/// roof — or what stood there before — which the project's own geometry replaces.
struct Field {
    west: f64,
    north: f64,
    cell: f64,
    cols: usize,
    rows: usize,
    /// Heights [m]; `NEG_INFINITY` for ignored cells.
    z: Vec<f64>,
    top: f64,
}

/// Distance around the project's buildings within which the survey is ignored [m].
const OWN_MARGIN_M: f64 = 0.75;

fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1])
        {
            c = !c;
        }
    }
    c
}

fn segment_distance(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 {
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dy)
}

impl Field {
    fn new(project: &Project) -> Option<Self> {
        let s = project.site.surroundings.as_ref()?;
        let footprints: Vec<Vec<[f64; 2]>> = project
            .buildings
            .iter()
            .map(|b| {
                b.footprint
                    .iter()
                    .map(|p| [b.origin[0] + p[0], b.origin[1] + p[1]])
                    .collect()
            })
            .collect();
        let (cols, rows) = (s.cols as usize, s.rows as usize);
        let mut z = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                let p = [
                    s.origin_east_m + (c as f64 + 0.5) * s.cell_m,
                    s.origin_north_m - (r as f64 + 0.5) * s.cell_m,
                ];
                let own = footprints.iter().any(|f| {
                    inside(f, p)
                        || (0..f.len()).any(|i| {
                            segment_distance(f[i], f[(i + 1) % f.len()], p)
                                < OWN_MARGIN_M + s.cell_m / 2.0
                        })
                });
                let h = s.heights_cm[r * cols + c];
                z.push(if own || h == pv_model::Surroundings::CUT {
                    f64::NEG_INFINITY
                } else {
                    f64::from(h) / 100.0
                });
            }
        }
        let top = z.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        Some(Self {
            west: s.origin_east_m,
            north: s.origin_north_m,
            cell: s.cell_m,
            cols,
            rows,
            z,
            top,
        })
    }

    /// Does the ray from `o` towards `d` (pointing upwards) pass under the surface?
    fn blocks(&self, o: V3, d: V3) -> bool {
        let horizontal = d[0].hypot(d[1]);
        if d[2] <= 0.0 || horizontal < 1e-9 {
            return false;
        }
        let (ux, uy, rise) = (d[0] / horizontal, d[1] / horizontal, d[2] / horizontal);
        let step = self.cell / 2.0;
        let mut s = step;
        loop {
            let z = o[2] + s * rise;
            if z > self.top {
                return false;
            }
            let (e, n) = (o[0] + s * ux, o[1] + s * uy);
            let c = ((e - self.west) / self.cell).floor();
            let r = ((self.north - n) / self.cell).floor();
            if c < 0.0 || r < 0.0 || c >= self.cols as f64 || r >= self.rows as f64 {
                return false;
            }
            if self.z[r as usize * self.cols + c as usize] > z {
                return true;
            }
            s += step;
        }
    }
}

struct Receiver {
    face: RoofFaceId,
    building: BuildingId,
    /// Sample points per module (see [`POINTS`]), lifted off its surface.
    modules: Vec<[V3; POINTS]>,
    /// Module centres, lifted off their surface.
    centres: Vec<V3>,
    /// Index in `modules` of each `(row, col)`.
    index: HashMap<(u32, u32), usize>,
    /// Module normal.
    normal: V3,
}

/// Sample point offsets in the module's own frame `(x across, y along slope)`, by mask bit.
fn sample_offsets(x: f64, y: f64) -> [[f64; 2]; POINTS] {
    let long_is_y = y >= x;
    let (short, long) = if long_is_y { (x, y) } else { (y, x) };
    let mut out = [[0.0; 2]; POINTS];
    for s in 0..STRIPS {
        for b in 0..BANDS {
            let across = short * ((s as f64 + 0.5) / STRIPS as f64 - 0.5);
            let along = long * ((b as f64 + 0.5) / BANDS as f64 - 0.5);
            out[s * BANDS + b] = if long_is_y {
                [across, along]
            } else {
                [along, across]
            };
        }
    }
    out
}

/// The tilted modules of one row: corners and footprint index.
type RowModules = Vec<([V3; 4], usize)>;

/// The beam's shade on an array for one sun bin: shaded fractions and the mask of each of its
/// modules, by opaque occluders and by trees' crowns only.
#[derive(Default)]
struct BeamShade {
    opaque: f64,
    crown: f64,
    opaque_masks: Vec<u16>,
    crown_masks: Vec<u16>,
}

/// Shading of every array of a project.
pub struct ShadeModel {
    tris: Vec<Tri>,
    clusters: Vec<Cluster>,
    footprints: Vec<[[f64; 2]; 4]>,
    horizon: Option<pv_model::Horizon>,
    horizon_views: HashMap<ArrayId, f64>,
    /// Frame and extent of every roof face.
    planes: HashMap<RoofFaceId, (pv_model::RoofFrame, f64, f64)>,
    field: Option<Field>,
    receivers: HashMap<ArrayId, Receiver>,
    /// Shaded fraction and module masks per array and sun bin.
    beam: HashMap<(ArrayId, (u16, u16)), BeamShade>,
    diffuse: HashMap<ArrayId, f64>,
    /// Share of the light the trees' crowns stop, by month (mean over the crowns).
    crown_opacity: [f64; 12],
}

impl ShadeModel {
    /// Builds the occluders and receivers of `project`.
    #[must_use]
    pub fn new(project: &Project) -> Self {
        let mut tris = Vec::new();
        let mut clusters = Vec::new();
        let mut planes = HashMap::new();
        for f in &project.roof_faces {
            let fr = f.frame();
            let world = |u: f64, v: f64| add(fr.origin, add(mul(fr.axis_u, u), mul(fr.axis_v, v)));
            let outline: Vec<[f64; 2]> = f.outline.as_ref().map_or_else(
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
            let pts: Vec<V3> = outline.iter().map(|p| world(p[0], p[1])).collect();
            let start = tris.len();
            fan(&pts, Some(f.id), None, &mut tris);
            close(&tris, start, &mut clusters);
            planes.insert(f.id, (fr, f.width_m, f.height_m));
            for o in f.outline.iter().flat_map(|o| &o.obstacles) {
                let start = tris.len();
                let base: Vec<V3> = o.footprint_uv.iter().map(|p| world(p[0], p[1])).collect();
                let top: Vec<V3> = base
                    .iter()
                    .map(|&p| add(p, mul(fr.normal, o.height_m)))
                    .collect();
                fan(&top, None, None, &mut tris);
                for i in 0..base.len() {
                    let j = (i + 1) % base.len();
                    fan(&[base[i], base[j], top[j], top[i]], None, None, &mut tris);
                }
                close(&tris, start, &mut clusters);
            }
        }
        for b in &project.buildings {
            let faces: Vec<&pv_model::RoofFace> = project
                .roof_faces
                .iter()
                .filter(|f| f.building_id == b.id)
                .collect();
            let start = tris.len();
            for w in pv_roof_walls(b, &faces) {
                let (s, e) = (w.start, w.end);
                let len = ((e[0] - s[0]).powi(2) + (e[1] - s[1]).powi(2))
                    .sqrt()
                    .max(1e-9);
                let at = |sd: f64, z: f64| -> V3 {
                    [
                        s[0] + (e[0] - s[0]) * sd / len,
                        s[1] + (e[1] - s[1]) * sd / len,
                        z,
                    ]
                };
                let mut poly: Vec<V3> = vec![at(0.0, w.base_z), at(len, w.base_z)];
                poly.extend(w.profile.iter().rev().map(|p| at(p[0], p[1])));
                fan(&poly, None, Some(b.id), &mut tris);
            }
            close(&tris, start, &mut clusters);
        }
        // Shading objects (trees, neighbouring buildings, roof fixtures, polygons).
        let mut crown_opacity = [0.0; 12];
        let mut crowns = 0.0;
        for o in &project.site.objects {
            let start = tris.len();
            object_tris(o, &mut tris);
            close(&tris, start, &mut clusters);
            if !o.opacity_by_month.is_empty() {
                crowns += 1.0;
                for (m, x) in crown_opacity.iter_mut().enumerate() {
                    *x += o.opacity(m);
                }
            }
        }
        if crowns > 0.0 {
            for x in &mut crown_opacity {
                *x /= crowns;
            }
        }
        let mut receivers = HashMap::new();
        // Footprints (x, y) of the tilted modules.
        let mut footprints: Vec<[[f64; 2]; 4]> = Vec::new();
        for a in &project.arrays {
            let Some(f) = project.roof_faces.iter().find(|f| f.id == a.roof_face_id) else {
                continue;
            };
            let fr = f.frame();
            let rot = a.rotation_in_roof();
            let [x, y, _] = a.local_size();
            let h = a.centre_offset();
            // Module-frame offset → world vector.
            let to_world = |l: V3| {
                let r = rot.rotate(l);
                add(
                    add(mul(fr.axis_u, r[0]), mul(fr.axis_v, r[1])),
                    mul(fr.normal, r[2]),
                )
            };
            let normal = to_world([0.0, 0.0, 1.0]);
            let lift = mul(normal, 0.05);
            let offsets = sample_offsets(x, y);
            let tilted = a.tilt_on_roof_deg.abs() > 1e-6;
            let mut modules = Vec::new();
            let mut centres = Vec::new();
            let mut index = HashMap::new();
            let mut rows: Vec<(u32, RowModules)> = Vec::new();
            for m in project
                .modules
                .iter()
                .filter(|m| m.array_id == a.id && m.state == ModuleState::Active)
            {
                let c = fr.to_world(m.u, m.v, h);
                index.insert((m.row, m.col), modules.len());
                modules.push(offsets.map(|o| add(add(c, to_world([o[0], o[1], 0.0])), lift)));
                centres.push(add(c, lift));
                // Tilted modules shade the rows behind them.
                if tilted {
                    let corners = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]]
                        .map(|k| add(c, to_world([k[0] * x / 2.0, k[1] * y / 2.0, 0.0])));
                    footprints.push(corners.map(|p| [p[0], p[1]]));
                    let entry = (corners, footprints.len() - 1);
                    match rows.iter_mut().find(|r| r.0 == m.row) {
                        Some(r) => r.1.push(entry),
                        None => rows.push((m.row, vec![entry])),
                    }
                }
            }
            for (_, row) in rows {
                let start = tris.len();
                for (corners, k) in row {
                    let first = tris.len();
                    fan(&corners, None, None, &mut tris);
                    for t in &mut tris[first..] {
                        t.module = Some(k);
                    }
                }
                close(&tris, start, &mut clusters);
            }
            receivers.insert(
                a.id,
                Receiver {
                    face: f.id,
                    building: f.building_id,
                    modules,
                    centres,
                    index,
                    normal,
                },
            );
        }
        Self {
            tris,
            clusters,
            footprints,
            horizon: project.site.horizon.clone(),
            horizon_views: HashMap::new(),
            planes,
            field: Field::new(project),
            receivers,
            beam: HashMap::new(),
            diffuse: HashMap::new(),
            crown_opacity,
        }
    }

    /// Is a sun at `azimuth_deg`, `elevation_deg` behind the far horizon?
    #[must_use]
    pub fn below_horizon(&self, azimuth_deg: f64, elevation_deg: f64) -> bool {
        self.horizon
            .as_ref()
            .is_some_and(|h| elevation_deg < h.elevation(azimuth_deg))
    }

    /// Share of the sky diffuse that the far horizon lets through to `array` (cosine-weighted
    /// around its normal; 1 without a horizon).
    pub fn horizon_view(&mut self, array: ArrayId) -> f64 {
        let Some(h) = &self.horizon else {
            return 1.0;
        };
        if let Some(&v) = self.horizon_views.get(&array) {
            return v;
        }
        let v = self.receivers.get(&array).map_or(1.0, |r| {
            let (mut seen, mut total) = (0.0, 0.0);
            for k in 0..45 {
                let el = f64::from(k) * 2.0 + 1.0;
                for j in 0..72 {
                    let az = f64::from(j) * 5.0 + 2.5;
                    let d = sun_vector(az, el);
                    let w = dot(d, r.normal).max(0.0) * el.to_radians().cos();
                    total += w;
                    if el >= h.elevation(az) {
                        seen += w;
                    }
                }
            }
            if total > 0.0 {
                seen / total
            } else {
                1.0
            }
        });
        self.horizon_views.insert(array, v);
        v
    }

    /// A roof face does not shade a point over its rectangle that is under it — inside the
    /// building (no module there) or in an opening of that roof (a dormer cut into it) — or
    /// just above it: a module lying on it, or on a mounting surface that doubles it (PV*SOL
    /// draws them a few centimetres off the roof, on either side).
    fn below(&self, face: Option<RoofFaceId>, o: V3) -> bool {
        face.and_then(|f| self.planes.get(&f))
            .is_some_and(|(fr, w, h)| {
                let (u, v, n) = fr.to_local(o);
                n < 0.5 && (0.0..=*w).contains(&u) && (0.0..=*h).contains(&v)
            })
    }

    fn hit(&self, r: &Receiver, o: V3, d: V3) -> Hit {
        let mut crown = false;
        for c in &self.clusters {
            if !crosses(c, o, d) {
                continue;
            }
            for t in &self.tris[c.start..c.end] {
                if t.face != Some(r.face)
                    && t.wall_of != Some(r.building)
                    && !t
                        .module
                        .is_some_and(|k| inside(&self.footprints[k], [o[0], o[1]]))
                    && !self.below(t.face, o)
                    && (!crown || !t.crown)
                    && hits(t, o, d)
                {
                    if !t.crown {
                        return Hit::Opaque;
                    }
                    crown = true;
                }
            }
        }
        if self.field.as_ref().is_some_and(|f| f.blocks(o, d)) {
            Hit::Opaque
        } else if crown {
            Hit::Crown
        } else {
            Hit::Clear
        }
    }

    fn beam_entry(&mut self, array: ArrayId, bin: (u16, u16)) -> &BeamShade {
        if !self.beam.contains_key(&(array, bin)) {
            let e = self
                .receivers
                .get(&array)
                .map_or_else(BeamShade::default, |r| {
                    let d = sun_vector(f64::from(bin.0) * 5.0 + 2.5, f64::from(bin.1) * 5.0 + 2.5);
                    let mut e = BeamShade::default();
                    for m in &r.modules {
                        let (mut opaque, mut crown) = (0u16, 0u16);
                        for (k, &p) in m.iter().enumerate() {
                            match self.hit(r, p, d) {
                                Hit::Opaque => opaque |= 1 << k,
                                Hit::Crown => crown |= 1 << k,
                                Hit::Clear => {}
                            }
                        }
                        e.opaque_masks.push(opaque);
                        e.crown_masks.push(crown);
                    }
                    let n = (r.modules.len() * POINTS).max(1) as f64;
                    let count = |masks: &[u16]| {
                        f64::from(masks.iter().map(|m| m.count_ones()).sum::<u32>()) / n
                    };
                    e.opaque = count(&e.opaque_masks);
                    e.crown = count(&e.crown_masks);
                    e
                });
            self.beam.insert((array, bin), e);
        }
        &self.beam[&(array, bin)]
    }

    /// Shaded fraction of the beam on `array` for a sun in `bin` (bin centre direction) in
    /// `month` (0 = January: the trees' crowns).
    pub fn beam_shade(&mut self, array: ArrayId, bin: (u16, u16), month: usize) -> f64 {
        let opacity = self.crown_opacity[month % 12];
        let e = self.beam_entry(array, bin);
        e.opaque + e.crown * opacity
    }

    /// Shade mask of module `index` of `array` (see [`Self::module_index`]) for a sun in `bin`
    /// in `month`: bit `strip * BANDS + band` set when that point is shaded. A point behind a
    /// crown counts as shaded when the crown stops most of the light.
    pub fn module_mask(
        &mut self,
        array: ArrayId,
        bin: (u16, u16),
        month: usize,
        index: usize,
    ) -> u16 {
        let dense = self.crown_opacity[month % 12] >= 0.5;
        let e = self.beam_entry(array, bin);
        let opaque = e.opaque_masks.get(index).copied().unwrap_or(0);
        let crown = e.crown_masks.get(index).copied().unwrap_or(0);
        if dense {
            opaque | crown
        } else {
            opaque
        }
    }

    /// Index of a module in its array's masks; `None` for an unknown or removed module.
    #[must_use]
    pub fn module_index(&self, m: ModuleRef) -> Option<usize> {
        self.receivers
            .get(&m.array)?
            .index
            .get(&(m.row, m.col))
            .copied()
    }

    /// Sky view factor of `array` (1 = unobstructed), cosine-weighted around its normal.
    pub fn diffuse_factor(&mut self, array: ArrayId) -> f64 {
        if let Some(&f) = self.diffuse.get(&array) {
            return f;
        }
        let crown_mean = self.crown_opacity.iter().sum::<f64>() / 12.0;
        let f = self.receivers.get(&array).map_or(1.0, |r| {
            let (mut seen, mut total) = (0.0, 0.0);
            // Centre points only (diffuse varies slowly); directions above the horizon.
            for &p in &r.centres {
                for el in (1..9).map(|k| f64::from(k) * 10.0 - 5.0) {
                    for az in (0..24).map(|k| f64::from(k) * 15.0) {
                        let d = sun_vector(az, el);
                        let w = dot(d, r.normal).max(0.0) * el.to_radians().cos();
                        if w <= 0.0 {
                            continue;
                        }
                        total += w;
                        seen += w * match self.hit(r, p, d) {
                            Hit::Clear => 1.0,
                            Hit::Crown => 1.0 - crown_mean,
                            Hit::Opaque => 0.0,
                        };
                    }
                }
            }
            if total > 0.0 {
                seen / total
            } else {
                1.0
            }
        });
        self.diffuse.insert(array, f);
        f
    }
}

/// Walls of a building (the roof generator's own rule).
fn pv_roof_walls(
    b: &pv_model::Building,
    faces: &[&pv_model::RoofFace],
) -> Vec<pv_model::WallSummary> {
    pv_roof::walls(b, faces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn any_hit(tris: &[Tri], o: V3, d: V3) -> bool {
        tris.iter().any(|t| hits(t, o, d))
    }

    #[test]
    fn a_concave_polygon_covers_only_itself() {
        // An L, 2 m high: (0,0)-(2,0)-(2,1)-(1,1)-(1,2)-(0,2).
        let l: Vec<V3> = [[0., 0.], [2., 0.], [2., 1.], [1., 1.], [1., 2.], [0., 2.]]
            .iter()
            .map(|p| [p[0], p[1], 2.0])
            .collect();
        let mut tris = Vec::new();
        triangulate(&l, &mut tris);
        assert_eq!(tris.len(), 4);
        let up = [0.0, 0.0, 1.0];
        assert!(any_hit(&tris, [0.5, 0.5, 0.0], up));
        assert!(any_hit(&tris, [1.5, 0.5, 0.0], up));
        assert!(any_hit(&tris, [0.5, 1.5, 0.0], up));
        assert!(!any_hit(&tris, [1.5, 1.5, 0.0], up), "the notch of the L");
    }

    #[test]
    fn objects_cast_shadows() {
        let square =
            |z: f64| -> Vec<[f64; 3]> { vec![[0., 0., z], [2., 0., z], [2., 2., z], [0., 2., z]] };
        // A 3 m high box: a ray towards it is stopped, one over it is not.
        let mut tris = Vec::new();
        object_tris(
            &pv_model::ShadingObject {
                name: "box".into(),
                kind: pv_model::ShadingObjectKind::Building,
                outline: square(0.0),
                height_m: 3.0,
                opacity_by_month: Vec::new(),
            },
            &mut tris,
        );
        let east = [1.0, 0.0, 0.0];
        assert!(any_hit(&tris, [-5.0, 1.0, 1.0], east));
        assert!(!any_hit(&tris, [-5.0, 1.0, 4.0], east));
        // A crown from 5 to 9 m, seen from right under it (through its rounded bottom).
        let mut crown = Vec::new();
        object_tris(
            &pv_model::ShadingObject {
                name: "tree".into(),
                kind: pv_model::ShadingObjectKind::Tree,
                outline: square(5.0),
                height_m: 4.0,
                opacity_by_month: vec![0.8; 12],
            },
            &mut crown,
        );
        assert!(crown.iter().all(|t| t.crown));
        assert!(any_hit(&crown, [1.0, 1.0, 0.0], [0.0, 0.0, 1.0]));
        assert!(!any_hit(&crown, [1.0, 1.0, 10.0], [0.0, 0.0, 1.0]));
    }

    #[test]
    fn ray_triangle() {
        let t = Tri {
            a: [0.0, 0.0, 1.0],
            e1: [1.0, 0.0, 0.0],
            e2: [0.0, 1.0, 0.0],
            face: None,
            wall_of: None,
            module: None,
            crown: false,
        };
        assert!(hits(&t, [0.2, 0.2, 0.0], [0.0, 0.0, 1.0]));
        assert!(!hits(&t, [0.8, 0.8, 0.0], [0.0, 0.0, 1.0]));
        assert!(
            !hits(&t, [0.2, 0.2, 2.0], [0.0, 0.0, 1.0]),
            "behind the origin"
        );
        assert_eq!(bin_of(182.0, 31.0), (36, 6));
        let s = sun_vector(180.0, 90.0);
        assert!((s[2] - 1.0).abs() < 1e-12);
    }
}
