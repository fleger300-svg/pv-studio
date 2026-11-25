//! Turns a read `.pvprj` into a PV Studio [`Project`], through the model's own commands (one
//! batch: all or nothing).
//!
//! What is imported: the module and inverter records PV*SOL used (frozen components), the
//! location and PV*SOL's weather year, one building per PV*SOL reference object, one roof
//! face per PV*SOL face carrying modules, one array per module formation with its modules at
//! PV*SOL's exact positions, and the strings (inverter, tracker, module order).
//!
//! What is not (yet) imported is listed in [`ImportOutcome::notes`]: module rows on flat
//! roofs, shading objects and the surroundings, roof faces without modules.

use std::collections::{BTreeMap, HashMap};

use pv_model::component::ComponentSnapshot;
use pv_model::{
    wgs84_to_lv95, ArrayGrid, ArrayId, ArrayLayoutParams, Building, BuildingId, Command,
    CommandError, Component, ComponentId, ComponentSpec, GeoLocation, InverterId, InverterUnit,
    ModuleRef, ModuleSlot, ModuleState, Orientation, PanelArray, Project, ProjectSite, PvString,
    RoofFace, RoofFaceId, RoofKind, ShadingObject, ShadingObjectKind, StringId, Wiring,
    WiringSettings,
};
use serde::Serialize;
use specta::Type;

use crate::geometry::{FaceKey, Formation, RowFormation, VarBuilding};
use crate::PvsolFile;

/// Something the import could not carry over, for the user (translated by `key`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportNote {
    pub key: String,
    pub params: BTreeMap<String, String>,
}

impl ImportNote {
    fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            params: BTreeMap::new(),
        }
    }

    fn with(mut self, name: &str, value: impl ToString) -> Self {
        self.params.insert(name.to_owned(), value.to_string());
        self
    }
}

/// The imported project and what was left out.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    pub project: Project,
    pub notes: Vec<ImportNote>,
}

/// Margin around the modules when a roof face is sized from its formations [m].
const FACE_MARGIN_M: f64 = 0.3;
/// Steepest face the model accepts (a facade is imported just below vertical) [°].
const MAX_TILT_DEG: f64 = 89.9;

struct FaceGroup<'a> {
    key: FaceKey,
    formations: Vec<&'a Formation>,
    id: RoofFaceId,
    building: BuildingId,
    origin: [f64; 3],
    tilt_deg: f64,
    azimuth_deg: f64,
    /// Offset added to PV*SOL's `(u, v)`: the face starts [`FACE_MARGIN_M`] before its modules.
    shift: [f64; 2],
    width_m: f64,
    height_m: f64,
}

fn formation_extent(f: &Formation) -> ([f64; 2], [f64; 2]) {
    let w = f64::from(f.cols) * (f.module_u_m + f.gap_u_m) - f.gap_u_m;
    let h = f64::from(f.rows) * (f.module_v_m + f.gap_v_m) - f.gap_v_m;
    (f.corner_uv, [f.corner_uv[0] + w, f.corner_uv[1] + h])
}

fn face_groups<'a>(formations: &'a [Formation]) -> Vec<FaceGroup<'a>> {
    let mut by_key: BTreeMap<FaceKey, Vec<&Formation>> = BTreeMap::new();
    for f in formations {
        by_key.entry(f.face).or_default().push(f);
    }
    by_key
        .into_iter()
        .map(|(key, fs)| {
            let n = fs.len() as f64;
            let mean: [f64; 3] =
                std::array::from_fn(|k| fs.iter().map(|f| f.face_origin()[k]).sum::<f64>() / n);
            let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
            for f in &fs {
                let (a, b) = formation_extent(f);
                for k in 0..2 {
                    lo[k] = lo[k].min(a[k]);
                    hi[k] = hi[k].max(b[k]);
                }
            }
            // The face is the modules' area plus a margin: a face reaching back to PV*SOL's eave
            // corner would cross the hips of a hipped roof and shade the neighbouring face.
            let shift = [FACE_MARGIN_M - lo[0], FACE_MARGIN_M - lo[1]];
            // Facades (90°): the model takes faces strictly below vertical.
            let tilt = (fs.iter().map(|f| f.tilt_deg).sum::<f64>() / n).min(MAX_TILT_DEG);
            let azimuth = fs[0].azimuth_deg;
            // Moving the face's origin by −shift keeps every point in place.
            let fr = pv_model::RoofFrame::new([0.0; 3], tilt, azimuth);
            let origin: [f64; 3] = std::array::from_fn(|k| {
                mean[k] - shift[0] * fr.axis_u[k] - shift[1] * fr.axis_v[k]
            });
            FaceGroup {
                key,
                formations: fs,
                id: RoofFaceId::new(),
                building: BuildingId::new(),
                origin,
                tilt_deg: tilt,
                azimuth_deg: azimuth,
                shift,
                width_m: hi[0] + shift[0] + FACE_MARGIN_M,
                height_m: hi[1] + shift[1] + FACE_MARGIN_M,
            }
        })
        .collect()
}

