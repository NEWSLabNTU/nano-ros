---
id: 491
title: "Leaf `[env] relative = true` values are per-leaf STRINGS, so siblings in a
  shared cargo group rebuild each other forever"
status: open
type: bug
area: build
related: [phase-340, rfc-0070, issue-0490, rfc-0048]
---

## Symptom

Two rows in the same shared cargo group cannot both be fresh. Building
`examples/qemu-arm-freertos/rust/listener` makes `…/talker` dirty, and building
talker makes listener dirty — indefinitely. Measured on a settled tree, after
issue 0490 was fixed (which is why 0490 had to go first: it made every row dirty
for an unrelated reason and hid this one):

```
A  talker alone, probed twice          -> fresh, fresh
B  six sibling rows, probed in order   -> five dirty on pass 1, all six on pass 2
C  build logging-smoke, re-probe talker-> talker dirty
```

## Cause

Each leaf's `.cargo/config.toml` carries

```toml
[env]
NROS_PLATFORM_FREERTOS_SRC = { value = "../../../../packages/platform/nros-platform-freertos/src", relative = true }
NROS_PLATFORM_CFFI_INCLUDE = { value = "../../../../packages/platform/nros-platform-api/include",  relative = true }
```

`relative = true` roots the value at THAT LEAF, so the string cargo hands the
build script is

```
.../examples/qemu-arm-freertos/rust/talker/../../../../packages/platform/nros-platform-freertos/src
.../examples/qemu-arm-freertos/rust/listener/../../../../packages/platform/nros-platform-freertos/src
```

Two different strings naming ONE directory. `nros-board-freertos` and
`zpico-sys` declare `cargo:rerun-if-env-changed` on them, and cargo compares the
env var **textually**, not by resolved path:

```
dirty: EnvVarChanged { name: "NROS_PLATFORM_FREERTOS_SRC",
  old_value: Some(".../rust/listener/../../../../packages/platform/nros-platform-freertos/src"),
  new_value: Some(".../rust/talker/../../../../packages/platform/nros-platform-freertos/src") }
```

so each sibling re-runs the board and zpico build scripts, and everything above
them rebuilds (`UnitDependencyInfoChanged` cascades to the leaf bin).

Per-leaf `target/` dirs hid this completely: each leaf had its own fingerprint
namespace, so the strings never met. **Sharing the dir is what surfaced it** —
it is a cost of the phase-340 group mechanism, present since B3 wave 2, not
introduced by P2 (the six freertos rows already shared one group before P2, and
test A above shows a row IS stable in isolation).

## Why it matters

It partially defeats the group's payoff. The disk win stands — one `deps/`
instead of N — but the CPU win does not: a sweep over N rows in one group still
recompiles the shared crates N times, and every staleness probe rebuilds them
again. Any measurement of "the group made the build faster" taken over more than
one row in a group is measuring this too.

## Fix sketch

The env value must be identical text for every member of a group, or the build
scripts must not fingerprint it.

1. **Watch the files, not the env string** (preferred). The build scripts want
   to rebuild when the platform SOURCES change; `cargo:rerun-if-changed=<the
   canonicalized dir>` says that directly, and canonicalisation makes every leaf
   agree. `rerun-if-env-changed` on a path variable is the wrong instrument.
2. Emit a normalized value from `nros sync`. Blocked: these `[env]` blocks are
   the AUTHORED half of the leaf config (RFC-0048 W9) and are tracked, so they
   cannot hold an absolute path — and a tracked relative path is exactly what
   `relative = true` resolves per leaf.

Option 1 is one edit per build script that declares such a variable. Sweep for
`rerun-if-env-changed` on any `NROS_PLATFORM_*` / `*_DIR` / `*_INCLUDE` name
before fixing the two this was measured on.
