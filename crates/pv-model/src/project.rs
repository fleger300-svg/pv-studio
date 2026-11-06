//! Project aggregate: buildings, roof faces, panel arrays and module instances.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::geometry::{Quat, RoofFrame, Vec3};
use crate::ids::{ArrayId, BuildingId, ModuleId, ProjectId, RoofFaceId};
use crate::layout::{ArrayLayoutParams, RoofOutlineSpec, SupportGridSpec};

/// Module frame thickness (m).
pub const MODULE_THICKNESS_M: f64 = 0.030;
/// Clearance between roof surface and the underside of a module (m).
pub const MODULE_MOUNT_GAP_M: f64 = 0.050;
/// Peak power of one module (Wp), used for the selection's kWp.
pub const MODULE_PEAK_POWER_WP: f64 = 430.0;
/// Fake string size used by the stress presets (string colouring mode only).
pub const MODULES_PER_STRING: u32 = 18;
/// Tolerance used by geometric validation (m).
const EPS_M: f64 = 1e-6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RoofKind {
    Gable,
    Flat,
}

/// Simple building volume: a box from the ground to the eave, plus the roof faces on top.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Building {
    pub id: BuildingId,
    pub name: String,
    /// Centre of the footprint at ground level (world ENU, m).
    pub origin: Vec3,
    /// Footprint size along x (east) and y (north), m.
    pub size_m: [f64; 2],
    pub eave_height_m: f64,
    /// Ridge height above ground; equals `eave_height_m` for a flat roof.
    pub ridge_height_m: f64,
    pub roof_kind: RoofKind,
}

/// Rectangular planar roof face.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RoofFace {
    pub id: RoofFaceId,
    pub building_id: BuildingId,
    pub name: String,
    /// World position of local `(u, v) = (0, 0)` (see [`crate::geometry`]).
    pub origin: Vec3,
    /// Slope from horizontal, degrees.
    pub tilt_deg: f64,
    /// Compass direction the downslope side faces, degrees.
    pub azimuth_deg: f64,
    /// Extent along `u`, m.
    pub width_m: f64,
    /// Extent along `v` (slope length), m.
    pub height_m: f64,
    /// Real outline when the face is not the `width_m × height_m` rectangle (trapezoid of a
    /// hipped roof, L-shapes, obstacles). `None` = the rectangle, eave at `v = 0`.
    #[specta(optional)]
    pub outline: Option<RoofOutlineSpec>,
    /// Seams / rafters of the covering (decide the fixation positions).
    #[specta(optional)]
    pub support_grid: Option<SupportGridSpec>,
}

impl RoofFace {
    #[must_use]
    pub fn frame(&self) -> RoofFrame {
        RoofFrame::new(self.origin, self.tilt_deg, self.azimuth_deg)
    }

    /// World ENU of roof `(u, v)` on the surface.
    #[must_use]
    pub fn uv_to_world(&self, u: f64, v: f64) -> Vec3 {
        self.frame().to_world(u, v, 0.0)
    }

    /// Roof `(u, v)` of a world point (projected onto the plane).
    #[must_use]
    pub fn world_to_uv(&self, p: Vec3) -> (f64, f64) {
        let (u, v, _) = self.frame().to_local(p);
        (u, v)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Orientation {
    /// Long side along the module's slope direction.
    Portrait,
    /// Long side across the module's slope direction.
    Landscape,
}

/// Regular grid on which an array's modules were laid out (used for click lookup).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ArrayGrid {
    pub rows: u32,
    pub cols: u32,
    /// Roof `(u, v)` of the centre of cell (row 0, col 0).
    pub origin_u: f64,
    pub origin_v: f64,
    /// Centre-to-centre spacing along `u` (columns) and `v` (rows).
    pub pitch_u: f64,
    pub pitch_v: f64,
    /// Mounting blocks of `block_cols × block_rows` modules (0 = no blocks), separated by
    /// `block_extra_*` on top of the normal pitch. Cells are then not evenly spaced: use
    /// [`ArrayGrid::cell_centre`].
    pub block_cols: u32,
    pub block_rows: u32,
    /// Extra distance added between two blocks along `u` / `v`, on top of the pitch, m.
    pub block_extra_u: f64,
    pub block_extra_v: f64,
}

impl ArrayGrid {
    /// Regular grid without blocks.
    #[must_use]
    pub fn regular(
        rows: u32,
        cols: u32,
        origin_u: f64,
        origin_v: f64,
        pitch_u: f64,
        pitch_v: f64,
    ) -> Self {
        Self {
            rows,
            cols,
            origin_u,
            origin_v,
            pitch_u,
            pitch_v,
            block_cols: 0,
            block_rows: 0,
            block_extra_u: 0.0,
            block_extra_v: 0.0,
        }
    }

