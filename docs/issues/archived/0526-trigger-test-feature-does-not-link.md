---
id: 526
title: "The `trigger-test` feature does not link, so four test binaries — including the issue-0317 wake-latency gate — never run"
status: resolved
type: bug
area: testing
related: [issue-0317, issue-0488, issue-0350]
---

## Symptom

```
$ cargo build -p nros-tests --features trigger-test --test trigger_conditions
rust-lld: error: undefined symbol: nros_platform_wake_storage_size
  >>> referenced by node_wake.rs:74 (packages/core/nros-node/src/executor/node_wake.rs:74)
  >>>               in archive .../libnros_node-*.rlib
error: could not compile `nros-tests` (test "trigger_conditions")
```

Six undefined-symbol references in one link. Every test file gated
`#![cfg(feature = "trigger-test")]` is therefore uncompilable, so
`cargo nextest list --features trigger-test` fails outright and the default
build lists **zero** tests from those files.

## Why it matters more than a broken feature usually would

`tests/wake_latency_cortex_m3.rs` is one of them. That file IS the CI gate issue
0317 asked for — "this lane guards the BUILD of both images" — and it has been
unrunnable, not failing. A gate that cannot be compiled reports nothing, and
nothing is what everybody has been reading.

This is the issue-0350 shape (a lane failing wholesale while the tier people run
locally reported skips), one level earlier: not a skip, an absent target.

## Cause, as far as the link message takes it

`nros-node` with `wake-latency-probe` calls the platform ABI
(`nros_platform_wake_storage_size` and friends). On the HOST those symbols come
from a C port that has to be linked in — the same requirement the metadata probe
solves by depping `nros-platform-cffi` with `features = ["posix-c-port"]`
(issue 0288 layer 5, quoted in `metadata_build.rs`). The `trigger-test` feature
set pulls `nros-node` without a provider for them.

Not investigated further here: this was found while fixing issue 0488's residue
1, and the fix belongs to whoever owns the feature's dependency set rather than
to a build-layout pass.

## What was fixed alongside, and what was not

Issue 0488 residue 1 moved the wake-latency pair onto a `[[fixture]]` row and its
resolver onto that row. While doing so, a SECOND defect in the same test surfaced
and was fixed: `bench_image()` spelled `target/thumbv7m-none-eabi/release/`,
while the build writes the FreeRTOS carve-out profile (`nros-minsizerel`). So
even with the feature linking, the image would not have been found and the test
would have taken its `[SKIPPED]` branch.

Both defects hid behind this one, and the ordering is worth stating plainly: the
path bug could not be observed because the file never compiled.

## Acceptance

* `cargo nextest run -p nros-tests --features trigger-test` links and runs.
* `wake_latency_cortex_m3_p99_within_bound` reaches its assertion or its
  documented `[SKIPPED]` for a stated environmental reason (no zenohd, no QEMU),
  not for a missing image.
* Whatever provides the platform symbols on the host is named in the manifest
  rather than inherited, so a feature-set change cannot silently unprovide it.

## Resolution (2026-08-12)

The dependency was there; the LINK was not. `trigger-test` pulls
`nros-platform-cffi` with `posix-c-port`, whose build script compiles
`libnros_platform_posix.a` — the archive that defines the `nros_platform_*`
symbols. Cargo passed `--extern nros_platform_cffi=…rlib` and the `-L` for its
OUT_DIR, and then emitted **no `-l static=nros_platform_posix`**, because a build
script's native-lib directives only apply when the crate that emitted them is
actually linked — and a dependency nothing references is one rustc does not link.
The archive was sitting in the directory the linker searched, unnamed.

Verified from the real command line rather than inferred: `cargo build -v`
showed the `--extern` and the `-L`, and no `-l static=nros_platform_posix`, while
`libnros_platform_posix.a` was present in exactly that OUT_DIR.

Fix is one reference, in `nros-tests/src/lib.rs`:

```rust
#[cfg(any(feature = "trigger-test", feature = "loan-e2e"))]
use nros_platform_cffi as _;
```

Same class as the `force_link_backend!` anchors CLAUDE.md documents for RMW
backends (issues 0155/0163): the symbol is in the rlib, absent from the link, and
the fix is a reference rather than a dependency.

**Acceptance met.** `cargo nextest list --features trigger-test` now finds
`wake_latency_cortex_m3_p99_within_bound`, and running it launches QEMU, finds
both images through their manifest row (issue 0488), runs the probe and takes its
DOCUMENTED environmental skip — "0 samples, likely QEMU CYCCNT not emulated" —
which is the stated hardware limitation, not a missing image. The issue-0317 gate
is live for the first time.
