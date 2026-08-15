# phase-359 — drop `std` from the core crates, and make `alloc` explicit

**Status (2026-08-15). W0–W4 landed (W2/W3 partially, by design); W5–W10 not started.** The campaign
removes `std` from the crates that run on targets, leaving `core` and
`core+alloc`. Implements the direction explored on 2026-08-15; supersedes the
"separate the std/no_std lanes" framing, which manages the split rather than
removing it.

## Why

`std` here is not a convenience layer over the platform — it is a SECOND
implementation of one. The measured surface is three primitives:

| std API | occurrences |
| --- | --- |
| `std::sync` | 172 |
| `std::time` | 50 |
| `std::thread` | 29 |
| `std::io` | 4 |
| `std::collections` | 2 |
| `thread_local!` | 1 |

No `fs`, `net`, `process` or printing. And `nros-platform-posix` — C over libc —
already provides every one of them: `nros_platform_clock_ns` (`clock_gettime`),
`nros_platform_wake_wait_ms`/`wake_signal` (`pthread_cond_timedwait`), the
`pthread_mutex_*` family, sleep/yield/task/alloc. The replacement exists before
the migration starts.

The cost of keeping both is visible in `Executor`'s fields, which carry each
concept twice — and in one case as two different TYPES:

| concept | std | alloc |
| --- | --- | --- |
| wake flag | `Arc<AtomicBool>` | `wake_flag_alloc` |
| node wake | `node_wake` | `node_wake_alloc` |
| wake ctx | `Arc<WakeCtx>` | `Arc<WakeCtxAlloc>` |
| async flag | `has_async_wake` | `has_async_wake_alloc` |
| spin clock | `last_spin_end: Instant` | `last_spin_end_us: u64` |

"What time is it" has three implementations: `Instant`, an injected
`clock_us_fn` pointer, and the unused `PlatformClock` trait.

**There is no layering obstacle.** `nros-node` already depends on
`nros-platform-api` (20 traits, including `PlatformClock`, `PlatformTime`,
`PlatformThreading`, `PlatformSleep`, `PlatformCriticalSection`) and
`nros-platform` does not depend back. The seam was bypassed, not unavailable.

### What the Condvar audit already settled

Performance is not the risk. Every flavour BLOCKS; none busy-spins. `std`
prefers `NodeWake::wait_ms` (kernel semaphore via `nros_platform_wake_*`) and
falls back to `Condvar` only when the platform vtable exposes no wake;
`alloc`/no_std blocks on the same ABI; core-only delegates to the transport's
blocking `drive_io`. On Linux `Condvar` IS `pthread_cond_wait`, so the seam
reaches the same syscall. **Dropping `std` deletes a redundant fallback, not a
mechanism.** (The `has_async_wake*` guard that sends poll-only backends — XRCE,
current Cyclone — to a full-timeout `drive_io` is deliberate, phase-127.C.4.)

## Baseline

**223** `cfg` mentions of the `std` feature and **388** `std::` paths, nine
crates (current, i.e. after W4):

| crate | cfg | `std::` |
| --- | --- | --- |
| `nros-node` | 112 | 309 |
| `nros` | 66 | 20 |
| `nros-c` | 13 | 9 |
| `nros-params` | 13 | 18 |
| `nros-core` | 8 | 6 |
| `nros-cpp` | 8 | 26 |
| `nros-log` / `nros-rmw` / `nros-serdes` | 1 each | 0 |

**Two earlier totals were wrong and are superseded.** The "~190" used while
planning was a hand-grep. W0's own first number, 181, came from a regex
anchored on `cfg(feature = "std")` / `cfg(not(feature = "std"))` that could not
see `cfg(all(feature = "std", ...))` — 26 of those in `spin.rs` alone. W2
exposed it: four cfg lines were deleted and the gate reported no movement. The
metric now counts any `feature = "std"` inside a cfg attribute, at any nesting.
A ruler blind to the commonest form of what it measures is worse than none,
because it reads as progress.

Excluded for cause, both found by measuring: `nros-macros` (`proc-macro = true`
— runs on the host, and its `std::` are tokens it EMITS) and
`nros-orchestration-ir` (documents itself as host-only schema code).

The "~190 sites" figure used while planning was a hand-grep and was wrong in
both directions. The gate's numbers supersede it.

