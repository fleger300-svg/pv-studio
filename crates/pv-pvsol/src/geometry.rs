//! `Visu3D/GeometrischeDaten.xml`: the PV*SOL 3-D scene.
//!
//! **World frame.** PV*SOL's scene is left-handed: `X` west, `Y` up, `Z` south (checked on
//! project A: the roof faces facing 190° lie on the `+Z` side of their ridge, and the open
//! map area is rotated by 180° from map coordinates east/north). Here every point is
//! converted to PV Studio's ENU: `east = −X`, `north = −Z`, `up = Y`.
//!
//! **Module formations** (`AnwObjTyp` 33) lie in the plane of a roof face. `Position` is
//! the formation's lower-left corner (world), `PosAufBezugsFL` the same corner in the face's
//! own `(u, v)` frame — `u` along the eave to the right of a viewer looking up the slope,
//! `v` up the slope, exactly PV Studio's roof frame. `Rotation`: compass azimuth the modules
//! face, and zenith angle of the plane (`tilt = 90° − zenith`). Modules sit on a grid of
//! `AnzModuleSp` rows (`Zeile`, 0 at the eave) × `AnzModuleZe` columns (`Spalte`, 0 on the
//! left), separated by `AbstandModule`; the reference module (`SignaturRefModul`, an object
//! of type 37) gives the module's size along the row (`Ebene1/BreiteR`) and up the slope
//! (`Ebene2/Hoehe`). Checked against the cable points of `Kabel.xml`, which carry module
//! centres in face coordinates.
//!
//! **Parametric buildings** (`AnwObjTyp` 15, `VarHaus`): storeys (`Etage`, type 91) are
//! boxes `BreiteR × TiefeR × Hoehe` whose `Position` is a footprint corner, rotated by the
//! building's azimuth; the roof storey (type 92) is not decoded (the roof faces come from the
//! formations on them).
//!
//! **Module row formations** (type 38, racks on flat roofs) and the other objects (trees,
//! chimneys, free buildings) are listed but not decoded yet.

use crate::guid;
use crate::xml::Node;
use crate::ReadError;

/// A point in PV Studio's ENU frame [m].
pub type Enu = [f64; 3];

/// Converts a PV*SOL scene position to ENU.
#[must_use]
pub fn to_enu(n: &Node) -> Option<Enu> {
    Some([-n.f64_at("X")?, -n.f64_at("Z")?, n.f64_at("Y")?])
}

/// Which face of which reference object a formation lies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FaceKey {
    pub object: i32,
    pub sub_object: i32,
    pub face: i32,
}

/// One module of a formation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormationModule {
    /// Normalised GUID (key of the wiring file).
    pub id: String,
    /// Row from the eave (`Zeile`).
    pub row: u32,
    /// Column from the left (`Spalte`).
    pub col: u32,
}

/// A module formation in a roof plane.
#[derive(Debug, Clone, PartialEq)]
pub struct Formation {
    pub id: String,
    pub name: String,
    pub face: FaceKey,
    /// Lower-left corner, world ENU.
    pub corner: Enu,
    /// The same corner in the face's `(u, v)` frame.
    pub corner_uv: [f64; 2],
    pub azimuth_deg: f64,
    pub tilt_deg: f64,
    pub rows: u32,
    pub cols: u32,
    /// Module size along the row (`u`) and up the slope (`v`) [m].
    pub module_u_m: f64,
    pub module_v_m: f64,
    /// Clear gaps between modules along `u` / `v` [m].
    pub gap_u_m: f64,
    pub gap_v_m: f64,
    /// PV*SOL database id of the module type (normalised; key into `DB.json`).
    pub module_type: String,
    pub modules: Vec<FormationModule>,
}

impl Formation {
    /// Centre of a module in face coordinates.
    #[must_use]
    pub fn module_centre_uv(&self, row: u32, col: u32) -> [f64; 2] {
        [
            self.corner_uv[0]
                + f64::from(col) * (self.module_u_m + self.gap_u_m)
                + self.module_u_m / 2.0,
            self.corner_uv[1]
                + f64::from(row) * (self.module_v_m + self.gap_v_m)
                + self.module_v_m / 2.0,
        ]
    }

    /// World position of the face's `(u, v) = (0, 0)` implied by this formation.
    #[must_use]
    pub fn face_origin(&self) -> Enu {
        let f = pv_model::RoofFrame::new([0.0; 3], self.tilt_deg, self.azimuth_deg);
        let [u, v] = self.corner_uv;
        [
            self.corner[0] - u * f.axis_u[0] - v * f.axis_v[0],
            self.corner[1] - u * f.axis_u[1] - v * f.axis_v[1],
            self.corner[2] - u * f.axis_u[2] - v * f.axis_v[2],
        ]
    }
}

