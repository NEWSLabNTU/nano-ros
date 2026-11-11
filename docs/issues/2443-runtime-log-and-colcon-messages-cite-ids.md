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
