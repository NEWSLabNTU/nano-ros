# phase-359 — drop `std` from the core crates, and make `alloc` explicit

**Status (2026-08-16). W0–W4, W6, W9 landed; W5 RE-SCOPED after measurement; W8 AUDIT COMPLETE; W7 IMPLEMENTED — NuttX is off `std`. W10 IN PROGRESS: the three `std::thread`-backed blocks are ported/deleted, `env` is split off as a CAPABILITY, and `nros-board-linux` — the last `std` platform — now runs the CORE crates on the `alloc` path, verified by a green native fixture build and `roundtrip_xprocess` 8/8. `nros-node` 106/76 -> 86/40. What remains of the manifest half: ~38 consumer manifests still naming a core crate's `std`, 10 generated message crates, 2 codegen template copies, and the ~86 `cfg` sites that must resolve before the `std` feature can be deleted from the nine crates.** The campaign
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

**166** `cfg` mentions of the `std` feature and **129** `std::` paths, nine
crates (current: after W4, W6 and the first W8 item, and after `#[cfg(test)]`
code was excluded — see the metric correction below):

| crate | cfg | `std::` |
| --- | --- | --- |
| `nros-node` | 105 | 76 |
| `nros` | 25 | 16 |
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
the plan's largest unknown. (W7 landed 2026-08-15: NuttX is `no_std` and its
`build-std` is `["core", "alloc", "panic_abort"]`, leaving **Linux as the only
`std` platform** — which is what W10 needs.)

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

### W5 — threads — **RE-SCOPED 2026-08-15, not implemented as written**

The item said "29 sites -> `PlatformThreading`". Both halves of that were wrong,
and measuring said so before any code moved.

**The count was inflated.** 15 of the 29 were `thread::sleep` inside
`executor/tests.rs`, which is `#[cfg(all(test, not(feature = "rmw-cffi")))]` —
host test code that never compiles for a target.

**The remaining 13 are all inside std-only blocks.** Checked one by one:

| site | gate |
| --- | --- |
| `spin_period`'s sleeping loop (x2) | `feature = "std"` |
| `ThreadHandle` spawn/join/field (x4) | `feature = "std"` |
| os-priority worker pool (x2) | `std` + `scheduler-os-priority` |
| signal-fd worker (x2) | `signal-fd-wake` + `target_os = "linux"` |

None is on the embedded path. These are host-only CONVENIENCES that a no_std
build never compiles, so "migrating" them is not the work — deciding whether
they should exist at all is, and that is W10.

**And the seam cannot take them anyway.** `PlatformThreading`, like
`PlatformClock`, is a trait of associated fns, so calling it requires a generic
parameter and `Executor` is deliberately non-generic. The alternative — the
`nros_platform_task_*` C ABI, following `node_wake.rs`'s precedent — is a HARD
link dependency, and the unit-test build deliberately avoids exactly that by
excluding `node_wake` under `not(feature = "rmw-cffi")`.

So W5 is not "do it later"; as written it is not the right change. What remains
of it is folded into W10.

### W6 — residue — **DONE 2026-08-15**

`std::sync::atomic::Ordering` and `std::time::Duration` are RE-EXPORTS of the
`core` types — identical types, so 39 sites in `nros-node` were spelling, not
dependency. Converted (`nros-node` path 309 -> 285), zero behaviour risk.

Deliberately NOT converted, because it would be motion rather than progress:

* `std::io::Error` in `WakeSignalFd` (4 sites). The type spawns a
  `std::thread`, holds a `std::sync::Arc`, and calls `libc::eventfd` behind
  `signal-fd-wake` + `target_os = "linux"`. Retyping its error while the body
  stays std leaves it exactly as std-bound as before. Same reasoning retires
  W3's `signal_fd()` deferral: it is one type's problem, and that type is a
  Linux-only feature.
* `HashMap` -> `BTreeMap` for `os_priority_workers` (2 sites). Its own comment
  says the field is std-gated "because workers need `std::thread` + `mpsc`".
  Swapping the map does not change that.
* `thread_local!` — there is none in `nros-node`. The one site counted in
  planning is in another crate.

### W0 metric correction — `#[cfg(test)]` excluded

Host unit tests link `std` even in a `no_std` crate, so their `std::` use can
never block a target build. The census counted it, and the distortion was large:
`nros-node` read **309** paths of which **209 were its `#[cfg(test)]` module**.
That inflation is where W5's "29 sites" came from.

Totals re-based from 223/388 to **213 cfg / 141 paths**. That drop is a
correction, not progress, and is recorded as such.

### What the residue actually is

Classifying every remaining `std::` path in `nros-node` outside the test module
(100 at the time of measuring):

| share | region |
| --- | --- |
| 60 % | inside `#[cfg(feature = "std")]` arms |
| 13 % | `signal-fd-wake` (Linux-only feature) |
| 12 % | `scheduler-os-priority` (needs `std::thread` + mpsc) |
| 15 % | ungated — and on inspection nearly all of THOSE are test helpers the
classifier's brace tracking missed, plus the single intentional std clock
provider from W4 |

**The shared executor path is essentially no_std-clean already.** What is left
is a set of host-only features plus one clock provider. That reframes the rest
of the campaign: W7-W10 are about deciding where those features live, not about
migrating executor code.

### W7 — NuttX off `std` — **IMPLEMENTED 2026-08-15**

