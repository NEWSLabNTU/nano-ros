---
id: 843
title: "`nros::node_runtime` is gated on `rmw-cffi`, not on `alloc`, so every
  cffi image needs a global allocator and the `heap-free` tier is unreachable"
status: resolved
type: bug
area: core, platform
related: [phase-391, issue-0816, issue-0832]
---

## Symptom

A `#![no_std]` binary that links `nros` with `alloc` OFF fails to link:

```
error: no global memory allocator found but one is required; link to std or
add `#[global_allocator]` to a static item that implements the GlobalAlloc trait
```

`cargo check` on the library does NOT show this — a lib check never requires a
global allocator, only a link does. That distinction is why this was missed:
phase-391 W1's feasibility note recorded "the core builds with no `alloc`" on
the strength of `cargo check -p nros --no-default-features`, which is true and
insufficient.

## Cause

`packages/api/nros/src/lib.rs:160`:

```rust
#[cfg(feature = "rmw-cffi")]
pub mod node_runtime;
```

`node_runtime.rs` uses `alloc::` / `Box` / `Vec`, and it is compiled whenever
`rmw-cffi` is enabled — regardless of the `alloc` feature. Every backend reaches
the runtime through the cffi vtable, so in practice this is every image.

The other alloc-using modules in the umbrella are gated on the feature that
matches their content: `env` and `init` behind `env`, `metadata_mode` behind
`metadata-mode`. `node_runtime` is the one gated on a TRANSPORT feature while
carrying an ALLOCATION dependency.

## The bisect, and how to re-run it correctly

Each step keeps a LIVE reference to the crate under test. This matters more than
it looks: a first attempt replaced the reference with `size_of::<u32>()`, dead-
code elimination stripped the dependency, and every row reported "links
heap-free" while measuring nothing. If you re-run this, keep the reference.

`thumbv7m-none-eabi`, `opt-level="z"` + LTO, `panic = "abort"`:

| probe | result |
| --- | --- |
| `nros-core` alone | links heap-free |
| `nros-rmw-cffi` + `nros-rmw`, ref `ret_from_error(...)` | links heap-free |
| `nros-node` + feature `rmw-cffi`, ref `RmwSubscriber` | links heap-free |
| `nros` + `rmw-cffi`, ref `nros::internals::RmwSession` | **NEEDS ALLOCATOR** |
| `nros` + `rmw-cffi`, ref `u8` only (`extern crate nros;`) | **NEEDS ALLOCATOR** |

The last row is what settles it. The requirement survives when NOTHING from
`nros` is referenced, so it is the crate's compiled surface and not a
monomorphisation of `RmwSession`. And since neither sub-crate reproduces it, the
alloc dependency is in the umbrella itself.

Feature bisect on `nros`, reference held live throughout:

| features | result |
| --- | --- |
| `[]` / `["ros-humble"]` | inconclusive — `RmwSession` does not exist (cffi-gated) |
| `["rmw-cffi"]` | **NEEDS ALLOCATOR** |
| `["rmw-cffi", "ros-humble"]` | **NEEDS ALLOCATOR** |

## Why it blocks phase 391

W1's gate (`scripts/check-no-alloc-image.py`) reports 0 of 4 book no-alloc
claims as backed by a built image, and the stated reason was that no image is
built no-alloc. This is the mechanism underneath that: **no image CAN be, on the
cffi path.** So W1's remaining half is not "write a fixture" — the fixture is
unbuildable until this is resolved, and W4 cannot stand up a `heap-free` lane
either.

Measured for scale: `qemu-bsp-talker` (mps2-an385) currently carries 8
allocation symbols at tier `heap-free` — one `zpico_alloc` `free` and seven
`__rustc::__rust_*` shim symbols. The shim symbols are the visible consequence
of this issue.

## PARTIAL FIX LANDED (2026-08-27) — and WHICH modules need alloc, measured

**Only `node_runtime` needs it.** `internals`, `Executor`, `sizes`, `time` and
`node.rs` do NOT. The fix is therefore two gates, not twenty-eight:

```rust
#[cfg(all(feature = "rmw-cffi", feature = "alloc"))]
pub mod node_runtime;
#[cfg(all(feature = "rmw-cffi", feature = "alloc"))]
pub use node_runtime::{ ... };
```

Why each one:

* **`node_runtime` — genuinely requires alloc.** 39 uses of
  `alloc::`/`Box`/`Vec`/`String` spread across its 1,828 lines (first at 55,
  last past 1,800). Not clustered, so it cannot be partially gated without
  restructuring the module.
* **`internals` — does NOT.** It is nothing but type aliases to
  `nros_rmw_cffi::Cffi*`, and `nros-rmw-cffi` gates its own
  `extern crate alloc` on its own `alloc` feature. The bisect above already
  showed that crate links heap-free.
* **`Executor` / `sizes` / `time` / `node.rs` — do NOT.** They come from
  `nros-node`, which the bisect showed links heap-free WITH `rmw-cffi` enabled.

A first attempt at this fix flipped EVERY `#[cfg(feature = "rmw-cffi")]` in the
crate (22 in `lib.rs`, 6 across `node.rs`/`time.rs`/`sizes.rs`) to include
`alloc`. It compiled and linked, and it was wrong: it gated away the whole
non-runtime API for no reason, which is precisely why the surviving heap-free
surface collapsed to compile-time constants. Narrowing it to the two sites that
earn it restores `internals`, `Executor`, `sizes` and `time` to a heap-free
build.

