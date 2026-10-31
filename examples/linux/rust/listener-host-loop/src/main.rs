//! Native Listener whose loop belongs to the program, not to nano-ros.
//!
//! `nros::main!()` (what `examples/linux/rust/listener` uses) emits both the
//! assembly the model describes AND the loop that drives it. A host that owns
//! its own loop — an async runtime, a GUI main loop, an Arduino `loop()` —
//! cannot hand that loop over (RFC-0112 Tier 0). `nros::setup!()` emits the
//! assembly alone, from the same `system.toml`:
//!
//! - `nros_boot(host)` — the board's start-up (RMW registration, log sinks,
//!   the executor sized as `nros::main!` sizes it), then `host`. No spin.
//! - `nros_setup(rt)` — installs every component the model declares.
//! - `nros_stop_requested()` — the board's SIGTERM guard. Returning from
//!   `host` lets the board close the RMW session cleanly.
//!
//! Everything after `nros_setup` is the program's: here a plain `std` loop
//! that interleaves its own work with `spin_once`.

nros::setup!();

fn main() {
    nros_boot(|rt| {
        nros_setup(&mut *rt)?;
        while !nros_stop_requested() {
            // nano-ros gets a 10 ms slice of the host's loop...
            let _ = rt.spin_once(10);
            // ...and the rest of the iteration is the host's own work.
        }
        Ok(())
    })
    .expect("boot");
}
