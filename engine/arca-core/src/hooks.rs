//! Progress reporting and cancellation for long-running operations.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Called with (bytes done, bytes total). May be called from any thread.
pub type ProgressFn = Arc<dyn Fn(u64, u64) + Send + Sync>;

#[derive(Clone, Default)]
pub struct Hooks {
    pub progress: Option<ProgressFn>,
    /// Set to `true` to stop early; the operation then fails with "cancelled".
    pub cancel: Option<Arc<AtomicBool>>,
}

impl Hooks {
    pub fn report(&self, done: u64, total: u64) {
        if let Some(p) = &self.progress {
            p(done, total);
        }
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }

    pub fn check(&self) -> anyhow::Result<()> {
        anyhow::ensure!(!self.cancelled(), "cancelled");
        Ok(())
    }
}

impl std::fmt::Debug for Hooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hooks")
            .field("progress", &self.progress.is_some())
            .field("cancel", &self.cancel.is_some())
            .finish()
    }
}