**Two live `std` platforms, not one.** Linux, and NuttX —
`nros-board-nuttx-qemu` depends on `nros-board-nuttx`, which requests `std`,
and NuttX targets compile std from source via `build-std`. NuttX is W7 and is
the plan's largest unknown.

## Work items

### W0 — census + ratchet — **DONE 2026-08-15**

`scripts/check-std-census.py`, wired into `check-fast`. Freezes the per-crate
counts and fails when one goes UP, or when a crate enters scope that the
baseline has never seen. Counts going DOWN also fail, on purpose: lowering the
baseline in the same commit puts progress in the diff.

Verified both directions — a planted `std::primitive::u8` fails with
`nros-rmw: path 0 -> 1`; a comment naming `std::sync::Condvar` does not count
(comment text is stripped, because sibling commits legitimately ADD such prose
while REMOVING the use).

### W1 — make the guarantee checkable — **DONE 2026-08-15**

`check-no-std` covered 9 of the 32 crates that declare `no_std`, and
`nros-node` — 85 of the cfg sites — was not among them. Added `nros-node`,
`nros-platform`, `nros-diagnostics`, `nros`, verified on both bare-metal
targets first.

**The trap this hit, which the next work item should expect too.** Adding the
crates was not enough: `nros-node`'s executor is behind
`#[cfg(any(has_rmw, test))]`, and `has_rmw` is set by build.rs only when an RMW
feature is on — so a bare `--no-default-features` check compiles the crate
SHELL and none of the 85 sites. A planted `std::string::String` passed it in
0.06 s (cached). The lane now also checks the `alloc,rmw-cffi` slice on both
targets, which does catch it. A gate written to prevent the issue-0196 shape
nearly shipped with that exact shape.

### W2 — collapse the duplicated pairs — **PARTIALLY DONE 2026-08-15**

`node_wake` and `has_async_wake` are now ONE field each
(`nros-node` cfg 139 -> 131, path 346 -> 342). `portable_atomic_util::Arc`
compiles on `std` too and `std` implies `alloc`, so the two arms were two
spellings of one thing; the inner `NodeWake` was already shared. Verified on
four combinations: std, std+rmw-cffi, alloc+rmw-cffi (thumbv7m), and core-only
+rmw-cffi with no alloc, where the fields correctly vanish.

**Three of the five pairs are NOT deletion, contrary to this doc's first
version**, and they move to the work items that actually own them:

* `wake_flag`, `halt_flag` — returned by PUBLIC methods as
  `std::sync::Arc<AtomicBool>` (`wake_handle()`, `halt_flag()`). Retyping them
  is the breaking change scheduled as **W3**, not a silent W2 edit.