/// Rectangle enclosing horizontal points, aligned with the eave of a face facing
/// `azimuth_deg` (counter-clockwise). Always a valid footprint, unlike a hull with short or
/// collinear edges.
fn enclosing_rectangle(points: &[[f64; 2]], azimuth_deg: f64) -> Vec<[f64; 2]> {
    let (s, c) = azimuth_deg.to_radians().sin_cos();
    let along = [-c, s];
    let across = [s, c];
    let proj = |d: [f64; 2]| {
        let v: Vec<f64> = points.iter().map(|p| p[0] * d[0] + p[1] * d[1]).collect();
        (
            v.iter().copied().fold(f64::INFINITY, f64::min),
            v.iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
                .max(v.iter().copied().fold(f64::INFINITY, f64::min) + 1.0),
        )
    };
    let ((a0, a1), (b0, b1)) = (proj(along), proj(across));
    let at = |a: f64, b: f64| [a * along[0] + b * across[0], a * along[1] + b * across[1]];
    let mut r = vec![at(a0, b0), at(a1, b0), at(a1, b1), at(a0, b1)];
    let twice_area: f64 = (0..4)
        .map(|i| r[i][0] * r[(i + 1) % 4][1] - r[(i + 1) % 4][0] * r[i][1])
        .sum();
    if twice_area < 0.0 {
        r.reverse();
    }
    r
}

