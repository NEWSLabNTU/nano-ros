---
id: 2046
title: "`examples/px4/cpp/bridge/ffi` reaches `nros-serdes` by a fixed repo-relative path, so the bridge only builds in place"
status: open
type: tech-debt
area: [examples, px4, build]
severity: low
found: 2026-10-25
related: [issue-1516, issue-0362, rfc-0026, phase-452]
---

# The px4 bridge's FFI crate reaches core by repository layout

**Home:** [phase-452](../roadmap/phase-452-copy-out-front-door.md) W7.

## Problem

`examples/px4/cpp/bridge/ffi/Cargo.toml:24`:

```toml
nros-serdes = { path = "../../../../../packages/core/nros-serdes", default-features = false }
```

The crate is its own `[workspace]` root inside an example tree. Issue 1516 ruled
that `examples/px4/` holds copy-INTO-PX4 sources: the user places them in a PX4
tree through `EXTERNAL_MODULES_LOCATION`. Once copied there, five `..` no longer
lead to a nano-ros checkout, and the bridge's PX4 build fails on an unresolved
path dependency.

Every other example reaches an in-repo crate by NAME (`version = "*"`), and the
toolchain supplies the path in one place: the generated `[patch.crates-io]`,
`nros-cargo.toml`, RFC-0098 D1. The same bridge's CMake half already takes the
root explicitly (`-DNROS_REPO_DIR` / `-Dnano_ros_ROOT`, its `CMakeLists.txt:19-27`).
So the cargo half is the one place that assumes where it sits.

Checklist G2 ("a path into another package"). Found while measuring that item
on 2026-10-24.

## Fix direction

Name the dependency (`nros-serdes = { version = "*", default-features = false }`)
and have the module's CMake pass the root it already resolves to cargo as a
`--config` patch: the generated central patch file, the same channel the leaves
use. Acceptance: the bridge builds from a copy outside the checkout with only
the root passed, and `git grep -nE 'path *= *"(\.\./){4,}packages/' -- 'examples/**/Cargo.toml'`
is empty.
