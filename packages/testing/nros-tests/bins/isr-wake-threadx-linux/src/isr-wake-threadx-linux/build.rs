//! Issue 2745 -- route the port's timer interrupt through this image's hook.
//!
//! The hosted ThreadX port calls `_tx_timer_interrupt` from its timer thread,
//! between its context save and restore. `--wrap` points that call at
//! `__wrap__tx_timer_interrupt` (in `main.rs`), which calls the port's own tick
//! first, so the hook runs in exactly the interrupt context the port gives its
//! tick, with no change to the vendored kernel.

fn main() {
    println!("cargo:rustc-link-arg-bins=-Wl,--wrap=_tx_timer_interrupt");
}