NuttX is the second and last `std` platform (W9's derivation: `Linux`,
`NuttxArm`, `NuttxRiscv`). Until it moves, W10 cannot happen — the `std` feature
cannot be removed while a target platform requires it.

#### Where the `std` actually is

Measured, not assumed:

| place | `std::` paths | what it does |
| --- | --- | --- |
| `nros-board-nuttx` | 48 | `process::exit`, `panic::set_hook`, `io::stdout().flush()`, `thread::sleep`, `Box` |
| `nros-board-nuttx-qemu` | 8 | inherits the base board |
| `examples/qemu-arm-nuttx/rust/**` | **0** | nothing — the examples are already portable |

The chain is: `nros-board-nuttx-qemu` enables no `std` itself but depends on
`nros-board-nuttx`, which requests `nros = [std, rmw-cffi]` and
`nros-platform = [std, platform-nuttx]`. The board's own comment already knows
what it costs: "this crate as no_std while its `std::` bodies are active ->
build errors".

Note what is NOT in the list: no application or example code. The port is
confined to one crate plus its qemu wrapper.

#### What each use needs instead

| std | replacement | difficulty |
| --- | --- | --- |
| `process::exit` | NuttX libc `exit()` via `extern "C"` | trivial |
| `io::stdout().flush()` | the platform log/putc C export | trivial |
| `thread::sleep` | `nros_platform_sleep_*` | trivial |
| `Box` | `alloc` | trivial |
| `panic::set_hook` | a `#[panic_handler]` | **the real change** |

The panic hook is the substantive one: std's panic runtime and a no_std
`#[panic_handler]` are different mechanisms, not two spellings. The pattern
already exists here — `nros-board-mps2-an385-freertos` and
`nros-board-threadx-qemu-riscv64` each define one — so NuttX would follow them,
with the usual constraint that exactly one handler may exist per image.

`nros-platform`'s `std` role is narrower than it looks: its ONLY `std` gate is
`#[cfg(all(feature = "global-allocator", not(feature = "std")))]`, i.e. "std
supplies the allocator, so do not install ours". Dropping `std` therefore does
not require writing an allocator — it requires ENABLING the one that already
exists.

`nros-platform-nuttx` has no `src/` at all (only `CMakeLists.txt`), so the
primitives come from C exactly as they do for posix. There is no Rust platform
layer to port.

#### The prize, and it is not the census count

`build-std = ["std", "panic_abort"]` in `nros-board-nuttx-qemu/nros-board.toml`
means **NuttX compiles the standard library from source** on every build. Two
consequences drop out if `std` goes:

* `build-std` narrows to `core`/`alloc`, which is a large build-time saving on
  every NuttX target and probe;
* the **patched libc may become unnecessary**. `nros-sizes-build` records why it
  exists: "The libc patch is mandatory: std's NuttX port references symbols the
  crates.io libc lacks (e.g. `_SC_HOST_NAME_MAX`)". That patch is a vendored
  fork carried at `third-party/nuttx/libc` — maintenance this campaign could
  retire outright.

That is a stronger reason to do W7 than the ~56 paths it removes.

#### Risks

* **Panic diagnostics regress.** The current hook flushes stdout and sleeps 5 s
  before diverging — deliberate, and recently load-bearing (issues 0572/0579
  turned on NuttX diagnostics to find a boot-tier fault). A `#[panic_handler]`
  must reproduce the flush, or a NuttX panic goes silent exactly where the last
  two bugs were found.
* **Two handlers link-fail.** If any dependency in a NuttX image already
  provides one, adding another is a hard link error rather than a warning.
* **The verification lane is QEMU + a cross toolchain**, so this cannot be
  proved by `check-no-std`; it needs `nuttx_qemu` runtime cells actually
  executed.

#### The three open questions — ANSWERED 2026-08-15

**1. Does the patched libc have any consumer other than std's NuttX port? No —
the fork is retired by W7.** The fork carries exactly four nano-ros commits on
upstream libc 0.2.178, and every one names std as the caller in its own title or
resolution:

| commit | patch | who needs it |
| --- | --- | --- |
| `bc6c8dfc6` | `_SC_HOST_NAME_MAX` | std's `sys/net/hostname/unix.rs` |
| `10d142a80` | missing pthread symbols | std's thread port |
| `826c4ca91` | `pthread_attr_t` sized for `CONFIG_SCHED_SPORADIC` | issue 0570's title: "every `pthread_attr_init`/`destroy` **from Rust std** smashes 36 bytes of the caller's frame" |
| `adb4c592e` | `poll()` `--wrap` ABI shim | issue 0167's title: "`struct pollfd` ABI mismatch (**std 8B** ↔ NuttX 24B)" |

The check that matters is the converse — whether nano-ros's OWN Rust code needs
the fork on NuttX — and it does not. `nros-node`'s `libc` dep is
`[target.'cfg(all(target_os = "linux"))'.dependencies]`, so it never reaches a
NuttX build at all; `nros-platform-cffi` declares `libc = "0.2"` but its sources
contain zero `libc::` paths. The remaining `libc` consumers (`nros-board-linux`,
`nros-cli-core`, `nros-tests`) are host/Linux-only.

