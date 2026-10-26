---
id: 2046
title: "`examples/px4/cpp/bridge/ffi` reaches `nros-serdes` by a fixed repo-relative path, so the bridge only builds in place"
status: resolved
type: tech-debt
area: [examples, px4, build]
severity: low
found: 2026-10-25
resolved: 2026-10-26
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

## Resolution (2026-10-26)

`nros-serdes = { version = "*", default-features = false }`. Both places that
run cargo on the crate pass the central patch table, the same
`[patch.crates-io]` every leaf resolves through: `just px4
build-bridge-example` and the `px4_bridge_ffi` compile check each call
`nros_ensure_central_patch` and add `--config <root>/nros-patch.toml`. The
module's FATAL_ERROR, which is what a user building the module by hand meets,
now prints that command with absolute paths, and the `--target-dir` its own
default archive path expects (it used to omit it, so following it built an
archive the check then still could not find).

**Verified:** the crate copied to a scratch dir outside the checkout, headers
generated with `nros generate-px4-msgs --topics debug_key_value`, then
`cargo build --release --config <root>/nros-patch.toml` builds
`libnros_px4_bridge_ffi.a` with `nros-serdes` from the checkout. Negative
control: the same copy with origin/main's manifest fails with
`failed to read /tmp/.../packages/core/nros-serdes/Cargo.toml`.

**The acceptance grep above is not empty, and that is not this crate.**
`git grep -nE 'path *= *"(\.\./){4,}packages/' -- 'examples/**/Cargo.toml'`
lists 24 node packages under `examples/workspaces/*/src/`, each path-depping
`nros` by five `..`. RFC-0085 §"copy-out" (line 765) rules an in-tree
workspace "not portable by construction" and a user's workspace names `nros`
by version; that ruling is the open question, raised to the maintainer, not
something this issue changes.
