//! The compute thread: owns the graph, applies input batches, runs passes, emits results.
//!
//! ```text
//! command thread                         compute thread
//! ──────────────                         ──────────────
//! apply command (store lock)             loop:
//! batch = batch_for_changes(…)             recv() — blocks while idle
//! link.submit(seq, batch) ──── queue ──▶   drain the queue: N batches applied in order
//!   └─ cancels the running pass            install a new CancelToken, drain again
//!                                          State{busy} → run_auto(token)
//!                                            Ok        → Ready{seq, fields} → State{idle}
//!                                            Cancelled → loop again (never waits: a batch
//!                                                        drained just before the token was
//!                                                        installed may have cancelled it)
//!                                            panic     → Failed, graph rebuilt, next submit
//!                                                        sends the whole project
//! ```
//!
//! The latest command wins: a newer batch cancels the pass in progress, the graph stays
//! consistent (cancellation never stores anything) and the next pass resumes the work.

use std::mem;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;

use pv_graph::{CancelToken, Graph, GraphError};
use pv_model::command::keys;
use pv_model::{ArrayId, ChangeSet, CommandError, Project};

use crate::batch::{apply_batch, batch_for_changes, batch_for_project, InputBatch};
use crate::new_graph;
use crate::report::{bom_report, command_error, field_reports, BomReport, FieldReport};

/// What the compute thread tells the application.
#[derive(Debug, Clone, PartialEq)]
pub enum ComputeEvent {
    /// A pass starts (`busy`) or the thread is idle again, for command `seq`.
    State { busy: bool, seq: u32 },
    /// Fields whose statics changed; fields that no longer exist; `reset`: the graph was
    /// rebuilt (new project, recovery), every previous result is obsolete.
    Ready {
        seq: u32,
        fields: Vec<FieldReport>,
        /// The project's bill of materials, when it changed.
        bom: Option<BomReport>,
        removed: Vec<ArrayId>,
        reset: bool,
    },
    /// The pass could not run (applying a batch failed, or a computation panicked).
    Failed { seq: u32, error: CommandError },
}

/// Receives the compute thread's events (the Tauri layer emits them to the front end).
pub trait EventSink: Send + 'static {
    fn emit(&mut self, event: ComputeEvent);
}

impl<F: FnMut(ComputeEvent) + Send + 'static> EventSink for F {
    fn emit(&mut self, event: ComputeEvent) {
        self(event);
    }
}

enum Job {
    Apply {
        seq: u32,
        batch: Box<InputBatch>,
        reset: bool,
    },
    Shutdown,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding this lock cannot leave a token half-written.
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Sending side, used on the command thread. Cheap to clone.
#[derive(Clone)]
pub struct WorkerLink {
    tx: Sender<Job>,
    current: Arc<Mutex<CancelToken>>,
    needs_full: Arc<AtomicBool>,
}

impl WorkerLink {
    fn send(&self, job: Job) {
        // Send first, then cancel: the worker always finds the job after a cancellation.
        // A send error means the thread is gone (shutdown): nothing to compute any more.
        let _ = self.tx.send(job);
        lock(&self.current).cancel();
    }

    /// A project was opened: the graph is rebuilt from it.
    pub fn open(&self, seq: u32, project: &Project) {
        self.needs_full.store(false, Ordering::Release);
        self.send_batch(seq, batch_for_project(project), true);
    }

    /// A command changed the project. Extraction runs here (the caller holds the project);
    /// after a recovery the whole project is sent instead.
    pub fn submit(&self, seq: u32, project: &Project, changes: &ChangeSet) {
        if self.needs_full.swap(false, Ordering::AcqRel) {
            self.send_batch(seq, batch_for_project(project), true);
        } else {
            self.send_batch(seq, batch_for_changes(project, changes), false);
        }
    }

    /// Low-level: queues a batch and cancels the running pass.
    pub fn send_batch(&self, seq: u32, batch: InputBatch, reset: bool) {
        self.send(Job::Apply {
            seq,
            batch: Box::new(batch),
            reset,
        });
    }

    /// Stops the compute thread after the jobs already queued.
    pub fn shutdown(&self) {
        self.send(Job::Shutdown);
    }

    /// Test hook: cancels the running pass without queuing anything.
    #[doc(hidden)]
    pub fn cancel_current(&self) {
        lock(&self.current).cancel();
    }
}

/// What a pass reports: the fields whose statics changed, and the bill of materials if it
/// changed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PassOutput {
    pub fields: Vec<FieldReport>,
    pub bom: Option<BomReport>,
}

/// A pass: run the graph, collect the reports. Replaceable in tests.
pub type PassFn = dyn FnMut(&mut Graph, &CancelToken, u32) -> Result<PassOutput, GraphError> + Send;

