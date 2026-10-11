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
//! 5. **Reused threads.** A small fixed pool waits for jobs instead of a thread
//!    being spawned per job. The engine's compiled-pattern cache is a
//!    `thread_local`, so a fresh thread would rebuild it every conversion, and
//!    -- because that cache leaks its patterns -- retain another copy each time.
//!    Reusing the pool threads keeps that cost to `max_threads` copies.
//!
//! Results travel back over a channel. In Qt the equivalent was a signal, and
//! the subtle failure was a sink parented to the window being destroyed while a
//! worker still held it; here dropping the receiver simply makes the send fail
//! and the worker carry on, so a late result during shutdown cannot crash.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
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

/// The state the pool threads share. It lives in its own `Arc`, separate from
/// [`ConversionWorker`], so a thread owns a reference that cannot dangle if the
/// front end drops its handle.
struct Shared {
    /// The waiting job and the counters. One lock, because `is_idle` must read
    /// "is a job waiting" and "is a thread busy" as one consistent pair.
    state: Mutex<State>,
    /// Signalled when a job arrives and when the pool should stop.
    ready: Condvar,
    converter: Arc<ConvertFn>,
    results: Sender<Outcome>,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

struct State {
    /// The single waiting job. At most one is ever held, so a burst of edits
    /// leaves one copy of the document waiting, not one per keystroke.
    pending: Option<Job>,
    /// Conversions running right now.
    in_flight: usize,
    /// Set by `Drop` to tell the threads to return.
    shutdown: bool,
}

/// Queues conversions and hands results back through [`ConversionWorker::poll`].
pub struct ConversionWorker {
    next_generation: AtomicU64,
    shared: Arc<Shared>,
    inbox: Mutex<Receiver<Outcome>>,
    /// Handles for the pool threads, joined on drop. Empty only if the OS
    /// refused every thread, which `convert` then reports per job.
    threads: Mutex<Vec<JoinHandle<()>>>,
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
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                pending: None,
                in_flight: 0,
                shutdown: false,
            }),
            ready: Condvar::new(),
            converter: Arc::new(converter),
            results,
            waker: Mutex::new(None),
        });

        // Long-lived threads, not one per job: see the module header. A refused
        // spawn shrinks the pool; `convert` handles a pool of none.
        let mut threads = Vec::new();
        for index in 0..max_threads.max(1) {
            let shared = Arc::clone(&shared);
            match std::thread::Builder::new()
                .name(format!("textrill-convert-{index}"))
                .spawn(move || worker_loop(&shared))
            {
                Ok(handle) => threads.push(handle),
                Err(_) => break,
            }
        }

        Arc::new(Self {
            next_generation: AtomicU64::new(0),
            shared,
            inbox: Mutex::new(inbox),
            threads: Mutex::new(threads),
        })
    }

    /// Set the callback that asks the UI to redraw when a result arrives.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *lock(&self.shared.waker) = Some(Arc::new(waker));
    }

    /// The newest generation handed out.
    pub fn generation(&self) -> u64 {
        self.next_generation.load(Ordering::SeqCst)
    }

    /// How many conversions are running right now.
    pub fn in_flight(&self) -> usize {
        lock(&self.shared.state).in_flight
    }

    /// Whether a job is waiting for a free thread.
    pub fn has_pending(&self) -> bool {
        lock(&self.shared.state).pending.is_some()
    }

    /// Whether there is no running and no waiting work.
    pub fn is_idle(&self) -> bool {
        let state = lock(&self.shared.state);
        state.in_flight == 0 && state.pending.is_none()
    }

    /// Queue a conversion, dropping whatever was waiting. Returns its generation.
    pub fn convert(&self, text: &str, opts: &Options) -> u64 {
        let generation = self.next_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let job = Job {
            generation,
            text: text.to_string(),
            opts: opts.clone(),
        };
        if lock(&self.threads).is_empty() {
            // The OS refused every thread. Report it rather than leave the UI on
            // "converting…" forever.
            let _ = self.shared.results.send(Outcome {
                generation,
                html: String::new(),
                error: Some(SPAWN_ERROR.to_string()),
                seconds: 0.0,
            });
            return generation;
        }
        {
            let mut state = lock(&self.shared.state);
            state.pending = Some(job);
        }
        self.shared.ready.notify_one();
        generation
    }

    /// Take the next finished conversion, or `None` if none is ready.
    pub fn poll(&self) -> Option<Outcome> {
        // The sender lives in `shared`, which the pool threads keep alive, so
        // the channel cannot be disconnected while the worker exists; `None`
        // just means no result is ready yet.
        lock(&self.inbox).try_recv().ok()
    }
}

