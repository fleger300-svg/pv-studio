//! E1 through the store: entity and structural commands are validated, journalled and
//! persisted per entity; the file reopened always equals the model in memory, and undo /
//! redo through the journal (JSON) are exact.

use pv_model::{ArrayId, BuildingId, Command, ModuleSlot, Project, RoofFaceId, StressPreset};
use pv_store::{Author, ProjectStore};
use uuid::Uuid;

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
    fn index(&mut self, len: usize) -> Option<usize> {
        (len > 0).then(|| self.below(len as u64) as usize)
    }
    fn uuid(&mut self) -> Uuid {
        Uuid::from_u64_pair(self.next(), self.next())
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

enum Op {
    Command(Command),
    Undo,
    Redo,
}

fn random_op(rng: &mut Rng, p: &Project) -> Op {
    let cmd = match rng.below(12) {
        0 => p
            .buildings
            .get(rng.index(p.buildings.len()).unwrap_or(0))
            .map(|b| {
                let mut b = b.clone();
                if rng.below(2) == 0 {
                    b.id = BuildingId(rng.uuid());
                }
                b.name = format!("B{}", rng.below(100));
                Command::UpsertBuilding {
                    building: Box::new(b),
                    at: Some(rng.below(4) as u32),
                }
            }),
        1 => rng.index(p.roof_faces.len()).map(|i| {
            let mut f = p.roof_faces[i].clone();
            if rng.below(2) == 0 {
                f.id = RoofFaceId(rng.uuid());
            }
            f.tilt_deg = (f.tilt_deg + 1.0).min(60.0);
            Command::UpsertRoofFace {
                face: Box::new(f),
                at: Some(rng.below(8) as u32),
            }
        }),
        2 | 3 => rng.index(p.arrays.len()).map(|i| {
            let mut a = p.arrays[i].clone();
            if rng.below(2) == 0 {
                a.id = ArrayId(rng.uuid());
                a.layout.paired_with = None;
            } else {
                a.layout.paired_with = rng
                    .index(p.arrays.len())
                    .map(|j| p.arrays[j].id)
                    .filter(|o| *o != a.id);
            }
            Command::UpsertArray {
                array: Box::new(a),
                at: Some(rng.below(8) as u32),
            }
        }),
        4..=6 => rng.index(p.arrays.len()).map(|i| {
            let id = p.arrays[i].id;
            Command::SetArrayModules {
                array_id: id,
                modules: slots(p, id)
                    .into_iter()
                    .filter(|_| rng.below(5) > 0)
                    .collect(),
            }
        }),
        7 => rng
            .index(p.buildings.len())
            .map(|i| Command::RemoveBuilding {
                id: p.buildings[i].id,
            }),
        8 => rng
            .index(p.roof_faces.len())
            .map(|i| Command::RemoveRoofFace {
                id: p.roof_faces[i].id,
            }),
        9 => rng
            .index(p.arrays.len())
            .map(|i| Command::RemoveArray { id: p.arrays[i].id }),
        10 => return Op::Undo,
        _ => return Op::Redo,
    };
    cmd.map_or(Op::Undo, Op::Command)
}

#[test]
fn entity_commands_persist_and_undo_exactly() {
    let dir = tempfile::tempdir().expect("tmp");
    let base = StressPreset::Residential1k.build();
    let initial = bits(&base);
    let mut accepted = 0;
    for seed in 0..40u64 {
        let path = dir.path().join(format!("e-{seed}.pvproj"));
        let mut store = ProjectStore::create(&path, base.clone()).expect("create");
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        for step in 0..15 {
            let op = random_op(&mut rng, store.project());
            let r = match &op {
                Op::Command(c) => store.apply(c, Author::User),
                Op::Undo => store.undo(),
                Op::Redo => store.redo(),
            };
            if r.is_ok() {
                accepted += 1;
            }
            store.project().validate().expect("invariants");
            if step % 5 == 4 {
                let memory = bits(store.project());
                drop(store);
                store = ProjectStore::open(&path).expect("reopen");
                assert_eq!(bits(store.project()), memory, "seed {seed} step {step}");
            }
        }
        // Undo everything through the journal, after a reopen.
        drop(store);
        let mut store = ProjectStore::open(&path).expect("reopen");
        while store.undo().is_ok() {}
        assert_eq!(bits(store.project()), initial, "seed {seed}: undo all");
        drop(store);
        let store = ProjectStore::open(&path).expect("reopen after undo");
        assert_eq!(
            bits(store.project()),
            initial,
            "seed {seed}: persisted undo"
        );
    }
    eprintln!("{accepted} operations accepted");
    assert!(accepted > 300, "{accepted}");
}

#[test]
fn a_rejected_layout_leaves_file_and_model_untouched() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("r.pvproj");
    let base = StressPreset::Residential1k.build();
    let mut store = ProjectStore::create(&path, base.clone()).expect("create");
    // Two copies of the same modules: they overlap.
    let a = base.arrays[0].id;
    let mut twice = slots(&base, a);
    twice.extend(slots(&base, a));
    let e = store
        .apply(
            &Command::SetArrayModules {
                array_id: a,
                modules: twice,
            },
            Author::User,
        )
        .expect_err("overlap");
    assert_eq!(e.to_string(), "errors.layout.moduleOverlap");
    assert_eq!(bits(store.project()), bits(&base));
    drop(store);
    let store = ProjectStore::open(&path).expect("reopen");
    assert_eq!(bits(store.project()), bits(&base));
    assert_eq!(store.history_state().expect("history"), (false, false));
}