**2. Does anything depend on unwinding? No.** `catch_unwind` /
`AssertUnwindSafe` appear only under `packages/testing/nros-tests` — the host
harness, which runs on Linux and is out of scope for a NuttX image. The
abort strategy is asserted in three independent places rather than one:
`build-std = [..., "panic_abort"]`, `panic = "abort"` in the workspace profiles,
and — decisively — `armv7a-nuttx-eabihf`'s own upstream target spec carries
`"panic-strategy": "abort"`. So the port owes the panic HOOK's behaviour
(diagnostics), never the unwinder.

**3. Is `_SC_HOST_NAME_MAX` reachable from a no_std path? No — purely std's.**
Every occurrence in the tree is the patch itself, a `.cargo/config.toml` comment
explaining the patch, or `nros-sizes-build`'s note about it. No nano-ros code
calls `sysconf` at all. Question 1 was therefore never open on this axis.

#### What the investigation additionally settled

**NuttX already has a complete non-std platform implementation, in C.**
`packages/platform/nros-platform-nuttx` is CMake-only — no `src/` — and builds
`libnros_platform_nuttx.a` from the POSIX `platform.c`/`net.c`/`timer.c`
verbatim, reaching NuttX's own libc. That is the path the C/C++ NuttX examples
already take today, on the same OS, with no Rust std anywhere. W7 is not
"write a platform layer for NuttX"; it is "make the Rust board use the one that
exists".

Mapping the base board's measured std surface onto the seam, all of it is
already exported by `<nros/platform.h>`:

| board's use | count | seam replacement |
| --- | --- | --- |
| `io::stdout` + `io::Write` | 30 | `nros_platform_log_write` / `_log_flush` |
| `process::exit` | 9 | `nros_platform_task_exit` — same call libstd makes (`_exit(2)`); an nsh-dispatched app task IS the process here |
| `thread::{Builder,spawn,scope}` | 4 | `nros_platform_task_init` / `_join` / `_detach`; its `attr` carries priority AND stack size, which is what issue 0246's `.stack_size` fix needed |
| `thread::{sleep,yield_now}` | 3 | `nros_platform_sleep_ms` / `nros_platform_yield_now` |
| `time::Duration` | 4 | none needed — `std::time::Duration` IS `core::time::Duration` |
| `fs::OpenOptions` on `/dev/urandom` | 1 | `nros_platform_random_fill` — the site DUPLICATES a seam facility |
| `os::unix::io::RawFd` | 1 | a `c_int` alias |
| `panic::{set_hook,take_hook}` | 2 | `#[panic_handler]` — still the one real change, as recorded above |

**One correction to the working hypothesis.** I expected upstream to declare the
NuttX targets no_std, which would have made building std there a fight against
the target spec. It does not: `rustc --print target-spec-json` reports
`"std": true` for both `armv7a-nuttx-eabihf` and
`riscv32imac-unknown-nuttx-elf`. std on NuttX is supported-but-tier-3, so the
cost is not illegitimacy — it is that no std ships prebuilt (hence `build-std`)
and libc's NuttX module lags (hence the four patches). The prize is real, but it
is build time and fork maintenance, not correctness.

#### Sizing, now that the questions are answered

Confined to two crates (`nros-board-nuttx` + its qemu wrapper, 56 paths), with
every replacement already exported by a C ABI that NuttX images link today. The
one genuinely new artifact is the `#[panic_handler]`, which must reproduce the
existing hook's flush-then-delay or NuttX panics go silent (issues 0572/0583).
Retiring `third-party/nuttx/libc` and narrowing `build-std` to `core`/`alloc`
are consequences of finishing, not separate work.

#### What the estimate above got wrong — IMPLEMENTED 2026-08-15

The two-crate figure was right about where the `std::` PATHS are and wrong about
where the WORK is. The code conversion was the small half; the flavour of an
image is decided in a dozen places that no `std::` grep reaches.

**The estimate missed that the C/C++ NuttX images are Rust bins too.** The
investigation established that NuttX has a complete C platform port and
concluded the C/C++ side was already std-free. It is not: `nros-nuttx-ffi` is a
Rust binary that supplies the kernel and calls `app_main()`, and it enabled
`std` on `nros-c` AND `nros-cpp`. So the family cannot go `no_std` two crates at
a time — `build-std` is a property of the TARGET, shared by both image shapes,
and it only narrows when both move.

**Two lang items, not one.** The risk section named the `#[panic_handler]`.
There is a second, `#[global_allocator]`, with the identical
exactly-once-per-image constraint and the identical two-owner problem
(`nros-c` supplies both for C/C++ images; the board must supply both for
pure-Rust ones). They are now ONE feature — `image-runtime`, default ON, which
the FFI bins switch off with `default-features = false` — because they are not
two decisions: an image that takes its allocator from `nros-c` takes its panic
handler from there too, and two flags would let a build pick one of each and
duplicate a lang item.

**The entry point changes shape.** Dropping `std` removes libstd's `lang_start`,
which is what defined the `main` symbol the board's `nsh_main` calls. So
`nros::main!` needs a `target_os = "nuttx"` arm emitting `extern "C" fn main`
(the shape the `target_os = "none"` C-runtime boards already use), every NuttX
Rust entry leaf needs `#![no_std] #![no_main]`, and the four hosted helpers
gated `not(target_os = "none")` need widening — NuttX had been silently taking
the hosted arm.

