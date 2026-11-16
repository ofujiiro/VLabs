// SPDX-License-Identifier: GPL-3.0-or-later

#![doc = include_str!("../README.md")]
// The allocator wrapper has to implement `GlobalAlloc`, an unsafe trait.
#![allow(
    unsafe_code,
    reason = "a global allocator must implement the unsafe GlobalAlloc trait"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

/// What happens when real-time code allocates or frees memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnViolation {
    /// Print what happened and where, then stop the program. The default:
    /// a mistake on the audio thread must be impossible to miss.
    Abort,
    /// Count the violation and carry on. Tests use this to assert that a
    /// piece of processing allocated nothing, with a readable failure.
    Count,
}

const MODE_ABORT: u8 = 0;
const MODE_COUNT: u8 = 1;

static MODE: AtomicU8 = AtomicU8::new(MODE_ABORT);
static VIOLATIONS: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// How many real-time regions the current thread is inside.
    static RT_DEPTH: Cell<u32> = const { Cell::new(0) };
    /// How many `permit_alloc` scopes the current thread is inside.
    static PERMIT_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Whether this build checks real-time regions at all.
///
/// The check runs in debug builds (the ones developers and tests use) and is
/// compiled away in release builds, where it would only cost time.
pub const CHECKS_ENABLED: bool = cfg!(debug_assertions);

/// The memory allocator to install in every program and test that runs
/// real-time code:
///
/// ```ignore
/// #[global_allocator]
/// static ALLOCATOR: daw_rt::RtCheckedAllocator = daw_rt::RtCheckedAllocator;
/// ```
///
/// It hands every request to the system allocator, after checking (in debug
/// builds) that the current thread is not inside a real-time region.
pub struct RtCheckedAllocator;

// SAFETY: every method forwards to the system allocator with the same
// arguments, so the GlobalAlloc contract is upheld by `System`. The extra
// check only reads thread-local counters and never allocates on its normal
// path; when it reports a violation it first marks the thread as reporting,
// so the allocations made while printing do not re-enter the check.
unsafe impl GlobalAlloc for RtCheckedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        check("allocated memory");
        // SAFETY: forwarded unchanged; the caller upholds the contract.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        check("allocated memory");
        // SAFETY: forwarded unchanged; the caller upholds the contract.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        check("freed memory");
        // SAFETY: forwarded unchanged; the caller upholds the contract.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        check("reallocated memory");
        // SAFETY: forwarded unchanged; the caller upholds the contract.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[inline]
fn check(what: &'static str) {
    if !CHECKS_ENABLED {
        return;
    }
    // `try_with` because an allocation can happen while the thread is being
    // torn down; the counters have no destructor, so this always succeeds in
    // practice.
    let in_rt = RT_DEPTH.try_with(Cell::get).unwrap_or(0) > 0;
    if !in_rt {
        return;
    }
    let permitted = PERMIT_DEPTH.try_with(Cell::get).unwrap_or(0) > 0;
    if permitted {
        return;
    }
    violation(what);
}

#[cold]
#[inline(never)]
fn violation(what: &'static str) {
    VIOLATIONS.fetch_add(1, Ordering::Relaxed);
    if MODE.load(Ordering::Relaxed) == MODE_COUNT {
        return;
    }
    // Leave the real-time region for good on this thread, so that printing
    // the report (which allocates) does not report itself.
    let _ = RT_DEPTH.try_with(|depth| depth.set(0));
    report_and_abort(what);
}

#[cold]
#[allow(
    clippy::print_stderr,
    reason = "the whole point is to fail loudly; this runs just before aborting"
)]
fn report_and_abort(what: &'static str) -> ! {
    let backtrace = std::backtrace::Backtrace::force_capture();
    eprintln!(
        "\n*** REAL-TIME VIOLATION ***\n\
         Code running on the audio thread {what}. Allocating or freeing memory there \
         can cause audible dropouts, so debug builds stop here.\n\
         Where it happened:\n{backtrace}"
    );
    std::process::abort();
}

/// Chooses what happens when real-time code allocates. Applies to every
/// thread. The default is [`OnViolation::Abort`].
pub fn set_on_violation(mode: OnViolation) {
    let value = match mode {
        OnViolation::Abort => MODE_ABORT,
        OnViolation::Count => MODE_COUNT,
    };
    MODE.store(value, Ordering::Relaxed);
}

/// How many violations have been counted since the program started (only
/// counted while [`OnViolation::Count`] is selected, and only in builds where
/// [`CHECKS_ENABLED`] is true).
pub fn violation_count() -> usize {
    VIOLATIONS.load(Ordering::Relaxed)
}

/// Marks the current thread as running real-time code until it is dropped.
///
/// Regions nest. A guard cannot be sent to another thread: it describes the
/// thread that created it.
#[must_use = "the real-time region ends as soon as the guard is dropped"]
pub struct RtGuard {
    /// Makes the guard `!Send` and `!Sync`.
    _thread_bound: PhantomData<*const ()>,
}

impl RtGuard {
    /// Enters a real-time region on the current thread.
    #[inline]
    pub fn enter() -> Self {
        if CHECKS_ENABLED {
            RT_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
        }
        Self {
            _thread_bound: PhantomData,
        }
    }
}

impl Drop for RtGuard {
    #[inline]
    fn drop(&mut self) {
        if CHECKS_ENABLED {
            let _ = RT_DEPTH.try_with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
}

/// Runs `f` as real-time code: in debug builds, any allocation inside it is
/// reported (see [`set_on_violation`]).
#[inline]
pub fn rt_region<R>(f: impl FnOnce() -> R) -> R {
    let _guard = RtGuard::enter();
    f()
}

/// Runs `f` with the allocation check suspended, inside a real-time region.
///
/// For code we do not control and cannot fix, such as a third-party plugin's
/// processing call. Never use it to silence a problem in our own code.
#[inline]
pub fn permit_alloc<R>(f: impl FnOnce() -> R) -> R {
    struct Permit;
    impl Drop for Permit {
        fn drop(&mut self) {
            let _ = PERMIT_DEPTH.try_with(|depth| depth.set(depth.get().saturating_sub(1)));
        }
    }
    if CHECKS_ENABLED {
        PERMIT_DEPTH.with(|depth| depth.set(depth.get().saturating_add(1)));
    }
    let _permit = Permit;
    f()
}

/// Whether the current thread is inside a real-time region (always `false`
/// when [`CHECKS_ENABLED`] is false).
pub fn in_rt_region() -> bool {
    CHECKS_ENABLED && RT_DEPTH.try_with(Cell::get).unwrap_or(0) > 0
}

#[cfg(test)]
mod tests {
    use super::{RtGuard, in_rt_region, rt_region};

    #[test]
    fn regions_nest_and_end_with_their_guard() {
        assert!(!in_rt_region());
        {
            let _outer = RtGuard::enter();
            assert_eq!(super::in_rt_region(), super::CHECKS_ENABLED);
            rt_region(|| assert_eq!(in_rt_region(), super::CHECKS_ENABLED));
            assert_eq!(in_rt_region(), super::CHECKS_ENABLED);
        }
        assert!(!in_rt_region());
    }

    #[test]
    fn regions_belong_to_one_thread() {
        let _guard = RtGuard::enter();
        let other = std::thread::spawn(in_rt_region).join().unwrap();
        assert!(!other, "a region on one thread must not affect another");
    }
}
