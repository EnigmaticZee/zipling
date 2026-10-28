//! Optional CPU-time accounting per pipeline stage, for profiling.
//!
//! Set `ARCA_DEBUG_PROFILE=1` to print, at the end of a pack or unpack, how many
//! seconds of CPU each stage used (thread CPU time, so oversubscribed threads
//! are not overcounted), how often it ran and how many bytes
//! it handled. Times are exclusive: when a thread runs another stage inside a
//! span (rayon steals work while a thread waits on a join), that time is
//! charged to the inner stage only. Off by default; when off, a `span` costs
//! one relaxed load.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Mutex;

/// CPU time of the calling thread, in nanoseconds.
fn thread_cpu_ns() -> u64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: valid clock id and out-pointer.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
}

static STATE: AtomicU8 = AtomicU8::new(0); // 0 unknown, 1 off, 2 on
static TOTALS: Mutex<Option<HashMap<&'static str, (u64, u64, u64)>>> = Mutex::new(None);

thread_local! {
    /// Time spent in nested spans, per open span on this thread.
    static NESTED: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) };
}

pub fn enabled() -> bool {
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = std::env::var_os("ARCA_DEBUG_PROFILE").is_some();
            STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Times the enclosing scope under `name`, counting `bytes` of work.
pub struct Span {
    name: &'static str,
    bytes: u64,
    start: Option<u64>,
}

pub fn span(name: &'static str, bytes: u64) -> Span {
    let start = enabled().then(|| {
        NESTED.with(|n| n.borrow_mut().push(0));
        thread_cpu_ns()
    });
    Span { name, bytes, start }
}

impl Drop for Span {
    fn drop(&mut self) {
        if let Some(t) = self.start {
            let total = thread_cpu_ns().saturating_sub(t);
            let ns = NESTED.with(|n| {
                let mut n = n.borrow_mut();
                let inner = n.pop().unwrap_or(0);
                if let Some(parent) = n.last_mut() {
                    *parent += total;
                }
                total.saturating_sub(inner)
            });
            let mut g = TOTALS.lock().unwrap();
            let e = g.get_or_insert_with(HashMap::new).entry(self.name).or_default();
            e.0 += ns;
            e.1 += 1;
            e.2 += self.bytes;
        }
    }
}

/// Prints and clears the totals (no-op when profiling is off).
pub fn report(title: &str) {
    if !enabled() {
        return;
    }
    let Some(map) = TOTALS.lock().unwrap().take() else { return };
    let mut rows: Vec<_> = map.into_iter().collect();
    rows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
    eprintln!("-- profile: {title} (CPU seconds, calls, MB)");
    for (name, (ns, n, bytes)) in rows {
        eprintln!("  {:<24} {:>9.3} s {:>8} {:>10.1}", name, ns as f64 / 1e9, n, bytes as f64 / 1e6);
    }
}
