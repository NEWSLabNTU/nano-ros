//! Entry pkg — boots the six-timer sizing showcase (issue 0257).
//!
//! The executor size this entry opens with is decided at macro-expansion time.
//! The SystemModel names zero callback entities for `burst_talker` (launch
//! wiring has no timer entity), so the model bound alone emits no sizing and
//! the executor opens at the four-slot build default — one slot short from the
//! fifth timer on. `nros::main!` also reads the `nros sync`-produced
//! source-metadata sidecar (phase-307 W4), which records all six timers,
//! and derives eight.
//!
//! So this binary booting at all is the assertion: it cannot start without the
//! sidecar being found, read, and applied.

nros::main!(model = "demo_bringup");
