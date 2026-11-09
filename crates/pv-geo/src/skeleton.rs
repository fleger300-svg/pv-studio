//! Weighted straight skeleton with per-edge start times.
//!
//! Each edge of a counter-clockwise polygon is a wavefront line moving inward at `speed`
//! (distance per unit of time) once `start` is reached, and standing still before. The time
//! at which an edge's wavefront reaches a point is then `start + d / speed` (`d`: distance to
//! the edge's line): with `time = height`, `speed = 1 / tan θ` and `start` = eave height,
//! it is exactly the roof plane of the edge, and the skeleton faces are the roof faces.
//! An edge with speed 0 (a gable wall) never moves: the vertices beside it slide along it.
//!
//! Event-driven simulation of the wavefront (circular vertex lists):
//! - *edge event*: an edge shrinks to nothing, its two vertices merge;
//! - *split event*: a reflex vertex reaches a non-adjacent edge, the list splits in two;
//! - *start event*: an edge starts moving (its vertices change velocity: the arcs bend).
//!
//! Polygons are small (tens of vertices): after every event all candidate events are
//! recomputed from scratch (O(n²) per event), which keeps the bookkeeping trivial and the
//! result deterministic. Simultaneous events (frequent on orthogonal footprints) are handled
//! one at a time at the same instant; a vertex event is a split followed by an edge event.

use crate::{Vec2, EPS};

/// One polygon edge's wavefront: inward speed, and the time it starts moving.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveEdge {
    pub speed: f64,
    pub start: f64,
}

/// Why the skeleton could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SkeletonError {
    #[error("the polygon or its edge data is degenerate")]
    Degenerate,
    #[error("the wavefront did not converge")]
    Stalled,
}

/// A skeleton node: where and when wavefront vertices met (or bent).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkeletonNode {
    pub pos: Vec2,
    pub time: f64,
}

/// The skeleton: per input edge, the polygons (counter-clockwise, usually one) swept by its
/// wavefront. Edges with speed 0 have none.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    pub nodes: Vec<SkeletonNode>,
    pub faces: Vec<Vec<Vec<Vec2>>>,
}

/// Time tolerance (time = height, m).
const T_EPS: f64 = 1e-9;

#[derive(Debug, Clone, Copy)]
struct Vert {
    pos: Vec2,
    vel: Vec2,
    /// Incoming and outgoing wavefront edges (input edge indices).
    inc: usize,
    out: usize,
    /// Node where the current arc of this vertex starts.
    node: usize,
    prev: usize,
    next: usize,
    alive: bool,
}

/// A straight piece of a vertex path: `from → to`, between wavefront edges `inc` and `out`.
#[derive(Debug, Clone, Copy)]
struct Arc {
    from: usize,
    to: usize,
    inc: usize,
    out: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Event {
    Start,
    Edge(usize),
    Split { r: usize, x: usize },
}

struct Sim<'a> {
    edges: &'a [WaveEdge],
    normal: Vec<Vec2>,
    dir: Vec<Vec2>,
    /// `normal · x = offset` at the edge's start.
    offset0: Vec<f64>,
    verts: Vec<Vert>,
    nodes: Vec<SkeletonNode>,
    arcs: Vec<Arc>,
    now: f64,
}

