# Phase 345 — The `std`/`alloc` contract, and what a firmware build actually compiles

**Status (2026-08-10).** IN PROGRESS — W1, W2.a, W3, W3.b and all of W8 landed;
the feature contract holds as a STATE (0 implicit `alloc`/`std` enables, no
`no_std` crate defaults to `std`, one `#[global_allocator]`). **W4, the gate
that would make it an invariant, is not written** — until it is, every number
here is a measurement. W2.b (two dead declarations) needs a decision; W5–W7
(dependency weight, issue 0494) are not started.

**Numbering.** Drafted offline as "phase-341" against a stale `main`; upstream
had already spent 341–344, and issues 0467–0471 besides. Renumbered on rebase
(2026-08-10) — 341 → 345, issues 0467–0471 → 0492–0496. Same class as the seven
collisions `just issue-new` exists to stop; the phase series has no equivalent
reservation, which is why this one had to be fixed by hand.

**Touches:** RFC-0005 (RMW layer), RFC-0006 (portable RMW/platform interface),
RFC-0033 (message field capacity — the `mode = "heap"` types this contract
governs), RFC-0034 (the `nros_platform_alloc` funnel W8.c enforces), RFC-0062
(unified dependency SSOT).
**Opens:** issue 0492 (`std` implies `alloc` in half the stack), issue 0493
(`default = ["std"]` splits compile identities), issue 0494 (47 of 57 crates in
a firmware build are proc-macro host tooling), issue 0496 (34 implicit
`alloc`/`std` enables; the allocator-ownership half is closed by W8.c).

**Not phase-334 / phase-340 territory.** Those two change *where* an artifact
lives and *when* it can be reused. This phase changes *what gets compiled at
all* and *what a feature flag means*. They touch disjoint files and compose:
issue 0493 names one of the five `-C metadata` identities issue 0446 counts, and
it is the one that survives any cache-layout fix, because to cargo the two units
are genuinely different feature sets.

## Goal

Two things a user should be able to answer without reading nano-ros source:

1. **"What do I turn on?"** — one axis, one meaning, the same in `packages/core`,
   `packages/rmw`, and `packages/platform`. Today `std` implies `alloc` in four
   crates and not in five others, and the source layer disagrees with the Cargo
   layer in `nros-core`.
2. **"What am I paying for?"** — a firmware build compiles the crates that end
   up in the image, plus what the proc-macro genuinely needs, and nothing that
   was added for an arm the user did not use.

## The map today

### Core

| crate | `default` | `std` | `alloc` |
| --- | --- | --- | --- |
| `nros` (umbrella) | `["std", "ffi-size-markers"]` | `["alloc", …5 crates]` | forwards to 5 |
| `nros-core` | `["std"]` | `["nros-serdes/std"]` | `["nros-serdes/alloc"]` |
| `nros-serdes` | `["std"]` | `[]` (1 cfg site) | `[]` (2 cfg sites) |
| `nros-params` | `["std"]` | `[]` (13 cfg sites) | `[]` (1 cfg site) |
| `nros-node` | `["std"]` | `["alloc", …]` | `[…, "dep:portable-atomic-util"]` |
| `nros-rmw` | **`[]`** | `["nros-core/std", "log"]` | `["nros-core/alloc"]` |

`nros-rmw` is the only converted one. Its manifest says *"explicitly (matches
nros-core). Previously `default = ["std"]`"* — but `nros-core` still declares
`default = ["std"]`. The convention was decided and applied once.

`nros-node` and `nros` say `std = ["alloc", …]`. `nros-core`, `nros-serdes`,
`nros-params`, `nros-rmw` do not. `nros-core/src/lib.rs:19` gates `extern crate
alloc` — and the `heap::{Vec, String}` re-export RFC-0033 codegen emits — on
`any(alloc, std)`, i.e. the source assumes the implication its own manifest does
not make. Issue 0492 has the reproducer: at `nros-core`'s **default** feature
set, `nros_core::heap::Vec<u32>` exists and has no `Serialize` impl.

