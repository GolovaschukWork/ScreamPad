//! Real-time guarantee of the mixer (ADR 0006): rendering never allocates or frees memory.
//!
//! This binary installs an allocator that counts every allocation, reallocation and deallocation made while a flag is
//! armed, and arms it only around `Mixer::render`. It holds exactly one test so that no other thread of the test harness
//! allocates while the flag is armed.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use soundboard_desktop_lib::audio::clip::Clip;
use soundboard_desktop_lib::audio::mixer::{MAX_VOICES, mixer};

static ARMED: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

// SAFETY: every method forwards to the system allocator unchanged; the counters are only incremented on the side.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ARMED.load(Ordering::Relaxed) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ARMED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

fn counted<T>(work: impl FnOnce() -> T) -> (T, usize, usize) {
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    ARMED.store(true, Ordering::SeqCst);
    let result = work();
    ARMED.store(false, Ordering::SeqCst);
    (
        result,
        ALLOCATIONS.load(Ordering::Relaxed),
        DEALLOCATIONS.load(Ordering::Relaxed),
    )
}

#[test]
fn rendering_never_allocates_or_frees_memory() {
    // Sanity check: the counter really sees allocations, otherwise a zero below would prove nothing.
    let (_vec, allocations, _) = counted(|| Vec::<u8>::with_capacity(4096));
    assert!(allocations >= 1, "the counting allocator is not active");

    let (mut mixer, mut handle) = mixer(2, 48_000);
    let short = Clip::new(vec![0.01; 2 * 64], 2, 48_000).expect("short clip");
    let long = Clip::new(
        (0..2 * 4800).map(|i| ((i % 100) as f32) / 400.0).collect(),
        2,
        48_000,
    )
    .expect("long clip");
    let mut buffer = vec![0.0_f32; 2 * 256];

    // Warm up once outside the measured window, as the real callback would have run before any user action.
    mixer.render(&mut buffer);

    let mut total_allocations = 0;
    let mut total_deallocations = 0;
    for round in 0..600 {
        // Control side (allocation is allowed here): queue a mix of commands, including ones the callback must reject.
        match round % 6 {
            0 => {
                for _ in 0..(MAX_VOICES + 8) {
                    // More plays than voices: the surplus is rejected inside the callback.
                    let _ = handle.play(Arc::clone(&short), 0.5);
                }
            }
            1 => {
                let _ = handle.play(Arc::clone(&long), 1.0);
                let _ = handle.set_master_gain(0.8);
            }
            2 => {
                let _ = handle.stop_all();
            }
            3 => {
                let voice = handle.play(Arc::clone(&long), 1.0).expect("play");
                let _ = handle.stop(voice);
            }
            4 => {
                // The control side hands over its only reference and forgets the clip, as a real "play and move on" does.
                // If the callback dropped it, this is where memory would be freed inside `render`.
                let only_owner = Clip::new(vec![0.02; 2 * 32], 2, 48_000).expect("clip");
                let _ = handle.play(only_owner, 1.0);
            }
            _ => {
                let _ = handle.play(Arc::clone(&short), 1.5);
            }
        }

        // The callback (nothing may be allocated or freed here).
        let ((), allocations, deallocations) = counted(|| mixer.render(&mut buffer));
        total_allocations += allocations;
        total_deallocations += deallocations;

        // Control side again: free what the callback handed back and read the events.
        let _ = handle.drain();
    }

    assert_eq!(total_allocations, 0, "render allocated memory");
    assert_eq!(total_deallocations, 0, "render freed memory");
    assert_eq!(handle.stats().leaked_clips.load(Ordering::Relaxed), 0);
    assert!(
        handle.stats().rejected_plays.load(Ordering::Relaxed) > 0,
        "the rejection path was exercised"
    );
}
