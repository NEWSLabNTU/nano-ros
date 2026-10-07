---
id: 1733
title: "A tiered NATIVE image intermittently loses whole tiers at boot: concurrent tier setups over one shared session fail publisher creation (C++ `create_publisher_in` rc -3, Rust `PublisherCreationFailed`)"
status: open
type: bug
severity: medium
area: [runtime, native, tiers, zenoh]
related: [phase-463, 1597, 1571]
found: 2026-10-07
---

## Measured

phase-463 W7's profiler joins a profiled run against the census, and that join
refused two in-tree images for a reason that has nothing to do with profiling.
It reproduces on an `origin/main` build with the profiler absent.

**C++, `examples/workspaces/derived-tiers-cpp`, image `native`.** There are four
derived tiers over one zenoh session, against a live `rmw_zenohd`. Each run
lost 0 to 2 tiers at setup, and WHICH tiers varied from run to run:

```
[ERROR] .../nros-cpp/include/nros/node.hpp:315 node "mrm_handler": FAILED at create_publisher_in (code=-3)
[nros] FATAL: node "mrm_handler" failed to construct at create_publisher_in (code=-3)
[ERROR] nros: tier 'derived-mrm_handler' setup FAILED (rc=-3) — tier will not run
```

| build | run | tiers lost |
| --- | --- | --- |
| `origin/main` (no profiler) | 1 | `mrm_handler` |
| `origin/main` (no profiler) | 2 | `mrm_comfortable_stop_operator`, `mrm_handler` |
| W7 branch, profiling off | 1 | `mrm_handler` |
| W7 branch, profiling on | 1 | `mrm_comfortable_stop_operator`, `mrm_handler` |
| W7 branch, profiling on (60 s) | 1 | none |

The rate, from 6 s runs under one router (`NROS_ENTRY_SPIN_MS=6000`). "Lost"
means a tier setup FAILED and the run continued; "crash" means the process
died on SIGSEGV or SIGABRT:

| build | runs | ok | lost tier(s) | crash |
| --- | --- | --- | --- | --- |
| `origin/main` | 24 | 9 | 11 | 4 |
| W7 branch, profiling off | 8 | 3 | 2 | 3 |
| W7 branch, profiling on | 8 | 6 | 1 | 1 |

About 60 % of boots of this image are bad on `origin/main` itself.

**Rust, `examples/workspaces/realtime-rust`, image `native_derived`.** There
are two tiers. The control tier was lost in 3 of 3 runs on the W7 branch. No
`origin/main` build of this image was run, and unlike the C++ image the loss
did not vary. `Control::register` is logged twice (the boot tier, then the
control tier), so this half may be a deterministic duplicate registration
rather than the race:

```
[ERROR] nros: node declaration failed — NodeError::Transport(PublisherCreationFailed)
nros: tier `derived-control_node` setup failed: NodeRegister("ctrl_pkg") — tier task exiting
```

**It can also be heap corruption.** One run of the C++ image, on the W7 branch
with profiling on, died outright. Under gdb that run aborted with
`malloc(): unaligned tcache chunk detected`:

* tier thread 5 was inside zenoh-pico's `_z_slist_new_empty` (`z_malloc` ->
  `nros_platform_alloc`) during setup;
* thread 6 was in `_z_declare_liveliness_token` for `mrm_handler`, waiting on
  the session mutex;
* the run first logged `z_declare_publisher failed: -78` for the
  `/diagnostics` publisher, which is the SAME one every tier creates.

So concurrent tier setups corrupt the heap through the shared zenoh-pico
session, not merely refuse a create. The profiler changes the setup's timing
and touches none of that code, which is why the symptom moves between a lost
tier and a crash.

## What it is not

* **It is not `nros_cpp_publisher_create`'s own argument checks.** I
  instrumented all seven `INVALID_ARGUMENT` returns in that function, and none
  fired. `-3` here is `transport_error_to_cpp_ret` mapping a BACKEND
  `TransportError::{InvalidArgument, InvalidConfig, TopicNameInvalid}`.
* **It is not reached by the census.** A census run sets up every tier on the
  boot executor, in tier order, and spawns no tier task. It records all four
  nodes every time. The failure needs concurrent setup.
* **It is probably not a fixed capacity.** A pool sized one short would lose
  the same number of tiers on every run. The number varies, from 0 to 2.

## Shape

The C++ native tier runner (`nros-cpp` `native_tier_trampoline`) and the Rust
one (`nros-board-linux`) both open one borrowed executor per tier over the
boot executor's session, each on its own platform task. Then they run that
tier's `setup` concurrently. Publisher creation on the shared session is
therefore concurrent. It is the most likely place for the race: the backend
shim's per-session publisher and liveliness bookkeeping, which a single
executor never exercises concurrently. Not yet proven: the next step is to
serialize the tier setups (or the shim's create path) and see the losses stop.

## Why it matters

A lost tier is reported only on stderr, and the image keeps running with the
remaining tiers. For the derived-tiers image that is the island's shape: a
safety function silently absent at boot. phase-463 W7's profile-vs-census
check (`scripts/check-profile-against-census.py`, P1) catches it from the
outside. That is how it was found.
