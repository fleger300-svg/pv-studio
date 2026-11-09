//! G4 performance: statics recomputation after real commands, on the stress presets.
//!
//! Architecture budget: « Automatiquement veut dire en moins de 100 ms pour 1000 panneaux,
//! sans bloquer l'interface ». Measured per stage:
//!
//! - command thread: `ProjectStore::apply` (validation, journal) + batch extraction;
//! - compute thread: `apply_batch` + `run_auto`;
//! - end to end through the real thread: submit → `Ready` received.
//!
//! Run in release (the budgets are asserted only there):
//! `cargo test --release -p pv-pipeline --test budget -- --ignored --nocapture`
//! Set `PV_BUDGET_OUT=<file.json>` to save the measurements.

mod common;

use std::sync::mpsc;
use std::time::{Duration, Instant};

use pv_graph::CancelToken;
use pv_model::{ChangeSet, Command, ModuleId, Project, StressPreset};
use pv_pipeline::{apply_batch, batch_for_changes, batch_for_project, ComputeEvent, ComputeHandle};
use pv_store::{Author, ProjectStore};
use serde::Serialize;

const REPEATS: usize = 21;
const BUDGET_MS: f64 = 100.0;

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[derive(Debug, Default, Clone, Serialize)]
struct Stage {
    median_ms: f64,
    max_ms: f64,
}

fn stage(mut v: Vec<f64>) -> Stage {
    v.sort_by(f64::total_cmp);
    Stage {
        median_ms: v[v.len() / 2],
        max_ms: *v.last().unwrap_or(&0.0),
    }
}

#[derive(Debug, Serialize)]
struct Scenario {
    name: &'static str,
    modules_changed: usize,
    command: Stage,
    extract: Stage,
    compute: Stage,
    total: Stage,
    end_to_end: Stage,
}

#[derive(Debug, Serialize)]
struct PresetReport {
    preset: String,
    modules: usize,
    open_full_pass_ms: f64,
    scenarios: Vec<Scenario>,
}

/// Modules of the first array: 40 from the middle, and the eave row (first grid row).
fn targets(p: &Project) -> (Vec<ModuleId>, Vec<ModuleId>) {
    let a = p.arrays[0].id;
    let of: Vec<_> = p.modules.iter().filter(|m| m.array_id == a).collect();
    let mid = of.len() / 2;
    let forty = of[mid..mid + 40].iter().map(|m| m.id).collect();
    let eave = of.iter().filter(|m| m.row == 0).map(|m| m.id).collect();
    (forty, eave)
}

/// What one preset's measurements act on: the project file, a graph driven inline (per-stage
/// timings) and the real compute thread (end to end).
struct Bench {
    store: ProjectStore,
    graph: pv_graph::Graph,
    handle: ComputeHandle,
    rx: mpsc::Receiver<ComputeEvent>,
    seq: u32,
}

impl Bench {
    fn wait_idle(&self, seq: u32) {
        loop {
            match self
                .rx
                .recv_timeout(Duration::from_secs(60))
                .expect("event")
            {
                ComputeEvent::State {
                    busy: false,
                    seq: s,
                } if s == seq => break,
                ComputeEvent::Failed { error, .. } => panic!("{error:?}"),
                _ => {}
            }
        }
    }

    /// Runs `cmd` then undo, `REPEATS` times; measures every stage of both.
    fn scenario(&mut self, name: &'static str, cmd: &Command, modules_changed: usize) -> Scenario {
        let (mut c, mut x, mut k, mut t, mut e) = (vec![], vec![], vec![], vec![], vec![]);
        for i in 0..REPEATS * 2 {
            let t0 = Instant::now();
            let changes: ChangeSet = if i % 2 == 0 {
                self.store
                    .apply(cmd, Author::User)
                    .expect("command accepted")
            } else {
                self.store.undo().expect("undo")
            };
            let t1 = Instant::now();
            let batch = batch_for_changes(self.store.project(), &changes);
            let t2 = Instant::now();
            apply_batch(&mut self.graph, batch).unwrap();
            self.graph.run_auto(&CancelToken::new()).unwrap();
            let t3 = Instant::now();
            // The same change through the real compute thread.
            self.seq += 1;
            let s0 = Instant::now();
            self.handle
                .link()
                .submit(self.seq, self.store.project(), &changes);
            self.wait_idle(self.seq);
            e.push(ms(s0.elapsed()));
            c.push(ms(t1 - t0));
            x.push(ms(t2 - t1));
            k.push(ms(t3 - t2));
            t.push(ms(t3 - t0));
        }
        Scenario {
            name,
            modules_changed,
            command: stage(c),
            extract: stage(x),
            compute: stage(k),
            total: stage(t),
            end_to_end: stage(e),
        }
    }
}