impl Sim<'_> {
    fn speed(&self, e: usize) -> f64 {
        let w = self.edges[e];
        if self.now >= w.start - T_EPS {
            w.speed
        } else {
            0.0
        }
    }

    /// `normal · x = k` for the edge's wavefront line now.
    fn line_k(&self, e: usize) -> f64 {
        let w = self.edges[e];
        self.offset0[e] + w.speed * (self.now - w.start).max(0.0)
    }

    fn node(&mut self, pos: Vec2) -> usize {
        self.nodes.push(SkeletonNode {
            pos,
            time: self.now,
        });
        self.nodes.len() - 1
    }

    /// Velocity of a vertex between `inc` and `out`; `None` when the lines are parallel and
    /// opposed (the region between them has collapsed).
    fn velocity(&self, inc: usize, out: usize) -> Result<Option<Vec2>, SkeletonError> {
        let (na, nb) = (self.normal[inc], self.normal[out]);
        let (sa, sb) = (self.speed(inc), self.speed(out));
        let det = na.cross(nb);
        if det.abs() < 1e-9 {
            if na.dot(nb) > 0.0 {
                // Collinear neighbours: one line, if they move alike.
                return if (sa - sb).abs() <= 1e-12 {
                    Ok(Some(na * sa))
                } else {
                    Err(SkeletonError::Degenerate)
                };
            }
            return Ok(None);
        }
        Ok(Some(Vec2::new(
            (sa * nb.v - na.v * sb) / det,
            (na.u * sb - sa * nb.u) / det,
        )))
    }

    /// Ends the current arc of `v` at a node, returns that node.
    fn end_arc(&mut self, v: usize, node: usize) {
        let x = self.verts[v];
        self.arcs.push(Arc {
            from: x.node,
            to: node,
            inc: x.inc,
            out: x.out,
        });
    }

    fn lav(&self, start: usize) -> Vec<usize> {
        let mut out = vec![start];
        let mut v = self.verts[start].next;
        while v != start && out.len() <= self.verts.len() {
            out.push(v);
            v = self.verts[v].next;
        }
        out
    }

    /// Reflex or straight: such a vertex can run into an opposite edge (a straight vertex
    /// between two collinear wavefront edges, left when a narrow wing between them has
    /// collapsed, runs into it head on).
    fn can_split(&self, v: usize) -> bool {
        let x = self.verts[v];
        self.dir[x.inc].cross(self.dir[x.out]) < 1e-12
    }

    /// Whether the vertex sits between parallel wavefront edges of the same direction moving
    /// at different speeds (their lines coincide there).
    fn overtaking(&self, v: usize) -> bool {
        let x = self.verts[v];
        let (na, nb) = (self.normal[x.inc], self.normal[x.out]);
        na.cross(nb).abs() < 1e-9
            && na.dot(nb) > 0.0
            && (self.speed(x.inc) - self.speed(x.out)).abs() > 1e-12
    }

    /// Two parallel wavefront edges meet on one line (the edge between them has collapsed)
    /// at different speeds: from now on the faster front is ahead, and the slower edge's
    /// whole segment is overtaken. Its face ends on that line (a horizontal slope break, as
    /// on a mansard roof) and the faster edge takes over the segment.
    fn overtake(&mut self, v: usize) {
        let x = self.verts[v];
        let slow_is_inc = self.speed(x.inc) < self.speed(x.out);
        let (slow, fast) = if slow_is_inc {
            (x.inc, x.out)
        } else {
            (x.out, x.inc)
        };
        // The slower edge's segment a → b, in its own direction.
        let (a, b) = if slow_is_inc {
            (x.prev, v)
        } else {
            (v, x.next)
        };
        let (na, nb) = (self.node(self.verts[a].pos), self.node(self.verts[b].pos));
        self.end_arc(a, na);
        self.end_arc(b, nb);
        // The slower face lies behind its front: it runs along the segment b → a.
        self.arcs.push(Arc {
            from: nb,
            to: na,
            inc: slow,
            out: fast,
        });
        self.verts[a].alive = false;
        self.verts[b].alive = false;
        let (other, node) = if slow_is_inc { (a, na) } else { (b, nb) };
        let o = self.verts[other];
        let (inc, out, prev, next) = if slow_is_inc {
            (o.inc, fast, o.prev, x.next)
        } else {
            (fast, o.out, x.prev, o.next)
        };
        if prev == other || next == other || prev == v || next == v {
            return; // the whole list was these two vertices
        }
        let w = self.push_vert(Vert {
            pos: o.pos,
            vel: Vec2::ZERO,
            inc,
            out,
            node,
            prev,
            next,
            alive: true,
        });
        self.verts[prev].next = w;
        self.verts[next].prev = w;
        self.finish_small(w);
    }

    /// Recomputes every velocity; vertices whose velocity changes bend (new arc).
    fn update_velocities(&mut self) -> Result<(), SkeletonError> {
        let mut v = 0;
        while v < self.verts.len() {
            if self.verts[v].alive && self.overtaking(v) {
                self.overtake(v);
                v = 0; // the lists changed: start over
                continue;
            }
            v += 1;
        }
        for v in 0..self.verts.len() {
            if !self.verts[v].alive {
                continue;
            }
            let x = self.verts[v];
            let vel = self.velocity(x.inc, x.out)?.unwrap_or(Vec2::ZERO);
            if (vel - x.vel).length() > 1e-12 {
                let at_node = self.nodes[x.node].pos.distance(x.pos) <= EPS;
                if !at_node {
                    let n = self.node(x.pos);
                    self.end_arc(v, n);
                    self.verts[v].node = n;
                }
                self.verts[v].vel = vel;
            }
        }
        Ok(())
    }

    /// Closes lists of one or two vertices (the last ridge of a region).
    fn finish_small(&mut self, v: usize) {
        if !self.verts[v].alive {
            return;
        }
        let lav = self.lav(v);
        match lav.len() {
            1 => {
                let n = self.node(self.verts[v].pos);
                self.end_arc(v, n);
                self.verts[v].alive = false;
            }
            2 => {
                let (u, w) = (lav[0], lav[1]);
                let nu = self.node(self.verts[u].pos);
                let nw = self.node(self.verts[w].pos);
                self.end_arc(u, nu);
                self.end_arc(w, nw);
                let x = self.verts[u];
                self.arcs.push(Arc {
                    from: nu,
                    to: nw,
                    inc: x.inc,
                    out: x.out,
                });
                self.verts[u].alive = false;
                self.verts[w].alive = false;
            }
            _ => {}
        }
    }

    fn next_event(&self) -> Option<(f64, Event)> {
        let mut best: Option<(f64, Event)> = None;
        let offer = |t: f64, e: Event, best: &mut Option<(f64, Event)>| {
            if best.is_none_or(|(bt, _)| t < bt - T_EPS) {
                *best = Some((t, e));
            }
        };
        // Edge events first: at equal times they win (vertex events resolve through them).
        for (a, va) in self.verts.iter().enumerate() {
            if !va.alive {
                continue;
            }
            let b = va.next;
            let vb = self.verts[b];
            let d = self.dir[va.out];
            let len = (vb.pos - va.pos).dot(d);
            let rate = (vb.vel - va.vel).dot(d);
            let t = if len <= EPS {
                if rate <= 1e-12 {
                    Some(self.now)
                } else {
                    None
                }
            } else if rate < -1e-15 {
                Some(self.now - len / rate)
            } else {
                None
            };
            if let Some(t) = t {
                offer(t.max(self.now), Event::Edge(a), &mut best);
            }
        }
        for (r, vr) in self.verts.iter().enumerate() {
            if !vr.alive || !self.can_split(r) {
                continue;
            }
            for x in self.lav(r) {
                let y = self.verts[x].next;
                if x == r || y == r {
                    continue;
                }
                let e = self.verts[x].out;
                let n = self.normal[e];
                let gap = n.dot(vr.pos) - self.line_k(e);
                let closing = n.dot(vr.vel) - self.speed(e);
                if gap < -1e-7 || closing >= -1e-15 {
                    continue;
                }
                let tau = (gap / -closing).max(0.0);
                let hit = vr.pos + vr.vel * tau;
                let (px, py) = (
                    self.verts[x].pos + self.verts[x].vel * tau,
                    self.verts[y].pos + self.verts[y].vel * tau,
                );
                let seg = py - px;
                let l2 = seg.dot(seg);
                let s = if l2 > 0.0 {
                    (hit - px).dot(seg) / l2
                } else {
                    0.0
                };
                let tol = 1e-9 / l2.sqrt().max(1e-9);
                if s < -tol || s > 1.0 + tol {
                    continue;
                }
                // At the very instant a vertex was created next to an edge end, touching
                // that end is not a split (the edge event takes care of it).
                if tau <= T_EPS {
                    let near = |p: Vec2| p.distance(hit) <= 1e-7;
                    if (near(px) && (x == vr.next || x == vr.prev))
                        || (near(py) && (y == vr.next || y == vr.prev))
                    {
                        continue;
                    }
                }
                offer(self.now + tau, Event::Split { r, x }, &mut best);
            }
        }
        for w in self.edges {
            if w.speed > 0.0 && w.start > self.now + T_EPS {
                offer(w.start, Event::Start, &mut best);
            }
        }
        best
    }

    fn advance(&mut self, t: f64) {
        let dt = t - self.now;
        for v in &mut self.verts {
            if v.alive {
                v.pos = v.pos + v.vel * dt;
            }
        }
        self.now = t;
    }

    fn push_vert(&mut self, v: Vert) -> usize {
        self.verts.push(v);
        self.verts.len() - 1
    }

    fn edge_event(&mut self, a: usize) {
        let b = self.verts[a].next;
        let p = (self.verts[a].pos + self.verts[b].pos) * 0.5;
        let n = self.node(p);
        self.end_arc(a, n);
        self.end_arc(b, n);
        self.verts[a].alive = false;
        self.verts[b].alive = false;
        if self.verts[b].next == a {
            return; // a two-vertex list closed on itself
        }
        let (prev, next) = (self.verts[a].prev, self.verts[b].next);
        let w = self.push_vert(Vert {
            pos: p,
            vel: Vec2::ZERO,
            inc: self.verts[a].inc,
            out: self.verts[b].out,
            node: n,
            prev,
            next,
            alive: true,
        });
        self.verts[prev].next = w;
        self.verts[next].prev = w;
        self.finish_small(w);
    }

    fn split_event(&mut self, r: usize, x: usize) {
        let y = self.verts[x].next;
        let vr = self.verts[r];
        let p = vr.pos;
        let n = self.node(p);
        self.end_arc(r, n);
        self.verts[r].alive = false;
        let e = self.verts[x].out;
        let w1 = self.push_vert(Vert {
            pos: p,
            vel: Vec2::ZERO,
            inc: vr.inc,
            out: e,
            node: n,
            prev: vr.prev,
            next: y,
            alive: true,
        });
        let w2 = self.push_vert(Vert {
            pos: p,
            vel: Vec2::ZERO,
            inc: e,
            out: vr.out,
            node: n,
            prev: x,
            next: vr.next,
            alive: true,
        });
        self.verts[vr.prev].next = w1;
        self.verts[y].prev = w1;
        self.verts[x].next = w2;
        self.verts[vr.next].prev = w2;
        self.finish_small(w1);
        self.finish_small(w2);
    }
}

