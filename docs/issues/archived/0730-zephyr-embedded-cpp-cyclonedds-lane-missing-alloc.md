---
id: 730
title: "Zephyr embedded C++ cyclonedds lane omits `alloc`, so nros-cpp's un-gated `TransportError::BackendDynamic` arm fails E0599 on aarch64-none"
status: resolved
type: bug
area: zephyr
related: [issue-0591, issue-0586]
---

# 0730 — embedded C++ cyclonedds lane can't compile nros-cpp (missing `alloc`)

`zephyr/CMakeLists.txt` composes the nros-cpp cargo features for the
cyclonedds RMW as (line ~387):

```cmake
set(_nros_cpp_features "rmw-cffi,platform-zephyr,ros-humble")
```

plus the panic suffix — no `alloc`. Meanwhile nros-cpp's error mapper
(`packages/api/nros-cpp/src/lib.rs`, ~793) references the alloc-gated variant
un-gated (phase-361 W3 / issue 0591 made the un-gating deliberate):

```
T::BackendDynamic(_) => NROS_CPP_RET_TRANSPORT_ERROR,
```

`TransportError::BackendDynamic` is `#[cfg(feature = "alloc")]` in
`nros-rmw/src/traits.rs`, so an embedded (non-native_sim) Zephyr C++
cyclonedds build dies:

```
error[E0599]: no variant, associated function, or constant named
`BackendDynamic` found for enum `nros_node::TransportError`
   --> packages/api/nros-cpp/src/lib.rs:793:12
```

The lib.rs comment argues "no buildable configuration lacks the variant"
because a build without alloc "fails to link at all (no global memory
allocator)". That holds for `--no-default-features --features rmw-cffi`
alone, but NOT for `rmw-cffi,platform-zephyr,...`: `platform-zephyr` →
`nros-c/platform-zephyr` → `nros-platform/global-allocator` provides the
allocator, so the image links fine and only the FEATURE `alloc` is absent.
The embedded cyclonedds C++ consumer is the counterexample the comment says
would "find out loudly" — and it did, four crates downstream of the cause.

Why in-tree coverage misses it: the zenoh lane goes through
`rmw-zenoh-cffi`, the xrce lane through `rmw-xrce-cffi`, and native_sim /
native_posix appends `,std` (implies `alloc`); the cyclonedds branch is the
only one left on bare `rmw-cffi`, and cyclonedds fixtures run on native_sim.
The failing coordinate is (zephyr-embedded × C++ × cyclonedds) — exactly a
pairwise-class gap (tier-2 sees each axis value, not the pairing).

## Repro

Downstream board-crate consumer (autoware-safety-island, board
`fvp-aemv8r-smp`, `aarch64-unknown-none`), `CONFIG_NROS_CPP_API=y` +
`CONFIG_NROS_RMW_CYCLONEDDS=y`, then build — nros-cpp cargo step fails
E0599 as above. Observed at `eace28852`; lane and lib.rs unchanged at
`2a891e5aa` (2026-08-20).

## Fix shape

Append `alloc` to the cyclonedds C++ branch (and audit the sibling
`_nros_c_for_cpp_features` / C-lane feature strings for the same coordinate)
— per the phase-361 W8.d contract the END USER spells `alloc`, and here the
Zephyr module IS the dep-site composing the consumer's feature set. Re-gating
the match arm is the wrong direction per issue 0586 (mappers stay exhaustive,
no `_` arm), and the arm's own comment says the correct outcome of hitting
this error is fixing the configuration, not the mapper.

## Downstream workaround (until fixed)

autoware-safety-island carries an idempotent sed patch
(`scripts/patches/nros-cpp-embedded-alloc-patch.sh`, applied by its build.sh)
that appends `,alloc` to the cyclonedds `_nros_cpp_features` line; it
self-retires when the pattern disappears upstream.

## Resolution (2026-08-20)

`alloc` appended to the cyclone `_nros_cpp_features` branch in
`zephyr/CMakeLists.txt`, with the causal chain in a comment at the site: the
mapper stays exhaustive (0586), the variant is alloc-gated because it carries
a heap diagnostic, and `platform-zephyr` provides the allocator WITHOUT the
feature — so this was the one bare-`rmw-cffi` composition (zenoh gets `alloc`
via nros-rmw-zenoh's pinned `nros-rmw-cffi/alloc`; native_sim via `,std`).
E0599 reproduced on `aarch64-unknown-none` before, compiles clean after; the
C lanes were audited (`nros-c` compiles the same coordinate without `alloc` —
`support.rs` maps the variants differently) and left unchanged.

The pairwise-class gap is now gated at the layer it failed: `just check-cpp`
compiles nros-cpp with the cyclone branch's feature string — READ from
`zephyr/CMakeLists.txt`, not a second spelling — on `aarch64-unknown-none`
(loud SKIP when the target is absent). Verified red without `alloc`, green
with.

The autoware-safety-island sed patch
(`scripts/patches/nros-cpp-embedded-alloc-patch.sh`) self-retires.
