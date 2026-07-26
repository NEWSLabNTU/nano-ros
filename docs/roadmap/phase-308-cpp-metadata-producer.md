# Phase 308 — the C/C++ source-metadata producer

**Status (2026-07-26): Draft.** Split out of phase-307 W3 once the Rust half
landed and the C/C++ half proved phase-sized on its own. Phase-307 delivered
the producer (W1), the trigger (W2), the consumer (W4), the coverage gate (W5)
and the bake-time regression test (W6 lane 1) for **Rust**. C and C++ node
packages still have no producer, so their entity counts fall back to the
SystemModel's timer-blind lower bound — the residual half of issue 0257.

Six example node packages are affected today, named by the phase-307 W5 ledger
test (`cpp_producer_gap_is_tracked_not_hidden`). That test is the acceptance
signal: when this phase lands, its `unsupported` list goes empty and the
assertion tightens from a bound to zero.

## Why it is not a small wave

The Rust producer works because the recorder and the runtime consume the same
`Component::register` declaration path, and a host harness can call it with a
recording sink in ~40 lines of generated code. C++ has no such seam:

- A C++ node declares its entities inside `configure(nros::Node&)`, against
  real `nros::Node` methods.
- Those methods call the `nros_cpp_*` C ABI directly
  (`nros-cpp/include/nros/nros_cpp_ffi.h`, ~137 exported functions), whose
  implementation is the `nros-c` Rust crate over a real RMW session.
- So "record instead of create" has to happen at the C ABI, not above it.

Timers are the reason a cheaper seam does not exist. A recording RMW backend
(implementing `nros_rmw_vtable_t`, the RFC-0054 seam) would be the obvious
minimal intercept — but timers are executor-side, not RMW-side, and timers are
exactly the entity the SystemModel already cannot see. A producer that misses
them reproduces the bug it exists to fix.

## Two candidate designs

**(a) Metadata mode inside `nros-c`.** A cargo feature under which the
declaration-path exports (`nros_cpp_node_create*`, `publisher_create`,
`subscription_create`, `timer_create` + variants, `service_{server,client}_create`,
`action_{server,client}_create`, `guard_condition_create`, plus no-op
`init`/`shutdown`/`spin`) record into the same `MetadataRecorder` the Rust
harness uses and dump the shared schema. ~20 of the 137 functions are on the
declaration path; the rest can stay unimplemented in this mode and abort loudly
if called, which doubles as a check that the probe only ever runs declarations.

Pro: one recorder, one schema, no fork — the phase-307 W3 note's explicit
requirement. Con: a second behavioural mode inside the crate that backs every
C/C++ image, so the mode gate must be airtight.

**(b) A standalone recording shim library.** A small C/C++ TU defining the same
declaration-path symbols, linked with the user's component instead of `nros-c`.

Pro: zero risk to the shipping runtime. Con: it IS a fork of the ABI's
behaviour, and drift between shim and runtime is silent in exactly the way
`check-ffi-struct-mirrors` exists to prevent elsewhere. The header being the
SSoT limits the damage (a signature change is a compile error) but semantic
drift is not caught.

**(a) is preferred** on the "must not fork" rule phase-307 W3 states, and
because phase-236 is separately turning the phase-235 recording NodeContext
into a real runtime — two recorders would be exactly the fork both phases warn
about. **Coordinate with phase-236 before starting.**

## Waves

### W1 — the probe driver

Mirror `metadata_build.rs`: generate a host probe that instantiates the node
class named by the cmake `nano_ros_node_register(CLASS …)` summary, runs its
declaration path against the recorder, and writes the sidecar. The CLI already
carries the C/C++ summaries (`cmake_component_metadata`) and already makes them
component declarations — phase-307 W5's gate asserts it — so the discovery half
is done; only the build+run half is missing.

### W2 — recording mode

Whichever design W0 settles on, behind a gate that cannot be enabled in a
firmware build.

### W3 — schema parity

The emitted sidecar must be byte-schema-identical to the Rust one: same
`SourceMetadata`, same `(package, executable)` key, so
`model_ingest::metadata_slot_counts` needs no language branch. One mechanism
with three front-ends, not three mechanisms.

### W4 — close the ledger

`nros sync` stops reporting "no producer for …"; phase-307's
`cpp_producer_gap_is_tracked_not_hidden` asserts zero.

## Acceptance

- [ ] Every C and C++ node package in `examples/` produces a schema-valid
      sidecar through the same discovery path and schema as Rust.
- [ ] `metadata_slot_counts` has no language branch.
- [ ] Phase-307's producer-gap ledger test asserts zero unsupported packages.
- [ ] The recording mode cannot be reached from a firmware build.