/// Faces of edge `e`: its input edge plus the arcs bordering it, chained into cycles.
fn faces_of(e: usize, n: usize, arcs: &[Arc], nodes: &[SkeletonNode]) -> Vec<Vec<Vec2>> {
    let mut half: Vec<(usize, usize)> = vec![(e, (e + 1) % n)];
    for a in arcs {
        if a.inc == e {
            half.push((a.from, a.to));
        }
        if a.out == e {
            half.push((a.to, a.from));
        }
    }
    let mut used = vec![false; half.len()];
    let mut cycles = Vec::new();
    for start in 0..half.len() {
        if used[start] {
            continue;
        }
        let mut cycle = vec![];
        let mut k = start;
        loop {
            used[k] = true;
            cycle.push(half[k].0);
            let to = half[k].1;
            if to == half[start].0 {
                break;
            }
            match (0..half.len()).find(|&j| !used[j] && half[j].0 == to) {
                Some(j) => k = j,
                None => {
                    cycle.clear();
                    break;
                }
            }
        }
        if cycle.len() >= 3 {
            cycles.push(cycle.iter().map(|&i| nodes[i].pos).collect());
        }
    }
    cycles
}

/// The weighted straight skeleton of a simple counter-clockwise polygon.
///
/// # Errors
/// [`SkeletonError`] on inconsistent input or when the simulation does not converge.
pub fn weighted_skeleton(poly: &[Vec2], edges: &[WaveEdge]) -> Result<Skeleton, SkeletonError> {
    let n = poly.len();
    if n < 3
        || edges.len() != n
        || edges
            .iter()
            .any(|w| !w.speed.is_finite() || w.speed < 0.0 || !w.start.is_finite())
    {
        return Err(SkeletonError::Degenerate);
    }
    let dir: Vec<Vec2> = (0..n)
        .map(|i| {
            let d = poly[(i + 1) % n] - poly[i];
            d * (1.0 / d.length())
        })
        .collect();
    let normal: Vec<Vec2> = dir.iter().map(|d| Vec2::new(-d.v, d.u)).collect();
    let offset0 = (0..n).map(|i| normal[i].dot(poly[i])).collect();
    let t0 = edges
        .iter()
        .filter(|w| w.speed > 0.0)
        .map(|w| w.start)
        .fold(f64::INFINITY, f64::min);
    if !t0.is_finite() {
        return Err(SkeletonError::Degenerate);
    }
    let mut sim = Sim {
        edges,
        normal,
        dir,
        offset0,
        verts: Vec::with_capacity(4 * n),
        nodes: poly
            .iter()
            .map(|&pos| SkeletonNode { pos, time: t0 })
            .collect(),
        arcs: Vec::new(),
        now: t0,
    };
    for (i, &pos) in poly.iter().enumerate() {
        sim.verts.push(Vert {
            pos,
            vel: Vec2::ZERO,
            inc: (i + n - 1) % n,
            out: i,
            node: i,
            prev: (i + n - 1) % n,
            next: (i + 1) % n,
            alive: true,
        });
    }
    sim.update_velocities()?;
    let guard = 20 * n * n + 200;
    for _ in 0..guard {
        if !sim.verts.iter().any(|v| v.alive) {
            let faces = (0..n)
                .map(|e| {
                    if edges[e].speed > 0.0 {
                        faces_of(e, n, &sim.arcs, &sim.nodes)
                    } else {
                        Vec::new()
                    }
                })
                .collect();
            return Ok(Skeleton {
                nodes: sim.nodes,
                faces,
            });
        }
        let Some((t, event)) = sim.next_event() else {
            return Err(SkeletonError::Stalled);
        };
        sim.advance(t);
        match event {
            Event::Start => {}
            Event::Edge(a) => sim.edge_event(a),
            Event::Split { r, x } => sim.split_event(r, x),
        }
        sim.update_velocities()?;
    }
    Err(SkeletonError::Stalled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convex::signed_area;

    fn poly(pts: &[(f64, f64)]) -> Vec<Vec2> {
        pts.iter().map(|&(u, v)| Vec2::new(u, v)).collect()
    }

    fn uniform(n: usize) -> Vec<WaveEdge> {
        vec![
            WaveEdge {
                speed: 1.0,
                start: 0.0
            };
            n
        ]
    }

    fn total_area(s: &Skeleton) -> f64 {
        s.faces.iter().flatten().map(|f| signed_area(f)).sum()
    }

    #[test]
    fn rectangle_gives_two_trapezoids_and_two_triangles() {
        let p = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (0.0, 4.0)]);
        let s = weighted_skeleton(&p, &uniform(4)).unwrap();
        assert!((total_area(&s) - 40.0).abs() < 1e-9);
        assert_eq!(s.faces[0].len(), 1);
        assert!((signed_area(&s.faces[0][0]) - (10.0 + 6.0) / 2.0 * 2.0).abs() < 1e-9);
        assert!((signed_area(&s.faces[1][0]) - 4.0).abs() < 1e-9);
        let top = s.nodes.iter().map(|n| n.time).fold(0.0, f64::max);
        assert!((top - 2.0).abs() < 1e-12, "ridge at half the width");
    }

    #[test]
    fn l_shape_has_a_valley() {
        let p = poly(&[
            (0.0, 0.0),
            (8.0, 0.0),
            (8.0, 4.0),
            (4.0, 4.0),
            (4.0, 8.0),
            (0.0, 8.0),
        ]);
        let s = weighted_skeleton(&p, &uniform(6)).unwrap();
        assert!((total_area(&s) - 48.0).abs() < 1e-9, "{}", total_area(&s));
        // The valley from the reflex corner (4, 4) runs at 45° to (2, 2).
        assert!(s
            .nodes
            .iter()
            .any(|n| n.pos.distance(Vec2::new(2.0, 2.0)) < 1e-9));
    }

    #[test]
    fn gable_walls_stand_still() {
        let p = poly(&[(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (0.0, 4.0)]);
        let mut w = uniform(4);
        w[1].speed = 0.0;
        w[3].speed = 0.0;
        let s = weighted_skeleton(&p, &w).unwrap();
        assert!(s.faces[1].is_empty() && s.faces[3].is_empty());
        assert!((signed_area(&s.faces[0][0]) - 20.0).abs() < 1e-9);
        assert!((signed_area(&s.faces[2][0]) - 20.0).abs() < 1e-9);
    }
}