/// The pass of the application.
///
/// # Errors
/// [`GraphError::Cancelled`].
pub fn default_pass(
    graph: &mut Graph,
    cancel: &CancelToken,
    seq: u32,
) -> Result<PassOutput, GraphError> {
    let ready = graph.run_auto(cancel)?;
    Ok(PassOutput {
        fields: field_reports(graph, &ready, seq),
        bom: bom_report(graph, &ready, seq),
    })
}

/// Receiving side: the compute loop.
pub struct WorkerQueue {
    rx: Receiver<Job>,
    current: Arc<Mutex<CancelToken>>,
    needs_full: Arc<AtomicBool>,
}

/// A link and its queue.
#[must_use]
pub fn channel() -> (WorkerLink, WorkerQueue) {
    let (tx, rx) = mpsc::channel();
    let current = Arc::new(Mutex::new(CancelToken::new()));
    let needs_full = Arc::new(AtomicBool::new(false));
    (
        WorkerLink {
            tx,
            current: Arc::clone(&current),
            needs_full: Arc::clone(&needs_full),
        },
        WorkerQueue {
            rx,
            current,
            needs_full,
        },
    )
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".to_owned())
}

impl WorkerQueue {
    /// Runs the loop on this thread until shutdown or until every link is dropped.
    pub fn run(self, sink: impl EventSink) {
        self.run_with_pass(sink, Box::new(default_pass));
    }

    /// [`WorkerQueue::run`] with another pass function (tests).
    #[doc(hidden)]
    pub fn run_with_pass(self, mut sink: impl EventSink, mut pass: Box<PassFn>) {
        let mut graph = new_graph();
        let mut seq = 0;
        let mut removed: Vec<ArrayId> = Vec::new();
        let mut reset = false;
        'idle: loop {
            let Ok(first) = self.rx.recv() else {
                return; // every link dropped
            };
            let mut jobs = vec![first];
            loop {
                jobs.extend(self.rx.try_iter());
                for job in jobs.drain(..) {
                    match job {
                        Job::Shutdown => return,
                        Job::Apply {
                            seq: s,
                            batch,
                            reset: r,
                        } => {
                            if r {
                                graph = new_graph();
                                removed.clear();
                                reset = true;
                            }
                            seq = s;
                            match apply_batch(&mut graph, *batch) {
                                Ok(report) => removed.extend(report.forgotten_arrays),
                                Err(e) => sink.emit(ComputeEvent::Failed {
                                    seq,
                                    error: command_error(&e),
                                }),
                            }
                        }
                    }
                }
                let token = CancelToken::new();
                *lock(&self.current) = token.clone();
                // A job sent before the token was installed cancelled the old one only.
                jobs.extend(self.rx.try_iter());
                if !jobs.is_empty() {
                    continue;
                }
                sink.emit(ComputeEvent::State { busy: true, seq });
                let outcome = catch_unwind(AssertUnwindSafe(|| pass(&mut graph, &token, seq)));
                match outcome {
                    Ok(Ok(PassOutput { fields, bom })) => {
                        sink.emit(ComputeEvent::Ready {
                            seq,
                            fields,
                            bom,
                            removed: mem::take(&mut removed),
                            reset: mem::take(&mut reset),
                        });
                    }
                    // A newer batch is queued, or the cancellation was spurious: loop again
                    // without waiting.
                    Ok(Err(GraphError::Cancelled)) => continue,
                    Ok(Err(e)) => sink.emit(ComputeEvent::Failed {
                        seq,
                        error: command_error(&e),
                    }),
                    Err(payload) => {
                        // The graph may be inconsistent: rebuild it from the whole project
                        // at the next command. The flag is raised *before* the event, so
                        // that a command reacting to it already sends the whole project.
                        graph = new_graph();
                        removed.clear();
                        reset = true;
                        self.needs_full.store(true, Ordering::Release);
                        sink.emit(ComputeEvent::Failed {
                            seq,
                            error: CommandError::new(keys::STATICS_INTERNAL)
                                .with_text("detail", panic_text(payload.as_ref())),
                        });
                    }
                }
                sink.emit(ComputeEvent::State { busy: false, seq });
                continue 'idle;
            }
        }
    }
}

/// The compute thread. Dropping the handle stops and joins it.
pub struct ComputeHandle {
    link: WorkerLink,
    thread: Option<JoinHandle<()>>,
}

impl ComputeHandle {
    /// Starts the compute thread.
    ///
    /// # Errors
    /// When the OS cannot create the thread.
    pub fn spawn(sink: impl EventSink) -> std::io::Result<Self> {
        let (link, queue) = channel();
        let thread = std::thread::Builder::new()
            .name("pv-compute".into())
            .spawn(move || queue.run(sink))?;
        Ok(Self {
            link,
            thread: Some(thread),
        })
    }

    #[must_use]
    pub fn link(&self) -> &WorkerLink {
        &self.link
    }
}

impl Drop for ComputeHandle {
    fn drop(&mut self) {
        self.link.shutdown();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
