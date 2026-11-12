---
id: 2443
title: "Runtime logs of a default image and the colcon plugin's errors still cite internal ids, and no gate reads them"
status: open
type: tech-debt
severity: low
area: [cli, ux, logging, runtime]
related: [issue-2305, phase-523]
found: 2026-11-11
---

**Home: [phase-523](../roadmap/phase-523-education-labs-adoption-probes.md).**
The two surfaces no gate reads after issue 2305 took CMake's default-level
output to a zero baseline and gated build-script warnings.

## What

RFC-0116 D2 says the user surface names an action, never an internal id. Two surfaces a user reaches are not read by the gate:

- **Runtime logs of a default image.** Measured 2026-11-11 with a string
  scan (literals outside comments and `#[cfg(test)]`, bindgen's
  `generated.rs` left out): about 40 literals across 23 files in
  `packages/{core,api,rmw,platform}` carry an id, the most in
  `zpico-sys/c/zpico/zpico.c` (13) and `nros-rmw-xrce/src/graph.c` (5).
  Not every literal is a log line; some are `assert!` texts or data.
- **The colcon plugin** (`packages/cli/colcon-cargo-ros2/colcon_nano_ros`):
  its refusals are raised in a user's `colcon build`, and at least
  `manifest.py`'s unknown-build-type error ends "— RFC-0087 D2."

## Fix direction

The same as issue 2305's: move each id into a comment above the message, and
add both surfaces to `check-user-surface-ids`. The runtime subject needs a
rule for which literals are log and panic text (an `nros_log` call, a
`printf`/`printk`, a `panic!`/`assert!` message), because a runtime crate
also holds data strings; a C subject needs a string lexer the gate does not
have yet.

## Progress (2026-11-12)

**The colcon plugin is done.** Its two refusals that cited ids now say what
to do: `manifest.build_path` names the `<build_type>` values to write, and
`manifest.deploy` keeps its remedy without the RFC and phase ids, which moved
to a comment above the raise. `check-user-surface-ids` reads the plugin as a
subject: every Python string literal under `colcon_nano_ros/` that is not a
docstring, baseline zero. Negative control: the same gate on the old
`manifest.py` reports its three ids.

**User ruling, 2026-11-12** ("Take your recommendation"), relayed by the
manager: "Error reporting = A NOW, then C later: user-reachable output,
build-stopping refusals included, names what/where/how-to-fix and NO
phase/issue/RFC id. Ids live in source comments and `-v` detail. (C, stable
codes + an explain verb, goes to 243 later.)" The issue was un-parked on the
same ruling.

**The CLI library refusals named here are done.** `build_type.rs`'s
retired-`<build_type>` warning, and `package_xml.rs`'s two refusals (one build
role per package; the retired `<nano_ros>` element), keep their remedy and
lose the RFC and phase ids, which moved to comments. The CMake twin of the
`<nano_ros>` refusal (`NanoRosPackageXml.cmake`, a `FATAL_ERROR`) loses them
too, so the two readers still say the same thing. `check-user-surface-ids`
reads both Rust files whole (`REFUSAL_FILES`, baseline zero). Negative
control: the gate on main's two files reports their 5 ids.

**Backlog the ruling opens, measured 2026-11-12, not done here:**
- `nros-cli-core/src` outside `cmd/` and `cargo-nano-ros/src` hold about 219
  id-bearing string literals (string scan; not all are messages, some are
  generated code or data). Most in `entity_inventory.rs` (26),
  `builder/cmake_root.rs` (12), `builder/west_app.rs` (12).
- CMake `FATAL_ERROR`/`SEND_ERROR` text cites an id at 103 sites in 52 files.
  `check-cmake-user-output-refs` excludes those two levels on purpose ("a
  build that stops is being diagnosed"). Ruling A reverses that exclusion.

**The backlog is ratcheted (2026-11-12).** The manager assigned the sweep,
behind a ratchet so the count cannot grow meanwhile:
- `check-user-surface-ids` counts ids per file in
  `.config/user-surface-ids-ratchet.json`, 226 in 59 files. That is 209 in the
  CLI library (`nros-cli-core/src` outside `cmd/`, `cargo-nano-ros/src`;
  test-only modules not read) and 17 in build-script panic texts.
- `check-cmake-user-output-refs` counts `FATAL_ERROR`/`SEND_ERROR` text too:
  102 sites in `.config/cmake-user-output-refs.json`.
- A count that rises fails, and one that falls fails until its row is
  lowered in the same change.
- Not counted, a stated gap: build-script text assembled away from its
  panic, and build-script literals that are not panic or warning text (84
  ids, mostly headers of files generated into `OUT_DIR`).

The sweep goes by file, biggest first. Each message names what, where and
how to fix; its id moves to a comment. Stable error codes (ruling C) come
later, so each message stays self-contained, with room for a code prefix.

**Left: runtime logs of a default image.** The rule for which runtime
literals are log or panic text, and the C string lexer, are still to do.

