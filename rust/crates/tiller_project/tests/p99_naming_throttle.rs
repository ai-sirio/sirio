//! P99 exercise of `F-CORE-DOM-07`: the auto-naming throttle.
//!
//! The clause: automatic naming is throttled to at least 30 seconds *and*
//! at least 200 characters of new transcript growth, **except for the first
//! run**. Each case feeds growth below one threshold, above both, or on a
//! boundary, and observes whether a generated-name request would fire —
//! `should_request` is exactly that gate (the reference implementation is
//! `TillerCore/AutoNamingThrottle.swift`, whose first-run path returns true
//! unconditionally).

use std::time::{Duration, Instant};

use tiller_project::AutoNamingThrottle;

#[test]
fn the_first_run_is_never_throttled() {
    let now = Instant::now();
    let throttle = AutoNamingThrottle::default();
    assert!(
        throttle.should_request(now, 0),
        "first run, empty transcript"
    );
    assert!(
        throttle.should_request(now, 50),
        "first run, below the growth threshold — the clause exempts the \
         first run from both gates"
    );
    assert!(
        throttle.should_request(now, 5_000),
        "first run, above the growth threshold"
    );
}

#[test]
fn growth_below_each_threshold_is_throttled_and_above_both_requests() {
    let start = Instant::now();
    let mut throttle = AutoNamingThrottle::default();
    throttle.record_request(start, 1_000);

    // Below the time threshold, above the growth threshold: no request.
    assert!(!throttle.should_request(start + Duration::from_secs(29), 1_500));
    // Above the time threshold, below the growth threshold: no request.
    assert!(!throttle.should_request(start + Duration::from_secs(120), 1_199));
    // Below both: no request.
    assert!(!throttle.should_request(start + Duration::from_secs(5), 1_010));
    // Above both: request.
    assert!(throttle.should_request(start + Duration::from_secs(31), 1_300));
    // "At least" makes both boundaries inclusive: exactly 30 s and exactly
    // 200 characters of growth request.
    assert!(throttle.should_request(start + Duration::from_secs(30), 1_200));
    // A transcript that shrank (cleared pane) is not growth.
    assert!(!throttle.should_request(start + Duration::from_secs(60), 900));
}

#[test]
fn recording_a_request_resets_both_baselines() {
    let start = Instant::now();
    let mut throttle = AutoNamingThrottle::default();
    throttle.record_request(start, 300);
    assert!(
        !throttle.should_request(start + Duration::from_secs(31), 400),
        "growth is measured from the last request's transcript length"
    );
    throttle.record_request(start + Duration::from_secs(40), 600);
    assert!(
        !throttle.should_request(start + Duration::from_secs(69), 900),
        "the interval is measured from the last request's time"
    );
    assert!(throttle.should_request(start + Duration::from_secs(70), 800));
}
