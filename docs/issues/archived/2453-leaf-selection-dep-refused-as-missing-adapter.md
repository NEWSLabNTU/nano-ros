---
id: 2453
title: "The issue 2448 refusal takes a leaf's `build/nros/selection` package for the facade's adapter"
status: resolved
type: bug
severity: high
area: [cli, build, testing]
related: [2448, 1305]
found: 2026-11-12
resolved: 2026-11-12
resolved_in: "the facade refuses only a dependency at the adapter's own home"
---

> **Resolved (2026-11-12).** The refusal now fires only for a dependency
> whose path is the adapter's home. See "Fix".

## Measured

On `main` at e72ef4971d, after the issue 2448 fix landed:

- `just ci gate`, `check::build`: 12 `linux-check#mps2-an385-baremetal-{c,cpp}-*`
  units fail with "facade: entry `baremetal_c_talker` depends on its adapter
  ... but names neither `nros` nor an `nros-board-*` crate".
- `just threadx_riscv64 build-fixtures` fails at
  `bins/logging-smoke-threadx-riscv64` with the same message, and every row
  after it in `build-fixture-extras` goes unbuilt.

## Why

The issue 2448 fix refuses an entry that names neither `nros` nor a board
crate but depends on its adapter. It recognised the adapter by crate NAME,
`<entry>_nros_derived` or the legacy `<entry>_nros_selection`, and did not
check the dependency's path.

The legacy name has a second owner. A standalone leaf (the twelve C/C++
bare-metal mps2 examples, and the seven board-only test bins not yet moved to
owned entries) depends on `<entry>_nros_selection = { path =
"build/nros/selection" }`. That is the leaf road's selection package
(RFC-0098 D6, issue 1305), which the leaf road generates with the board,
backend and runtime crates in it. Such an entry already gets the image's
features, so the facade has nothing to write. Before issue 2448 the facade
returned `Ok(None)` for every entry without `nros`, so these leaves never
reached the check.

The code already knew about this name clash: the legacy-rename branch further
down checks the path against the adapter's previous home and has a comment
saying so. The new refusal did not do the same.

## Fix

`orchestration/facade.rs::is_adapter_home`: a dependency is the adapter only
if its path ends in `nros-derived/<entry>` or `nros-selection/<entry>`. An
entry that names neither crate and depends on the legacy name at any other
path is a leaf-road entry, and the facade returns `Ok(None)` without a
warning. The refusal and the warning still apply to everything else.

Sweep (every tracked manifest naming the adapter's crate names, filtered to
those with no `nros` or `nros-board-*` dependency):

```
git grep -lE '^[a-z0-9_]+_nros_(selection|derived) *= *\{' -- '*Cargo.toml'
```

This found 19 entries: 12 under `examples/mps2-an385-baremetal/{c,cpp}/`,
plus condvar-contract-{freertos-mps2,threadx-linux},
logging-smoke-{freertos-mps2,nuttx-qemu-arm,threadx-linux,threadx-riscv64}
and pool-exhaustion-threadx-linux. All 19 point at `build/nros/selection`,
and none points at the adapter's home.

Unit test: the two new cases in
`orchestration::facade::tests::a_board_only_owned_entry_gets_an_adapter_without_the_nros_axes`.
Measured: `linux-check` units `mps2-an385-baremetal-c-talker` and
`mps2-an385-baremetal-cpp-listener`, and
`fixtures-build.sh threadx-riscv64 rust --id threadx-riscv64-logging-smoke`,
all build.
