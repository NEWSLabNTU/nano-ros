---
id: 1401
title: "A build that resolves no platform rung never watches the board
  descriptor, so editing `nros-board.toml` does not rebuild it — the watch sits
  inside the early return that skipped it"
status: open
type: bug
area: [build, core]
severity: medium
found: 2026-09-21
related: [1390, 1388, 0491, 0196, 1018]
---

## What happens

`BuildRungs::from_build_env()` (`packages/tooling/nros-platform-config/src/platform_config.rs`)
emits the `cargo:rerun-if-changed` that makes a board descriptor a build input:

```rust
pub fn from_build_env() -> Option<Self> {
    let platform = std::env::var("NROS_PLATFORM_NAME")
        .ok()
        .filter(|s| !s.is_empty())?;          // <- early return, line ~326
    …
            .map(|raw| {
                // issue 0491 — fingerprint the file's CONTENT.
                println!("cargo:rerun-if-changed={raw}");   // <- line 342
```

VERIFIED by reading: the watch is at line 342, the function spans 323–355, and
the `?` on `NROS_PLATFORM_NAME` returns before it. So a unit compiled **without**
`NROS_PLATFORM_NAME` emits no watch on `NROS_BOARD_TOML`'s file — and cargo has
no edge from that unit to the descriptor.

## Why that is reachable

Issue 1390 measured the population directly: of 18 `nros-node` compilations in
one `just threadx_linux build-examples`, **11 resolved no rung at all**
(`from_build_env()` -> `None`) — `nros_c-static` / `nros_cpp-static` in three
cmake workspace roots, two cargo-built workspace entries, and four size probes.
Every one of those is a unit that will not rebuild when a board descriptor
changes.

It is also self-masking in the usual direction: the units that DO resolve a rung
watch the file correctly, so a board edit does rebuild *something*, and the
build looks responsive while a subset of it is stale.

## How it was noticed

It made a positive control vacuous. While measuring issue 1390, a hand-run
control changed the board's `backing_u64s` between runs and all three cases
passed — because nothing recompiled. `from_build_env()` had returned `None` for
want of `NROS_PLATFORM_NAME`, so the rung was never read AND the file was never
watched. The control was measuring cargo's cache, not the knob.

That is the same shape as issue 1018 (a configure-time emitter with no `DEPENDS`
to carry its tool) one layer over: the edge is missing, so freshness reduces to
"did something else happen to rebuild".

## What is NOT established

Whether any unit that resolves no rung actually CONSUMES a board fact today. If
none does, the missing edge is currently harmless and the defect is that the
guarantee is not what it appears to be; if one does, it is a live staleness bug.
Issue 1390's measurement says the 11 no-rung units compare a defaulted number
with itself, which suggests the former for the executor knobs specifically —
but that is one knob family, not the whole board surface.

## Direction

Emit the `rerun-if-changed` for `NROS_BOARD_TOML` (and the platform descriptor
search path) BEFORE the `NROS_PLATFORM_NAME` early return, so the edge exists
whether or not a rung resolves. Watching a file a build did not read is cheap
and fails safe; not watching one it might read is issue 0196's shape.

Acceptance: edit a board descriptor, and every `nros-node` unit in the lane
re-runs its build script — not just the ones that named a platform.
