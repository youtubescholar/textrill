// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! Running conversions off the GUI thread.
//!
//! The concurrency contract, carried across from the retired Python worker in
//! intent:
//!
//! 1. **Generation counter.** Every request gets a number; the front end drops
//!    any result older than the newest request.
//! 2. **Queue-drop.** Only the newest job waits. Replacing the waiting slot
//!    discards the previous one, and with it its copy of the document. A queue
//!    that grows while the user types is a queue that grows in memory.
//! 3. **Bounded work.** At most `max_threads` conversions are ever in flight;
//!    the rest are dropped rather than accumulated.
//! 4. **Completion on every path.** A panic is caught and turned into an error
//!    result, so nothing can leave the front end waiting for a result that
//!    never comes.
//!
//! Results travel back over a channel. In Qt the equivalent was a signal, and
//! the subtle failure was a sink parented to the window being destroyed while a
//! worker still held it; here dropping the receiver simply makes the send fail
//! and the worker carry on, so a late result during shutdown cannot crash.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use textrill::convert::Converter as EngineConverter;
use textrill::options::Options;

/// What the user sees when an engine panic is caught.
pub const PANIC_MESSAGE: &str = "The converter stopped on invalid input.\n\n\
     This is a bug in textrill. The input is probably the cause.";

/// What the user sees when the OS refuses to start a worker thread.
const SPAWN_ERROR: &str = "Could not start a converter thread; the system may be out of resources.";

/// The result of one conversion.
pub struct Outcome {
    /// The request this answers.
    pub generation: u64,
    /// The HTML, or empty if `error` is set.
    pub html: String,
    /// The error message, if the conversion failed.
    pub error: Option<String>,
    /// How long the conversion took.
    pub seconds: f64,
}

struct Job {
    generation: u64,
    text: String,
    opts: Options,
}

type ConvertFn = dyn Fn(&str, &Options) -> String + Send + Sync;

/// Queues conversions and hands results back through [`ConversionWorker::poll`].
pub struct ConversionWorker {
    next_generation: AtomicU64,
    /// The single waiting job. At most one is ever held, so a burst of edits
    /// leaves one copy of the document waiting, not one per keystroke.
    pending: Mutex<Option<Job>>,
    in_flight: AtomicUsize,
    max_threads: usize,
    converter: Arc<ConvertFn>,
    inbox: Mutex<Receiver<Outcome>>,
    results: Sender<Outcome>,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

/// Lock a mutex, recovering from poisoning.
///
/// The worker never mutates shared state from inside a conversion, so a panic
/// cannot leave a half-written value behind. Recovering keeps a panic in a
/// waker callback from wedging the UI on its next lock.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl ConversionWorker {
    /// A worker that converts with the engine, allowing `max_threads` at once.
    pub fn new(max_threads: usize) -> Arc<Self> {
        Self::with_converter(max_threads, |text, opts| {
            let mut converter = EngineConverter::new(opts.clone());
            converter.convert_text(text)
        })
    }

    /// As [`ConversionWorker::new`], but with a caller-supplied conversion.
    ///
    /// This is the seam the panic test uses: the engine no longer has a
    /// documented input that panics, and the thing
    /// under test is the handler, not the defect.
    pub fn with_converter(
        max_threads: usize,
        converter: impl Fn(&str, &Options) -> String + Send + Sync + 'static,
    ) -> Arc<Self> {
        let (results, inbox) = mpsc::channel();
        Arc::new(Self {
            next_generation: AtomicU64::new(0),
            pending: Mutex::new(None),
            in_flight: AtomicUsize::new(0),
            max_threads: max_threads.max(1),
            converter: Arc::new(converter),
            inbox: Mutex::new(inbox),
            results,
            waker: Mutex::new(None),
        })
    }

    /// Set the callback that asks the UI to redraw when a result arrives.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *lock(&self.waker) = Some(Arc::new(waker));
    }

    /// The newest generation handed out.
    pub fn generation(&self) -> u64 {
        self.next_generation.load(Ordering::SeqCst)
    }

    /// How many conversions are running right now.
    pub fn in_flight(&self) -> usize {
        self.in_flight.load(Ordering::SeqCst)
    }

    /// Whether a job is waiting for a free thread.
    pub fn has_pending(&self) -> bool {
        lock(&self.pending).is_some()
    }

    /// Whether there is no running and no waiting work.
    pub fn is_idle(&self) -> bool {
        self.in_flight() == 0 && !self.has_pending()
    }

    /// Queue a conversion, dropping whatever was waiting. Returns its generation.
    pub fn convert(self: &Arc<Self>, text: &str, opts: &Options) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let job = Job {
            generation,
            text: text.to_string(),
            opts: opts.clone(),
        };
        *lock(&self.pending) = Some(job);
        self.dispatch();
        generation
    }

    /// Take the next finished conversion, or `None` if none is ready.
    pub fn poll(&self) -> Option<Outcome> {
        // The sender lives in this struct, so the channel cannot be
        // disconnected while the worker exists; `None` just means no result is
        // ready yet.
        lock(&self.inbox).try_recv().ok()
    }

    /// Start a thread for the waiting job, up to `max_threads`.
    ///
    /// Called from `convert` and again when a thread finishes, which is what
    /// closes the gap where a job arrives while every thread is busy.
    fn dispatch(self: &Arc<Self>) {
        loop {
            let active = self.in_flight.load(Ordering::SeqCst);
            if active >= self.max_threads {
                return;
            }
            if self
                .in_flight
                .compare_exchange(active, active + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                continue;
            }
            let job = lock(&self.pending).take();
            match job {
                Some(job) => {
                    let generation = job.generation;
                    let worker = Arc::clone(self);
                    let spawned = std::thread::Builder::new()
                        .name("textrill-convert".to_string())
                        .spawn(move || {
                            let outcome = run(&*worker.converter, job);
                            let _ = worker.results.send(outcome);
                            // Clone the waker out and release the lock before
                            // calling it, so a panic inside a foreign callback
                            // cannot poison the mutex.
                            let waker = lock(&worker.waker).clone();
                            if let Some(waker) = waker {
                                waker();
                            }
                            worker.in_flight.fetch_sub(1, Ordering::SeqCst);
                            worker.dispatch();
                        });
                    if spawned.is_err() {
                        // The OS refused a thread. Report it rather than leave
                        // the UI on "converting…" forever, and release the slot
                        // so later jobs can try again.
                        let _ = self.results.send(Outcome {
                            generation,
                            html: String::new(),
                            error: Some(SPAWN_ERROR.to_string()),
                            seconds: 0.0,
                        });
                        self.in_flight.fetch_sub(1, Ordering::SeqCst);
                        return;
                    }
                }
                None => {
                    self.in_flight.fetch_sub(1, Ordering::SeqCst);
                    return;
                }
            }
        }
    }
}

/// Run one job, turning a panic into an error result.
fn run(converter: &ConvertFn, job: Job) -> Outcome {
    let started = Instant::now();
    let result = catch_unwind(AssertUnwindSafe(|| converter(&job.text, &job.opts)));
    let seconds = started.elapsed().as_secs_f64();
    match result {
        Ok(html) => Outcome {
            generation: job.generation,
            html,
            error: None,
            seconds,
        },
        Err(_) => Outcome {
            generation: job.generation,
            html: String::new(),
            error: Some(PANIC_MESSAGE.to_string()),
            seconds,
        },
    }
}
