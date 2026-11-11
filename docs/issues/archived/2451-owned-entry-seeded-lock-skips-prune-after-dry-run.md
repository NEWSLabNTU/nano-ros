---
id: 2451
title: "A `--dry-run` plan seeds an owned entry's Cargo.lock, and the real build then skips the prune and fails `--frozen`"
status: resolved
type: bug
severity: medium
area: [cli, build, testing]
related: [1716, 2448]
found: 2026-11-12
resolved: 2026-11-12
resolved_in: "nros build prunes an owned entry's lock until it names the entry's own package"
---

> **Resolved (2026-11-12).** `nros build` now decides whether an owned
> entry's lock still needs its `cargo update --workspace` by asking whether
> the lock names the entry's own package, not whether the file exists. See
> "Fix".

## Measured

`bins/logging-smoke-nuttx-qemu-arm`, migrated to an owned-entry workspace
(phase-531 W7), built through `scripts/build/workspace-fixtures-build.sh
nuttx rust --id nuttx-logging-smoke`:

```
error: cannot update the lock file .../src/logging-smoke-nuttx-qemu-arm/Cargo.lock
because --frozen was passed to prevent this
```

The lock held the 438 registry rows of the pin set and no path package, not
even the entry itself. `nros build` run directly on the same workspace, with no
lock present, built.

## Why

Planning writes (issue 1716): for an owned entry it seeds the lock from the
pin set (RFC-0104 D3) and plans the `cargo update --workspace` that prunes the
seed only when there was no lock before the seed (`had_lock`). The fixture
builder's cache warm-up (`nros_warm_leaf_cache`) runs `nros build --dry-run`
first. That plan seeds the lock and runs nothing. The real build then finds a
lock, takes it for the user's, and plans no prune.

Most rows survived it by accident. `drift_before_build` realigns a checkout
lock with `cargo update --precise`, which resolves the root as a side effect.
A NuttX entry's root cannot be resolved alone (its `libc` patch exists only
in the build's settings file), so the realign leaves the raw seed, and the
`--frozen` build refuses to add the entry's path packages.

## Fix

`cmd/build.rs::lock_names_package`: a lock is the user's resolved lock only
if it names the root package. Cargo writes every workspace member into the
lock it resolves, so a seed no resolve has touched never does. `had_lock` is
that answer, so the prune runs on the build after a dry run too. A lock that
lists no package, or one cargo cannot parse, is still left alone as the
user's.

Unit test: `cmd::build::lock_names_package_tests::a_pin_seed_is_not_a_resolved_lock`.
Measured: the NuttX row, started from the raw seed, builds through the fixture
script, and `logging_smoke_nuttx_qemu_arm_emits_every_severity` passes.
