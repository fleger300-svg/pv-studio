//! E1: entity commands (buildings, roof faces, arrays, array modules, batches).
//!
//! Property: over 500 random sequences mixing every command (valid and invalid ones), a
//! rejected command leaves the project untouched, an accepted one keeps the invariants
//! (`Project::validate`: canonical module ids, contiguous blocks in array order, references),
//! and applying the inverses in reverse order returns to the initial state bit for bit — then
//! re-applying the commands gives back the final state.

use pv_model::command::keys;
use pv_model::{
    ArrayId, Building, BuildingId, ChangeSet, Command, ModuleId, ModuleSlot, ModuleState,
    PanelArray, Project, RoofFace, RoofFaceId, StressPreset,
};
use uuid::Uuid;

/// xorshift64*: deterministic, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
    fn pick<'a, T>(&mut self, v: &'a [T]) -> Option<&'a T> {
        (!v.is_empty()).then(|| &v[self.below(v.len() as u64) as usize])
    }
    fn signed(&mut self, a: f64) -> f64 {
        (self.below(2001) as f64 / 1000.0 - 1.0) * a
    }
    fn uuid(&mut self) -> Uuid {
        Uuid::from_u64_pair(self.next(), self.next())
    }
    fn at(&mut self, len: usize) -> Option<u32> {
        (self.below(3) > 0).then(|| self.below(len as u64 + 2) as u32)
    }
}

fn bits(p: &Project) -> String {
    serde_json::to_string(p).expect("serialise")
}

fn slots(p: &Project, a: ArrayId) -> Vec<ModuleSlot> {
    p.modules
        .iter()
        .filter(|m| m.array_id == a)
        .map(|m| ModuleSlot {
            row: m.row,
            col: m.col,
            u: m.u,
            v: m.v,
            state: m.state,
            string_index: m.string_index,
        })
        .collect()
}

fn random_building(rng: &mut Rng, p: &Project) -> Command {
    let mut b: Building = match rng.pick(&p.buildings) {
        Some(b) => b.clone(),
        None => StressPreset::Residential1k.build().buildings[0].clone(),
    };
    if rng.below(2) == 0 {
        b.id = BuildingId(rng.uuid());
        b.name = format!("B{}", rng.below(100));
    }
    b.size_m[0] += rng.signed(1.0);
    if rng.below(15) == 0 {
        b.ridge_height_m = b.eave_height_m - 1.0; // invalid
    }
    Command::UpsertBuilding {
        building: Box::new(b),
        at: rng.at(p.buildings.len()),
    }
}

fn random_face(rng: &mut Rng, p: &Project) -> Option<Command> {
    let mut f: RoofFace = rng.pick(&p.roof_faces)?.clone();
    match rng.below(3) {
        0 => {
            f.id = RoofFaceId(rng.uuid());
            if let Some(b) = rng.pick(&p.buildings) {
                f.building_id = b.id;
            }
        }
        1 => f.tilt_deg = (f.tilt_deg + rng.signed(5.0)).clamp(0.0, 60.0),
        _ => f.name = format!("F{}", rng.below(100)),
    }
    if rng.below(15) == 0 {
        f.building_id = BuildingId(rng.uuid()); // dangling reference
    }
    if rng.below(20) == 0 {
        f.width_m = f64::NAN;
    }
    Some(Command::UpsertRoofFace {
        face: Box::new(f),
        at: rng.at(p.roof_faces.len()),
    })
}

fn random_array(rng: &mut Rng, p: &Project) -> Option<Command> {
    let mut a: PanelArray = rng.pick(&p.arrays)?.clone();
    match rng.below(4) {
        0 => {
            a.id = ArrayId(rng.uuid());
            a.layout.paired_with = None;
            if let Some(f) = rng.pick(&p.roof_faces) {
                a.roof_face_id = f.id;
            }
        }
        1 => a.layout.paired_with = rng.pick(&p.arrays).map(|o| o.id).filter(|o| *o != a.id),
        2 => a.layout.paired_with = None,
        _ => a.grid.origin_u += rng.signed(0.1),
    }
    if rng.below(15) == 0 {
        a.roof_face_id = RoofFaceId(rng.uuid()); // dangling reference
    }
    if rng.below(20) == 0 {
        a.layout.paired_with = Some(a.id); // paired with itself
    }
    Some(Command::UpsertArray {
        array: Box::new(a),
        at: rng.at(p.arrays.len()),
    })
}