/// A parametric building (type 15): its storeys' footprint and heights.
#[derive(Debug, Clone, PartialEq)]
pub struct VarBuilding {
    pub name: String,
    /// Index among the scene's reference objects (what formations refer to).
    pub reference_index: i32,
    /// Footprint corners at ground level, ENU (counter-clockwise).
    pub footprint: Vec<[f64; 2]>,
    pub ground_m: f64,
    /// Total height of the full storeys (the eave).
    pub eave_height_m: f64,
}

/// An object of the scene the importer does not use, by type and name (reported).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OtherObject {
    pub kind: u32,
    pub name: String,
}

/// The decoded scene.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    pub formations: Vec<Formation>,
    pub buildings: Vec<VarBuilding>,
    /// Module row formations (racks on flat roofs).
    pub row_formations: Vec<RowFormation>,
    pub others: Vec<OtherObject>,
    /// The map section (aerial image and height map), when the project has one.
    pub map: Option<MapSection>,
}

/// The rectangular map section of the scene (`AnwObjTyp` 65): what `HeightMap.bin` covers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapSection {
    /// ENU of the `Position` corner (south-west for the usual 180° rotation).
    pub corner: [f64; 2],
    /// Extent along the map's width (east) and depth (north) [m].
    pub width_m: f64,
    pub depth_m: f64,
    pub rotation_deg: f64,
}

impl Scene {
    /// Modules over all decoded formations.
    #[must_use]
    pub fn module_count(&self) -> usize {
        self.formations.iter().map(|f| f.modules.len()).sum()
    }
}

/// Object types that carry no physical geometry (selection, edge distances, map area,
/// terrain, horizon, module reference objects, barred areas).
const NON_PHYSICAL: [u32; 8] = [0, 4, 37, 47, 48, 65, 79, 87];

fn kind(o: &Node) -> u32 {
    o.f64_at("StandardDaten/AnwObjTyp")
        .map_or(u32::MAX, |k| k.round() as u32)
}

fn face_key(o: &Node) -> FaceKey {
    let i = |k: &str| {
        o.f64_at(&format!("Zuordn_XML/{k}"))
            .map_or(-1, |x| x.round() as i32)
    };
    FaceKey {
        object: i("Index_BezugsObj"),
        sub_object: i("Index_BezugsUnterObj"),
        face: i("Index_BezugsFlaeche"),
    }
}

/// A reference module object: its size (along the row, up the slope) and module type.
struct RefModule {
    id: String,
    along: f64,
    up: f64,
    module_type: String,
}

fn ref_module(o: &Node) -> Option<RefModule> {
    let s = o.at("StandardDaten/ZweifacheSkalierung")?;
    let along = s.f64_at("Ebene1/BreiteR")?;
    let up = s.f64_at("Ebene2/Hoehe")?;
    (along > 0.1 && up > 0.1).then(|| RefModule {
        id: guid::normalise(o.text_at("StandardDaten/SignaturXML").unwrap_or_default()),
        along,
        up,
        module_type: guid::normalise(
            o.text_at("SpezifDaten/BezugsFlaechenObjekt/Modul/Modultyp_GUID_XML")
                .unwrap_or_default(),
        ),
    })
}

fn formation(o: &Node, refs: &[RefModule]) -> Option<Formation> {
    let sd = o.child("StandardDaten")?;
    let f = o.at("SpezifDaten/ModulFormation")?;
    let ref_id = guid::normalise(f.text_at("SignaturRefModul")?);
    let r = refs.iter().find(|r| r.id == ref_id)?;
    let (mu, mv) = (r.along, r.up);
    let modules: Vec<FormationModule> = f
        .children("Module")
        .filter_map(|m| {
            Some(FormationModule {
                id: guid::normalise(m.text_at("SignaturXML")?),
                row: u32::try_from(m.f64_at("Zeile")?.round() as i64).ok()?,
                col: u32::try_from(m.f64_at("Spalte")?.round() as i64).ok()?,
            })
        })
        .collect();
    let zenith = sd.f64_at("Rotation/ZenitWinkel")?;
    Some(Formation {
        id: guid::normalise(sd.text_at("SignaturXML").unwrap_or_default()),
        name: sd.text_at("Bezeichnung").unwrap_or_default().to_owned(),
        face: face_key(o),
        corner: to_enu(sd.child("Position")?)?,
        corner_uv: [
            sd.f64_at("PosAufBezugsFL/X")?,
            sd.f64_at("PosAufBezugsFL/Y")?,
        ],
        azimuth_deg: sd.f64_at("Rotation/AzimutWinkel")?.rem_euclid(360.0),
        tilt_deg: (90.0 - zenith).clamp(0.0, 90.0),
        rows: f.f64_at("AnzModuleSp").map_or(0, |x| x.round() as u32),
        cols: f.f64_at("AnzModuleZe").map_or(0, |x| x.round() as u32),
        module_u_m: mu,
        module_v_m: mv,
        gap_u_m: f.f64_at("AbstandModule/Horizontal").unwrap_or(0.02),
        gap_v_m: f.f64_at("AbstandModule/Vertikal").unwrap_or(0.02),
        module_type: r.module_type.clone(),
        modules,
    })
}