    /// Offset of column `col` from column 0 along `u`, m.
    #[must_use]
    pub fn col_offset(&self, col: u32) -> f64 {
        let blocks = col.checked_div(self.block_cols).unwrap_or(0);
        f64::from(col) * self.pitch_u + f64::from(blocks) * self.block_extra_u
    }

    /// Offset of row `row` from row 0 along `v`, m.
    #[must_use]
    pub fn row_offset(&self, row: u32) -> f64 {
        let blocks = row.checked_div(self.block_rows).unwrap_or(0);
        f64::from(row) * self.pitch_v + f64::from(blocks) * self.block_extra_v
    }

    /// Roof `(u, v)` of the centre of a cell.
    #[must_use]
    pub fn cell_centre(&self, row: u32, col: u32) -> (f64, f64) {
        (
            self.origin_u + self.col_offset(col),
            self.origin_v + self.row_offset(row),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct PanelArray {
    pub id: ArrayId,
    pub roof_face_id: RoofFaceId,
    pub module_length_m: f64,
    pub module_width_m: f64,
    pub orientation: Orientation,
    /// Extra tilt relative to the roof plane (racks on flat roofs); 0 = coplanar.
    pub tilt_on_roof_deg: f64,
    /// Module azimuth relative to the roof face azimuth, degrees (e.g. ±90 for east-west).
    pub azimuth_offset_deg: f64,
    pub grid: ArrayGrid,
    /// Layout parameters (field outline, gaps, blocks, setbacks).
    pub layout: ArrayLayoutParams,
}

impl PanelArray {
    /// Module dimensions in its own local frame before rotation: `(x across, y along slope, z)`.
    #[must_use]
    pub fn local_size(&self) -> Vec3 {
        let (x, y) = match self.orientation {
            Orientation::Portrait => (self.module_width_m, self.module_length_m),
            Orientation::Landscape => (self.module_length_m, self.module_width_m),
        };
        [x, y, MODULE_THICKNESS_M]
    }

    /// Half extents of the module's footprint projected on the roof plane, along `(u, v)`.
    #[must_use]
    pub fn half_footprint(&self) -> (f64, f64) {
        let [x, y, _] = self.local_size();
        let hx = x / 2.0;
        let hy = y * self.tilt_on_roof_deg.to_radians().cos() / 2.0;
        let (s, c) = (-self.azimuth_offset_deg).to_radians().sin_cos();
        let (s, c) = (s.abs(), c.abs());
        (c * hx + s * hy, s * hx + c * hy)
    }

    /// Rotation of a module relative to the roof frame: azimuth offset about the roof
    /// normal, then tilt about the module's own horizontal axis.
    #[must_use]
    pub fn rotation_in_roof(&self) -> Quat {
        // Compass angles grow clockwise seen from above; rotations about +z are
        // counter-clockwise, hence the sign flip.
        let az = Quat::from_axis_angle([0.0, 0.0, 1.0], -self.azimuth_offset_deg.to_radians());
        let tilt = Quat::from_axis_angle([1.0, 0.0, 0.0], self.tilt_on_roof_deg.to_radians());
        az * tilt
    }

    /// Height of the module centre above the roof plane, m.
    #[must_use]
    pub fn centre_offset(&self) -> f64 {
        let [_, y, z] = self.local_size();
        MODULE_MOUNT_GAP_M + z / 2.0 + y / 2.0 * self.tilt_on_roof_deg.to_radians().sin()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ModuleState {
    Active,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInstance {
    pub id: ModuleId,
    pub array_id: ArrayId,
    pub row: u32,
    pub col: u32,
    /// Roof `(u, v)` of the module centre, m.
    pub u: f64,
    pub v: f64,
    pub state: ModuleState,
    /// Fake string index (string colouring mode only).
    pub string_index: u32,
}

/// The whole in-memory model of an open project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    pub buildings: Vec<Building>,
    pub roof_faces: Vec<RoofFace>,
    pub arrays: Vec<PanelArray>,
    /// Invariant: `modules[i].id == ModuleId(i)`, and modules of one array are contiguous.
    pub modules: Vec<ModuleInstance>,
}

/// Error raised when a project violates a structural invariant.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProjectInvariantError {
    #[error("module at index {0} has a non-matching id")]
    ModuleIdMismatch(usize),
    #[error("module {0} references an unknown array")]
    UnknownArray(u32),
    #[error("array {0} references an unknown roof face")]
    UnknownRoofFace(usize),
    #[error("modules of array {0} are not contiguous")]
    NonContiguousArray(usize),
}

impl Project {
    /// Checks the invariants relied upon by commands and by the binary encoder.
    ///
    /// # Errors
    /// Returns the first violated invariant.
    pub fn validate(&self) -> Result<(), ProjectInvariantError> {
        for (i, a) in self.arrays.iter().enumerate() {
            if !self.roof_faces.iter().any(|f| f.id == a.roof_face_id) {
                return Err(ProjectInvariantError::UnknownRoofFace(i));
            }
        }
        let mut last_array: Option<usize> = None;
        let mut seen = vec![false; self.arrays.len()];
        for (i, m) in self.modules.iter().enumerate() {
            if m.id.index() != i {
                return Err(ProjectInvariantError::ModuleIdMismatch(i));
            }
            let ai = self
                .array_index(m.array_id)
                .ok_or(ProjectInvariantError::UnknownArray(m.id.0))?;
            if last_array != Some(ai) {
                if seen[ai] {
                    return Err(ProjectInvariantError::NonContiguousArray(ai));
                }
                seen[ai] = true;
                last_array = Some(ai);
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn array_index(&self, id: ArrayId) -> Option<usize> {
        self.arrays.iter().position(|a| a.id == id)
    }

    #[must_use]
    pub fn roof_face(&self, id: RoofFaceId) -> Option<&RoofFace> {
        self.roof_faces.iter().find(|f| f.id == id)
    }

    #[must_use]
    pub fn module(&self, id: ModuleId) -> Option<&ModuleInstance> {
        self.modules.get(id.index())
    }

    /// Array and roof face of a module.
    #[must_use]
    pub fn module_context(&self, m: &ModuleInstance) -> Option<(&PanelArray, &RoofFace)> {
        let array = self.arrays.get(self.array_index(m.array_id)?)?;
        Some((array, self.roof_face(array.roof_face_id)?))
    }

    /// World position of the module centre and its world rotation.
    #[must_use]
    pub fn module_transform(&self, m: &ModuleInstance) -> Option<(Vec3, Quat)> {
        let (array, face) = self.module_context(m)?;
        let frame = face.frame();
        let pos = frame.to_world(m.u, m.v, array.centre_offset());
        let rot = (frame.rotation() * array.rotation_in_roof()).normalized();
        Some((pos, rot))
    }

    /// Whether a module centred at `(u, v)` lies fully inside its roof face rectangle.
    #[must_use]
    pub fn fits_on_roof(array: &PanelArray, face: &RoofFace, u: f64, v: f64) -> bool {
        let (hu, hv) = array.half_footprint();
        u - hu >= -EPS_M
            && u + hu <= face.width_m + EPS_M
            && v - hv >= -EPS_M
            && v + hv <= face.height_m + EPS_M
    }

    /// Number of active modules.
    #[must_use]
    pub fn active_count(&self) -> usize {
        self.modules
            .iter()
            .filter(|m| m.state == ModuleState::Active)
            .count()
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use super::*;

    /// One south-facing 30° roof face, 10 m × 6 m, with a 2 × 3 portrait array.
    pub fn small_project() -> Project {
        let building = Building {
            id: BuildingId::new(),
            name: "B1".into(),
            origin: [5.0, 0.0, 0.0],
            size_m: [10.0, 10.4],
            eave_height_m: 6.0,
            ridge_height_m: 9.0,
            roof_kind: RoofKind::Gable,
        };
        let face = RoofFace {
            id: RoofFaceId::new(),
            building_id: building.id,
            name: "South".into(),
            origin: [0.0, -5.2, 6.0],
            tilt_deg: 30.0,
            azimuth_deg: 180.0,
            width_m: 10.0,
            height_m: 6.0,
            outline: None,
            support_grid: None,
        };
        let pitch_u = 1.134 + 0.02;
        let pitch_v = 1.762 + 0.02;
        let array = PanelArray {
            id: ArrayId::new(),
            roof_face_id: face.id,
            module_length_m: 1.762,
            module_width_m: 1.134,
            orientation: Orientation::Portrait,
            tilt_on_roof_deg: 0.0,
            azimuth_offset_deg: 0.0,
            grid: ArrayGrid {
                rows: 2,
                cols: 3,
                origin_u: 0.3 + 1.134 / 2.0,
                origin_v: 0.3 + 1.762 / 2.0,
                pitch_u,
                pitch_v,
                block_cols: 0,
                block_rows: 0,
                block_extra_u: 0.0,
                block_extra_v: 0.0,
            },
            layout: ArrayLayoutParams::default(),
        };
        let mut modules = Vec::new();
        for row in 0..2 {
            for col in 0..3 {
                let id = ModuleId(modules.len() as u32);
                modules.push(ModuleInstance {
                    id,
                    array_id: array.id,
                    row,
                    col,
                    u: array.grid.origin_u + f64::from(col) * pitch_u,
                    v: array.grid.origin_v + f64::from(row) * pitch_v,
                    state: ModuleState::Active,
                    string_index: id.0 / MODULES_PER_STRING,
                });
            }
        }
        Project {
            id: ProjectId::new(),
            name: "Test".into(),
            buildings: vec![building],
            roof_faces: vec![face],
            arrays: vec![array],
            modules,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::small_project;
    use super::*;

    #[test]
    fn fixture_is_valid() {
        assert_eq!(small_project().validate(), Ok(()));
    }

    #[test]
    fn coplanar_module_transform_matches_roof() {
        let p = small_project();
        let m = &p.modules[0];
        let (pos, rot) = p.module_transform(m).expect("transform");
        let face = &p.roof_faces[0];
        let (u, v, w) = face.frame().to_local(pos);
        assert!((u - m.u).abs() < 1e-9 && (v - m.v).abs() < 1e-9);
        assert!((w - p.arrays[0].centre_offset()).abs() < 1e-9);
        // Module's local z must coincide with the roof normal.
        let n = rot.rotate([0.0, 0.0, 1.0]);
        let fn_ = face.frame().normal;
        assert!((0..3).all(|i| (n[i] - fn_[i]).abs() < 1e-9));
    }

    #[test]
    fn east_west_rack_faces_east_and_west() {
        let mut p = small_project();
        p.roof_faces[0].tilt_deg = 0.0;
        p.arrays[0].orientation = Orientation::Landscape;
        p.arrays[0].tilt_on_roof_deg = 10.0;
        p.arrays[0].azimuth_offset_deg = 90.0; // roof 180° + 90° = west
        let (_, rot) = p.module_transform(&p.modules[0]).expect("transform");
        let n = rot.rotate([0.0, 0.0, 1.0]);
        assert!(
            n[0] < -0.17 && n[2] > 0.98,
            "normal tilted 10° towards west: {n:?}"
        );
        p.arrays[0].azimuth_offset_deg = -90.0;
        let (_, rot) = p.module_transform(&p.modules[0]).expect("transform");
        assert!(rot.rotate([0.0, 0.0, 1.0])[0] > 0.17, "east-facing");
        // Landscape E-W rack: long side runs north-south.
        let along = rot.rotate([1.0, 0.0, 0.0]);
        assert!(along[1].abs() > 0.99);
    }

    #[test]
    fn footprint_of_rotated_rack() {
        let mut p = small_project();
        p.arrays[0].orientation = Orientation::Landscape;
        p.arrays[0].tilt_on_roof_deg = 10.0;
        p.arrays[0].azimuth_offset_deg = 90.0;
        let (hu, hv) = p.arrays[0].half_footprint();
        assert!((hu - 1.134 * 10f64.to_radians().cos() / 2.0).abs() < 1e-9);
        assert!((hv - 1.762 / 2.0).abs() < 1e-9);
    }

    #[test]
    fn fits_on_roof_checks_edges() {
        let p = small_project();
        let (a, f) = (&p.arrays[0], &p.roof_faces[0]);
        assert!(Project::fits_on_roof(a, f, 1.134 / 2.0, 1.762 / 2.0));
        assert!(!Project::fits_on_roof(a, f, 1.134 / 2.0 - 0.01, 3.0));
        assert!(!Project::fits_on_roof(a, f, 5.0, 6.0 - 1.762 / 2.0 + 0.01));
    }
}