fn random_set_modules(rng: &mut Rng, p: &Project) -> Option<Command> {
    let target = rng.pick(&p.arrays)?.id;
    let modules = match rng.below(4) {
        // Lay out again with a random subset (some modules gone for good).
        0 | 1 => slots(p, target)
            .into_iter()
            .filter(|_| rng.below(4) > 0)
            .collect(),
        // Take the modules of another array (often rejected: out of this roof).
        2 => slots(p, rng.pick(&p.arrays)?.id),
        _ => Vec::new(),
    };
    Some(Command::SetArrayModules {
        array_id: target,
        modules,
    })
}

fn random_module_command(rng: &mut Rng, p: &Project) -> Option<Command> {
    let n = p.modules.len() as u64;
    if n == 0 {
        return None;
    }
    let start = rng.below(n);
    let ids: Vec<ModuleId> = (0..1 + rng.below(4))
        .map(|j| ModuleId(((start + j) % n) as u32))
        .collect();
    Some(match rng.below(4) {
        0 => Command::MoveModules {
            ids,
            du: rng.signed(0.05),
            dv: rng.signed(0.05),
        },
        1 => Command::RemoveModules { ids },
        2 => Command::RestoreModules {
            ids: p
                .modules
                .iter()
                .filter(|m| m.state == ModuleState::Removed)
                .map(|m| m.id)
                .take(3)
                .collect(),
        },
        _ => Command::SetDiscount {
            manufacturer: "K2 Systems".into(),
            rate: (rng.below(3) > 0).then(|| rng.below(50) as f64 / 100.0),
        },
    })
}

fn random_command(rng: &mut Rng, p: &Project, depth: u32) -> Command {
    loop {
        let c = match rng.below(14) {
            0 | 1 => random_module_command(rng, p),
            2 => Some(random_building(rng, p)),
            3 => random_face(rng, p),
            4 | 5 => random_array(rng, p),
            6 | 7 => random_set_modules(rng, p),
            8 => rng
                .pick(&p.buildings)
                .map(|b| Command::RemoveBuilding { id: b.id }),
            9 => rng
                .pick(&p.roof_faces)
                .map(|f| Command::RemoveRoofFace { id: f.id }),
            10 | 11 => rng
                .pick(&p.arrays)
                .map(|a| Command::RemoveArray { id: a.id }),
            12 => Some(Command::RemoveArray {
                id: ArrayId(rng.uuid()), // unknown
            }),
            _ if depth == 0 => {
                // A batch whose commands each see the state left by the previous ones.
                let mut work = p.clone();
                let mut commands = Vec::new();
                for _ in 0..2 + rng.below(3) {
                    let c = random_command(rng, &work, depth + 1);
                    let _ = c.apply(&mut work);
                    commands.push(c);
                }
                Some(Command::Batch { commands })
            }
            _ => None,
        };
        if let Some(c) = c {
            return c;
        }
    }
}

