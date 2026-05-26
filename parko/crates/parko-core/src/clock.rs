use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Abstraction over wall-clock time for Kirra control and inference loops.
///
/// All timing logic inside ControlLoop calls `clock.now_ms()`; no direct
/// use of wall-clock APIs inside timing-sensitive code (ADL-004).
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// Production implementation backed by system time (UNIX epoch, milliseconds).
pub struct WallClock;

impl Clock for WallClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Manually advanceable clock for deterministic tests.
///
/// `MockClock` is `Clone` — the clone and original share the same `AtomicU64`
/// counter, so test code can call `advance()` on one handle while the other
/// is held inside a `ControlLoop`.
#[derive(Clone)]
pub struct MockClock {
    current_ms: Arc<AtomicU64>,
}

impl MockClock {
    pub fn new(start_ms: u64) -> Self {
        Self {
            current_ms: Arc::new(AtomicU64::new(start_ms)),
        }
    }

    /// Advance virtual time by `ms` milliseconds.
    ///
    /// Uses `fetch_add` so multiple `advance()` calls compose correctly and
    /// concurrent advances remain thread-safe.
    pub fn advance(&self, ms: u64) {
        self.current_ms.fetch_add(ms, Ordering::SeqCst);
    }
}

impl Clock for MockClock {
    fn now_ms(&self) -> u64 {
        self.current_ms.load(Ordering::SeqCst)
    }
}
