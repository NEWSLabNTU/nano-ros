---
id: 121
title: "`workspace-rust-threadx-linux` fixture fails E0463 (`nros_platform` rlib not produced) on a cyclonedds-provisioned clean full build — feature unification forces `platform-threadx` onto the x86_64-host `nros`"
status: open
type: bug
area: build
related: [phase-267, 0120]
---

## Summary

Split out from **[#120](archived/0120-bridge-workspace-fixtures-fail-when-cyclonedds-submodule-absent.md)**
(whose actionable half — the cyclonedds bridge gate — is resolved). On a **cyclonedds-provisioned
clean full build**, the `workspace-rust-threadx-linux` fixture (`threadx_linux_entry`, built for
`--target x86_64-unknown-linux-gnu`) fails compiling `nros`:

```
error[E0463]: can't find crate for `nros_platform`
  --> packages/core/nros/src/lib.rs
     pub use nros_platform::{BoardConfig, BoardTransportConfig};
```

`nros` declares `nros-platform` unconditionally (`packages/core/nros/Cargo.toml`), and the entry
pins `nros-platform` with `features = ["platform-threadx"]`
(`examples/workspaces/rust/src/threadx_linux_entry/Cargo.toml`). On the `x86_64`-host build,
Cargo feature/target unification forces `nros-platform[platform-threadx]` and leaves no usable
`nros_platform` rlib for `nros`'s `pub use` — so `nros` fails to link its platform re-exports.

## Reproduction nuance (why it can look green)

Building the entry in **isolation** — `cargo build -p threadx_linux_entry --target
x86_64-unknown-linux-gnu` — pulls only that entry's zenoh subgraph; **cyclonedds is not in the
crate graph**, the unification conflict never triggers, and the build passes. That is a false
green. The failure needs the **full cyclonedds-provisioned matrix build** (the graph that pulls
`nros-rmw-cyclonedds` + the platform crates together) to surface. (This is why an earlier #120
investigation on a box that probed the wrong cyclonedds path wrongly concluded "cannot
reproduce.")

## Suspected root cause / direction

- `nros`'s unconditional `pub use nros_platform::{BoardConfig, BoardTransportConfig, …}` means
  `nros` needs a linkable `nros_platform` for whatever platform feature the unified graph selects.
  When `platform-threadx` is forced on the host build, the produced `nros-platform` artifact isn't
  usable as a plain host rlib for `nros`.
- Options to investigate: gate `nros`'s platform re-export on a platform feature; make the
  threadx-linux entry not force `platform-threadx` onto the shared host `nros` build (per-target
  dep); or confirm/adjust the `nros-platform` `platform-threadx` build so it yields a host rlib.

## Status

Not yet root-caused with a captured clean-build log in this tree. Needs: a cyclonedds-provisioned
clean `build-test-fixtures` (or `just threadx-linux build-fixtures` in that state) that captures
the exact `cargo build -p threadx_linux_entry` feature resolution + the failing rustc invocation,
then a fix on one of the directions above.
