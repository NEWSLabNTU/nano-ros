---
id: 687
title: "The `env` capability keeps `std` in the core crates, and an ABI env
  getter is the wrong fix — env resolution belongs at the hosted EDGE"
status: resolved
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

## Costed 2026-08-19 — the move needs a decision this issue did not carry

Attempted, and stopped short deliberately. Two measurements decide it.

### `$NROS_RMW` already has an edge, and the core duplicates it

The same variable is read in THREE places:

* `nros-node/executor/spin.rs` — inside `Executor::open`, the core path
* `nros/src/lib.rs:812` — the facade's `open_session`
* `nros-c/src/executor.rs:321` — the C entry

So "push it to the edge" is not a move; the edge readers exist already and the
core one is a third copy. But deleting the core read is NOT behaviour-preserving:
it serves callers that reach `Executor::open` DIRECTLY rather than through the
facade, and the multi-edition harness drives example binaries with `NROS_RMW`
set expecting exactly that (`ros_env.rs` pins the selector so the choice is
"self-contained — never at the mercy of an ambient `NROS_RMW`"). Removing it for
a one-site win is a behavioural gamble against a live harness, so it was not
taken.

**Resolved differently, 2026-08-19 — one READER, not one site.** The
disagreement was never that three crates read the variable; it was that they
read it with three semantics (raw OS bytes; UTF-8 filtered for empty; UTF-8
passed through EVEN WHEN EMPTY) and a fourth reader in `nros::init` added an
`RMW_IMPLEMENTATION` fallback the others did not have. `nros_node::rmw_selector`
is now the single answer to "which backend did the user ask for": empty and
non-UTF-8 both mean unset, and it returns `heapless::String<32>` so
`Executor::open` can call it in a build with no allocator. The three edge
callers keep their own edges — the core read is NOT deleted, so the harness
behaviour this section defends is untouched — but they now share one semantic.
`$RMW_IMPLEMENTATION` stays OUT: it holds ROS names (`rmw_cyclonedds_cpp`) where
the selector holds registry names (`cyclonedds`), so unifying them without a
mapping converts today's "ignored" into `Unknown` = failed open. `nros::init`
keeps that fallback locally, for the `Context.rmw` hint, which is a different
quantity. This does not shrink the `from_env` problem below; it removes the
smaller of the two env questions from it.

### `from_env` has 86 call sites

| where | callers |
| --- | --- |
| `nros-node` (its own tests) | 15 |
| `cli/testing` | 15 |
| `examples/native` | 10 |
| `nros-bench` | 8 |
| `cli/third-party`, `nros`, `rmw/zenoh`, `cargo-nano-ros`, … | 22 |

`ExecutorConfig` is defined in `nros-node`, so `from_env` is an INHERENT method:
it cannot be reimplemented in a hosted crate and still be spelled
`ExecutorConfig::from_env()`. Every mechanism that moves it changes call syntax
or import at all 86 sites, or keeps a shim.

### The two mechanisms, and what each costs

1. **Extension trait in the facade** — `nros::ExecutorConfigEnvExt::from_env`.
   The core loses `env` entirely. Call sites keep the spelling ONLY where the
   trait is in scope, so every site needs an import (or the prelude, which not
   all of them use). Honest, breaking, mechanical.
2. **Resolver injected into the core** — the core keeps `from_env` but calls an
   installed `fn(&str) -> Option<String>`; a hosted crate installs a
   `std::env::var` one. No call site changes and no `std` in the core — but
   nothing installs it automatically (this tree has no ctors on RTOS by rule),
   so an image that forgets silently resolves defaults. That failure mode is
   worse than the flavour it removes.

**Recommendation: (1), as its own phase item with the 86 sites in scope**, not
folded into a flavour cleanup. It is an API change and should be reviewed as
one.

## Landed 2026-08-19 — mechanism (1), and the 86 was an overcount

`nros-node` has no `env` feature. `src/env.rs` in `nros` is the tree's one
reader of the process environment.