/// One pool thread: wait for a job, run it off the lock, report it, repeat.
fn worker_loop(shared: &Arc<Shared>) {
    loop {
        let job = {
            let mut state = lock(&shared.state);
            while state.pending.is_none() && !state.shutdown {
                state = shared
                    .ready
                    .wait(state)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
            if state.shutdown {
                return;
            }
            state.in_flight += 1;
            match state.pending.take() {
                Some(job) => job,
                None => {
                    state.in_flight -= 1;
                    continue;
                }
            }
        };

        let outcome = run(shared.converter.as_ref(), job);
        let _ = shared.results.send(outcome);
        // Clone the waker out and release the lock before calling it, so a
        // panic inside a foreign callback cannot poison the mutex.
        let waker = lock(&shared.waker).clone();
        if let Some(waker) = waker {
            waker();
        }
        lock(&shared.state).in_flight -= 1;
    }
}

impl Drop for ConversionWorker {
    /// Stop the pool. Setting the flag under the lock and notifying wakes a
    /// waiting thread; a thread mid-conversion finishes it, then sees the flag.
    /// The state lock is released before joining, so a thread can still take it
    /// to update `in_flight`.
    fn drop(&mut self) {
        {
            let mut state = lock(&self.shared.state);
            state.shutdown = true;
        }
        self.shared.ready.notify_all();
        let mut threads = lock(&self.threads);
        for handle in threads.drain(..) {
            let _ = handle.join();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    /// Poll until `generation`'s outcome arrives, or fail.
    fn wait_for(worker: &ConversionWorker, generation: u64) -> Outcome {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Some(outcome) = worker.poll() {
                if outcome.generation == generation {
                    return outcome;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("generation {generation} never produced a result");
    }

    /// One conversion at a time, so the queue-drop contract cannot discard any.
    #[test]
    fn conversions_report_back_in_order() {
        let worker = ConversionWorker::with_converter(2, |text, _| text.to_uppercase());
        for i in 0..5u64 {
            let text = format!("job {i}");
            let generation = worker.convert(&text, &Options::default());
            let outcome = wait_for(&worker, generation);
            assert_eq!(outcome.generation, generation);
            assert_eq!(outcome.html, text.to_uppercase());
            assert!(outcome.error.is_none());
        }
        assert!(worker.is_idle(), "the pool should be idle after draining");
        assert_eq!(worker.in_flight(), 0);
    }

    /// The point of the pool: two sequential conversions run on the *same*
    /// thread, so the engine's thread-local caches are reused rather than
    /// rebuilt (and re-leaked) per job. A thread-per-job implementation fails
    /// this by construction.
    #[test]
    fn sequential_conversions_reuse_one_thread() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&seen);
        let worker = ConversionWorker::with_converter(1, move |_, _| {
            recorded.lock().unwrap().push(std::thread::current().id());
            String::new()
        });
        for _ in 0..3u64 {
            let generation = worker.convert("x", &Options::default());
            wait_for(&worker, generation);
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert!(
            seen.iter().all(|id| *id == seen[0]),
            "one pool thread should serve every conversion, saw {seen:?}"
        );
    }

    #[test]
    fn a_panicking_conversion_becomes_an_error() {
        let worker = ConversionWorker::with_converter(1, |_, _| panic!("boom"));
        let generation = worker.convert("x", &Options::default());
        let outcome = wait_for(&worker, generation);
        assert_eq!(outcome.error.as_deref(), Some(PANIC_MESSAGE));
        assert!(outcome.html.is_empty());
    }

    #[test]
    fn the_waker_fires_when_a_result_lands() {
        let count = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&count);
        let worker = ConversionWorker::with_converter(1, |_, _| "html".to_string());
        worker.set_waker(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        let generation = worker.convert("x", &Options::default());
        wait_for(&worker, generation);
        // The waker runs after the result is sent, so give it a moment to land.
        let deadline = Instant::now() + Duration::from_secs(10);
        while count.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(count.load(Ordering::SeqCst) >= 1, "the waker never fired");
    }
}