**Where the flavour is actually written down.** Seven places, and a grep for
`std` in the board crates finds none of them: the board descriptor
(`nros-board.toml`, ×2 witnesses), the leaf `.cargo/nros-board.toml` copies
`nros sync` generates from it (×9, tracked), `examples/fixtures.toml`'s per-row
`CARGO_UNSTABLE_BUILD_STD` (×3), a hand-written `.cargo/config.toml` in
`nros-tests/bins/logging-smoke-nuttx-qemu-arm` (the one copy sync does not
reach), each entry leaf's own `nros` feature list (×9), the node packages'
`crate-type` (a `staticlib` is a final artifact and needs the lang items a node
package must not own — the other `no_std` families are `rlib`-only for exactly
this reason), and the two FFI bins' feature lists.

**Two latent bugs surfaced, neither caused by this work:**

* `run_entry` was never `#[cfg]`-gated, while `run_tiers` and every helper both
  call are. This is issue 0579's class exactly — the gate one line above it
  belonged to `install_stdout_panic_hook`, which sat between the two, so it read
  as gated in context while being ungated in fact. Nothing noticed because a
  host build still had `std` to compile the ungated body against.
* `nros sync`'s source-metadata probe inherits `[unstable] build-std` from the
  board config through the same config walk-up its `[patch.crates-io]` entries
  require. `--target` was already overridden for that reason; `build-std` is its
  sibling and was not. It stayed invisible while the value was `["std", …]`
  (cargo then built `std` from source for the host too — consistent, merely
  slow) and became a hard error the moment it said `["core", "alloc", …]`:
  `core` built from source, linked beside the prebuilt host `std` that depends
  on a different one.

**Not done here, deliberately:** retiring `third-party/nuttx/libc`. It is
unnecessary now and its `[patch.crates-io]` simply goes unused, but actually
removing it touches the SDK index, `nros-sizes-build` and the CLI's patch
emission. That belongs after this is proven on hardware, not bundled ahead of
it.

#### Correction: `env` REQUIRES `std`, it does not grant it (2026-08-16)

The first cut of the `env` capability was `env = ["std"]`, and the first cut of
`metadata-mode` was changed the same way. Both are wrong, and
`check-feature-contract` clause (a) says so: *a capability/backend/platform
feature REQUIRES the heap, it does not grant it — emit `compile_error!` naming
the feature the user must add.*

`metadata-mode = []` plus a `compile_error!` was already the sanctioned shape in
this repo; changing it to `["std"]` looked like tightening an unenforced doc
comment and was actually a contract violation. Both now carry the guard
instead, in `nros` and in `nros-node`.

The consequence is worth stating plainly rather than hiding: **28 dep-sites
that name `env` name `std` again.** `env` reads `std::env::var`, so a hosted
consumer of `from_env` / `nros::init*` cannot be a `std`-free build today, and
pretending otherwise by having the capability switch the flavour on is exactly
the implicit-flavour problem this campaign exists to remove.

What the campaign still buys, unchanged: a target build names neither and never
compiles either item, and the ~39 `from_env` call sites keep working. What it
does NOT yet buy: those hosted consumers moving off `std`. That needs `env` to
reach the environment without the `std` FEATURE — an OS-detection path, or the
capability accepting that it is hosted-only — and it is not done here.

#### The `std` forward chain, mapped (2026-08-16)

The last step — deleting `std` from the nine crates — fails at RESOLUTION, not
compile, for anything still naming it. So the set is measured up front rather
than discovered one `cargo` error at a time:

| feature | manifest lines naming it |
| --- | --- |
| `nros-core/std` | 13 |
| `nros-serdes/std` | 12 |
| `nros-rmw/std` | 6 |
| `nros-node/std` | 5 |
| `nros/std` | 2 |
| `nros-params/std` | 1 |
| `nros-log/std` | 0 |

`nros-core/std`'s 13 are the shape of the whole problem: three are INTERNAL
forwards between core crates, eight are committed GENERATED message crates under
`packages/interfaces/**`, and two are node packages in `tests/simple-workspace`.

Which fixes the ordering. The codegen template
(`packs/scaffold/cargo_nros.toml.jinja`, byte-identical to its `templates/`
copy) emits `std = ["alloc", "nros-core/std", "nros-serdes/std"]`, so it cannot
be changed AHEAD of the core crates — a template that names a feature its
dependency no longer has is the same resolution error, just deferred to the next
`nros sync`. Template, generated crates, and core feature tables move in ONE
commit, and the two template copies move together.

#### The backend tier — **DONE 2026-08-16**

`nros-rmw-zenoh`, `nros-rmw-cffi` and `nros-bridge` each forwarded a core
crate's `std` (`nros-rmw/std` twice, `nros-node/std` once). That is why the
consumer sweep did not move the goal: measured on
`bins/qos-override-pubsub`, a leaf asking for `alloc` still resolved
`nros-core/std` ON, and `cargo tree -e features -i nros-core` named the reason —

    nros-core feature "std" <- nros-rmw feature "std" <- nros-rmw-zenoh feature "std" <- the leaf

The forward existed for exactly ONE thing: `Clock::system()`. `nros-core/std`
gates `extern crate std` and a wall-clock arm of `Clock::now()`, and nothing
else — without it, `ClockType::SystemTime` fell back to the STEADY counter,
which is not a degraded wall clock but a different quantity presented as time
since the Unix epoch.

