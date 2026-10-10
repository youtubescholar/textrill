// Copyright (C) 2026 the textrill authors.
//
// This program is free software: you can redistribute it and/or modify it
// under the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.  See the LICENSE file for the full text.

//! The concurrency contract, as tests.
//!
//! These are the Rust form of `BacklogTests` in the Python suite, plus the
//! panic-handler test. They use a converter that sleeps, so the race the
//! contract is about (a keystroke burst queuing faster than the workers drain)
//! is deterministic instead of depending on how slow the engine happens to be
//! on a given machine.

use std::sync::Arc;
use std::time::{Duration, Instant};

use textrill::options::Options;
use textrill_gui::worker::{ConversionWorker, Outcome};

/// A worker whose conversions take ~150 ms, so a tight burst piles up.
fn slow_worker(max_threads: usize) -> Arc<ConversionWorker> {
    ConversionWorker::with_converter(max_threads, |text, _opts| {
        std::thread::sleep(Duration::from_millis(150));
        text.to_ascii_uppercase()
    })
}

/// Collect results until the worker has been idle for a quiet moment.
fn drain_until_idle(worker: &Arc<ConversionWorker>, timeout: Duration) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut idle_since = None;
    while Instant::now() < deadline {
        while let Some(outcome) = worker.poll() {
            outcomes.push(outcome);
        }
        if worker.is_idle() {
            let since = idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() > Duration::from_millis(100) {
                break;
            }
        } else {
            idle_since = None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    outcomes
}

/// A burst must not run every queued conversion.
#[test]
fn a_burst_does_not_run_every_queued_conversion() {
    let worker = slow_worker(1);
    let queued = 8;
    for _ in 0..queued {
        worker.convert("word ", &Options::default());
    }
    let outcomes = drain_until_idle(&worker, Duration::from_secs(20));

    // One job is on the thread and one may be waiting behind it; the rest were
    // dropped when the next was queued.
    assert!(
        outcomes.len() <= 2,
        "{} of {} queued conversions ran; the queue is not being dropped",
        outcomes.len(),
        queued
    );
    assert!(!outcomes.is_empty(), "no conversion ever finished");
}

/// The last generation queued is the one whose result matters.
#[test]
fn the_newest_generation_is_the_one_that_survives() {
    let worker = slow_worker(1);
    let mut latest = 0;
    for _ in 0..6 {
        latest = worker.convert("word ", &Options::default());
    }
    let outcomes = drain_until_idle(&worker, Duration::from_secs(20));

    let newest = outcomes
        .iter()
        .map(|outcome| outcome.generation)
        .max()
        .expect("no conversion ever finished");
    assert_eq!(newest, 6, "the newest generation was dropped");
    assert_eq!(latest, 6);
}

/// After the burst, nothing is running and nothing is waiting.
#[test]
fn the_worker_returns_to_idle() {
    let worker = slow_worker(2);
    for _ in 0..4 {
        worker.convert("word ", &Options::default());
    }
    drain_until_idle(&worker, Duration::from_secs(20));

    assert!(worker.is_idle(), "work was left behind");
    assert_eq!(worker.in_flight(), 0);
}

/// A panic in the conversion is reported as an error, not left hanging.
#[test]
fn a_panicking_conversion_is_reported_not_hung() {
    let worker = ConversionWorker::with_converter(1, |_text, _opts| {
        panic!("injected for the worker panic handler test")
    });
    let generation = worker.convert("hello", &Options::default());
    let outcomes = drain_until_idle(&worker, Duration::from_secs(10));

    let outcome = outcomes
        .into_iter()
        .find(|outcome| outcome.generation == generation)
        .expect("the panicking job never delivered a result");
    let error = outcome.error.expect("a panic must be reported as an error");
    assert!(
        error.contains("stopped on invalid input"),
        "unexpected error: {error}"
    );
    assert!(worker.is_idle(), "the worker stayed busy after a panic");
}
