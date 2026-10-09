---
id: 1766
title: "`nros::main!()` without `launch =` ignores the leaf's declared capability axes: `features = [\"param_services\"]` builds an image with no parameter services and no error"
status: open
type: bug
area: [api, macros, examples]
severity: low
found: 2026-10-09
related: [1706, 0274, phase-314]
---

## What

A single-package Rust leaf states its capabilities in `system.toml`'s
`[system] features`, the same key a bringup uses. `nros::main!` reads that axis
only on its LAUNCH arm (`launch = "..."` / `model = "..."`): there it wires
`apply_param_services` and const-asserts that `nros` was built with the
`param-services` feature (phase-314). Form 1 -- `nros::main!()` or
`nros::main!(panic = ...)`, the shape every `examples/*/rust/<role>` leaf uses
-- never opens the model, so the axis is dropped in silence. No services, no
const-assert, no warning.

## Measured (2026-10-09)

`examples/mps2-an385-freertos/rust/talker` (`nros::main!(panic = "platform")`),
QEMU + `rmw_zenohd`:

| `[system] features` | `nros` cargo feature `param-services` | parameter store built | heap peak |
| --- | --- | --- | --- |
| `param_services` | no | no | 163,472 |
| `param_services` | yes | no | 163,472 |

163,472 is the parameter-less number issue 1706 recorded. The text grew by
53 KB with the feature, so the services were compiled and never registered.

## Why it matters beyond the missing services

Every derivation that reads the axis assumes the entry wires it: the
queryable pool counts six parameter servers per node for it (issue 1270), and
since issue 1706 the cargo-leaf sizing descriptor states `[params] store =
"param_services"` when the axis AND the cargo feature are both present, which
carves 280,832 B of `.bss` for the store. On a Form-1 leaf that store is never
used. (The axis alone, without the feature, carves nothing -- that was ruled in
issue 1706 from the first row above.)

## Direction

Form 1 should read the leaf's own resolved model (`nros sync` writes it under
`build/nros/models/`) for the capability axes, exactly as the launch arm does,
including the phase-314 const-assert. Then the axis means one thing on every
leaf, and the descriptor's reading of it is exact.
