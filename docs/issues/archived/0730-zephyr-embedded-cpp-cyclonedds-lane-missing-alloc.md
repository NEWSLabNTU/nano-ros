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

## Fixed 2026-08-20 — `alloc` named ONCE for every backend, not patched per branch

The issue's fix shape, with the audit it asked for — and the audit changed the
answer.

### Reproduced without the board

The report needed a downstream aarch64-none board crate. It is reachable in
seconds from this tree:

```
cargo check -p nros-cpp --no-default-features \
    --features "rmw-cffi,platform-zephyr,ros-humble,panic-platform" \
    --target aarch64-unknown-none

error[E0599]: no variant, associated function, or constant named
  `BackendDynamic` found for enum `nros_node::TransportError`
```

Worth recording, because the issue's repro implies hardware and this is a
15-second check on any host with the target installed.

### The audit found a SECOND branch

Measured per branch on `aarch64-unknown-none`:

| nros-cpp features | result |
| --- | --- |
| `rmw-cffi` (cyclonedds) | **E0599** — the reported one |
| `rmw-cffi,rmw-xrce-cffi` | **E0599** — not reported |
| `rmw-zenoh-cffi` | builds |

zenoh survives only because `rmw-zenoh-cffi` pulls `dep:nros-rmw-zenoh`, which
drags `alloc` in transitively. So two of three branches were broken, and the
third was right by accident.

The C lane is NOT affected: all three `_nros_c_for_cpp_features` coordinates
compile without `alloc`. That sibling was audited because the issue said to, and
it is clean — recorded so nobody re-audits it.

### Why one append rather than two fixes

`string(APPEND _nros_cpp_features ",alloc")` runs once after the if/elseif
chain. Patching the two failing branches would leave the shape that produced the
bug: a capability INHERITED from whichever backend happens to depend on it. With
one append, a backend added later cannot reintroduce the gap, and a dependency
change under zenoh cannot silently remove it. phase-361 W8.d puts the spelling
at the dep-site, and for a Zephyr image this module IS that site.

The un-gated arm stays un-gated. Its comment already explains why gating is
wrong (the implication runs one way, and cargo unifies features across the
graph), and it PREDICTED this exact failure: "if a genuinely alloc-free build is
ever made to work, this line fails to compile with 'no variant named
BackendDynamic' — a loud, local error pointing straight here". It did, and it
did point straight here.

### Verified

* All three branches build on `aarch64-unknown-none` with `alloc`.
* `just zephyr build-cpp` completes with **0 errors** — the module change
  reaches the real cargo step, not just a hand-run `cargo check`.

