//! The stress presets compute end to end (the UI panel and G4 measure them): every field
//! gets a result, no error.

use std::time::Instant;

use pv_graph::{CancelToken, Instance};
use pv_model::StressPreset;
use pv_pipeline::{new_graph, sync_project, FieldStatics};

#[test]
fn stress_presets_compute_without_error() {
    for preset in [StressPreset::Residential1k, StressPreset::Industrial10k] {
        let p = preset.build();
        let mut g = new_graph();
        let t = Instant::now();
        sync_project(&mut g, &p).unwrap();
        let ready = g.run_auto(&CancelToken::new()).unwrap();
        eprintln!(
            "{preset:?}: {} modules, first pass {:?}",
            p.modules.len(),
            t.elapsed()
        );
        // Every field, and the bill of materials.
        assert_eq!(ready.len(), p.arrays.len() + 1);
        let bom = g
            .get::<pv_pipeline::nodes::bom::ProjectBom>(Instance::Project)
            .unwrap_or_else(|e| panic!("{preset:?} BOM: {e}"));
        // Rails and clamps on the barns; the east-west system has no parts in its preset.
        assert_eq!(
            bom.total.lines.is_empty(),
            preset == StressPreset::Industrial10k
        );
        for a in &p.arrays {
            let s = g
                .get::<FieldStatics>(Instance::Array(a.id))
                .unwrap_or_else(|e| panic!("{preset:?}: {e}"));
            match preset {
                StressPreset::Residential1k => {
                    assert!(!s.decisions.is_empty() && s.fixations.unwrap() > 0);
                }
                StressPreset::Industrial10k => {
                    // One half carries the ballast of the east-west system.
                    assert!(s.decisions.is_empty());
                }
            }
        }
        if preset == StressPreset::Industrial10k {
            let blocks: u32 = p
                .arrays
                .iter()
                .filter_map(|a| {
                    g.get::<FieldStatics>(Instance::Array(a.id))
                        .unwrap()
                        .ballast_blocks
                })
                .sum();
            assert!(blocks > 0);
        }
    }
}
