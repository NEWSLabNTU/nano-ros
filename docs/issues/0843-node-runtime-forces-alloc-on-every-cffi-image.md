---
id: 843
title: "`nros::node_runtime` is gated on `rmw-cffi`, not on `alloc`, so every
  cffi image needs a global allocator and the `heap-free` tier is unreachable"
status: open
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

## Directions, none chosen here

* Gate `node_runtime`'s alloc-dependent parts on `alloc` rather than the whole
  module on `rmw-cffi`, so a cffi image without `alloc` compiles the rest.
* Split the alloc-dependent surface into a separate module gated on
  `rmw-cffi` + `alloc`.
* Decide that the cffi path REQUIRES a heap and say so — then `heap-free`
  belongs only to non-cffi consumers, and the book's four claims need rewording
  rather than enforcing. This is a legitimate answer; it is just not the one the
  book currently makes.

Note this is independent of [issue 0832](0832-platform-alloc-funnel-unreferenced-on-cyclone-and-xrce.md):
0832 is about allocation bypassing the funnel on two backends, this is about
allocation being unavoidable at all on the cffi path.
