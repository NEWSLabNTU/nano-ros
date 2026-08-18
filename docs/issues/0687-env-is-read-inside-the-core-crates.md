---
id: 687
title: "The `env` capability keeps `std` in the core crates, and an ABI env
  getter is the wrong fix — env resolution belongs at the hosted EDGE"
status: open
type: design
area: api
related: [phase-359, rfc-0054, issue-0080]
---

## The question phase-359 W10 ends on

W10 moved every host facility the core crates used onto the platform ABI: the
monotonic clock, the wall clock, sleep, tasks, the log sink. One is left.
`ExecutorConfig::from_env`, `nros::init*`, the `$NROS_RMW` selector and the C++
component entries all call `std::env::var` directly, so the `env` capability
REQUIRES `std` (enforced by a `compile_error!` in `nros` and `nros-node`), and
`std` therefore cannot be deleted from those crates.

## Measured, so the fix is chosen against evidence

Non-test `std::` paths per crate, after W10's spelling pass:

| crate | env | everything else |
| --- | --- | --- |
| `nros-node` | 16 | `sync::{Mutex, OnceLock}`, `time::Instant`, `ffi` |
| `nros` | 8 | `path::Path`, `sync::Mutex` |
| `nros-cpp` | 6 | `fs::write` |
| `nros-c` | 1 | — |
| `nros-core` | 0 | `time::SystemTime` |
| `nros-rmw`, `nros-serdes`, `nros-log`, `nros-params` | 0 | **none** |

So env is 31 of ~41 — the dominant term, and four crates are already clean apart
from `extern crate std`.

## Why NOT an ABI env getter

The obvious move, by analogy with every other W10 port, is
`nros_platform_env_get(name, buf, len)`. Rejected, for a reason the analogy
hides: the clock, sleep, tasks and the log sink are things every RTOS HAS, so an
ABI entry models a real per-platform capability. A process environment is not.
Five of the six ports would implement it as `return 0` — permanent ABI surface
for a facility that exists on exactly one platform family, and every embedded
port would carry a stub asserting it has no environment.

## The shape that fits

Push env resolution to the hosted EDGE and let the core crates take values.

That is already how embedded works and how this campaign has been resolving the
same tension elsewhere: a board installs `ExecutorConfig::clock_us` /
`epoch_us`; the core never asks where they came from. `ExecutorConfig` is
already the boundary — `from_env` is the only constructor that reaches behind
it.

Concretely: the readers move OUT of `nros-node`/`nros`/`nros-cpp` into the
hosted callers that already exist (`nros-board-linux`, the entry macro's hosted
expansion, `nros-c`'s init), which hand a fully-resolved `ExecutorConfig` in.
Core crates keep no `env` feature at all.

## What this does NOT finish

Deleting `std` from the nine crates needs three more answers, none of them env:

1. **`Mutex` / `OnceLock`** — the env cache in `nros-node`, the recorder in
   `nros`'s metadata probe. Both are process-global state; a no_std tree has
   `portable_atomic_util` + `spin`, at the cost of a dependency edge (the same
   trade issue 0669 records for `Handoff`).
2. **`Instant` / `SystemTime`** — the fallbacks for a build with NO platform
   port. Deleting them removes a capability from a real configuration
   (`Executor::from_session` accepts any `Session`; a non-cffi backend is a
   supported consumer), so they need a decision, not a deletion.
3. **`fs` / `Path`** — `metadata-mode`'s probe writes a file; `nros::init`'s
   launch path checks existence. Both are host-only FEATURES rather than
   flavours, and could move to a host-side crate.

Anyone reading the census as "33 sites from done" should read this list first:
the remaining sites are cheap to count and each carries a design question.
