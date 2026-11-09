//! The compute loop: one pass for many batches, the latest command wins, spurious
//! cancellations do not stall it, a panic is contained and recovered, the threaded handle
//! shuts down cleanly.

mod common;

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::*;
use pv_graph::Instance;
use pv_model::{ChangeSet, ModuleId, Project};
use pv_pipeline::worker::channel;
use pv_pipeline::{
    batch_for_project, new_graph, sync_project, ComputeEvent, ComputeHandle, FieldOutcome,
    FieldStatics, WorkerLink,
};

type Log = Arc<Mutex<Vec<ComputeEvent>>>;

fn busy_seqs(log: &[ComputeEvent]) -> Vec<u32> {
    log.iter()
        .filter_map(|e| match e {
            ComputeEvent::State { busy: true, seq } => Some(*seq),
            _ => None,
        })
        .collect()
}

fn ready(log: &[ComputeEvent]) -> Vec<(u32, usize, bool)> {
    log.iter()
        .filter_map(|e| match e {
            ComputeEvent::Ready {
                seq, fields, reset, ..
            } => Some((*seq, fields.len(), *reset)),
            _ => None,
        })
        .collect()
}

/// Moves module `id` by 2 mm and returns the change set.
fn nudge(p: &mut Project, id: u32) -> ChangeSet {
    p.modules[id as usize].u += 0.002;
    ChangeSet::modules(vec![ModuleId(id)])
}

#[test]
fn opening_a_project_computes_every_field() {
    let p = ref_a_roof1();
    let (link, queue) = channel();
    link.open(1, &p);
    drop(link);
    let log: Log = Arc::default();
    let sink = Arc::clone(&log);
    queue.run(move |e| sink.lock().unwrap().push(e));
    let log = log.lock().unwrap();
    assert_eq!(busy_seqs(&log), vec![1]);
    assert_eq!(ready(&log), vec![(1, 2, true)]);
    assert!(matches!(
        log.last(),
        Some(ComputeEvent::State {
            busy: false,
            seq: 1
        })
    ));
    let ComputeEvent::Ready { fields, bom, .. } = &log[1] else {
        panic!("{:?}", log[1]);
    };
    assert!(fields
        .iter()
        .all(|f| matches!(f.outcome, FieldOutcome::Ok { .. }) && f.seq == 1));
    // The bill of materials comes with the first results.
    let bom = bom.as_ref().expect("bill of materials");
    assert_eq!(bom.seq, 1);
    assert!(matches!(bom.outcome, pv_pipeline::BomOutcome::Ok { .. }));
}

#[test]
fn queued_batches_are_coalesced_into_one_pass() {
    let mut p = ref_a_roof1();
    let (link, queue) = channel();
    link.open(1, &p);
    let n = p.modules.len() as u32;
    for seq in 2..=6 {
        let changes = nudge(&mut p, (seq * 7) % n);
        link.submit(seq, &p, &changes);
    }
    drop(link);
    let log: Log = Arc::default();
    let sink = Arc::clone(&log);
    queue.run(move |e| sink.lock().unwrap().push(e));
    let log = log.lock().unwrap();
    // One pass, on the last state.
    assert_eq!(busy_seqs(&log), vec![6]);
    assert_eq!(ready(&log), vec![(6, 2, true)]);
    let ComputeEvent::Ready { fields, .. } = &log[1] else {
        panic!();
    };
    let mut full = new_graph();
    sync_project(&mut full, &p).unwrap();
    for f in fields {
        let FieldOutcome::Ok { result } = &f.outcome else {
            panic!("{f:?}");
        };
        assert_eq!(
            *result,
            *full
                .get::<FieldStatics>(Instance::Array(f.array_id))
                .unwrap()
        );
    }
}

/// A command arrives while a pass runs: the pass is cancelled, the newer state computed.
#[test]
fn the_latest_command_wins() {
    let mut p = ref_a_roof1();
    let (link, queue) = channel();
    link.open(1, &p);
    let log: Log = Arc::default();
    let sink_log = Arc::clone(&log);
    let sink_link: WorkerLink = link.clone();
    drop(link);
    let mut injected = false;
    queue.run(move |e| {
        match &e {
            ComputeEvent::State { busy: true, .. } if !injected => {
                injected = true;
                let changes = nudge(&mut p, 3);
                sink_link.submit(2, &p, &changes);
            }
            ComputeEvent::Ready { .. } => sink_link.shutdown(),
            _ => {}
        }
        sink_log.lock().unwrap().push(e);
    });
    let log = log.lock().unwrap();
    assert_eq!(busy_seqs(&log), vec![1, 2]);
    // Nothing is published for the cancelled pass; the open's reset flag is carried over.
    assert_eq!(ready(&log), vec![(2, 2, true)]);
}

