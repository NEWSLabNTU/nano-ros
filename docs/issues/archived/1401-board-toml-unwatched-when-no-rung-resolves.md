---
id: 1401
title: "A build that resolves no platform rung never watches the board
  descriptor, so editing `nros-board.toml` does not rebuild it — the watch sits
  inside the early return that skipped it"
status: resolved
resolved_in: fix/1401-1383-build-tooling — watches emitted before the platform-name early return
type: bug
area: [build, core]
severity: medium
found: 2026-09-21
resolved: 2026-09-28
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

## Resolution (2026-09-28)

`BuildRungs::from_build_env()` now prints its watches FIRST, through a new
`build_env_watches(env)`, and only then takes the `NROS_PLATFORM_NAME` early
return. What it emits unconditionally:

* `rerun-if-env-changed=NROS_PLATFORM_NAME` and `=NROS_BOARD` — names, so their
  text is the fact. The platform-name watch was ALSO behind the early return,
  so a unit built without a platform did not re-run when one was exported
  later; that edge was missing too, and is the more consequential one.
* `rerun-if-changed=<NROS_BOARD_TOML>` — by CONTENT, never by spelling (issue
  0491), and only when the path exists (issue 0490: a trigger on a missing path
  is permanently dirty; the resolving arm still fails loudly on it).

The issue's second half is done as well: when a rung resolves, every
`nros-platform.toml` on the search path is watched per FILE
(`platform_descriptor_files`). Nothing watched them before, in either arm. Not
a directory watch — `packages/platform` holds whole crates.

### Evidence, behavioural

A scratch crate whose `build.rs` emits its own `rerun-if-changed=build.rs` (as
every real consumer does) and calls `from_build_env()`, with `NROS_BOARD_TOML`
set, `NROS_PLATFORM_NAME` unset, target dir and board file outside the package:

```text
BEFORE (origin/main)
=== 3. after editing the board descriptor
       Fresh w1401b v0.0.0
=== 4. after exporting NROS_PLATFORM_NAME=posix
       Fresh w1401b v0.0.0
recorded: cargo:rerun-if-changed=build.rs

AFTER
=== 3. after editing the board descriptor
       Dirty w1401b v0.0.0: the file `./board2.toml` has changed
     Running `.../build/w1401b-be55606a40fdc29b/build-script-build`
=== 4. after exporting NROS_PLATFORM_NAME=posix
       Dirty w1401b v0.0.0: the env variable NROS_PLATFORM_NAME changed
     Running `.../build-script-build`
recorded: rerun-if-changed=build.rs, rerun-if-env-changed=NROS_PLATFORM_NAME,
          rerun-if-env-changed=NROS_BOARD, rerun-if-changed=./board2.toml
```

(Step 4 then panics on the scratch crate's empty platform search path, which
is itself the proof the script re-ran.)

### Evidence, unit

The resolver body now takes its three variables as values
(`from_env_values(BuildEnv, emit)`), so a test can see both the `None` and the
directives. `a_build_that_resolves_no_rung_still_watches_its_inputs` asserts a
no-platform build returns `None` AND declares the board content watch and the
`NROS_PLATFORM_NAME` watch. Mutation — moving the watch loop back below the
`?` — fails it with `no content watch on the board descriptor when no rung
resolves: []`. `build_env_watches_board_by_content_and_only_if_present` pins
the 0491/0490 rule and `platform_descriptor_files_lists_every_root_per_file`
the descriptor listing.

### Not changed, on purpose

An `NROS_BOARD_TOML` that goes from UNSET to set still has no edge of its own —
it is a path, and watching its text is issue 0491. The lanes export it together
with `NROS_PLATFORM_NAME`, whose watch now covers that transition.
`nros_sizing_descriptor::from_build_env()` has the same property for
`NROS_SIZING_DESCRIPTOR` (documented there as deliberate).