* `wake_ctx` — `WakeCtx` carries the `Condvar` + `Mutex` pair;
  `WakeCtxAlloc` carries `flag` + `node_wake`. They are different types because
  the std one wraps a mechanism this campaign is REMOVING, so merging them
  belongs after the Condvar path goes (**W4**/**W10**), not before.

`WakeCtx::node_wake` was retyped to the portable Arc so the one shared field
has one type across both structs.

### W3 — public API conversion — **MOSTLY DONE 2026-08-15**

Converted:

```rust
spin_period(std::time::Duration)  -> core::time::Duration
halt_flag()   -> portable_atomic_util::Arc<portable_atomic::AtomicBool>
wake_handle() -> portable_atomic_util::Arc<portable_atomic::AtomicBool>
```

`spin_period` was not a change of type at all — `std::time::Duration` IS
`core::time::Duration`, re-exported. Only the spelling moved.

The point of doing this before W4–W6 was to unblock a W2 pair, and it did:
`wake_flag` / `wake_flag_alloc` are now ONE field (the public `wake_handle()`
handing out a `std::sync::Arc` is exactly what pinned them apart). `WakeCtx`'s
`flag` and both `ThreadHandle::halt` fields follow the same type.

`nros-node` cfg 131 -> 127, path 342 -> 321.

**This doc claimed "every native consumer touches `halt_flag`/`wake_handle`".
That was wrong.** Neither has a single caller outside `nros-node` — the only
tracked mentions are its own source, its own tests, and prose in docs. The
in-repo break is therefore nil; the semver break for out-of-tree users is real
but the crate is 0.5.0.

Two deferred, each to the item that actually owns it rather than forced through
here:

* `signal_fd() -> std::io::Result<c_int>` -> **W6**. The signature is not the
  problem: `WakeSignalFd::new` is itself `std::io`-based, so changing the return
  type while the implementation stays `io::Error` buys nothing. It is triple
  gated (`signal-fd-wake` + `rmw-cffi` + `target_os = "linux"`).
* `join() -> std::thread::Result<()>` -> **W5**. It belongs to `ThreadHandle`,
  which wraps a `std::thread::JoinHandle` and is `#[cfg(feature = "std")]`
  throughout. That is not an API-spelling change, it is the threading work item.

### W4 — one spelling of "what time is it" — **DONE 2026-08-15**

There were **five**, not the three this doc claimed:

1. `last_spin_end: Instant` (spin accounting, std)
2. `monitor_clock_base: Instant` (monitor windows, std)
3. a `static EPOCH: OnceLock<Instant>` for the sporadic refill
4. a second `static EPOCH: OnceLock<Instant>` for the major-frame phase
5. the injected `clock_us_fn` (no_std)

(`PlatformClock` was the sixth and is still uncallable — its methods are
associated fns, so using it would make `Executor` generic, which the FFI design
forbids. That is why it went unused, and it is not a W4 fix.)

All now route through one `now_us()`, read ONCE per spin. `clock_base` is the
single std epoch, seeded eagerly at construction so `last_spin_end_us: Some(0)`
still means "construction" and the first spin still credits pre-spin setup
time — the property the old std seed existed for.

`nros-node` cfg 127 -> 112, path 321 -> 309.

**Two real defects fell out of the unification**, both consequences of "no clock
on no_std" — which is false whenever a `clock_us` hook is injected:

* The polled Sporadic budget refill was `#[cfg(feature = "std")]` with NO
  no_std arm, so on embedded a budget exhausted once and never refilled. It now
  runs on either flavour when a clock exists, and is skipped when none does —
  identical to today where there is no clock.
* The major-frame phase used `now_us = delta_us` on no_std: a per-spin INTERVAL
  used as an absolute phase clock. It now uses the real clock when one is
  injected, falling back to the old approximation only when none is.

The per-dispatch latency measurement kept its flavour difference — std measures
unconditionally because the std-only sporadic runtime accounting consumes it —
but expresses it as a VALUE, `lat_active || dl_active || cfg!(feature = "std")`,
instead of four cfg arms across two sites.

### W5 — threads through `PlatformThreading` / `task_*`

29 sites.

### W6 — residue

`std::io` (4), `HashMap` -> `BTreeMap` (2, tiny N), the single `thread_local!`.

### W7 — NuttX off `std`

The other live `std` platform. `nros-platform-nuttx` exists and NuttX is POSIX,
so the posix C layer is the template. **Unsized** — the plan's biggest unknown.

### W8 — make `alloc` explicit

`alloc` is already a separate feature (`std = ["alloc", ...]`), so the real
flavours are `core` / `core+alloc` / `std`. The `_alloc` field names prove the
code already treats it as first-class. Document what each implies, and gate it.

### W9 — lanes and cell checks

Separate `std`/`no_std` lanes (shapes genuinely differ: 74 `Linux` cells vs 132
elsewhere) plus a per-cell flavour assertion. Best after W2–W6 shrink the
divergence.

### W10 — flip the default, delete the feature

`nros` currently defaults to `std`.

## Costs accepted

* **Panic ergonomics** — no `RUST_BACKTRACE`, typically `panic=abort`. Partly
  recoverable: the application binary may still link `std`.
* **Core dependency freedom** — permanently constrains what core may depend on.
  Cost today ~zero (`heapless`, `portable-atomic`, `atomic-waker` are already
  no_std).
* **`std::thread` conveniences** — join results, names, scoped threads.

## Not measured

`nros` (61 cfg sites) and `nros-params` (11) are uninspected — a large part of
the work and the weakest estimate here. W7 is unsized.

And expect untested code. Two paths in one session turned out to have no lane
at all: seven `std`-gated `nros-node` tests that no lane ran, one of which had
NEVER passed (issue 0577), and the extra-session wake install that was `std`-only
on the dynamic path with no no_std multi-RMW test to catch it. Budget for that
rather than treating each as a surprise.