/// Size of the footprint along the first face's eave and across it.
fn footprint_size(points: &[[f64; 2]], azimuth_deg: f64) -> [f64; 2] {
    let (s, c) = azimuth_deg.to_radians().sin_cos();
    let along = [-c, s];
    let across = [s, c];
    let span = |d: [f64; 2]| {
        let v: Vec<f64> = points.iter().map(|p| p[0] * d[0] + p[1] * d[1]).collect();
        v.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - v.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let (a, b) = (span(along), span(across));
    [a.max(b).max(0.1), a.min(b).max(0.1)]
}

fn face_corners(g: &FaceGroup) -> Vec<[f64; 3]> {
    let fr = pv_model::RoofFrame::new(g.origin, g.tilt_deg, g.azimuth_deg);
    [
        (0.0, 0.0),
        (g.width_m, 0.0),
        (g.width_m, g.height_m),
        (0.0, g.height_m),
    ]
    .iter()
    .map(|&(u, v)| fr.to_world(u, v, 0.0))
    .collect()
}

fn building_for(
    object: i32,
    groups: &[&FaceGroup],
    var: Option<&VarBuilding>,
    name: String,
) -> Building {
    let corners: Vec<[f64; 3]> = groups.iter().flat_map(|g| face_corners(g)).collect();
    let top = corners
        .iter()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let lowest_face = corners.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let (absolute, ground, eave) = match var {
        Some(v) => (v.footprint.clone(), v.ground_m, v.eave_height_m),
        None => {
            // No parametric building: a box around the faces, on the terrain (z = 0).
            let ground = 0.0f64.min(lowest_face - 0.1);
            let flat: Vec<[f64; 2]> = corners.iter().map(|p| [p[0], p[1]]).collect();
            (
                enclosing_rectangle(&flat, groups[0].azimuth_deg),
                ground,
                (lowest_face - ground).max(0.1),
            )
        }
    };
    let n = absolute.len().max(1) as f64;
    let cx = absolute.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = absolute.iter().map(|p| p[1]).sum::<f64>() / n;
    let azimuths: Vec<f64> = groups.iter().map(|g| g.azimuth_deg).collect();
    let roof_kind = match groups.len() {
        1 if groups[0].tilt_deg < 5.0 => RoofKind::Flat,
        1 => RoofKind::Monopitch,
        2 if ((azimuths[0] - azimuths[1]).rem_euclid(360.0) - 180.0).abs() < 10.0 => {
            RoofKind::Gable
        }
        _ => RoofKind::Hipped,
    };
    let _ = object;
    Building {
        id: groups[0].building,
        name,
        origin: [cx, cy, ground],
        footprint: absolute.iter().map(|p| [p[0] - cx, p[1] - cy]).collect(),
        size_m: footprint_size(&absolute, groups[0].azimuth_deg),
        eave_height_m: eave.max(0.0),
        ridge_height_m: (top - ground).max(eave.max(0.0)),
        roof_kind,
        roof: None,
        site: None,
    }
}

fn module_component(file: &PvsolFile, id: &str) -> Option<Component> {
    file.db.module(id).cloned().or_else(|| {
        // A formation may refer to a record saved under another id: match by name.
        let area = file.config.areas.iter().find(|a| a.module_id == id)?;
        file.db
            .modules
            .iter()
            .find(|(_, c)| c.model == area.module_name)
            .map(|(_, c)| c.clone())
    })
}

/// Imports a read `.pvprj` as a new PV Studio project.
///
/// # Errors
/// The model's [`CommandError`] if the imported entities are refused (a bug or an
/// unexpected file: the import is all or nothing).
pub fn import(file: &PvsolFile, fallback_name: &str) -> Result<ImportOutcome, CommandError> {
    let name = if file.title.trim().is_empty() {
        fallback_name.to_owned()
    } else {
        file.title.clone()
    };
    let mut project = Project::empty(&name);
    let mut notes = Vec::new();
    let mut commands = Vec::new();

    // --- Components ------------------------------------------------------------------------
    let mut snapshots: Vec<ComponentSnapshot> = Vec::new();

    // --- Site and weather ------------------------------------------------------------------
    let weather = file.weather.as_ref();
    let horizon = file.horizon.as_ref().map(|points| pv_model::Horizon {
        source: "PV*SOL (horizon du projet)".to_owned(),
        points: points.clone(),
    });
    if let (Some(lat), Some(lon)) = (file.site.lat_deg, file.site.lon_deg) {
        let (east, north) = wgs84_to_lv95(lat, lon);
        let label = if file.site.address.is_empty() {
            name.clone()
        } else {
            file.site.address.clone()
        };
        commands.push(Command::SetSite {
            site: Box::new(ProjectSite {
                location: Some(GeoLocation {
                    label,
                    lat_deg: lat,
                    lon_deg: lon,
                    east_m: east,
                    north_m: north,
                    altitude_m: weather.map_or(0.0, |(_, w)| w.altitude_m),
                }),
                weather: weather.map(|(n, w)| w.to_weather_year(n, file.site.utc_offset_h)),
                // Added once the modules are placed (see below).
                surroundings: None,
                horizon: horizon.clone(),
                objects: file.scene.as_ref().map_or_else(Vec::new, shading_objects),
            }),
        });
    } else {
        notes.push(ImportNote::new("pvsol.note.noLocation"));
    }
    if weather.is_none() {
        notes.push(ImportNote::new("pvsol.note.noWeather"));
    }

    // --- Geometry ----------------------------------------------------------------------------
    let mut module_refs: HashMap<String, ModuleRef> = HashMap::new();
    let scene = file.scene.as_ref();
    // Formations that belong to no module area are not part of PV*SOL's configuration.
    let formations: Vec<Formation> = scene
        .map_or(&[][..], |s| s.formations.as_slice())
        .iter()
        .filter(|f| file.form_areas.is_empty() || file.form_areas.contains_key(&f.id))
        .cloned()
        .collect();
    let groups = face_groups(&formations);
    // One building per reference object.
    let mut by_object: BTreeMap<i32, Vec<&FaceGroup>> = BTreeMap::new();
    for g in &groups {
        by_object.entry(g.key.object).or_default().push(g);
    }
    let mut building_of_object: HashMap<i32, BuildingId> = HashMap::new();
    for (k, (object, gs)) in by_object.iter().enumerate() {
        let var = scene.and_then(|s| s.buildings.iter().find(|b| b.reference_index == *object));
        let bname = var.map_or_else(|| format!("Bâtiment {}", k + 1), |v| v.name.clone());
        let b = building_for(*object, gs, var, bname);
        building_of_object.insert(*object, b.id);
        commands.push(Command::UpsertBuilding {
            building: Box::new(b),
            at: None,
        });
    }
    for (k, g) in groups.iter().enumerate() {
        let building = building_of_object[&g.key.object];
        if g.tilt_deg >= MAX_TILT_DEG {
            notes.push(ImportNote::new("pvsol.note.facade").with("face", k + 1));
        }
        commands.push(Command::UpsertRoofFace {
            face: Box::new(RoofFace {
                id: g.id,
                building_id: building,
                name: format!("Pan {}", k + 1),
                origin: g.origin,
                tilt_deg: g.tilt_deg,
                azimuth_deg: g.azimuth_deg,
                width_m: g.width_m,
                height_m: g.height_m,
                outline: None,
                support_grid: None,
            }),
            at: None,
        });
        for f in &g.formations {
            let Some(module) = module_component(file, &f.module_type) else {
                notes.push(ImportNote::new("pvsol.note.moduleMissing").with("array", &f.name));
                continue;
            };
            let ComponentSpec::Module(spec) = &module.spec else {
                continue;
            };
            let landscape = f.module_u_m > f.module_v_m;
            let (along, up) = if landscape {
                (spec.length_m, spec.width_m)
            } else {
                (spec.width_m, spec.length_m)
            };
            if (along - f.module_u_m).abs() > 0.02 || (up - f.module_v_m).abs() > 0.02 {
                notes.push(
                    ImportNote::new("pvsol.note.moduleSize")
                        .with("array", &f.name)
                        .with("module", &module.label),
                );
            }
            let module_id = adopt(&module, &mut snapshots);
            let id = ArrayId::new();
            let origin_u = f.corner_uv[0] + g.shift[0] + f.module_u_m / 2.0;
            let origin_v = f.corner_uv[1] + g.shift[1] + f.module_v_m / 2.0;
            let array = PanelArray {
                id,
                roof_face_id: g.id,
                module_length_m: spec.length_m,
                module_width_m: spec.width_m,
                orientation: if landscape {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                },
                tilt_on_roof_deg: 0.0,
                azimuth_offset_deg: 0.0,
                grid: ArrayGrid::regular(
                    f.rows.max(1),
                    f.cols.max(1),
                    origin_u,
                    origin_v,
                    f.module_u_m + f.gap_u_m,
                    f.module_v_m + f.gap_v_m,
                ),
                layout: ArrayLayoutParams {
                    module_gap_u_m: f.gap_u_m,
                    module_gap_v_m: f.gap_v_m,
                    ..ArrayLayoutParams::default()
                },
                module: Some(module_id),
                mounting: None,
            };
            let slots: Vec<ModuleSlot> = f
                .modules
                .iter()
                .map(|m| {
                    let [u, v] = f.module_centre_uv(m.row, m.col);
                    module_refs.insert(
                        m.id.clone(),
                        ModuleRef {
                            array: id,
                            row: m.row,
                            col: m.col,
                        },
                    );
                    ModuleSlot {
                        row: m.row,
                        col: m.col,
                        u: u + g.shift[0],
                        v: v + g.shift[1],
                        state: ModuleState::Active,
                        string_index: 0,
                    }
                })
                .collect();
            commands.push(Command::UpsertArray {
                array: Box::new(array),
                at: None,
            });
            commands.push(Command::SetArrayModules {
                array_id: id,
                modules: slots,
            });
        }
    }
    // Module rows on flat roofs: one flat face per formation, one array per half row.
    let row_formations: &[RowFormation] = scene.map_or(&[], |s| s.row_formations.as_slice());
    let mut skipped_rows = (0usize, 0usize);
    let mut unused_rows = (0usize, 0usize);
    for (k, rf) in row_formations.iter().enumerate() {
        let configured = file.form_areas.is_empty()
            || rf
                .rows
                .iter()
                .flat_map(|r| &r.halves)
                .any(|h| file.form_areas.contains_key(&h.id));
        if !configured {
            unused_rows.0 += 1;
            unused_rows.1 += rf.module_count();
            continue;
        }
        let Some(plan) = plan_rows(file, rf) else {
            skipped_rows.0 += 1;
            skipped_rows.1 += rf.module_count();
            continue;
        };
        let building = match building_of_object.get(&rf.face.object) {
            Some(&b) => b,
            None => {
                let var = scene.and_then(|s| {
                    s.buildings
                        .iter()
                        .find(|b| b.reference_index == rf.face.object)
                });
                let corners = rect_corners(&plan.face);
                let b = flat_building(&corners, var, plan.face.azimuth_deg, &rf.name);
                building_of_object.insert(rf.face.object, b.id);
                let id = b.id;
                commands.push(Command::UpsertBuilding {
                    building: Box::new(b),
                    at: None,
                });
                id
            }
        };
        let mut face = plan.face;
        face.building_id = building;
        face.name = format!("Toit plat {}", k + 1);
        commands.push(Command::UpsertRoofFace {
            face: Box::new(face),
            at: None,
        });
        for half in plan.halves {
            let module_id = adopt(&half.module, &mut snapshots);
            let mut array = half.array;
            array.module = Some(module_id);
            for (guid, r) in half.refs {
                module_refs.insert(guid, r);
            }
            let id = array.id;
            commands.push(Command::UpsertArray {
                array: Box::new(array),
                at: None,
            });
            commands.push(Command::SetArrayModules {
                array_id: id,
                modules: half.slots,
            });
        }
    }
    match scene {
        None => notes.push(ImportNote::new("pvsol.note.noScene")),
        Some(s) => {
            if unused_rows.0 > 0 {
                notes.push(
                    ImportNote::new("pvsol.note.unusedFormations")
                        .with("count", unused_rows.0)
                        .with("modules", unused_rows.1),
                );
            }
            if skipped_rows.0 > 0 {
                notes.push(
                    ImportNote::new("pvsol.note.rowFormations")
                        .with("count", skipped_rows.0)
                        .with("modules", skipped_rows.1),
                );
            }
            if !s.others.is_empty() {
                let mut names: Vec<String> = s.others.iter().map(|o| o.name.clone()).collect();
                names.sort();
                names.dedup();
                notes.push(
                    ImportNote::new("pvsol.note.objectsSkipped")
                        .with("count", s.others.len())
                        .with("names", names.join(", ")),
                );
            }
        }
    }

    if !module_refs.is_empty() {
        // PV*SOL does not describe the mounting system: the statics and the bill of materials
        // wait for the user's choice.
        notes.push(ImportNote::new("pvsol.note.chooseMounting"));
    }

    // --- Wiring ------------------------------------------------------------------------------
    let mut wiring = Wiring {
        inverters: Vec::new(),
        strings: Vec::new(),
        settings: WiringSettings::default(),
    };
    let mut unknown_modules = 0usize;
    for (k, inv) in file.wiring.iter().enumerate() {
        let Some(c) = file.db.inverter(&inv.component) else {
            notes.push(ImportNote::new("pvsol.note.inverterMissing").with("inverter", &inv.label));
            continue;
        };
        let trackers = match &c.spec {
            ComponentSpec::Inverter(i) => i.dc.trackers.len(),
            _ => 0,
        };
        let component: ComponentId = adopt(c, &mut snapshots);
        let unit = InverterId::new();
        wiring.inverters.push(InverterUnit {
            id: unit,
            name: format!("WR{}", k + 1),
            component,
        });
        for t in &inv.trackers {
            if t.index as usize >= trackers {
                notes.push(
                    ImportNote::new("pvsol.note.trackerMissing")
                        .with("inverter", &inv.label)
                        .with("tracker", t.index + 1),
                );
                continue;
            }
            for s in &t.strings {
                let modules: Vec<ModuleRef> = s
                    .modules
                    .iter()
                    .filter_map(|m| {
                        let r = module_refs.get(m).copied();
                        unknown_modules += usize::from(r.is_none());
                        r
                    })
                    .collect();
                if !modules.is_empty() {
                    wiring.strings.push(PvString {
                        id: StringId::new(),
                        inverter: unit,
                        tracker: t.index,
                        modules,
                        locked: false,
                    });
                }
            }
        }
    }
    if unknown_modules > 0 {
        notes.push(ImportNote::new("pvsol.note.unwiredModules").with("count", unknown_modules));
    }
    if file.wiring.is_empty() && !file.config.inverters.is_empty() {
        notes.push(ImportNote::new("pvsol.note.noWiring"));
    }

    // Components first: arrays and inverters refer to them.
    let mut all = vec![Command::AdoptComponents { snapshots }];
    all.extend(commands);
    if !wiring.inverters.is_empty() {
        all.push(Command::SetWiring {
            wiring: Box::new(wiring),
        });
    }
    Command::Batch { commands: all }.apply(&mut project)?;

    // A solid standing over modules is the project's own building (a reference object the
    // formations do not name directly), or a fixture PV*SOL lets the modules overlap: not a
    // shading object.
    if !project.site.objects.is_empty() {
        let centres: Vec<[f64; 2]> = project
            .modules
            .iter()
            .filter_map(|m| {
                let (_, f) = project.module_context(m)?;
                let w = f.uv_to_world(m.u, m.v);
                Some([w[0], w[1]])
            })
            .collect();
        let mut site = project.site.clone();
        let before = site.objects.len();
        site.objects.retain(|o| {
            // (A tree's crown may well hang over modules; a drawn polygon is kept as drawn,
            // but not a dormer's roof face.)
            (o.height_m <= 0.0 && o.kind != ShadingObjectKind::Dormer)
                || !o.opacity_by_month.is_empty()
                || {
                    let outline: Vec<[f64; 2]> = o.outline.iter().map(|p| [p[0], p[1]]).collect();
                    !centres.iter().any(|&c| point_in_polygon(&outline, c))
                }
        });
        if site.objects.len() != before {
            Command::SetSite {
                site: Box::new(site),
            }
            .apply(&mut project)?;
        }
    }
    if !project.site.objects.is_empty() {
        notes.push(
            ImportNote::new("pvsol.note.objectsImported").with("count", project.site.objects.len()),
        );
    }

    // Surroundings, cut out around the modules as placed (rows included).
    if let (Some(h), Some(map)) = (
        file.height_map.as_ref(),
        file.scene.as_ref().and_then(|s| s.map.as_ref()),
    ) {
        let centres: Vec<[f64; 3]> = project
            .modules
            .iter()
            .filter_map(|m| {
                let (_, f) = project.module_context(m)?;
                Some(f.uv_to_world(m.u, m.v))
            })
            .collect();
        // Foot of the building carrying the modules (parametric building), else the terrain.
        let ground = file.scene.as_ref().and_then(|s| {
            let object = s
                .formations
                .first()
                .map(|f| f.face.object)
                .or_else(|| s.row_formations.first().map(|r| r.face.object))?;
            s.buildings
                .iter()
                .find(|b| b.reference_index == object)
                .map(|b| b.ground_m)
        });
        match crate::heightmap::surroundings(
            h,
            map,
            &centres,
            ground.unwrap_or(0.0),
            file.cut_margin_m,
        ) {
            Some(s) if project.site.location.is_some() => {
                let mut site = project.site.clone();
                site.surroundings = Some(s);
                Command::SetSite {
                    site: Box::new(site),
                }
                .apply(&mut project)?;
            }
            _ => notes.push(ImportNote::new("pvsol.note.surroundingsSkipped")),
        }
    }
    Ok(ImportOutcome { project, notes })
}

/// Is `p` inside the polygon (even-odd rule)?
fn point_in_polygon(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
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

/// Sides of the regular polygon standing for a round outline (trees).
const ROUND_SIDES: usize = 8;

/// The scene's shading objects, except the reference objects that carry modules (the
/// project's own buildings and surfaces, imported as buildings and roof faces).
fn shading_objects(scene: &crate::geometry::Scene) -> Vec<ShadingObject> {
    use crate::geometry::SceneShape;
    let carrying: std::collections::HashSet<i32> = scene
        .formations
        .iter()
        .map(|f| f.face.object)
        .chain(scene.row_formations.iter().map(|r| r.face.object))
        .collect();
    let mut out = Vec::new();
    for o in &scene.objects {
        if o.reference_index.is_some_and(|i| carrying.contains(&i)) {
            continue;
        }
        let kind = match o.kind {
            2 | 15 | 16 | 18 | 67 => ShadingObjectKind::Building,
            70 | 71 => ShadingObjectKind::Wall,
            23 | 24 => ShadingObjectKind::Tree,
            40 | 42 | 43 => ShadingObjectKind::Dormer,
            82 => ShadingObjectKind::Polygon,
            _ if o.on_object > 0 => ShadingObjectKind::RoofFixture,
            _ => ShadingObjectKind::Building,
        };
        match &o.shape {
            SceneShape::Prism {
                outline,
                base_m,
                height_m,
            } => out.push(ShadingObject {
                name: o.name.clone(),
                kind,
                outline: outline.iter().map(|p| [p[0], p[1], *base_m]).collect(),
                height_m: *height_m,
                opacity_by_month: Vec::new(),
            }),
            SceneShape::Polygon(points) => out.push(ShadingObject {
                name: o.name.clone(),
                kind,
                outline: points.clone(),
                height_m: 0.0,
                opacity_by_month: Vec::new(),
            }),
            SceneShape::Dormer { storey, roof } => {
                if let Some((outline, base_m, height_m)) = storey {
                    out.push(ShadingObject {
                        name: o.name.clone(),
                        kind,
                        outline: outline.iter().map(|p| [p[0], p[1], *base_m]).collect(),
                        height_m: *height_m,
                        opacity_by_month: Vec::new(),
                    });
                }
                for face in roof {
                    out.push(ShadingObject {
                        name: o.name.clone(),
                        kind,
                        outline: face.clone(),
                        height_m: 0.0,
                        opacity_by_month: Vec::new(),
                    });
                }
            }
            SceneShape::Tree {
                centre,
                base_m,
                height_m,
                crown_diameter_m,
                crown_height_m,
                trunk_diameter_m,
                opacity_by_month,
            } => {
                let round = |d: f64, z: f64| -> Vec<[f64; 3]> {
                    (0..ROUND_SIDES)
                        .map(|k| {
                            let a = std::f64::consts::TAU * k as f64 / ROUND_SIDES as f64;
                            [
                                centre[0] + d / 2.0 * a.cos(),
                                centre[1] + d / 2.0 * a.sin(),
                                z,
                            ]
                        })
                        .collect()
                };
                let crown_base = base_m + height_m - crown_height_m;
                if *crown_diameter_m > 0.0 && *crown_height_m > 0.0 {
                    out.push(ShadingObject {
                        name: o.name.clone(),
                        kind,
                        outline: round(*crown_diameter_m, crown_base),
                        height_m: *crown_height_m,
                        opacity_by_month: opacity_by_month.clone(),
                    });
                }
                if crown_base > *base_m {
                    // The trunk, opaque.
                    out.push(ShadingObject {
                        name: o.name.clone(),
                        kind,
                        outline: round(*trunk_diameter_m, *base_m),
                        height_m: crown_base - base_m,
                        opacity_by_month: Vec::new(),
                    });
                }
            }
        }
    }
    out
}

/// Freezes `c` once into the import's snapshots; returns its id.
fn adopt(c: &Component, snapshots: &mut Vec<ComponentSnapshot>) -> ComponentId {
    if !snapshots.iter().any(|s| s.component.id == c.id) {
        snapshots.push(ComponentSnapshot {
            component: c.clone(),
            price: None,
        });
    }
    c.id
}

/// One half row set of a row formation, ready to be added.
struct HalfPlan {
    array: PanelArray,
    module: Component,
    slots: Vec<ModuleSlot>,
    /// Scene module id → its position in the array (for the wiring).
    refs: Vec<(String, ModuleRef)>,
}

/// A row formation laid out on its own flat face (the face's building set by the caller).
struct RowPlan {
    face: RoofFace,
    halves: Vec<HalfPlan>,
}

/// Lays out a row formation. Orientation and tilt of each half come from its PV*SOL module
/// area (the wiring file lists the half rows under their area), else from the formation and
/// its rack; the rows keep PV*SOL's positions on the roof, projected on the pitch direction
/// and the row direction. `None` without a module record.
fn plan_rows(file: &PvsolFile, rf: &RowFormation) -> Option<RowPlan> {
    let rack = file.mounting_systems.iter().find(|m| m.id == rf.mounting);
    let area_of = |half: &str| {
        let id = file.form_areas.get(half)?;
        file.config.areas.iter().find(|a| &a.id == id)
    };
    let first_area = rf
        .rows
        .iter()
        .flat_map(|r| r.halves.first())
        .find_map(|h| area_of(&h.id));
    let az0 = first_area.map_or(rf.azimuth_deg, |a| a.azimuth_deg);
    let rack_tilt = rack.map_or(10.0, |m| m.tilt_deg);
    let module_type = first_area
        .map(|a| a.module_id.clone())
        .or_else(|| file.config.areas.first().map(|a| a.module_id.clone()))?;
    let module = module_component(file, &module_type)?;
    let ComponentSpec::Module(spec) = &module.spec else {
        return None;
    };
    // Landscape when the rack's slant depth is the module's short side.
    let depth = rack.map_or(spec.width_m, |m| m.table_depth_m);
    let landscape = (depth - spec.width_m).abs() <= (depth - spec.length_m).abs();
    let (along, slant) = if landscape {
        (spec.length_m, spec.width_m)
    } else {
        (spec.width_m, spec.length_m)
    };
    let gap = rack.map_or(0.005, |m| m.gap_m);

    // Row offsets in PV*SOL's face frame, split into the pitch (the axis the rows step
    // along) and the along-the-row component.
    let r0 = rf.rows[0].corner_uv;
    let deltas: Vec<[f64; 2]> = rf
        .rows
        .iter()
        .map(|r| [r.corner_uv[0] - r0[0], r.corner_uv[1] - r0[1]])
        .collect();
    let spread = |k: usize| deltas.iter().map(|d| d[k].abs()).fold(0.0, f64::max);
    let pitch_axis = usize::from(spread(1) > spread(0));
    let sign = if deltas.iter().map(|d| d[pitch_axis]).sum::<f64>() < 0.0 {
        -1.0
    } else {
        1.0
    };
    let local: Vec<[f64; 2]> = deltas
        .iter()
        .map(|d| [sign * d[1 - pitch_axis], sign * d[pitch_axis]])
        .collect();

    let mut halves = Vec::new();
    let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
    let half_count = rf.rows.iter().map(|r| r.halves.len()).max().unwrap_or(0);
    // Half rows that belong to no module area are not part of PV*SOL's configuration (left
    // in the scene, not simulated): they are not imported.
    let in_config = |id: &str| file.form_areas.is_empty() || file.form_areas.contains_key(id);
    for h in 0..half_count {
        let area = rf
            .rows
            .iter()
            .filter_map(|r| r.halves.get(h))
            .find_map(|x| area_of(&x.id));
        let az = area.map_or(az0 + 180.0 * h as f64, |a| a.azimuth_deg);
        let tilt = area.map_or(rack_tilt, |a| a.tilt_deg).clamp(0.0, 89.0);
        let d = slant * tilt.to_radians().cos();
        let id = ArrayId::new();
        let mut slots = Vec::new();
        let mut refs = Vec::new();
        let mut cols = 0;
        for (k, row) in rf.rows.iter().enumerate() {
            let Some(half) = row.halves.get(h).filter(|x| in_config(&x.id)) else {
                continue;
            };
            let row_index = u32::try_from(k).unwrap_or(u32::MAX);
            for m in &half.modules {
                let u = local[k][0] + f64::from(m.col) * (along + gap) + along / 2.0;
                let v = local[k][1] + h as f64 * d + d / 2.0;
                lo = [lo[0].min(u - along / 2.0), lo[1].min(v - d / 2.0)];
                hi = [hi[0].max(u + along / 2.0), hi[1].max(v + d / 2.0)];
                cols = cols.max(m.col + 1);
                slots.push(ModuleSlot {
                    row: row_index,
                    col: m.col,
                    u,
                    v,
                    state: ModuleState::Active,
                    string_index: 0,
                });
                refs.push((
                    m.id.clone(),
                    ModuleRef {
                        array: id,
                        row: row_index,
                        col: m.col,
                    },
                ));
            }
        }
        if slots.is_empty() {
            continue;
        }
        let pitch = if local.len() > 1 {
            (local[local.len() - 1][1] - local[0][1]) / (local.len() - 1) as f64
        } else {
            2.0 * d
        };
        halves.push(HalfPlan {
            array: PanelArray {
                id,
                roof_face_id: RoofFaceId::new(),
                module_length_m: spec.length_m,
                module_width_m: spec.width_m,
                orientation: if landscape {
                    Orientation::Landscape
                } else {
                    Orientation::Portrait
                },
                tilt_on_roof_deg: tilt,
                azimuth_offset_deg: (az - az0).rem_euclid(360.0),
                grid: ArrayGrid::regular(
                    u32::try_from(rf.rows.len()).unwrap_or(1).max(1),
                    cols.max(1),
                    along / 2.0,
                    h as f64 * d + d / 2.0,
                    along + gap,
                    pitch.max(d),
                ),
                layout: ArrayLayoutParams {
                    module_gap_u_m: gap,
                    row_pitch_m: Some(pitch.max(d)),
                    ..ArrayLayoutParams::default()
                },
                module: None,
                mounting: None,
            },
            module: module.clone(),
            slots,
            refs,
        });
    }
    if halves.is_empty() {
        return None;
    }
    // The face: the modules area plus a margin; local (0, 0) at the first row corner.
    let shift = [FACE_MARGIN_M - lo[0], FACE_MARGIN_M - lo[1]];
    let fr = pv_model::RoofFrame::new([0.0; 3], 0.0, az0);
    let origin: [f64; 3] =
        std::array::from_fn(|k| rf.corner[k] - shift[0] * fr.axis_u[k] - shift[1] * fr.axis_v[k]);
    let face_id = RoofFaceId::new();
    for half in &mut halves {
        half.array.roof_face_id = face_id;
        half.array.grid.origin_u += shift[0];
        half.array.grid.origin_v += shift[1];
        for s in &mut half.slots {
            s.u += shift[0];
            s.v += shift[1];
        }
    }
    Some(RowPlan {
        face: RoofFace {
            id: face_id,
            building_id: BuildingId::new(),
            name: String::new(),
            origin,
            tilt_deg: 0.0,
            azimuth_deg: az0,
            width_m: hi[0] - lo[0] + 2.0 * FACE_MARGIN_M,
            height_m: hi[1] - lo[1] + 2.0 * FACE_MARGIN_M,
            outline: None,
            support_grid: None,
        },
        halves,
    })
}

fn rect_corners(f: &RoofFace) -> Vec<[f64; 3]> {
    let fr = f.frame();
    [
        (0.0, 0.0),
        (f.width_m, 0.0),
        (f.width_m, f.height_m),
        (0.0, f.height_m),
    ]
    .iter()
    .map(|&(u, v)| fr.to_world(u, v, 0.0))
    .collect()
}

/// A flat-roofed building under a flat face (or the parametric building PV*SOL has there).
fn flat_building(
    corners: &[[f64; 3]],
    var: Option<&VarBuilding>,
    azimuth_deg: f64,
    name: &str,
) -> Building {
    let roof_z = corners.iter().map(|p| p[2]).fold(f64::INFINITY, f64::min);
    let (absolute, ground, eave) = match var {
        Some(v) => (v.footprint.clone(), v.ground_m, v.eave_height_m),
        None => {
            let ground = 0.0f64.min(roof_z - 0.1);
            let flat: Vec<[f64; 2]> = corners.iter().map(|p| [p[0], p[1]]).collect();
            (
                enclosing_rectangle(&flat, azimuth_deg),
                ground,
                (roof_z - ground).max(0.1),
            )
        }
    };
    let n = absolute.len().max(1) as f64;
    let cx = absolute.iter().map(|p| p[0]).sum::<f64>() / n;
    let cy = absolute.iter().map(|p| p[1]).sum::<f64>() / n;
    Building {
        id: BuildingId::new(),
        name: var.map_or_else(|| format!("Bâtiment ({name})"), |v| v.name.clone()),
        origin: [cx, cy, ground],
        footprint: absolute.iter().map(|p| [p[0] - cx, p[1] - cy]).collect(),
        size_m: footprint_size(&absolute, azimuth_deg),
        eave_height_m: eave.max(0.0),
        ridge_height_m: eave.max(0.0),
        roof_kind: RoofKind::Flat,
        roof: None,
        site: None,
    }
}
