//! C2: component commands. Adopting, updating, assigning and forgetting components; every
//! inverse restores the project bit for bit; compatibility is checked on assignment.

use pv_model::command::keys;
use pv_model::component::{presets, ComponentSpec};
use pv_model::{
    Command, ComponentSnapshot, MountingSystem, Project, StressPreset, SupportGridKind,
    SupportGridSpec,
};

fn bits(p: &Project) -> String {
    serde_json::to_string(p).expect("serialise")
}

fn catalogue() -> Vec<ComponentSnapshot> {
    presets::k2_single_rail_parts()
        .into_iter()
        .chain([presets::lr7_54hvh_490m(), presets::k2_single_rail()])
        .map(|component| ComponentSnapshot {
            component,
            price: None,
        })
        .collect()
}

/// Applies `cmd`, checks the invariants, then that its inverse restores the exact state.
fn apply_and_check_undo(p: &mut Project, cmd: &Command) {
    let before = bits(p);
    let inverse = cmd.inverse(p);
    cmd.apply(p).expect("apply");
    p.validate().expect("invariants");
    let after = bits(p);
    let mut undone = p.clone();
    inverse.apply(&mut undone).expect("inverse");
    assert_eq!(bits(&undone), before, "undo of {cmd:?}");
    let redo = cmd.clone();
    redo.apply(&mut undone).expect("redo");
    assert_eq!(bits(&undone), after, "redo");
}

/// Residential preset with standing seams under array 0.
fn project() -> Project {
    let mut p = StressPreset::Residential1k.build();
    let face = p.arrays[0].roof_face_id;
    let f = p
        .roof_faces
        .iter_mut()
        .find(|f| f.id == face)
        .expect("face");
    f.support_grid = Some(SupportGridSpec {
        kind: SupportGridKind::StandingSeam,
        origin_m: 0.0,
        spacing_m: 0.6,
    });
    p
}

#[test]
fn adopt_assign_update_forget_with_exact_undo() {
    let mut p = project();
    let adopt = Command::AdoptComponents {
        snapshots: catalogue(),
    };
    apply_and_check_undo(&mut p, &adopt);
    // Adopting again changes nothing.
    assert!(adopt.apply(&mut p.clone()).expect("again").is_empty());
    assert!(p
        .components
        .windows(2)
        .all(|w| w[0].component.id < w[1].component.id));

    let assign = Command::SetArrayComponents {
        array_id: p.arrays[0].id,
        module: Some(presets::LR7_54HVH_490M),
        mounting: Some(presets::K2_SINGLE_RAIL),
    };
    apply_and_check_undo(&mut p, &assign);
    let a = &p.arrays[0];
    assert_eq!(
        (a.module_length_m, a.module_width_m),
        (1.8, 1.134),
        "datasheet copied"
    );
    let resolved = p.resolve_mounting(a).expect("resolve").expect("system");
    assert_eq!(resolved.rail_lengths(), vec![4.8, 2.4]);
    assert_eq!(
        MountingSystem {
            rail_options: Vec::new(),
            ..resolved
        },
        MountingSystem::k2_single_rail()
    );

    // A new revision of the module: the arrays using it follow its dimensions.
    let mut module = p
        .component(presets::LR7_54HVH_490M)
        .expect("module")
        .clone();
    module.component.revision = 2;
    if let ComponentSpec::Module(m) = &mut module.component.spec {
        m.length_m = 1.762;
        m.mass_kg = 21.0;
    }
    let update = Command::UpdateComponents {
        snapshots: vec![module],
    };
    let changes = update.apply(&mut p.clone()).expect("update");
    assert!(changes.components);
    assert_eq!(changes.arrays, vec![p.arrays[0].id]);
    apply_and_check_undo(&mut p, &update);
    assert_eq!(p.arrays[0].module_length_m, 1.762);
    let resolved = p
        .resolve_mounting(&p.arrays[0])
        .expect("resolve")
        .expect("system");
    assert_eq!(resolved.module_mass_kg, 21.0);

    // Components in use cannot be forgotten.
    let err = Command::ForgetComponents {
        ids: vec![presets::K2_SEAM_CLAMP],
    }
    .apply(&mut p.clone())
    .expect_err("used by the system");
    assert_eq!(err.key, keys::ENTITY_REFERENCE);

    // Unassigned, everything can go.
    let unassign = Command::SetArrayComponents {
        array_id: p.arrays[0].id,
        module: None,
        mounting: None,
    };
    apply_and_check_undo(&mut p, &unassign);
    let ids: Vec<_> = catalogue().iter().map(|s| s.component.id).collect();
    apply_and_check_undo(&mut p, &Command::ForgetComponents { ids });
}

#[test]
fn invalid_adoptions_and_assignments_are_refused() {
    let mut p = project();
    let before = bits(&p);

    // A system without its parts.
    let err = Command::AdoptComponents {
        snapshots: vec![ComponentSnapshot {
            component: presets::k2_single_rail(),
            price: None,
        }],
    }
    .apply(&mut p)
    .expect_err("parts missing");
    assert_eq!(err.key, keys::COMPONENT_UNKNOWN);
    assert_eq!(bits(&p), before, "untouched on error");

    Command::AdoptComponents {
        snapshots: catalogue(),
    }
    .apply(&mut p)
    .expect("adopt");

    // Another copy under the same id: refused (UpdateComponents is the way).
    let mut other = catalogue();
    other[0].component.label = "changed".into();
    let err = Command::AdoptComponents { snapshots: other }
        .apply(&mut p)
        .expect_err("conflict");
    assert_eq!(err.key, keys::COMPONENT_EXISTS);

    let assign = |module, mounting| Command::SetArrayComponents {
        array_id: p.arrays[0].id,
        module,
        mounting,
    };
    // A system needs a module.
    let err = assign(None, Some(presets::K2_SINGLE_RAIL))
        .apply(&mut p.clone())
        .expect_err("no module");
    assert_eq!(err.key, keys::COMPONENT_UNKNOWN);
    // A part is not a module.
    let err = assign(Some(presets::K2_SEAM_CLAMP), None)
        .apply(&mut p.clone())
        .expect_err("not a module");
    assert_eq!(err.key, keys::COMPONENT_WRONG_FAMILY);
    // Standing-seam system on rafters.
    let face = p.arrays[0].roof_face_id;
    let mut on_rafters = p.clone();
    if let Some(f) = on_rafters.roof_faces.iter_mut().find(|f| f.id == face) {
        f.support_grid = Some(SupportGridSpec {
            kind: SupportGridKind::Rafters,
            origin_m: 0.0,
            spacing_m: 0.8,
        });
    }
    let err = assign(Some(presets::LR7_54HVH_490M), Some(presets::K2_SINGLE_RAIL))
        .apply(&mut on_rafters)
        .expect_err("rafters");
    assert_eq!(err.key, keys::COMPAT_SUPPORT);
}

#[test]
fn component_commands_round_trip_as_json() {
    let cmd = Command::SetArrayComponents {
        array_id: pv_model::ArrayId::new(),
        module: Some(presets::LR7_54HVH_490M),
        mounting: None,
    };
    let json = serde_json::to_string(&cmd).expect("ser");
    assert!(
        json.starts_with(r#"{"type":"SetArrayComponents""#),
        "{json}"
    );
    assert_eq!(serde_json::from_str::<Command>(&json).expect("de"), cmd);
    let adopt = Command::AdoptComponents {
        snapshots: catalogue(),
    };
    let back: Command =
        serde_json::from_str(&serde_json::to_string(&adopt).expect("ser")).expect("de");
    assert_eq!(back, adopt);
}