So the fix is a clock, not a manifest edit. `nros-core` gained
`platform-clock`: the port's `nros_platform_time_since_epoch_{secs,nanos}`,
declared as a direct `extern` exactly as `nros-node` already declares
`nros_platform_clock_ns` ("every platform port exports it through the same
linkage contract" — nros-core sits BELOW nros-platform and cannot depend on
it). `nros-rmw-cffi/alloc` forwards it, since a C-ABI build always has a port.

Two details worth keeping:

* The ABI spends one instant over TWO symbols, and each call samples the clock
  separately. A second boundary landing between them pairs the old second with
  the new sub-second remainder — a timestamp that jumps a full second
  BACKWARDS, rarely and silently. The reader retries on a seconds re-read,
  bounded at three attempts. This is the concrete cost of the split that issue
  0532's remaining half removes; when it collapses to one `time_now_ns`, that
  loop is what goes away.
* A port with no wall clock answers `0/0`. That returns `None` rather than the
  Unix epoch, so the counter fallback stands instead of a wrong answer stated
  confidently.

Cost: `nros-core` cfg 3 -> 5, both arms selecting the clock. Bought: three
backends stop granting `std` to every graph that links them.

**Forwarders: 20 -> 17.** What remains is 6 core/api crates, 8 committed
generated message crates, `nros-tests`, and the 2 `tests/simple-workspace` node
packages — no backend among them.

#### The final commit's exact file set (2026-08-16)

After the consumer sweep (d48e78ea0), the crates that still name a core `std`
are a closed set of **20 tracked manifests plus the 2 codegen template copies**
— not an open-ended discovery. `git grep -l -E '"nros-(core|serdes|rmw|node|params|log|platform|prelude)/std"|"nros/std"' -- '*Cargo.toml'`
reproduces it:

- 6 core/api crates — `nros`, `nros-c`, `nros-cpp`, `nros-core`, `nros-node`, `nros-rmw`
- 8 committed generated message crates under `packages/interfaces/**`
- 3 backend/support crates — `nros-rmw-zenoh`, `nros-rmw-cffi` (`packages/rmw/cffi`), `nros-bridge`
- `packages/testing/nros-tests`
- 2 node packages in `tests/simple-workspace`