#[test]
fn random_entity_sequences_undo_to_the_initial_state() {
    let base = StressPreset::Residential1k.build();
    let initial = bits(&base);
    let (mut accepted, mut rejected, mut structural) = (0usize, 0usize, 0usize);
    for seed in 0..500u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut p = base.clone();
        let mut done: Vec<(Command, Command)> = Vec::new();
        for step in 0..12 {
            let cmd = random_command(&mut rng, &p, 0);
            let before = bits(&p);
            let inverse = cmd.inverse(&p);
            match cmd.apply(&mut p) {
                Ok(changes) => {
                    accepted += 1;
                    structural += usize::from(changes.structure);
                    p.validate()
                        .unwrap_or_else(|e| panic!("seed {seed} step {step}: {e:?}\n{cmd:?}"));
                    done.push((cmd, inverse));
                }
                Err(e) => {
                    rejected += 1;
                    assert!(!e.key.is_empty());
                    assert_eq!(
                        bits(&p),
                        before,
                        "seed {seed} step {step}: changed on error"
                    );
                }
            }
        }
        let last = bits(&p);
        for (cmd, inverse) in done.iter().rev() {
            inverse
                .apply(&mut p)
                .unwrap_or_else(|e| panic!("seed {seed}: inverse rejected {e:?}\n{cmd:?}"));
            p.validate().expect("valid after undo");
        }
        assert_eq!(bits(&p), initial, "seed {seed}: undo is not exact");
        for (cmd, _) in &done {
            cmd.apply(&mut p).expect("redo");
        }
        assert_eq!(bits(&p), last, "seed {seed}: redo is not exact");
    }
    eprintln!("accepted {accepted}, rejected {rejected}, structural {structural}");
    assert!(accepted > 2000 && rejected > 300 && structural > 500);
}

fn residential() -> Project {
    StressPreset::Residential1k.build()
}

#[test]
fn removing_a_building_cascades_and_undo_restores_it() {
    let mut p = residential();
    let original = p.clone();
    let b = p.buildings[1].id;
    let faces: Vec<RoofFaceId> = p
        .roof_faces
        .iter()
        .filter(|f| f.building_id == b)
        .map(|f| f.id)
        .collect();
    assert!(!faces.is_empty());
    let cmd = Command::RemoveBuilding { id: b };
    let inverse = cmd.inverse(&p);
    let changes = cmd.apply(&mut p).expect("remove");
    assert!(changes.structure && changes.module_ids.is_empty());
    assert_eq!(changes.buildings, vec![b]);
    assert_eq!(changes.faces.len(), faces.len());
    assert!(p.roof_faces.iter().all(|f| f.building_id != b));
    assert!(p.modules.len() < original.modules.len());
    p.validate().expect("compacted");
    inverse.apply(&mut p).expect("undo");
    assert_eq!(bits(&p), bits(&original));
}

#[test]
fn removing_a_paired_array_unpairs_its_partner() {
    let mut p = StressPreset::Industrial10k.build();
    let original = p.clone();
    let (east, west) = (p.arrays[0].id, p.arrays[1].id);
    assert_eq!(p.arrays[1].layout.paired_with, Some(east));
    let cmd = Command::RemoveArray { id: east };
    let inverse = cmd.inverse(&p);
    let changes = cmd.apply(&mut p).expect("remove");
    assert_eq!(p.arrays.len(), 1);
    assert_eq!(p.arrays[0].layout.paired_with, None);
    assert!(changes.arrays.contains(&west), "the partner changed too");
    // The remaining modules were renumbered from 0.
    assert_eq!(p.modules[0].id, ModuleId(0));
    p.validate().expect("valid");
    inverse.apply(&mut p).expect("undo");
    assert_eq!(bits(&p), bits(&original));
}

#[test]
fn removing_a_face_with_two_paired_arrays_is_undone_exactly() {
    let mut p = StressPreset::Industrial10k.build();
    let face = p.arrays[0].roof_face_id;
    assert_eq!(p.arrays[1].roof_face_id, face, "both arrays on one face");
    let original = p.clone();
    let cmd = Command::RemoveRoofFace { id: face };
    let inverse = cmd.inverse(&p);
    cmd.apply(&mut p).expect("remove");
    assert!(p.arrays.is_empty() && p.modules.is_empty());
    inverse.apply(&mut p).expect("undo");
    assert_eq!(bits(&p), bits(&original));
}

