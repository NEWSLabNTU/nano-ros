// Issue 2981.
//! A peer thread that cancels a running `spin` and cannot lose the race.
//!
//! `spin`, `spin_period` and every loop that goes through `enter_spin_loop`
//! CLEAR the halt flag on entry, on purpose: a cancel requested before this
//! spin is not a cancel of this spin. So a peer that sleeps for a fixed time
//! and stores `true` once is racing the spinning thread's ENTRY. If that thread
//! is descheduled past the sleep (a loaded host does that), the store lands
//! first, the entry clears it, and an untimed spin never returns. On host 142,
//! under a load average around 20, that hung `node-std-tests` for its whole
//! 2400 s budget.
//!
//! The peer therefore keeps re-asserting the cancel every millisecond until
//! the test says the spin has returned. The clear on entry happens once, so a
//! later store always wins, and no `unsafe` access to the executor is needed
//! to watch it.
//!
//! Use:
//! ```ignore
//! let peer = CancelPeer::after(exec.halt_flag(), Duration::from_millis(50));
//! exec.spin(SpinOptions::default())?;
//! peer.finish(); // joins; panics if the peer did
//! ```

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// The executor's cross-thread cancel handle, as `Executor::halt_flag` returns it.
pub(crate) type Halt = portable_atomic_util::Arc<portable_atomic::AtomicBool>;

pub(crate) struct CancelPeer {
    done: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl CancelPeer {
    /// Sleep `delay`, run `before_cancel` once, then cancel repeatedly until
    /// [`finish`](Self::finish) is called. `before_cancel` is where a test
    /// records what the peer observed before the first cancel.
    pub(crate) fn after_with(
        halt: Halt,
        delay: Duration,
        before_cancel: impl FnOnce() + Send + 'static,
    ) -> Self {
        let done = Arc::new(AtomicBool::new(false));
        let peer_done = Arc::clone(&done);
        let handle = thread::spawn(move || {
            thread::sleep(delay);
            before_cancel();
            while !peer_done.load(Ordering::SeqCst) {
                halt.store(true, portable_atomic::Ordering::SeqCst);
                thread::sleep(Duration::from_millis(1));
            }
        });
        Self {
            done,
            handle: Some(handle),
        }
    }

    /// [`after_with`](Self::after_with) with nothing to record.
    pub(crate) fn after(halt: Halt, delay: Duration) -> Self {
        Self::after_with(halt, delay, || {})
    }

    /// Stop re-asserting and join. Call it once the spin has returned.
    pub(crate) fn finish(mut self) {
        self.stop();
        if let Some(h) = self.handle.take() {
            h.join().expect("cancel peer thread must not panic");
        }
    }

    fn stop(&self) {
        self.done.store(true, Ordering::SeqCst);
    }
}

impl Drop for CancelPeer {
    /// A test that panics before `finish` must not leave the peer spinning.
    fn drop(&mut self) {
        self.stop();
    }
}
