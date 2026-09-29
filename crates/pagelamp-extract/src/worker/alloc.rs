//! The worker's memory cap: a global allocator that counts live heap bytes.
//!
//! Installed as `#[global_allocator]` by the `pagelamp` binary (the only process that runs
//! the worker). Counting is always on (two relaxed atomics per allocation); the cap is off
//! (0) except in the worker, which sets it before extracting. Past the cap the process exits
//! at once with `MEMORY_EXIT_CODE`: unwinding or even printing could allocate again.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{MEMORY_EXIT_CODE, exit_now};

static USED: AtomicUsize = AtomicUsize::new(0);
static CAP: AtomicUsize = AtomicUsize::new(0);

/// `System`, plus the live-byte count and the cap. Use as
/// `#[global_allocator] static A: CountingAllocator = CountingAllocator;`.
pub struct CountingAllocator;

impl CountingAllocator {
    fn charge(size: usize) {
        let used = USED.fetch_add(size, Ordering::Relaxed).saturating_add(size);
        let cap = CAP.load(Ordering::Relaxed);
        if cap != 0 && used > cap {
            exit_now(MEMORY_EXIT_CODE);
        }
    }

    fn release(size: usize) {
        USED.fetch_sub(size, Ordering::Relaxed);
    }
}

// SAFETY: every method forwards to `System` with the same arguments; the bookkeeping around
// it doesn't touch the memory.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        Self::charge(layout.size());
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract, which `System` shares.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        Self::charge(layout.size());
        // SAFETY: as for `alloc`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was allocated by this allocator (i.e. `System`) with `layout`.
        unsafe { System.dealloc(ptr, layout) };
        Self::release(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size > layout.size() {
            Self::charge(new_size - layout.size());
        } else {
            Self::release(layout.size() - new_size);
        }
        // SAFETY: as for `dealloc`, and `new_size` follows `GlobalAlloc::realloc`'s rules.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Cap the live heap at `bytes` (0: no cap). Takes effect when this allocator is the global one.
pub(crate) fn set_cap(bytes: u64) {
    CAP.store(
        usize::try_from(bytes).unwrap_or(usize::MAX),
        Ordering::Relaxed,
    );
}
