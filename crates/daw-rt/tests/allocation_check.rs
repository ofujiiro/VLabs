// SPDX-License-Identifier: GPL-3.0-or-later

//! Proves the allocation check works: it notices allocations inside a
//! real-time region, ignores them outside, and stops the program by default.

use std::hint::black_box;
use std::process::Command;

use daw_rt::{OnViolation, RtCheckedAllocator};

#[global_allocator]
static ALLOCATOR: RtCheckedAllocator = RtCheckedAllocator;

/// Set in the child process started by `abort_mode_stops_the_program`.
const CHILD_FLAG: &str = "DAW_RT_TEST_ABORT_CHILD";

#[test]
fn counts_allocations_inside_real_time_regions_only() {
    if std::env::var_os(CHILD_FLAG).is_some() {
        return; // Only the abort test runs in the child process.
    }
    daw_rt::set_on_violation(OnViolation::Count);
    let before = daw_rt::violation_count();

    // Outside a region: allocating is fine.
    drop(black_box(vec![1_u8; 64]));
    assert_eq!(daw_rt::violation_count(), before);

    // Inside a region: allocating and freeing are both violations.
    daw_rt::rt_region(|| drop(black_box(vec![1_u8; 64])));
    let expected = if daw_rt::CHECKS_ENABLED { 2 } else { 0 };
    assert_eq!(daw_rt::violation_count() - before, expected);

    // A permitted scope inside a region is not checked.
    let before = daw_rt::violation_count();
    daw_rt::rt_region(|| daw_rt::permit_alloc(|| drop(black_box(vec![1_u8; 64]))));
    assert_eq!(daw_rt::violation_count(), before);

    // Real-time code that only touches memory reserved beforehand is fine.
    let mut buffer = vec![0.0_f32; 512];
    let before = daw_rt::violation_count();
    daw_rt::rt_region(|| {
        for (index, sample) in buffer.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss, reason = "test signal")]
            let phase = index as f32;
            *sample = (phase * 0.01).sin();
        }
    });
    assert_eq!(daw_rt::violation_count(), before);
    black_box(&buffer);
}

#[test]
fn abort_mode_stops_the_program() {
    if std::env::var_os(CHILD_FLAG).is_some() {
        // In the child: allocate inside a region with the default behaviour.
        daw_rt::set_on_violation(OnViolation::Abort);
        daw_rt::rt_region(|| drop(black_box(Box::new(42_u64))));
        return; // Reached only when checks are compiled out.
    }
    if !daw_rt::CHECKS_ENABLED {
        return; // Release builds have no check to test.
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "abort_mode_stops_the_program",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_FLAG, "1")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !output.status.success(),
        "the child should have been stopped, but it exited normally"
    );
    assert!(
        stderr.contains("REAL-TIME VIOLATION"),
        "the child should explain why it stopped; its error output was:\n{stderr}"
    );
}