The backend three are the reason the campaign is not done when the examples
stop saying `std`. `nros-rmw-zenoh`'s own `std = ["alloc", "zpico-sys/std",
"nros-rmw/std", "log"]` keeps `nros-core/std` ON in every resolved native
graph, whatever the leaf asked for. A backend may legitimately keep a `std`
feature of its own — `zpico-sys/std` is a real POSIX shim — but it must stop
FORWARDING to a core crate's.

The untracked `bins/*/generated/` trees need no entry here: `git ls-files`
reports zero tracked files under them, so they are re-emitted from the template
on the next `nros sync` and follow it automatically.

### W8 — make `alloc` explicit — **STARTED 2026-08-15**

`alloc` is a separate feature (`std = ["alloc", ...]`), so the real flavours are
`core` / `core+alloc` / `std`. Measuring `nros` — the second-largest block and
until now unexamined — showed the gates were not merely undocumented, they were
WRONG.

**`node_metadata.rs`: 37 of the crate's 64 `std` gates re-labelled to `alloc`.**
The file's entire reason for a gate was one import:

```rust
use std::{format, string::String as StdString, vec::Vec as StdVec};
```

All three live in `alloc`. Gating them as `std` did not just mislabel them, it
WITHHELD the source-metadata JSON API from `no_std + alloc` targets that can run
it. Verified in both directions rather than assumed: with the re-gate a probe
referencing `SourceMetadataExport` compiles for `thumbv7m-none-eabi` under
`--no-default-features --features alloc`; with `node_metadata.rs` reverted the
same probe fails `E0432: unresolved import`.

`nros` cfg 64 -> 25 (-61 %), path 18 -> 17. The std build is unchanged, because
`std` implies `alloc`.

Remaining in `nros`, and it is genuinely hosted — not more mislabelling:

| count | what |
| --- | --- |
| ~10 | `init.rs` — `std::env::var` x6 (`NROS_LOCATOR`, `ROS_DOMAIN_ID`, `RMW_IMPLEMENTATION`), `std::path::Path` |
| ~10 | `lib.rs` — `extern crate std`, `pub mod init`, and the re-exports that follow from it |
| 3 | `time.rs` — an `Instant`+`OnceLock` epoch, the same pattern W4 unified inside `nros-node` |
| 4 | `node_runtime.rs` |

There is no `no_std` equivalent of an environment, and RFC-0045 makes the env
rung hosted-only by design, so `init.rs` stays.

#### Follow-up A — `core::error::Error` — **DONE 2026-08-15, and bigger than stated**

Framed as a spelling change. It is not. `nros-core` carried

```rust
#[cfg(feature = "std")]
impl std::error::Error for NanoRosError { … }   // + RclReturnCode
```

with a comment explaining that nested errors "don't implement
`std::error::Error` in no_std … a limitation of no_std". That limitation ENDED
when `core::error::Error` stabilised in Rust 1.81. The gate had stopped
describing a constraint and was simply WITHHOLDING the trait from every embedded
build — no `dyn Error`, no `?` against an error trait object, on the targets
that most need small error paths.

Both impls are now UNCONDITIONAL (`Display`/`Debug` were already). Verified in
both directions: a probe returning `&dyn core::error::Error` from a
`NanoRosError` compiles for `thumbv7m-none-eabi` under `--no-default-features`;
restoring the `std` gate fails it with
`E0277: the trait bound NanoRosError: core::error::Error is not satisfied`.

`nros-core` cfg 7 -> 5, path 5 -> 2. `nros`'s `init.rs` took the same spelling
(its `cfg` stays — that module reads the environment).

The `source()` still returns `None`, but the reason changed: the nested types
simply have no `Error` impls yet. That is now a to-do, not a platform limit, and
the comment says so.

#### Follow-up B — "`time.rs` should reuse W4's clock provider" — **WRONG, declined**

My own item, and inspection killed it. `time.rs`'s no_std arm ALREADY calls
`nros_platform_clock_ns` — the platform export — so there is no divergence to
fix there. Only the std arm uses an `Instant` epoch.

And there is nothing to reuse: W4's provider is `Executor::now_us()`, a PRIVATE
method requiring an `Executor` instance, while `nros::time::now()` is a free
function for portable node code that has no executor. Different requirements,
not duplicate implementations.

Unifying the std arm onto `nros_platform_clock_ns` would add a hard LINK
dependency to std builds that today need none — the same blocker that re-scoped
W5. The two epochs are also harmless: both are monotonic, and the module's own
contract is "compare instants, never interpret one absolutely".

#### The audit, completed 2026-08-15 — every remaining crate checked

**`nros-params` — same mislabel, fixed.** `types.rs` gated the
`ParameterVariant` impls for `String`, `Vec<i64>`, `Vec<f64>`, `Vec<bool>` and
`Vec<String>` on `std`; all five are `alloc` types. Re-gated to `alloc`
(cfg 13 -> 7, path 8 -> 1).

One trap on the way, and it would have been a silent regression: unlike
`nros` / `nros-node` / `nros-core`, this crate had `std = []` and `alloc = []`
as INDEPENDENT features, so re-gating alone would have removed the impls from
any consumer enabling only `std`. `std = ["alloc"]` now, matching the others.

Verified three ways rather than two, because the change had to prove both a
gain and the absence of a loss:

* probe on `--features std` compiles -> no regression for std consumers;
* probe on `--no-default-features --features alloc` for `thumbv7m-none-eabi`
  compiles -> the impls are newly available on embedded;
* restoring the old `std` gate makes that same build fail
  `E0599: no method named to_parameter_value` -> they genuinely were absent.

**`nros-cpp`, `nros-c`, `nros-node` — audited, NOT mislabelled.** Their `std`
use is real:

| crate | what it actually uses |
| --- | --- |
| `nros-cpp` | `std::env::var` (hosted boot config), `std::eprintln!`, `std::thread` |
| `nros-c` | `std::env::var`, `std::time::{SystemTime, Instant}` |
| `nros-node` | host-only features — see the residue classification above |

So W8's mechanical part is finished. What is left in these crates is genuinely
hosted (environment, threads, wall clock) or host-only features, not gates
pointing at the wrong flavour.

#### Score for the mislabel class

Three gates were found to WITHHOLD capability rather than protect it, and each
fix added something to embedded builds rather than only lowering a count:

| gate | withheld |
| --- | --- |
| `nros::node_metadata` | the source-metadata JSON API |
| `nros-core` error impls | the `Error` trait itself (no `dyn Error` on no_std) |
| `nros-params::types` | `ParameterVariant` for `String`/`Vec<…>` |

### W9 — lanes and cell checks — **DONE 2026-08-15**

Two pieces: a gate that makes the flavour of a platform knowable, and a lane
that consumes it.

**`scripts/check-flavour-lanes.py`** (wired into `check-fast`). Every
`matrix_platform` must resolve to exactly ONE flavour, so a lane keyed on the
platform cannot mix std and no_std images. Nothing is asserted: the flavour is
DERIVED — a board is std iff it enables `std` on its `nros`/`nros-platform`
deps, followed transitively through board->board deps, with the registry
supplying the board -> platform relation it already maintains.

The transitive part is not decoration. Reading each board's own manifest
reported **NuttX as no_std**, which is wrong — `nros-board-nuttx-qemu` enables
nothing itself, but the `nros-board-nuttx` it links does, and NuttX compiles
the standard library from source via `build-std`. The walk is cycle-safe
(`nuttx` and `nuttx-qemu` depend on each other, as do the threadx pair) and
over-approximates toward std, which is the safe direction: a doubtful board
stays OUT of the no_std lane.

Derived today — 3 std, 8 no_std:

| flavour | platforms |
| --- | --- |
| std | `Linux`, `NuttxArm`, `NuttxRiscv` |
| no_std | `Esp32Qemu`, `FreertosMps2`, `Fvp`, `QemuBaremetal`, `ThreadxLinux`, `ThreadxRiscv64`, `ZephyrNativeSim`, `ZephyrQemuCortexM` |

Verified both ways: pointing a no_std board at `Linux` in the registry makes the
gate fail naming both sides; restoring it passes.

**`lane-filter.sh nostd`** emits the no_std lane from that same derivation, so
gate and lane cannot disagree. Two hazards were found while writing it, and both
are the reason it is an INCLUSION union rather than exclusions:

* The host `Linux` has NO token, while `ThreadxLinux` — a no_std board running
  as a Linux process — contains "linux". "Exclude the std platforms" would
  therefore have dropped a platform the lane exists to run.
* Family tokens overlap across flavours: `QemuBaremetal`'s token is "qemu", and
  `tests/nuttx_qemu.rs` matches it, so the union ALONE would have pulled std
  NuttX into the no_std lane. The lane therefore also emits
  `not test(~nuttx)` — caught by inspection before shipping, and exactly the
  mixing this item exists to prevent.

**On "checks on cells".** The gate is platform-level, and that IS the cell
guarantee rather than a weaker substitute: every cell names a platform, and a
platform now provably carries one flavour, so no cell can straddle. A separate
per-cell assertion would re-check the same fact.

**Not delivered:** a `std` lane mode. The host half is already served by the
existing `native` lane, and the only other std platform is NuttX, whose tests
are toolchain-gated out of tier 1 anyway — so a `std` mode would today select
either exactly `native` or a set nothing can run. Adding it when it selects
something real is better than adding it now.

#### What phase-361's gate expects from W10 (added 2026-08-16)

`check-feature-contract` (phase-361 W4) currently asserts three things ABOUT
`std` that stop meaning anything the moment this work item lands, and they
should be edited in the SAME change rather than after it — a gate enforcing a
contract about a deleted feature is its own kind of stale:

* **(a/manifest)** "a crate declaring both lists `std = ["alloc", …]`" — with no
  `std` feature there is nothing to declare. The half that SURVIVES is "no
  feature other than `std` may enable `alloc`", which narrows to "no feature may
  enable `alloc`".
* **(a/source)** "`cfg(any(feature = "alloc", feature = "std"))` is rejected" —
  unstateable afterwards, and harmless to keep, but it should go with the rest.
* **(b)** "no `no_std`-capable crate defaults to `std`/`alloc`" — narrows to
  `alloc`.

Two issues close by construction when this lands: **0591** (a `default` cannot
name a feature that does not exist) and **0598** (nothing can imply `alloc`).
**0594 does not** — it is about `alloc`, which survives as the remaining axis,
and its last open site is the `std` forward this work item removes
(`nros-tests`' `trigger-test = [… "nros-node/std"]`).

### W10 — flip the default, delete the feature — **IN PROGRESS: `nros-node`'s three std-backed blocks are ported/deleted 2026-08-16**

`nros` currently defaults to `std`. (`nros-platform` no longer does —
phase-361 W3 made it `default = []`.)

#### What deleting the feature actually touches

| surface | count | note |
| --- | --- | --- |
| `cfg` sites in the nine crates | 166 | `nros-node` is 106 of them |
| `std::` paths | 129 | `nros-node` 76, `nros-cpp` 26 |
| consumer manifests naming `std` on a core crate | 54 | plus `nros-board-linux` |
| generated message crates declaring `std = ["nros-core/std", …]` | 8 | committed output |
| codegen templates that EMIT that declaration | 2 copies | `packs/scaffold/` and `templates/` — they must move together, or `nros sync` re-adds the feature to every regenerated crate |

The manifest half is mechanical but must land in ONE change: a manifest naming
a feature that no longer exists is a hard resolution error, not a warning.

#### `nros-node`'s 106 sites are mostly paired, and the remainder is three features

Measured over `spin.rs` (79 of the 106): 52 `std` arms against 23 `not(std)`
arms. Pairing them by item name — after stripping the `_alloc` suffix the
no_std twins carry — leaves 40 unpaired, and those cluster into exactly three
things plus noise:

1. **The condvar wake fallback** (~11 sites: `wake_cv`, `wake_mu`, `WakeCtx`,
   `nros_rmw_runtime_wake_cb{,_from_isr}`, `wake_ctx_ptr`,
   `install_wake_signal_on_primary`). **Decision-free — delete it.** It was
   never the wake mechanism: `spin_once` picks at RUNTIME on
   `node_wake.is_some()`, preferring the kernel-native semaphore and reaching
   the condvar only when no platform wake primitive is linked. Every supported
   std host links one (the POSIX C port has had `nros_platform_wake_*` since
   phase 130, with its own `c_port_posix_wake` test), so on Linux the branch is
   already dead code. Where no wake exists, the `alloc` arm installs no callback
   and drives the transport for the full timeout — it still BLOCKS, which is
   what the condvar audit checked for.
2. **`scheduler-os-priority`** (~16 sites: `OsPriorityWorker`, `ThreadHandle`,
   `os_priority_workers: std::collections::HashMap`, `WorkItem`,
   `open_threaded`, two `Drop` impls). A real capability built on `std::thread`
   + `mpsc` + `HashMap`.
3. **`signal-fd-wake`** (Linux-only: `WakeSignalFd` spawns a
   `std::thread::JoinHandle` worker on an `eventfd`, and its worker signals the
   condvar directly — so item 1 cannot be finished without deciding this one).

The rest is the std clock provider (`clock_base: Instant`, `std_epoch_us` — W4
already unified the ACCESSOR, this is the last provider) and one `eprintln!`.

#### The decision — SETTLED 2026-08-16: port, do not move

Items 2 and 3 are `std::thread`-backed capabilities living in a crate W10 makes
`no_std`. The choice was between porting them onto the platform task ABI and
moving them out of `nros-node` into a std-side crate. **Ported**, so both become
reachable on every platform instead of staying std-only corners of a core crate,
and no public import path breaks.

##### What the port needed first

`nros_platform_task_init` takes "opaque caller-provided storage (size determined
by the implementor)" and had **no way to ask what that size is**. Fine for a C
caller — POSIX writes `pthread_t t;` and passes `&t` — and impossible from Rust,
which is the caller being added. Hard-coding it instead is issue 0570 exactly.

The wake primitive had already solved this with
`nros_platform_wake_storage_{size,align}`, so tasks now match it: the same five
ports gained `nros_platform_task_storage_{size,align}` returning their own
storage type (`pthread_t`, `nros_freertos_task_t`, `TX_THREAD`,
`nros_esp_task_t`), plus the C test stub, with the committed bindgen output
regenerated because the header is the ABI SSoT (RFC-0054).

##### The three landings

| what | `nros-node` cfg / path |
| --- | --- |
| ABI probes (prerequisite, no consumer) | — |
| `scheduler-os-priority` -> platform tasks | 106/76 -> 95/65 |
| `signal-fd-wake` -> platform task | 95/65 -> 95/54 |
| condvar wake path deleted | 95/54 -> **91/40** |

`std::thread::spawn` -> `nros_platform_task_init`; `JoinHandle::join` ->
`nros_platform_task_join`; `std::sync::mpsc` -> `heapless::mpmc::MpMcQueue` +
`NodeWake`; `recv_timeout(10ms)` -> `NodeWake::wait_ms(10)`; `HashMap` ->
`heapless::FnvIndexMap`. The allocate-spawn-join sequence is shared
(`executor/platform_task.rs`) rather than written twice, and `join` is an
explicit method rather than `Drop` because both callers must signal their worker
BEFORE waiting — a `Drop` that joined implicitly would deadlock against a worker
still blocked in its own wait.

##### Four consequences, all deliberate

* **The mailbox is bounded.** `mpsc` was unbounded; a producer outrunning a
  worker grew it without limit. `try_dispatch` now reports refusal and the
  caller dispatches cooperatively — backpressure instead of unbounded memory on
  an RT path. The old spawn was infallible (`.expect`) and the old queue never
  refused, so the dispatch site had no failure branch; it has one now.
* **The pool is capacity-limited** (8 priorities) and remembers a level the
  platform refused, so a failing spawn is tried once rather than every cycle.
* **A platform is now REQUIRED to host workers.** `NodeWake` is gated
  `all(feature = "alloc", feature = "rmw-cffi")` because it calls
  `nros_platform_*`; blocking on it inherits that. A std thread needed no
  platform, so the port ADDS this dependency — and it is the honest one: a
  worker cannot be given an OS priority without an OS. Registering the
  dispatcher stays available everywhere; only the pool needs the platform.
* **`signal-fd-wake` keeps its `target_os = "linux"`.** The eventfd is what
  makes the write async-signal-safe. What it stopped being is std-only.

##### Coverage moved, and one gap was already there

`test_os_priority_worker_dispatches_callback` lives in a module compiled
`not(feature = "rmw-cffi")` (a real backend would displace `MockSession`), so in
that configuration there is no pool and the test now rides the cooperative
fallback. Its assertion still holds; its wording was corrected rather than left
implying worker coverage it no longer has.

`tests/signal_fd_wake.rs` cannot link in ANY configuration — `nros-node` has no
platform provider in its dev-dependencies — and no lane enables the feature.
Both predate this work; filed as **#0612**, issue 0577's class. The signalfd
port is therefore compile-verified only.

#### The condvar deletion, which the ports unblocked

Attempted first and reverted on 2026-08-16, because `WakeSignalFd`'s worker
called `cv.notify_all()` and deleting the pair mid-way leaves the hottest path
half-migrated. Landed after both ports: `Executor::{wake_cv, wake_mu}`,
`WakeCtx::{cv, mu}` and the `wait_timeout_while` arm are gone, and the runtime
wake callback signals ONE primitive because `spin_once` waits on one. Phase
130.3 predicted this edit when it added the semaphore beside the condvar — "a
future migration to a single primitive flips one branch instead of two".

Where that arm was reached (no platform wake linked), the spin now drives the
transport for the full timeout: an async wake can no longer cut the wait short,
but it still BLOCKS, and it is what the `alloc` arm always did there.

#### What remains of W10

The three `nros-node` blocks are done; 91 cfg sites and 40 paths remain there,
now mostly paired arms that collapse mechanically. Still untouched: the other
eight crates (60 cfg sites), the 54 consumer manifests, the 8 generated message
crates, the 2 codegen template copies, and `nros-board-linux` — the manifest
half, which must land as one change because a manifest naming a deleted feature
is a resolution error.

## Costs accepted

* **Panic ergonomics** — no `RUST_BACKTRACE`, typically `panic=abort`. Partly
  recoverable: the application binary may still link `std`.
* **Core dependency freedom** — permanently constrains what core may depend on.
  Cost today ~zero (`heapless`, `portable-atomic`, `atomic-waker` are already
  no_std).
* **`std::thread` conveniences** — join results, names, scoped threads.

## Not measured

`nros` (61 cfg sites) and `nros-params` (11) are uninspected — a large part of
the work and the weakest estimate here. (Both were measured later — see W8.
W7 was sized on 2026-08-15 and is no longer the unknown this paragraph
described.)

And expect untested code. Two paths in one session turned out to have no lane
at all: seven `std`-gated `nros-node` tests that no lane ran, one of which had
NEVER passed (issue 0577), and the extra-session wake install that was `std`-only
on the dynamic path with no no_std multi-RMW test to catch it. Budget for that
rather than treating each as a surprise.
