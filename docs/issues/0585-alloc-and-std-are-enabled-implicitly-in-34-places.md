---
id: 585
title: "`alloc` and `std` are turned on implicitly in 34 places — picking a PLATFORM enables the heap"
status: open
type: bug
area: build
related: [issue-0581, issue-0582, issue-0464, rfc-0033, rfc-0034, phase-360]
---

## The rule this issue exists to enforce

On embedded, `std` and `alloc` are enabled **only when the end user says so**,
per package. If a nano-ros crate needs the heap or the standard library, the
user must be made to spell it — the crate must never turn it on behind their
back. Orthogonally: the `malloc` and `panic` implementations must be **unified
per platform** — exactly one provider per image, selected by the platform, not
assembled by accident from whichever features happened to be on.

Those two are in tension today, and the workspace resolves the tension the wrong
way: it makes the *platform* selection enable the *heap*.

## The 34 sites

Every feature that is not `std`/`alloc`/`default` and whose body turns one of
them on:

### A. Selecting a platform enables the heap — 13 sites

```toml
# packages/api/nros-c/Cargo.toml
platform-zephyr   = ["alloc", "global-allocator", "critical-section", "nros-platform/platform-zephyr", …]
platform-freertos = ["alloc", "global-allocator", …]
platform-nuttx    = ["alloc", …]
platform-threadx  = ["alloc", "global-allocator", …]

# packages/api/nros-cpp/Cargo.toml — same four, each ["alloc", "nros-c/platform-X"]
# packages/rmw/zenoh/nros-rmw-zenoh-staticlib/Cargo.toml
platform-zephyr-baremetal = ["alloc"]
platform-freertos         = ["alloc"]
platform-nuttx            = ["alloc"]
platform-threadx          = ["alloc"]
platform-threadx-std      = ["std"]
platform-posix            = ["nros-rmw-zenoh/std"]
```

`platform-<rtos>` is a statement about the hardware and the kernel. `alloc` is a
statement about whether this image is allowed to allocate. Conflating them means
a bare-metal ThreadX image gets a heap because it said which RTOS it runs on.
nros-cpp's manifest says the quiet part out loud: *"Embedded platforms imply
`alloc` so the C++ FFI layer's `extern crate alloc` compiles."* — the
implication exists to make a nano-ros internal compile, paid for by the user's
image.

### B. Capabilities enable what they need — 9 sites

```toml
nros-c/param-services      = ["alloc"]      nros-node/param-services     = ["alloc"]
nros-c/lifecycle-services  = ["alloc"]      nros-node/lifecycle-services = ["alloc"]
nros-cpp/bridge            = ["alloc"]      nros-bridge/cffi             = ["alloc"]
nros/metadata-mode         = ["std"]        nros-bridge/config           = ["std"]
nros-node/signal-fd-wake   = ["std"]        nvidia-ivc/unix-mock         = ["std"]
```

These genuinely require what they enable — AGENTS.md already records
*"`param_services`/`lifecycle` are alloc-gated"*. The defect is enable-vs-require:
the user asks for parameter services and silently receives a heap.

### C. `global-allocator = ["alloc"]` — 3 sites, and it is gratuitous

`nros-c`, `nros-cpp`, `nros-platform` all declare it. But none of the three
allocator modules touches the `alloc` crate:

```rust
// nros-c/src/lib.rs:116          // nros-platform/src/lib.rs:133
mod platform_alloc {              mod global_allocator {
    use core::alloc::{GlobalAlloc, Layout};   use core::{alloc::{GlobalAlloc, Layout}, ffi::c_void};
```

`core::alloc::GlobalAlloc` is in `core`. Installing a `#[global_allocator]` does
not require the `alloc` feature; `extern crate alloc` in those crates is already
gated on `feature = "alloc"` separately (`nros-c/src/lib.rs:46`). These three
implications can simply be deleted.

### D. Backend selection enables the heap — 6 example sites

`examples/qemu-riscv64-threadx/rust/*/Cargo.toml`: `rmw-cyclonedds = ["nros/alloc"]`.

### E. `std` implies `alloc` — NOT a defect, and this entry was wrong

Counted here originally: `nros`, `nros-c`, `nros-cpp`, `nros-node`, `nros-log`,
`nros-rmw-zenoh` declared `std = ["alloc", …]`, and it was read as implicit heap
enablement of the same kind as A–D. **It is not.** A `std` build links an
allocator by definition, so `std` ⇒ heap is a fact about the standard library,
not a favour one crate does another; the only question is where it is written
down.

phase-360 W2 removed it, and the removal was reverted (2026-08-15) once the cost
was measured: the code needs the implication either way, so deleting it from six
manifests re-created it at 123 use sites as
`cfg(any(feature = "alloc", feature = "std"))` — 88 more std-mentioning branches
for phase-359 to unwind, invisible to that campaign's ratchet. The edge is now
declared in **twelve** crates (the six above plus `nros-core`, `nros-serdes`,
`nros-params`, `nros-rmw`, `nros-rmw-cffi`, `nros-bridge`, which had been
carrying it in `cfg` without declaring it), and every heap gate in the workspace
is `cfg(feature = "alloc")`.