### RMW

| crate | `default` | `std` | `alloc` |
| --- | --- | --- | --- |
| `nros-rmw-cffi` | `[]` | `["nros-rmw/std"]` | `["nros-rmw/alloc"]` |
| `nros-rmw-zenoh` | `["platform-aliases", "link-ip"]` | `["alloc", "zpico-sys/std", "nros-rmw/std", "log"]` | `["nros-rmw/alloc"]` |
| `nros-rmw-zenoh-staticlib` | `[]` | `["nros-rmw-zenoh/std"]` | `["nros-rmw-zenoh/alloc"]` |
| `nros-rmw-xrce-cffi` | `[]` | `[]` (1 cfg site) | — |
| `nros-rmw-cyclonedds` | `[]` | `[]` (**0 cfg sites, no forwarding — dead**) | — |
| `nros-rmw-cyclonedds-sys` | `["vendored"]` | `[]` (1 cfg site) | — |
| `zpico-sys` | `["platform-aliases", "link-ip"]` | `[]` (1 cfg site) | — |
| `nros-bridge` | `[]` | `["nros-node/std"]` | `["nros-node/alloc"]` (0 cfg sites — pass-through) |

This layer is already close to right: `default = []` everywhere, and where
`default` is non-empty it selects *linkage* (`link-ip`, `platform-aliases`,
`vendored`), never `std`. The two defects are `nros-rmw-zenoh` implying `alloc`
from `std` while its sibling `nros-rmw-cffi` does not, and the dead
`nros-rmw-cyclonedds/std`.

### Platform

| crate | `default` | `std` | `alloc` |
| --- | --- | --- | --- |
| `nros-platform` | **`["std"]`** | `[]` (1 cfg site) | `[]` (**0 cfg sites, forwards nowhere — dead**) |
| `nros-platform-cffi` | `[]` | — | — |
| `nros-platform-mps2-an385` | `["libc-stubs"]` | — | — |
| `nros-platform-esp32-qemu`, `-stm32f4`, `nros-baremetal-common` | `[]` | — | — |

`nros-platform` is the only crate in its layer that defaults to `std`, and its
`alloc` feature does nothing except get implied by `global-allocator = ["alloc"]`.
A user enabling `nros-platform/alloc` to get an allocator gets a no-op.

## What it costs

`cargo check --workspace --timings`, fresh target dir, 48 cores (the run stops
at `cyclonedds-sys`, issue 0390 — lower bound):

```
units = 497   cpu = 113 s   19 crates compiled under >1 feature set   ~8 s redundant

nros-core   0.60 s [alloc, std]   +  0.31 s [alloc, std, default]
nros-params 0.64 s [alloc, std]   +  0.31 s [alloc, std, default]
nros-node   0.59 s [alloc, log, std] + 0.87 s [alloc, log, std, default, rmw-cffi]
```

For `nros-core` and `nros-params` the two units differ by **the inert string
`default` and nothing else**. `libc`, `crossbeam-utils`, `winnow`, `toml_parser`
and `memchr` split the same way in the same run.

`cargo build -p nros --target thumbv7em-none-eabihf --no-default-features
--timings`, fresh target dir:

```
units = 96   cpu = 23.5 s   wall = 7.8 s
57 unique crates; 47 reachable only through the nros-macros proc-macro; 11 of runtime

attributed by name:  macro subtree 20.9 s (89 %)   everything else 2.6 s

  1.45 s  syn                        1.05 s  serde_core
  1.24 s  toml_edit                  0.91 s  winnow
  1.23 s  ros-launch-manifest-model  0.76 s  ros-launch-manifest-sched
  1.21 s  serde_derive               0.65 s  serde_yaml_ng
  1.15 s  nros-macros                0.56 s  thiserror-impl
```

