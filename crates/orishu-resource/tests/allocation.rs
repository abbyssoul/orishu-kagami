//! A discriminator is validated before it is copied.
//!
//! This is a hostile-input property, not a micro optimisation. `apiVersion`
//! and `kind` are read before anything else about a document is trusted, so
//! they are the fields an oversized or malformed input reaches first. If
//! construction copied first and checked second, a caller holding a borrowed
//! megabyte would allocate a megabyte in order to reject it — and the bound
//! declared by `MAX_LEN` would describe only what is *stored*, not what an
//! attacker can make the process do.
//!
//! Asserting the eventual error variant cannot catch that: the wrong ordering
//! returns exactly the same `TooLong`. Counting allocations can, so this test
//! installs a counting allocator and measures the construction itself.
//!
//! It lives in its own test binary because `#[global_allocator]` is
//! process-wide, and the counter is thread-local so a parallel test or the
//! harness's own allocations cannot perturb a measurement.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use orishu_resource::{ApiVersion, Kind, ResourceError};

thread_local! {
    /// `const`-initialised so that reading it never itself allocates, which
    /// would recurse back into the allocator below.
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

// SAFETY: every method forwards to `System` unchanged. The only added work is
// incrementing a `Copy` thread-local counter, which allocates nothing and so
// cannot recurse.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn record() {
    // `try_with` because a thread tearing down may still allocate after its
    // thread-locals are gone; a missed count there cannot affect a
    // measurement taken inside a test body.
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// Run `body` and report how many allocations it made on this thread.
fn allocations_during<R>(body: impl FnOnce() -> R) -> (R, usize) {
    let before = ALLOCATIONS.with(Cell::get);
    let value = body();
    let after = ALLOCATIONS.with(Cell::get);
    (value, after - before)
}

#[test]
fn rejecting_an_oversized_borrowed_discriminator_copies_nothing() {
    // Built before measuring, so only the construction attempt is counted.
    let oversized = "a".repeat(ApiVersion::MAX_LEN + 1);
    let oversized: &str = &oversized;

    let (result, allocations) = allocations_during(|| ApiVersion::new(oversized));

    assert!(matches!(result, Err(ResourceError::TooLong { .. })));
    assert_eq!(
        allocations, 0,
        "an oversized discriminator must be refused against the borrowed value; copying it \
         first would let an untrusted input allocate past MAX_LEN in order to be rejected"
    );

    let oversized_kind = "A".repeat(Kind::MAX_LEN + 1);
    let (result, allocations) = allocations_during(|| Kind::new(oversized_kind.as_str()));
    assert!(matches!(result, Err(ResourceError::TooLong { .. })));
    assert_eq!(allocations, 0);
}

#[test]
fn rejecting_an_empty_discriminator_copies_nothing() {
    let (result, allocations) = allocations_during(|| ApiVersion::new(""));
    assert!(matches!(result, Err(ResourceError::Empty { .. })));
    assert_eq!(allocations, 0);
}

#[test]
fn a_rejected_malformed_discriminator_copies_at_most_its_own_bound() {
    // The syntax error echoes the offending value, which is a deliberate
    // single bounded copy: the length check has already passed, so what is
    // copied cannot exceed `MAX_LEN`.
    let malformed = "@".repeat(ApiVersion::MAX_LEN);
    let (result, allocations) = allocations_during(|| ApiVersion::new(malformed.as_str()));

    let Err(ResourceError::Syntax { found, .. }) = result else {
        panic!("expected a syntax error");
    };
    assert!(found.len() <= ApiVersion::MAX_LEN);
    assert_eq!(
        allocations, 1,
        "only the bounded echoed value should be allocated"
    );
}

#[test]
fn accepting_a_borrowed_discriminator_copies_it_exactly_once() {
    let (result, allocations) = allocations_during(|| ApiVersion::new("orishu.dev/v1"));
    assert!(result.is_ok());
    assert_eq!(
        allocations, 1,
        "a valid borrowed value is copied once, on the success path only"
    );
}

#[test]
fn accepting_an_owned_discriminator_moves_it_rather_than_copying_again() {
    let owned = String::from("kagami.catalog/v1");
    let (result, allocations) = allocations_during(|| ApiVersion::new(owned));

    assert!(result.is_ok());
    assert_eq!(
        allocations, 0,
        "an already-owned valid value must be moved; validating a borrowed view must not cost \
         the caller a second allocation"
    );
}

#[test]
fn the_counter_actually_observes_allocations() {
    // A guard against every assertion above passing because the allocator was
    // never installed or the counter never moves.
    let (_, allocations) = allocations_during(|| String::from("something heap-allocated"));
    assert!(
        allocations >= 1,
        "the counting allocator is not observing allocations, so the measurements above prove \
         nothing"
    );
}