fn measure(preset: StressPreset, dir: &tempfile::TempDir) -> PresetReport {
    let project = preset.build();
    let store =
        ProjectStore::create(&dir.path().join(format!("{preset:?}.pvproj")), project).unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = ComputeHandle::spawn(move |e| {
        let _ = tx.send(e);
    })
    .unwrap();
    handle.link().open(1, store.project());

    // Opening: the whole project, first pass.
    let t = Instant::now();
    let mut graph = pv_pipeline::new_graph();
    apply_batch(&mut graph, batch_for_project(store.project())).unwrap();
    graph.run_auto(&CancelToken::new()).unwrap();
    let open = ms(t.elapsed());
    let mut b = Bench {
        store,
        graph,
        handle,
        rx,
        seq: 1,
    };
    b.wait_idle(1);

    let (forty, eave) = targets(b.store.project());
    let n_eave = eave.len();
    let scenarios = vec![
        b.scenario(
            "move 40 modules (+2 mm)",
            &Command::MoveModules {
                ids: forty,
                du: 0.002,
                dv: 0.0,
            },
            40,
        ),
        b.scenario(
            "remove the eave row (zone set changes)",
            &Command::RemoveModules { ids: eave },
            n_eave,
        ),
    ];
    PresetReport {
        preset: format!("{preset:?}"),
        modules: b.store.project().modules.len(),
        open_full_pass_ms: open,
        scenarios,
    }
}

#[test]
#[ignore = "performance: run in release with --ignored"]
fn statics_recompute_budget() {
    let dir = tempfile::tempdir().unwrap();
    let reports: Vec<PresetReport> = [StressPreset::Residential1k, StressPreset::Industrial10k]
        .into_iter()
        .map(|p| measure(p, &dir))
        .collect();
    let release = !cfg!(debug_assertions);
    println!("\nbuild: {}", if release { "release" } else { "debug" });
    for r in &reports {
        println!(
            "\n{} ({} modules) — open, full pass: {:.1} ms",
            r.preset, r.modules, r.open_full_pass_ms
        );
        println!("| scenario (cmd + undo, median / max ms) | command | extract | compute | total | end to end |");
        println!("|---|---|---|---|---|---|");
        for s in &r.scenarios {
            let f = |s: &Stage| format!("{:.2} / {:.2}", s.median_ms, s.max_ms);
            println!(
                "| {} ({} modules) | {} | {} | {} | {} | {} |",
                s.name,
                s.modules_changed,
                f(&s.command),
                f(&s.extract),
                f(&s.compute),
                f(&s.total),
                f(&s.end_to_end)
            );
        }
    }
    if let Ok(path) = std::env::var("PV_BUDGET_OUT") {
        let json = serde_json::json!({
            "schema": "pv-studio-statics-budget/1",
            "build": if release { "release" } else { "debug" },
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "budgetMs": BUDGET_MS,
            "repeats": REPEATS,
            "presets": reports,
        });
        std::fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
        println!("\nwritten to {path}");
    }
    if release {
        for r in &reports {
            for s in &r.scenarios {
                assert!(
                    s.total.max_ms < BUDGET_MS && s.end_to_end.max_ms < BUDGET_MS,
                    "{} {}: {:?}",
                    r.preset,
                    s.name,
                    s
                );
            }
        }
    }
}