#[test]
fn a_batch_is_all_or_nothing() {
    let mut p = residential();
    let before = bits(&p);
    let a = p.arrays[0].id;
    let batch = Command::Batch {
        commands: vec![
            Command::SetArrayModules {
                array_id: a,
                modules: vec![],
            },
            Command::RemoveArray {
                id: ArrayId(Uuid::from_u128(7)),
            },
        ],
    };
    let e = batch.apply(&mut p).expect_err("second command fails");
    assert_eq!(e.key, keys::ENTITY_UNKNOWN);
    assert_eq!(bits(&p), before);
}

#[test]
fn set_array_modules_renumbers_and_merges_changes() {
    let mut p = residential();
    let (a0, a1) = (p.arrays[0].id, p.arrays[1].id);
    let first = slots(&p, a0);
    let mut c = Command::SetArrayModules {
        array_id: a0,
        modules: first[..first.len() - 2].to_vec(),
    }
    .apply(&mut p)
    .expect("fewer modules");
    assert!(c.structure);
    p.validate().expect("canonical ids");
    // Merging a later module change: ids before a renumbering are dropped, structure stays.
    c.merge(ChangeSet::modules(vec![ModuleId(3)]));
    assert!(c.structure && c.module_ids.is_empty());
    assert_eq!(c.arrays, vec![a0]);
    // Modules that do not fit the roof are rejected.
    let mut far = slots(&p, a1);
    far[0].v = 1e6;
    let e = Command::SetArrayModules {
        array_id: a1,
        modules: far,
    }
    .apply(&mut p)
    .expect_err("off roof");
    assert_eq!(e.key, keys::OUT_OF_ROOF);
}

#[test]
fn upsert_validates_references_and_values() {
    let mut p = residential();
    let mut f = p.roof_faces[0].clone();
    f.id = RoofFaceId(Uuid::from_u128(1));
    f.building_id = BuildingId(Uuid::from_u128(2));
    let e = Command::UpsertRoofFace {
        face: Box::new(f.clone()),
        at: None,
    }
    .apply(&mut p)
    .expect_err("dangling building");
    assert_eq!(e.key, keys::ENTITY_REFERENCE);
    f.building_id = p.buildings[0].id;
    f.tilt_deg = 90.0;
    let e = Command::UpsertRoofFace {
        face: Box::new(f),
        at: None,
    }
    .apply(&mut p)
    .expect_err("vertical face");
    assert_eq!(e.key, keys::ENTITY_INVALID);
    // A new array inserted first: no module moves, ids stay canonical.
    let mut a = p.arrays[2].clone();
    a.id = ArrayId(Uuid::from_u128(3));
    let before = p.modules.clone();
    let cmd = Command::UpsertArray {
        array: Box::new(a),
        at: Some(0),
    };
    let inverse = cmd.inverse(&p);
    assert_eq!(
        inverse,
        Command::RemoveArray {
            id: ArrayId(Uuid::from_u128(3))
        }
    );
    cmd.apply(&mut p).expect("insert");
    assert_eq!(p.arrays[0].id, ArrayId(Uuid::from_u128(3)));
    assert_eq!(p.modules, before);
    p.validate().expect("valid");
}

#[test]
fn entity_command_json_round_trip() {
    let p = residential();
    let cmd = Command::Batch {
        commands: vec![
            Command::UpsertArray {
                array: Box::new(p.arrays[0].clone()),
                at: Some(0),
            },
            Command::SetArrayModules {
                array_id: p.arrays[0].id,
                modules: slots(&p, p.arrays[0].id)[..2].to_vec(),
            },
            Command::RemoveBuilding {
                id: p.buildings[0].id,
            },
        ],
    };
    let json = serde_json::to_string(&cmd).expect("serialise");
    assert!(json.starts_with(r#"{"type":"Batch","commands":[{"type":"UpsertArray""#));
    assert!(json.contains(r#""stringIndex":"#));
    let back: Command = serde_json::from_str(&json).expect("deserialise");
    assert_eq!(back, cmd);
}