fn var_building(o: &Node, reference_index: i32) -> Option<VarBuilding> {
    let sd = o.child("StandardDaten")?;
    let storeys: Vec<&Node> = o
        .at("SpezifDaten/VarHaus")?
        .children("Etage")
        .filter_map(|e| e.child("StandardDaten"))
        .filter(|s| {
            s.f64_at("AnwObjTyp")
                .is_some_and(|k| (k - 91.0).abs() < 0.5)
        })
        .collect();
    let first = storeys.first()?;
    let corner = to_enu(first.child("Position")?)?;
    let az = first.f64_at("Rotation/AzimutWinkel")?.to_radians();
    let b = first.f64_at("ZweifacheSkalierung/Ebene2/BreiteR")?;
    let t = first.f64_at("ZweifacheSkalierung/Ebene2/TiefeR")?;
    // PV*SOL +X (west) and +Z (south) rotated by the azimuth, in ENU.
    let x = [-az.cos(), az.sin()];
    let z = [-az.sin(), -az.cos()];
    let p0 = [corner[0], corner[1]];
    let mut footprint = vec![
        p0,
        [p0[0] + b * x[0], p0[1] + b * x[1]],
        [p0[0] + b * x[0] + t * z[0], p0[1] + b * x[1] + t * z[1]],
        [p0[0] + t * z[0], p0[1] + t * z[1]],
    ];
    let area2: f64 = (0..4)
        .map(|i| {
            let (a, c) = (footprint[i], footprint[(i + 1) % 4]);
            a[0] * c[1] - c[0] * a[1]
        })
        .sum();
    if area2 < 0.0 {
        footprint.reverse();
    }
    let height: f64 = storeys
        .iter()
        .filter_map(|s| s.f64_at("ZweifacheSkalierung/Ebene2/Hoehe"))
        .sum();
    (b > 0.0 && t > 0.0 && height > 0.0).then(|| VarBuilding {
        name: sd.text_at("Bezeichnung").unwrap_or_default().to_owned(),
        reference_index,
        footprint,
        ground_m: corner[2],
        eave_height_m: height,
    })
}

/// Reads the scene.
///
/// # Errors
/// [`ReadError::Format`] when the scene element is missing.
pub fn parse(doc: &Node) -> Result<Scene, ReadError> {
    let szene = doc.child("Szene").ok_or_else(|| ReadError::Format {
        file: "Visu3D/GeometrischeDaten.xml".into(),
        what: "Szene".into(),
    })?;
    let objects: Vec<&Node> = szene.children("ZeichenObjekt").collect();
    let refs: Vec<RefModule> = objects
        .iter()
        .filter(|o| kind(o) == 37)
        .filter_map(|o| ref_module(o))
        .collect();
    let mut scene = Scene::default();
    // Reference objects (what `Index_BezugsObj` counts): the objects flagged as references,
    // except the terrain.
    let mut reference_index = -1;
    for o in &objects {
        let k = kind(o);
        let is_reference = o
            .f64_at("StandardDaten/IstBezugsObj")
            .is_some_and(|x| x != 0.0)
            && k != 4;
        if is_reference {
            reference_index += 1;
        }
        let name = o
            .text_at("StandardDaten/Bezeichnung")
            .unwrap_or_default()
            .to_owned();
        match k {
            33 => {
                if let Some(f) = formation(o, &refs) {
                    scene.formations.push(f);
                } else {
                    scene.others.push(OtherObject { kind: k, name });
                }
            }
            38 => match row_formation(o) {
                Some(f) => scene.row_formations.push(f),
                None => scene.others.push(OtherObject { kind: k, name }),
            },
            15 => match var_building(o, reference_index) {
                Some(b) => scene.buildings.push(b),
                None => scene.others.push(OtherObject { kind: k, name }),
            },
            65 => {
                let sd = o.child("StandardDaten");
                scene.map = sd.and_then(|sd| {
                    let p = to_enu(sd.child("Position")?)?;
                    Some(MapSection {
                        corner: [p[0], p[1]],
                        width_m: sd.f64_at("ZweifacheSkalierung/Ebene1/BreiteR")?,
                        depth_m: sd.f64_at("ZweifacheSkalierung/Ebene1/TiefeR")?,
                        rotation_deg: sd.f64_at("Rotation/AzimutWinkel").unwrap_or(0.0),
                    })
                });
            }
            k if NON_PHYSICAL.contains(&k) => {}
            _ => scene.others.push(OtherObject { kind: k, name }),
        }
    }
    Ok(scene)
}