Verified:

| configuration | result |
| --- | --- |
| `rmw-cffi,ros-humble` (no alloc), `thumbv7m-none-eabi` | **compiles AND links** (was: "no global memory allocator found") |
| `rmw-cffi,ros-humble,alloc` | 0 errors |
| host default / host `+env,std` | 0 errors |
| `cargo check --workspace --all-targets` | unchanged from baseline (2 pre-existing errors in `nros-rmw-zenoh-staticlib`) |

## What is still NOT proved — do not quote the tier as reached

A `no_std` probe linking the fixed crate passes the W1 gate at tier
`heap-free`, and **that pass is vacuous**:

```
symbols read: 1
OK — no allocation symbol is present in ...
```

One symbol. Both probes tried so far reference only compile-time things —
`qos::DEFAULT`, `node_metadata::DEFAULT_MAX_METADATA_NODES`, and
`size_of::<internals::RmwSession>()` — and `size_of` pulls no code even for a
real type, so everything folds away. Nothing was exercised; nothing was proved.
This is the "gate that passes because nothing exercises it" failure the memory
campaign warns about, reproduced three times here before being recognised.

**The open question:** is there enough of `nros` reachable with `alloc` off to
build an image that does ROS work? What would settle it is a probe that CALLS
runtime functions rather than naming types, links, and reports `symbols read`
well above 1 with the gate still green. Until then this fix is a correct
decoupling — a transport feature no longer gates an allocation dependency — and
not a demonstration of the `heap-free` tier.

## The layering, and who actually decides to allocate (2026-08-27)

The gate this issue is about conflated two independent things, and naming the
layers makes the remaining work obvious.

All four backends reach the executor through ONE seam: `cyclonedds`, `uorb`,
`xrce` and `zenoh` each map to `rmw-<x>-cffi`
(`cmake/NanoRosRmwDispatch.cmake`). The RMW supplies a session; the Executor
implements polling and waking over it. So the transport's allocation choice is
the RMW's, and nano-ros's is its own — they meet at the vtable and nowhere else.

| layer | who decides to allocate | what it allocates for |
| --- | --- | --- |
| RMW backend | the backend | its own transport needs — zenoh-pico's 42 `z_malloc` sites, Cyclone's `ddsrt_malloc`, … |
| Executor ALGORITHM | nobody | polling/waking needs no heap; it links heap-free |
| Executor CONVENIENCES | **nano-ros** | leak-an-arena constructors; boxed event callbacks |
| `node_runtime` | **nano-ros** | `String`/`Vec`/`Arc` registries — its own bookkeeping |

