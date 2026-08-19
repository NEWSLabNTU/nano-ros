# Phase 369 — ThreadX-RV64 gets ONE entry point and ONE build path

**Status (2026-08-19). W1 IN PROGRESS.** Direction chosen: the **Zephyr shape** —
both RMWs build through CMake, the lib-side entry is the only entry, and
`src/main.rs` goes away.

**Owns:** [issue 0666](../issues/0666-threadx-zenoh-and-cyclonedds-build-paths-diverge.md),
[issue 0668](../issues/0668-threadx-rv64-example-shape-differs-from-every-other-standalone.md).
**Unblocked by:** 0674 → 0678 → 0692, now closed end to end; 0692's fix removed
the panic collision that made this urgent, so this is consistency work with a
measured cost, not a firefight.

## Why

Six `examples/qemu-riscv64-threadx/rust/*` leaves are the only standalone
examples in the tree that own TWO entry points and TWO build paths:

```
src/main.rs      nros::main!()                        -> the bin  (cargo, zenoh)
src/app_main.rs  <board>::app_main!(crate::register)   -> the .a   (CMake, cyclone)
                 nros::panic_to_platform!()            <- the extra line
```

The RMW picks which path runs. Issue 0666 records the cost in one sentence, and
0688 paid it: **a bespoke build path silently misses machinery the shared one
applies, and the failure surfaces four crates away from its cause** — nothing
gave `nros-c` a panic policy, because the seam is not `nano_ros_entry()`.

## Why the Zephyr shape and not cargo-only

0668 says either single-entry shape is fine. The cargo-only shape needs
CycloneDDS reachable from a cargo build on a bare-metal riscv64 target.
`packages/rmw/cyclonedds/cyclonedds-sys/build.rs` does drive CMake and produce a
static `libddsc.a`, so it is not hypothetical — but it is wired only into
`nros-board-linux`, and making it cross-compile for bare metal is a project, not
a cleanup.

The Zephyr shape needs no new cross-compilation capability: the CMake path
already exists and already builds five of the six leaves' RMW today.

**Cost, stated up front:** the pure cargo build is the faster inner loop and the
one contributors reach for. This trades it for one shape. If that loop matters
more than the uniformity, the decision should be revisited BEFORE W3 — after
that the `[[bin]]` is gone.

## Waves

**W1 — make the seam RMW-neutral.** `nros_threadx_rv64_rust_cyclone_app()`
hardcodes `FEATURES rmw-cyclonedds alloc`. Take the feature from the RMW
selection instead and rename to `nros_threadx_rv64_rust_app()`. The leaf
CMakeLists already parameterise `NROS_RMW` as a cache variable, so this is the
only thing standing between them and a zenoh configure.
*Acceptance:* one leaf configures and links with `-DNROS_RMW=zenoh` through
CMake, producing the same ELF the cargo path produces today.

**W2 — move the six zenoh rows onto the CMake path.** `examples/fixtures.toml`'s
six `rmw=zenoh` rows for `qemu-riscv64-threadx/rust/*` change from cargo rows to
cmake rows; `just threadx_riscv64 build-examples` stops calling
`fixtures-build.sh threadx-riscv64 rust` for them.
*Acceptance:* `just threadx_riscv64 build-fixtures` builds all twelve rows
(6 zenoh + 6 cyclone) through CMake, from a WIPED tree — see the note below.

**W3 — delete the second entry.** Remove `src/main.rs` and the `[[bin]]` section
from all six `Cargo.toml`. `crate-type` stays `["staticlib", "rlib"]`.
*Acceptance:* `git grep -l "nros::main!" examples/qemu-riscv64-threadx` is empty,
and the leaves still build.

**W4 — follow the binaries.** The test-side resolvers move from the cargo target
dir to the CMake build dir (`nros_tests::fixtures::binaries::threadx_riscv64`).
*Acceptance:* the threadx-riscv64 runtime tests resolve and run, or skip for a
stated reason that is not "binary not found".

**W5 — drop the hand-written panic line.** With one entry, `app_main.rs`'s
`nros::panic_to_platform!()` is what the entry macro emits everywhere else.
0668 predicts it becomes redundant; VERIFY rather than assume — 0692 showed the
two handlers coexist by symbol locality, so removing one is a link-level change,
not a formality.
*Acceptance:* exactly one global `rust_begin_unwind` in the final image, checked
with `nm`, as 0692 did.

## Do not

* **Do not test incrementally on this board.** `CMAKE_C_COMPILER` is sticky;
  issue 0678 spent a day looking unfixed because 22 stale `build-*` dirs kept
  resolving Debian's compiler. Wipe the build dirs before believing any result
  here, green or red.
* **Do not conflate the allocator.** `#[global_allocator]` has the same shape
  problem in the same files; 0616/0594 own it. This should not make it worse and
  should not try to fix it.