/// One half of a module row (`FormsMRF`): the modules facing one way.
#[derive(Debug, Clone, PartialEq)]
pub struct RowHalf {
    /// Normalised signature (listed under its module area in the wiring file).
    pub id: String,
    pub modules: Vec<FormationModule>,
}

/// One row of a row formation (`ModulreiheSparVar`).
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleRow {
    /// Corner of the row in the flat face's `(u, v)` frame.
    pub corner_uv: [f64; 2],
    /// One half for rows facing one way, two for east-west racks.
    pub halves: Vec<RowHalf>,
}

/// A module row formation (`AnwObjTyp` 38): racks on a flat roof.
#[derive(Debug, Clone, PartialEq)]
pub struct RowFormation {
    pub id: String,
    pub name: String,
    pub face: FaceKey,
    /// Corner of the first row, world ENU.
    pub corner: Enu,
    /// Compass direction of the formation (the modules of the first halves face it or its
    /// opposite).
    pub azimuth_deg: f64,
    /// Mounting system (normalised GUID, key into `Visu3D/MoSys.xml`).
    pub mounting: String,
    pub rows: Vec<ModuleRow>,
}

impl RowFormation {
    #[must_use]
    pub fn module_count(&self) -> usize {
        self.rows
            .iter()
            .flat_map(|r| &r.halves)
            .map(|h| h.modules.len())
            .sum()
    }
}

fn row_formation(o: &Node) -> Option<RowFormation> {
    let sd = o.child("StandardDaten")?;
    let f = o.at("SpezifDaten/ModulreihenFormation")?;
    let rows: Vec<ModuleRow> = f
        .children("ModulreiheSparVar")
        .filter_map(|r| {
            Some(ModuleRow {
                corner_uv: [r.f64_at("PosAufBezugsFL/X")?, r.f64_at("PosAufBezugsFL/Y")?],
                halves: r
                    .children("FormsMRF")
                    .map(|h| RowHalf {
                        id: guid::normalise(h.text_at("SignaturXML").unwrap_or_default()),
                        modules: h
                            .children("Modul")
                            .filter_map(|m| {
                                Some(FormationModule {
                                    id: guid::normalise(m.text_at("SignaturXML")?),
                                    row: u32::try_from(m.f64_at("Zeile")?.round() as i64).ok()?,
                                    col: u32::try_from(m.f64_at("Spalte")?.round() as i64).ok()?,
                                })
                            })
                            .collect(),
                    })
                    .collect(),
            })
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    Some(RowFormation {
        id: guid::normalise(sd.text_at("SignaturXML").unwrap_or_default()),
        name: sd.text_at("Bezeichnung").unwrap_or_default().to_owned(),
        face: face_key(o),
        corner: to_enu(sd.child("Position")?)?,
        azimuth_deg: sd.f64_at("Rotation/AzimutWinkel")?.rem_euclid(360.0),
        mounting: guid::normalise(f.text_at("MoSys_GUID").unwrap_or_default()),
        rows,
    })
}

/// A mounting system of `Visu3D/MoSys.xml` (racks of the row formations).
#[derive(Debug, Clone, PartialEq)]
pub struct MountingSystem {
    pub id: String,
    /// 1: rows facing one way; 2: east-west.
    pub kind: u32,
    /// Module tilt on the rack [°].
    pub tilt_deg: f64,
    /// Table size along the row and its slant depth [m].
    pub table_along_m: f64,
    pub table_depth_m: f64,
    pub modules_along: u32,
    /// Gap between modules along the row [m].
    pub gap_m: f64,
}

/// Reads `Visu3D/MoSys.xml`.
#[must_use]
pub fn mounting_systems(doc: &Node) -> Vec<MountingSystem> {
    doc.children("MoSys")
        .filter_map(|m| {
            Some(MountingSystem {
                id: guid::normalise(m.text_at("GUID")?),
                kind: m.f64_at("Typ").map_or(1, |k| k.round() as u32),
                tilt_deg: m.f64_at("NeigungRelativ_RAD")?.to_degrees(),
                table_along_m: m.f64_at("DimModulTisch/X").unwrap_or(0.0),
                table_depth_m: m.f64_at("DimModulTisch/Y").unwrap_or(0.0),
                modules_along: m
                    .f64_at("ModulAnzahl/Horz")
                    .map_or(1, |n| n.round().max(1.0) as u32),
                gap_m: m.f64_at("ModulAbst/Horz").unwrap_or(0.005),
            })
        })
        .collect()
}