Every one of the twelve most expensive units is host tooling for the
proc-macro. Note the triple: `thumbv7m-none-eabi` is **not** in
`rust-toolchain.toml`'s target set, so a build against it truncates and its
timings are not comparable — use `thumbv7em-none-eabihf`.

## Work items

**W1 — the contract, written once (REVISED).** Add to
`docs/design/ARCHITECTURE.md` §2 (feature axes) as normative text:

> `std` and `alloc` are INDEPENDENT axes, and both are enabled only by the end
> user, per package. `std` does not imply `alloc`. No feature other than `std`
> or `alloc` may enable either — a crate that requires the heap or the standard
> library says so with `compile_error!` naming the feature the user must add,
> and never turns it on for them.
>
> Orthogonally: `malloc` and `panic` are UNIFIED PER PLATFORM. Exactly one
> `#[global_allocator]` and one `#[panic_handler]` per image, selected by the
> `platform-<rtos>` feature — which selects the provider and nothing else.

Every crate feature table points here instead of restating it. *No code.*
Acceptance: the rule exists in exactly one place.

*(The earlier version of W1 said the opposite — "a crate that declares both must
declare `std = ["alloc", …]`". That is wrong for an embedded target: it makes
`std` a way to acquire a heap without asking for one. Superseded; issue 0492's
defect is fixed on the source side instead, see W2.)*

**W2 — make the manifests obey it.** Two halves, because only one of them is
monotonic.

*W2.a — SUPERSEDED, then reversed (LANDED).* This first landed as
`std = ["alloc", …]` on six crates. That is implicit heap enablement and is
exactly what W1 (revised) forbids, so it was reverted, and the `std ⇒ alloc`
edge was removed from the six crates that already had it too —
`nros`, `nros-node`, `nros-c`, `nros-cpp`, `nros-log`, `nros-rmw-zenoh`.
**No crate in the workspace now has `std` listing `alloc`.**

The edge was never load-bearing. Every crate compiles with the axes independent:

```
nros-core / nros-serdes / nros-params / nros-rmw / nros-rmw-cffi / nros-bridge
    --features std   (no alloc)   ok
    --features alloc (no std)     ok
nros-node / nros / nros-c / nros-log
    --features std   (no alloc)   ok
nros-node / nros     --features std,alloc              ok
```

Issue 0492 is fixed on the SOURCE side instead: `nros-core/src/lib.rs` gated
`extern crate alloc` and the RFC-0033 `heap::{Vec, String}` re-export on
`any(alloc, std)`; both are now `alloc` alone. A `std`-only build therefore no
longer gets heap types whose `Serialize` impls `nros-serdes` — which received
only `std` — was never asked to compile. The reproducer fails at the import
(`unresolved import nros_core::heap`, pointing at the gate) rather than at a
missing trait impl deep in serialization.

*W2.b — the dead declarations (OPEN, needs a decision).* `nros-platform/alloc`
is commented *"capability feature — enabled by RMW shims to declare
requirements"*: declarative by design, inert in fact — 0 `cfg` sites, forwards
nowhere, and nothing in-tree names it. Either wire it (forward to the platform
crates that actually allocate) or delete it and make `global-allocator = []`.
`nros-rmw-cyclonedds/std` is unambiguous: delete. `nros-platform` is also the
only crate in its layer still on `default = ["std"]`, which is W3's problem —
decide W2.b and W3 together for that crate.

**W3 — `default = []` on every `no_std`-capable crate (LANDED).** `nros-core`,
`nros-serdes`, `nros-params`, `nros-node`, `nros-platform`, `nros`, `nros-c`,
`nros-cpp`, plus every in-tree dep-site made explicit (10 of them; the rest of
the workspace — 199 of 200 `nros` dep-sites — already spelled
`default-features = false` and named its features). Breaking for out-of-tree
consumers: `nros-core = "0.5"` is now a `no_std` build. Needs a release note.