/// A cancellation without a new batch (a batch drained just before the token was installed)
/// must not leave the loop waiting: the pass is simply run again.
#[test]
fn a_spurious_cancellation_does_not_stall() {
    let p = ref_a_roof1();
    let (link, queue) = channel();
    link.open(1, &p);
    let log: Log = Arc::default();
    let sink_log = Arc::clone(&log);
    let sink_link = link.clone();
    drop(link);
    let mut cancelled = false;
    queue.run(move |e| {
        match &e {
            ComputeEvent::State { busy: true, .. } if !cancelled => {
                cancelled = true;
                sink_link.cancel_current();
            }
            ComputeEvent::Ready { .. } => sink_link.shutdown(),
            _ => {}
        }
        sink_log.lock().unwrap().push(e);
    });
    let log = log.lock().unwrap();
    assert_eq!(busy_seqs(&log), vec![1, 1]);
    assert_eq!(ready(&log), vec![(1, 2, true)]);
}

#[test]
fn a_panic_is_contained_and_the_next_command_resyncs_everything() {
    let mut p = ref_a_roof1();
    let (link, queue) = channel();
    link.open(1, &p);
    let log: Log = Arc::default();
    let sink_log = Arc::clone(&log);
    let sink_link = link.clone();
    drop(link);
    let mut first = true;
    let pass = Box::new(
        move |g: &mut pv_graph::Graph, c: &pv_graph::CancelToken, seq| {
            assert!(!std::mem::take(&mut first), "injected failure");
            pv_pipeline::worker::default_pass(g, c, seq)
        },
    );
    queue.run_with_pass(
        move |e| {
            match &e {
                // The next command after the failure: only one module changed, but the
                // whole project must be sent.
                ComputeEvent::Failed { .. } => {
                    let changes = nudge(&mut p, 0);
                    sink_link.submit(2, &p, &changes);
                }
                ComputeEvent::Ready { .. } => sink_link.shutdown(),
                _ => {}
            }
            sink_log.lock().unwrap().push(e);
        },
        pass,
    );
    let log = log.lock().unwrap();
    let failed: Vec<_> = log
        .iter()
        .filter_map(|e| match e {
            ComputeEvent::Failed { seq, error } => Some((*seq, error.key.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(failed, vec![(1, "errors.statics.internal".to_owned())]);
    // Both fields recomputed from scratch, and the UI told to drop everything it had.
    assert_eq!(ready(&log), vec![(2, 2, true)]);
}

#[test]
fn deleted_fields_are_reported_as_removed() {
    let mut p = ref_a_roof1();
    let (link, queue) = channel();
    link.open(1, &p);
    let gone = p.arrays.pop().unwrap();
    p.modules.retain(|m| m.array_id != gone.id);
    link.send_batch(2, batch_for_project(&p), false);
    drop(link);
    let log: Log = Arc::default();
    let sink = Arc::clone(&log);
    queue.run(move |e| sink.lock().unwrap().push(e));
    let log = log.lock().unwrap();
    let removed: Vec<_> = log
        .iter()
        .filter_map(|e| match e {
            ComputeEvent::Ready { removed, .. } => Some(removed.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(removed, vec![vec![gone.id]]);
}

#[test]
fn the_threaded_handle_computes_and_shuts_down() {
    let mut p = ref_a_roof1();
    let (tx, rx) = mpsc::channel();
    let handle = ComputeHandle::spawn(move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    let wait_idle = |seq: u32| loop {
        match rx.recv_timeout(Duration::from_secs(60)).expect("event") {
            ComputeEvent::State {
                busy: false,
                seq: s,
            } if s == seq => break,
            ComputeEvent::Failed { error, .. } => panic!("{error:?}"),
            _ => {}
        }
    };
    handle.link().open(1, &p);
    wait_idle(1);
    let changes = nudge(&mut p, 10);
    handle.link().submit(2, &p, &changes);
    wait_idle(2);
    drop(handle); // joins the thread
}