Measured: `nros-node` carries 83 `#[cfg(feature = "alloc")]` sites under
`src/executor/` (38 `spin.rs`, 26 `handles.rs`, 9 `storage.rs`, 9 `tests.rs`,
1 `types.rs`), and what they gate is NOT the dispatch algorithm:

* **`spin.rs`** — `leak_default_backing()`, feeding `from_session`,
  `from_session_with`, `from_session_ptr`, `open_with_session`. The executor
  LEAKS a heap arena when the caller does not supply backing storage. A
  convenience constructor; the explicit-sizing path (`ExecutorSizing`,
  `open_in`) already exists and needs no heap.
* **`handles.rs`** — `on_liveliness_lost`, `on_offered_deadline_missed`,
  `EventReg`/`EventRegs`. Boxed closures, i.e. type-erased user callbacks. `fn`
  pointers or `&'static dyn` express the same thing without a heap.

**So every allocation standing between this tree and a useful `heap-free` tier
is nano-ros's own, not a vendor's** — arena-leaking, closure-boxing, and
name-owning. None of them needs a heap; each needs an explicit capacity or an
explicit lifetime. That is a much smaller and more tractable problem than
"nano-ros requires a heap", which is how this issue read when filed.

It also re-weights the directions below: the `heapless` port (direction 4) stops
looking speculative, because the same substitution — capacity instead of
allocation — answers all three sites, and `node_metadata` already solves the
identical registry shape that way with const-generic bounds
(`MAX_NODES` / `MAX_ENTITIES` / `MAX_CALLBACKS`).

**Not verified:** that `Arc<ComponentCell>` converts. Shared ownership is the
one case where "capacity instead of allocation" does not obviously apply, and it
decides whether the `node_runtime` port is mechanical or a design change.

## Directions, none chosen here

* Gate `node_runtime`'s alloc-dependent parts on `alloc` rather than the whole
  module on `rmw-cffi`, so a cffi image without `alloc` compiles the rest.
* Split the alloc-dependent surface into a separate module gated on
  `rmw-cffi` + `alloc`.
* Decide that the cffi path REQUIRES a heap and say so — then `heap-free`
  belongs only to non-cffi consumers, and the book's four claims need rewording
  rather than enforcing. This is a legitimate answer; it is just not the one the
  book currently makes. NOTE the layering section above weakens this option:
  the cffi seam itself allocates nothing, so "the cffi path requires a heap" is
  a statement about OUR layers above it, not about the transport.
* **Port the three allocating sites to explicit capacity** — `heapless::String`
  / `heapless::Vec` for `node_runtime`'s registries, the existing
  `ExecutorSizing` path instead of `leak_default_backing`, and `fn` pointers or
  `&'static dyn` instead of boxed event callbacks. This is the only direction
  that makes the tier USEFUL rather than merely reachable. Gated on the
  `Arc<ComponentCell>` question above.

Note this is independent of [issue 0832](0832-platform-alloc-funnel-unreferenced-on-cyclone-and-xrce.md):
0832 is about allocation bypassing the funnel on two backends, this is about
allocation being unavoidable at all on the cffi path.

## Progress (2026-08-28, phase-391 W5-endgame)

The `Arc<ComponentCell>` question resolved itself in issue 0857's fix: cells
are PLACED in per-class static storage (slot + cell + ctx-slab pairs), the
trampoline contexts moved into the cell, the sink's node table went heapless,
and `node_runtime`'s gate narrowed from `all(rmw-cffi, alloc)` to `rmw-cffi` —
the dynamic `ExecutorNodeRuntime` half is item-gated on `alloc` inside. The
macro install path (`nros::node!` → `install_node_typed_*_in`) now performs
zero heap allocation. What keeps this issue open: the W1 gate proving a real
linked image heap-free (`check-no-alloc-image --tier heap-free`, symbols read
well above 1) — the fixture + gate work tracked as the campaign's next task.