*The acceptance criterion this phase originally stated was wrong, and the
measurement is recorded in issue 0493.* `default = []` did **not** merge any
compile unit: 497 units and 19 split crates before and after. An empty `default`
is still a feature NAME (only omitting the key removes it), and `--workspace`
builds every member as a root with its own defaults anyway. The two units are
the resolver-v2 host graph and target graph, which are legitimately different.

What W3 actually bought, and why it stays:

- **The host/proc-macro side no longer compiles the core crates with `std`.**
  Before, `nros-core` on the `nros-macros` → `nros-orchestration-ir` →
  `nros-rmw` path resolved `[alloc, std]`; now `[]`. Real work removed — it
  shows up as a cheaper unit, not a merged one.
- **Nothing can acquire `std` without saying so**, which is the user-facing
  property this phase exists for. A consumer picks per package: `std` here,
  `alloc` there, neither in the entry.
- **It surfaced issue 0495** — `nros/ffi-size-markers` was reachable only
  through the `default` set that both C/C++ consumers disable, so a `-p nros-c`
  build (what cmake/corrosion runs) never had the markers. Now requested
  explicitly at all four dep-sites.

Verified after landing:

```
host        nros / nros-c / nros-cpp / nros-node / nros-platform, --features std   ok
host        nros, no features                                                      ok
thumbv7em-none-eabihf   bare and alloc, 8 core+rmw crates                          ok
aarch64-unknown-none    bare                                                       ok
armv7r-none-eabihf      alloc                                                      ok
cargo metadata --workspace                                                         ok
```

`cargo check --workspace` still stops at `cyclonedds-sys` and
`nros-rmw-xrce-cffi` — every vendored `-sys` submodule is uninitialised on this
host (`git submodule status` shows 10+ at `-`), which is issue 0390's class and
predates this work. The C/C++ and RMW lanes therefore remain UNVERIFIED here;
they need a provisioned tree and `just ci-matrix`.

**W3.b — the codegen template (LANDED, and the premise was wrong).**

The claim this item started from — "every user-generated message package
silently defaults to `std`" — is FALSE, and the correction matters more than the
change. Measured by running the real generator into a scratch dir:

```
nros generate-rust --force -o <tmp> --rename builtin_interfaces=... --rename rcl_interfaces=...

[features]
default = []
std = ["nros-core/std", "nros-serdes/std"]
```

Users' generated crates have been `default = []` all along. The live manifest
comes from a hardcoded `format!` in `rosidl_bindgen::generator::
generate_cargo_toml` (`generator.rs:540`, written at `:629`), not from a
template.

What is actually true: `packages/cli/rosidl-codegen/packs/scaffold/
cargo_nros.toml.jinja` (mirrored byte-identical at `templates/`) DOES render a
`Cargo.toml` carrying `default = ["std"]` — and **that render is discarded**.
`rosidl-bindgen` calls `generate_nros_message_package` and consumes only
`generated.message_rs`; `GeneratedNrosPackage::cargo_toml` has no consumer
outside `rosidl-codegen`'s own tests. So the block drifted with nothing to
notice.

Landed anyway: the template's `[features]` now matches the live emitter's
`default`, carries `std = ["alloc", …]` per W2, and says in a comment that its
own output is dead on the current path. Rationale — phase-335 is wiring the
language-neutral IR path, and a dormant template that disagrees with the live
emitter is exactly how the next `default` regression arrives. The `#[used]`
lesson from issue 0495 is the same shape: a value that is only correct by
accident stays correct until the accident stops.

No regeneration of the six committed in-tree crates is needed — they already
carry `default = []`, and the `generate-lifecycle-msgs` recipe's own closing
`NOTE` says those manifests get workspace inheritance re-applied by hand after
generation, so they were never a byte-for-byte template product.

**W4 — the gate (OPEN — the state is right, nothing holds it there).**
`scripts/check-feature-contract.sh`, wired into `just ci`, asserting over every
workspace member:

- **(a)** no feature body other than `std`/`alloc`/`default` enables `std` or
  `alloc` — the user spells the heap at their own dep-site, per package.
  *An earlier draft of this clause said the opposite* ("a crate declaring both
  `std` and `alloc` lists `alloc` in `std`"), written before W1/W2 decided to
  DELETE the `std ⇒ alloc` edge. Enforcing it would re-break issue 0492.
- **(b)** no `no_std`-capable crate declares a non-empty `default` containing
  `std` or `alloc`.
- **(c)** every declared `std`/`alloc` feature has a `cfg` site or forwards to a
  dependency — catches `nros-platform/alloc`, `nros-rmw-cyclonedds/std` and
  `nros-cpp/global-allocator`, all dead declarations found by hand.
- **(d)** no feature listed in a `default` set is UNREACHABLE from every
  non-`default-features` dep-site in the workspace — catches issue 0495.
- **(e)** exactly ONE `#[global_allocator]` definition exists in the tree, and
  it is `nros-platform`'s — the W8.c invariant. A grep-level check: the audit
  that found four of them was a grep, and the fifth will be too.

Acceptance: the script fails on a deliberate reintroduction of each of the five.
Until it exists, every number in this phase is a measurement, not an invariant.

**W5 — gate the `model = "…"` arm.** Feature-gate
`ros-launch-manifest-{model,sched}` in `nros-orchestration-ir` (which has no
`[features]` block today) and `nros-macros`; the arm emits `compile_error!`
naming the feature when off. Drops 7 crates including `unsafe-libyaml` and the
duplicate `thiserror` major, and removes a **git** dependency from every cold
firmware build. Acceptance: the 57-crate count drops to 50.

**W6 — `toml` 0.8 → 0.9** in `nros-macros`, `nros-orchestration-ir`,
`nros-board-common`. Drops `toml_edit`, `winnow 0.7` (the most expensive unit in
the firmware build), `toml_write`, `serde_spanned 0.6`, `toml_datetime 0.6`, and
un-splits the resolver against the 0.9 that `nros-tests`/`cbindgen` already
pull. **Blocked on W5** — the git deps pin 0.8 and would hold the old tree
alive. Acceptance: one `toml` version in `cargo tree`; 45-crate count.

**W7 — `nros-macros` optional.** A default-on `macros` feature on `nros`, so a
consumer that hand-writes its entry point (already supported) drops all 47.
Acceptance: `cargo tree -e normal -p nros --no-default-features --features std
--target thumbv7em-none-eabihf` is the 11 runtime crates plus `paste`.

**W8 — no feature may enable `alloc` or `std` but `alloc`/`std` (LANDED).**
Issue 0496 enumerated 34 sites. **0 remain**, and the `#[global_allocator]`
count is 4 → 1.

- **W8.a (done)** — `global-allocator = []` on `nros-c`, `nros-cpp`,
  `nros-platform`. The `["alloc"]` was gratuitous: all three allocator modules
  use only `core::alloc::GlobalAlloc`, and `extern crate alloc` is gated
  separately.
- **W8.b (done)** — new `panic-spin` feature on `nros-c` (forwarded by
  `nros-cpp`). `#[panic_handler]` moved off `all(global-allocator, not(std),
  not(panic-halt))` onto `all(panic-spin, not(std), not(panic-halt))`, so "I
  need a panic handler" is sayable without "I need a heap". The `platform-*`
  rows select it, keeping malloc and panic unified per platform.
- **W8.c (LANDED) — `nros-platform` is the single owner of the allocator.**
  Four crates could install a `#[global_allocator]`: `nros-platform` (over
  `<ConcretePlatform as PlatformAlloc>`), `nros-c` (a direct `extern "C"
  nros_platform_alloc`), `nros-platform-mps2-an385` (its own `FreeListHeap`
  static) and `zpico-alloc` (a `GlobalAlloc` impl for that heap). The first two
  sat under *identical* gates and were kept apart by a manifest comment —
  `nros-c` deps `nros-platform` non-optionally, so any image enabling both got
  a duplicate lang item.

  The earlier note called this undecidable because "cargo offers no way for
  either crate to detect the other's feature". That framed it as a detection
  problem when it is an ownership problem: with ONE definition site, cargo's own
  feature unification makes the collision unspellable, and no detection is
  needed. `nros-c/global-allocator` forwards to
  `nros-platform/global-allocator`; the other three definitions are deleted.

  `nros-platform` is the right owner because it is the only one that covers
  both link shapes. Every `platform-*` feature resolves `ConcretePlatform` to
  `CffiPlatform` (`resolve.rs`), whose `PlatformAlloc` impl *is*
  `nros_platform_alloc` — the same funnel nros-c called directly — while the
  bare-metal Rust crates (mps2-an385, stm32f4, esp32-qemu) reach their own
  arena through the same trait. One API, one arena, per RFC-0034 D6.

  Three things fell out of it:

  - **`extern crate nros_platform` is load-bearing.** A `#[global_allocator]`
    reaches the image only if the crate DEFINING it is linked, and a dependency
    never named in code is dropped first — the `FORCE_LINK` DCE class again.
    Without it `nros-c --features platform-threadx,alloc` fails with *"no global
    memory allocator found"* while `cargo tree` shows
    `nros-platform feature "global-allocator"` enabled. `alloc-stats` masked the
    failure by giving the crate an unrelated reason to be referenced, so the
    matrix below deliberately tests `alloc` WITHOUT it.
  - **The `alloc-stats` counter moved to `nros-platform`,** beside the allocator
    it instruments. `nros-c`/`nros-cpp` keep the four `#[no_mangle]` C names and
    read the accessors. Both had defined their own `HeapStats` static exporting
    the SAME symbols, so enabling `alloc-stats` on both was a duplicate-symbol
    error waiting to happen. The counter is a pair of `AtomicUsize` written
    inline; pulling `zpico-alloc` (RMW layer) into the platform layer for it
    would invert RFC-0001's layer map, and the dep is gone from both API crates.
  - **Over-aligned requests now FAIL instead of silently succeeding.** Both
    deleted allocators discarded `layout.align()` and returned 8-aligned memory
    for any alignment — UB no build could observe. The platform ABI has no
    alignment parameter, so the surviving allocator answers `align > 8` with
    null and lets `handle_alloc_error` fire, which is what `zpico-alloc`'s impl
    already did. Behaviour change, deliberate: nothing in the nros runtime
    exceeds 8-byte alignment, so a request that does was already broken.

  `nros-cpp/global-allocator` was deleted outright — a dead declaration with
  zero `cfg` sites whose comment claimed it installed an allocator, while the
  single-runtime rule two lines below said nros-c owns it. This is exactly the
  class W4 clause (c) exists to catch.

  Verified:

  ```
  nros-c    platform-{threadx,zephyr,freertos},alloc  (thumbv7em)      ok
  nros-c    platform-threadx (no alloc) | +alloc,alloc-stats           ok
  nros-c    std,rmw-cffi,platform-posix,ros-humble [,alloc-stats]      ok
  nros-cpp  platform-{zephyr,threadx},alloc [,alloc-stats] (thumbv7em) ok
  nros-platform  platform-threadx,global-allocator[,alloc-stats]       ok
  nros-platform-mps2-an385  [cffi-export]  (thumbv7m)                  ok
  boards: mps2-an385, mps2-an385-freertos, threadx-qemu-riscv64        ok
  zpico-alloc  --no-default-features | stats  (10 tests)               ok
  check-no-direct-kernel-alloc.sh                                      clean
  `#[global_allocator]` definitions in the tree              4 -> 1
  ```
- **W8.d (done)** — 13 `platform-*` bodies across `nros-c`, `nros-cpp`,
  `nros-rmw-zenoh-staticlib` (plus the `n_board_agnostic_run_plan` fixture's
  `posix`) no longer list `alloc`/`std`. They still select the malloc/panic
  provider — that half is correct and stays.
- **W8.e (done)** — 11 capabilities now `compile_error!` naming the feature to
  add: `param-services`, `lifecycle-services` (nros-c + nros-node), `bridge`,
  `cffi`, `config`, `metadata-mode`, `signal-fd-wake`, `unix-mock`, and the six
  example `rmw-cyclonedds` rows (each example gained an `alloc = ["nros/alloc"]`
  passthrough, so the build is `--features rmw-cyclonedds,alloc`).

### What W8 uncovered — the `node_wake` predicate split

`executor/node_wake.rs` is `#![cfg(all(feature = "alloc", feature = "rmw-cffi"))]`,
but every consumer in `executor/spin.rs` was gated `all(std, rmw-cffi)`. Two
different predicates for the same thing, agreeing only because `std` implied
`alloc`. Removing the edge produced:

```
error[E0433]: cannot find `node_wake` in `super`
   --> packages/core/nros-node/src/executor/spin.rs:623
```

**The first fix was wrong** and is recorded because the failure mode is
instructive: a `compile_error!` making `nros-node`'s `std` require `alloc`. It
compiled the workspace, and then fired on the exact combinations CI builds —
`nros --features std,rmw-cffi,ros-humble`, `nros-c --features
std,rmw-cffi,platform-posix,ros-humble` — i.e. it moved the cost onto every
hosted user and contradicted the hosted shape in "Target usage" below. Reverted.

The landed fix gates the five field/initializer sites on `alloc` and gives the
hot spin path ONE shape across the axis via an accessor pair:

```rust
#[cfg(all(feature = "std", feature = "rmw-cffi", feature = "alloc"))]
fn node_wake_ref(&self) -> Option<&std::sync::Arc<super::node_wake::NodeWake>> {
    self.node_wake.as_ref()
}
#[cfg(all(feature = "std", feature = "rmw-cffi", not(feature = "alloc")))]
fn node_wake_ref(&self) -> Option<&NeverWake> { None }   // NeverWake is uninhabited
```

so without `alloc` the wake-primitive branch is statically dead rather than
restructured — no behaviour change on the path that has a heap, which is every
shipping configuration. A second site fell out of the same audit:
`read_rmw_selector_env`, gated `all(std, rmw-cffi)`, returned
`alloc::vec::Vec<u8>`; it is `std::vec::Vec<u8>` now.

Verified after W8:

```
nros-node   std,rmw-cffi / std,rmw-cffi,alloc / alloc,rmw-cffi / std      ok
nros        std,rmw-cffi,ros-humble | ros-iron | rmw-cffi (bare)          ok
nros-c      std,rmw-cffi,platform-posix,ros-humble                        ok
nros-c      platform-threadx  (thumbv7em, NO alloc)                       ok
nros-c      platform-threadx,alloc (thumbv7em)                            ok
nros-cpp    platform-zephyr,alloc (thumbv7em)                             ok
nros-bridge std,alloc | alloc                                             ok
implicit alloc/std enables across packages/ + examples/                   0
```

## Target usage — what a consumer's project looks like

The point of W1/W3/W8 is that these four shapes are the WHOLE vocabulary, and
that reading a manifest tells you whether the image has a heap.

### Rust — hosted (native / Linux board)

```toml
[dependencies]
nros = { version = "*", default-features = false, features = ["std", "rmw-cffi"] }
```

Unchanged. 93 of the 99 in-tree leaves already look like this.

### Rust — embedded WITH a heap (Zephyr, FreeRTOS, NuttX)

```toml
[dependencies]
# `alloc` is named. Nothing else can turn it on.
nros = { version = "*", default-features = false, features = ["alloc", "rmw-cffi"] }
# The platform selects the malloc/panic PROVIDER. It does not decide whether
# this image may allocate — the line above did that.
nros-platform = { version = "*", default-features = false, features = ["platform-zephyr"] }
```

Unchanged — `examples/zephyr/rust/talker` is already exactly this.

### Rust — embedded with NO heap on the nros surface

```toml
[dependencies]
nros = { version = "*", default-features = false, features = ["rmw-cffi", "ros-humble"] }
nros-platform = { version = "*", default-features = false,
                  features = ["platform-threadx", "global-allocator", "critical-section"] }
```

`examples/qemu-riscv64-threadx/rust/talker`, verbatim and unchanged. Note what
this says: the platform installs a `#[global_allocator]` (the RTOS heap exists
and C code uses it) while `nros` itself is compiled with no `alloc` — those are
different questions, and after W8 they stay different. **This is the shape the
whole contract exists to make readable.**

What DOES change here is the RMW row:

```toml
# before — selecting a backend silently added a heap to the nros surface
rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys", "nros/alloc"]
# after — the backend row selects a backend; if it needs the heap, the user adds it
rmw-cyclonedds = ["dep:nros-rmw-cyclonedds-sys"]
```

with `nros-rmw-cyclonedds` carrying

```rust
#[cfg(not(feature = "alloc"))]
compile_error!("the Cyclone DDS backend allocates: add \"alloc\" to your `nros` features");
```

### C/C++ — already correct, and this is the model

The CMake path does not have this defect. `nros_feature_set()`
(`cmake/NanoRosFeatureSet.cmake:99-135`) emits the axis EXPLICITLY, per
platform, next to the platform feature:

```cmake
posix             -> std   platform-posix
nuttx             -> std   platform-nuttx
threadx_linux     -> std   platform-threadx
freertos/esp_idf  -> alloc panic-halt platform-freertos
threadx_riscv64   -> alloc panic-halt platform-threadx
<unknown cross>   -> alloc panic-halt platform-<X>
```

A C/C++ consumer writes only intent, and the axis is derived once, in one
readable table:

```cmake
find_package(nano_ros REQUIRED)
set(NANO_ROS_FEATURES "param_services;lifecycle")   # capabilities, image-level
nano_ros_add_node(talker ...)
```

`panic-halt` beside `alloc` is the malloc/panic unification already working:
one provider, chosen by the platform, named in the build. W8 makes the Rust
manifests agree with this table instead of duplicating half of it implicitly.

**Consequence: W8.d is a no-op for every C/C++ consumer** — CMake already passes
`std`/`alloc` itself, so deleting `"alloc"` from the `platform-*` feature bodies
changes nothing on that path.

### Blast radius — measured, not estimated

```
in-tree leaves depending on nros / nros-c / nros-cpp   : 99
  already name `alloc` or `std` at the dep-site        : 93
  rely on an implicit enable                           :  6
      examples/qemu-riscv64-threadx/rust/{talker,listener,
        service-{client,server},action-{client,server}}
```

All six are the `rmw-cyclonedds = [..., "nros/alloc"]` row above.

**That count is right for the question it asked and WRONG for the migration.**
It counted leaves naming `alloc` *or* `std`. But once `std` no longer implies
`alloc`, naming `std` is not enough for any consumer that touches an
alloc-gated API — and on the hosted side that is nearly all of them. The real
figure:

```
dep-sites naming `std` but NOT `alloc`  : 77
```

`nros-board-linux` is the first one the build hit: it deps
`nros = { features = ["std", "rmw-cffi"] }` and calls
`Executor::from_session_in`, which is `alloc`-gated —
`error[E0599]: no function or associated item named 'from_session_in'`.

This is the open decision recorded under "W8 — the hosted question" below.
Earlier drafts said "~30" (a guess) and then "6" (a correct answer to the wrong
question); neither is the migration cost.

## Sequencing

W1 → W2 → W3 → W4 is one thread (manifests + gate); W5 → W6 → W7 is the other
(dependency weight). They touch different manifests and can run in parallel.
W3 and W7 are both breaking for out-of-tree consumers — land them in the same
release and write one note, not two.

## Measurement

Re-run both commands after each work item and record the numbers in the item.
The two acceptance numbers are the **57-crate firmware count** and the **count
of crates compiled under more than one feature set** in a workspace check.