**What moved:** `EnvCache` + `env_cache()`, `from_env`, `try_resolve`'s hosted
rung, and `rmw_selector`. **What replaced it in the core:** values —
`ExecutorConfig::resolve_with(baked, Option<EnvRung>)`, where `EnvRung` is the
environment rung of precedence model A as already-resolved fields.

Three findings that only surfaced by doing it:

1. **~27 call sites, not 86.** The costing counted every `from_env` in the tree,
   including `LinkFeatures::from_env`, `AmentIndex::from_env`,
   `env_logger::Builder::from_env` and `nros-node`'s own tests. Real
   `ExecutorConfig::from_env` sites outside the core: ~27, and 25 of them go
   through `nros::prelude::*`, which now carries `ExecutorConfigEnvExt`. Two
   needed an import added. The API break is real but its blast radius was a
   quarter of the estimate.
2. **`hosted_env: bool` was a compile-time constant everywhere** — `true` at
   `nros-board-linux` plus `nros-c`/`nros-cpp`'s entries, `false` at the other
   five boards. A parameter with one value per call site is a fork in disguise,
   so it became two functions.
3. **`$NROS_RMW` had to move too, and that is what made it honest.** This issue
   argued the core read should stay because callers reaching `Executor::open`
   directly rely on it. The answer is neither "keep the read" nor "delete it":
   the selection became `ExecutorConfig::rmw`, filled by `from_env` /
   `resolve_hosted`, so every hosted path keeps the behaviour while
   `Executor::open` stops reading the environment. `nros::open_session` keeps a
   direct read, because it takes a caller-built config that has no selector in
   it.

Side effect worth naming: the core's boot-config tests no longer touch the
process environment, so the shared-mutex-plus-frozen-cache race of issue 0607
cannot recur there. It survives only where env is genuinely under test, in
`nros::env`'s own tests.

Census: `nros-node` cfg 11 -> 10, path 20 -> 7; `nros` path 9 -> 22; total 38,
unchanged — the sites moved from the core to the edge, which is the goal, not
the number.

### Follow-up, same day — the sibling readers

The move above created one reader in the core's place and left three siblings at
the edge, which is the defect this issue is about, one layer over. Swept:

* **`nros-cpp`'s two native entries** read `$NROS_LOCATOR` / `$ROS_DOMAIN_ID`
  and passed them down as the BAKED rung — while `nros_cpp_init` re-resolved the
  same two variables through `try_resolve_hosted`. Deleted rather than unified:
  the resolver was already doing it, and doing it better. The entries did not
  accept the legacy `$ZENOH_LOCATOR`, and coerced a malformed or out-of-range
  `$ROS_DOMAIN_ID` to a SILENT domain 0 — the #206 failure mode, surviving on
  the one path nobody had swept. `$ROS_DOMAIN_ID=300` on a C++ entry now exits
  `NROS_CPP_RET_INVALID_ARGUMENT` (measured: exit 253) instead of quietly
  running on domain 0.
* **`nros::init::read_env_context`** was a third parse of the same four
  variables, with no deprecation warning on the legacy spellings and no range
  check. It calls `try_resolve_hosted` now.
* **`from_env` itself** read `$ROS_DOMAIN_ID` a fourth way, through the cache,
  which silently resolved a bad value to 0 while `resolve_hosted` errored on it
  — two answers for one variable inside one module. `from_env` IS
  `resolve_hosted(BootConfig::default())` now, structurally rather than by
  assertion, so it fails loud like every other path. The cached domain field is
  gone with it.
* **`$NROS_ENTRY_SPIN_MS`** was parsed twice at two widths (`u32`, `u64`), so the
  same name meant "unbounded" above 4.29e9 ms at one entry and the literal value
  at the other. One reader, saturating.

Census: 38 -> 27 `std::` paths (`nros` 22 -> 16, `nros-cpp` 7 -> 2).

The three classes listed below are NOT closed by this and keep their questions.
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
