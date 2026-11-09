//! Batch extraction runs on the command thread: it must stay cheap.

use std::time::{Duration, Instant};

use pv_model::{ChangeSet, StressPreset};
use pv_pipeline::{batch_for_changes, batch_for_project};

#[test]
fn extracting_a_module_command_is_cheap() {
    // Industrial 10k: two east-west halves of ~5 000 modules each.
    let p = StressPreset::Industrial10k.build();
    let changes = ChangeSet {
        module_ids: vec![p.modules[0].id],
    };
    let batch = batch_for_changes(&p, &changes);
    assert_eq!(batch.modules.len(), 1);
    let n = batch.modules[0].1.len();
    assert!(n > 4_000, "{n}");
    assert!(batch.index.is_none() && batch.arrays.is_empty());

    let best = (0..7)
        .map(|_| {
            let t = Instant::now();
            let b = batch_for_changes(&p, &changes);
            let t = t.elapsed();
            assert_eq!(b.modules[0].1.len(), n);
            t
        })
        .min()
        .unwrap();
    eprintln!("batch of {n} modules: {best:?}");
    assert!(best < Duration::from_millis(1), "{best:?}");
}

#[test]
fn a_full_batch_carries_the_whole_project() {
    let p = StressPreset::Residential1k.build();
    let b = batch_for_project(&p);
    assert_eq!(b.arrays.len(), p.arrays.len());
    assert_eq!(
        b.modules.iter().map(|(_, m)| m.len()).sum::<usize>(),
        p.modules.len()
    );
    assert_eq!(b.index.unwrap().arrays.len(), p.arrays.len());
}