What A–D have and E does not: A–D let a feature that is not about the heap turn
the heap on — a BACKEND, a PLATFORM, a capability. Those 34 sites are still 0.
The rule is "no feature other than `std` enables `alloc`", not "nothing does".

Issue 0581 is fixed by this edge: `nros-core`'s `std` forwards `alloc`, which
forwards `nros-serdes/alloc`, so `heap::{Vec, String}` and their serializer
impls arrive together. Verified at `nros-core --features std` alone.

## The malloc / panic half

Providers of the two lang items, in tracked source:

| lang item | crate | gate |
| --- | --- | --- |
| `#[global_allocator]` | `nros-c` | `all(global-allocator, not(std))` |
| `#[global_allocator]` | `nros-platform` | `all(global-allocator, not(std))` |
| `#[global_allocator]` | `nros-board-esp32-qemu` | `esp-alloc` |
| `#[global_allocator]` | `nros-platform-mps2-an385` | `memory.rs` |
| `#[panic_handler]` | `nros-c` | `all(global-allocator, not(std), not(panic-halt))` |
| `#[panic_handler]` | `nros-board-threadx-qemu-riscv64` | board-owned (issue #45) |
| `#[panic_handler]` | `nros-board-mps2-an385-freertos` | board-owned (issue #45) |
| `#[panic_handler]` | `panic-halt` / `panic-semihosting` | `use … as _` |

Two problems.

**1. `nros-c` and `nros-platform` define `#[global_allocator]` under IDENTICAL
gates.** Nothing structural keeps them apart — only a hand-written rule in
nros-cpp's manifest: *"must NOT enable nros-cpp's OWN copies — two of any would
conflict in the one crate graph."* The separation is achieved by
`nros-c/platform-zephyr` forwarding `nros-platform/platform-zephyr` but
deliberately NOT `nros-platform/global-allocator`. A consumer who enables
`nros-platform/global-allocator` themselves gets two.
*Not reproduced here:* `cargo build -p nros-c --features
platform-threadx,nros-platform/global-allocator --target thumbv7em-none-eabihf`
succeeds — both features resolve on (confirmed via `cargo tree --format
"{p} {f}"`), but duplicate-`#[global_allocator]` is a final-link diagnostic and
an rlib/staticlib build does not reach it.

**2. `#[panic_handler]` is gated on the ALLOCATOR feature.** In `nros-c` there
is no way to ask for a panic handler without asking for a global allocator —
"I need panic" is spelled `global-allocator`. This is not theoretical: with
phase-360 W3's `default = []`, a plain host build of `nros-c` has neither `std`
nor `global-allocator` and dies with

```
error: `#[panic_handler]` function required, but not found
error: unwinding panics are not supported without std
```

which is what `scripts/build/compile-check-fixtures.sh:490` hit — and it hit it
inside a `|| echo` best-effort branch, so the config headers silently vanished
and every snippet needing them skipped. Issue 0464's shape again.

## Direction

The two requirements are compatible once the axes are separated:

1. **`platform-<rtos>` selects the PROVIDER of `malloc` + `panic`, and nothing
   else.** That is a platform fact and belongs in the platform feature — this is
   the "unified per platform" requirement, and keeping it there is correct.
2. **`platform-<rtos>` does NOT enable `alloc`.** Whether the image may
   allocate is the user's call, spelled `alloc` at their own dep-site.
3. **Split the two lang items apart from the heap feature.** `global-allocator`
   should install the allocator; a separate `panic-handler` (or the platform
   feature directly) should install the panic handler. Delete
   `global-allocator = ["alloc"]` — it is unnecessary (§C).
4. **Requirements become `compile_error!`, not silent enables.** A capability
   that needs the heap says so:
   ```rust
   #[cfg(all(feature = "param-services", not(feature = "alloc")))]
   compile_error!("`param-services` needs a heap: add `alloc` to this crate's features");
   ```
   The user learns what to type instead of silently receiving it.
5. **One `#[global_allocator]` owner.** Decide whether it is `nros-c` or
   `nros-platform` and delete the other, or gate them mutually exclusively so
   the conflict is a `compile_error!` naming both, not a link diagnostic.

Blast radius, measured rather than guessed: of the **99** in-tree leaves that
depend on `nros`/`nros-c`/`nros-cpp`, **93 already name `alloc` or `std` at the
dep-site**. Only **6** rely on an implicit enable — the
`examples/qemu-riscv64-threadx/rust/*` set, all via the §D row. And the C/C++
path is unaffected entirely: `nros_feature_set()`
(`cmake/NanoRosFeatureSet.cmake:99-135`) already emits `std`/`alloc` +
`panic-halt` explicitly per platform, so step 2 is a no-op there. This is much
smaller than it looks from the 34-site count.

## Gate

phase-360 W4's `check-feature-contract` gains: **no feature body may list
`alloc` or `std`, or forward `<dep>/alloc` / `<dep>/std`, unless the feature IS
`alloc`/`std`.** That is a mechanical check over `[features]` and it is exactly
the 34 rows above.
