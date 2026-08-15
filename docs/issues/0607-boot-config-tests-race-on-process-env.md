---
id: 607
title: "`boot_config_tests` race on process-global env under `cargo test`, so `check-node-std-tests` fails ~1 run in 3"
status: open
type: bug
area: testing
related: [issue-0466, phase-359]
---

## Symptom

`just ci` fails at `check-node-std-tests`:

```
thread 'executor::types::boot_config_tests::noop_resolve_matches_from_env'
  panicked at packages/core/nros-node/src/executor/types.rs:1343:9:
assertion `left == right` failed
  left: ""
 right: "tcp/env:7447"
```

Nine of the twenty-four `boot_config_tests` fail together when it happens.

## It is a race, and the rate is measured

Same tree, same binary, no changes between runs:

| run | mode | result |
| --- | --- | --- |
| 1 | parallel (default) | ok, 24 passed |
| 2 | parallel | **FAILED, 15 passed / 9 failed** |
| 3 | parallel | ok, 24 passed |
| — | `--test-threads=1` | ok, 24 passed |

So roughly **one run in three**, and never single-threaded.

## Cause

These tests set and read PROCESS-GLOBAL environment variables
(`NROS_LOCATOR`, `NROS_DOMAIN_ID`, `NROS_NODE_NAME`, …) to exercise the
baked-vs-env resolution ladder. `cargo test` runs a crate's unit tests as
threads in ONE process, so two of them mutating the same variable interleave and
each sees the other's value — hence `left: ""` where the test had just set
something.

The tree's own runner is nextest, which gives every test its own PROCESS, and
under it this class cannot happen. That is why the same hazard is written down
as an assumption elsewhere:

```rust
// SAFETY: nextest runs each test in its own process.
unsafe { std::env::set_var("NROS_TEST_SCOPE", "native") }
```
(`nros-tests`, `gated_absence_is_a_hard_failure`)

`check-node-std-tests` runs `cargo test`, not nextest, so the assumption does
not hold there.

## Why it matters

It is on the `just ci` line, so tier 1 fails for a reason unrelated to the
change under test roughly a third of the time. A flake on the default gate
trains people to re-run rather than read, which is how a real red gets waved
through.

## Fix shapes

1. **Serialise the env-touching tests** on a shared mutex, the idiom
   `nros-sizes-build`'s tests already use (`env_lock()`), and the narrowest fix.
2. **Stop mutating process env**: thread the resolution inputs through a
   parameter so the ladder is testable without globals. Larger, and it removes
   the hazard rather than scheduling around it.
3. **Run this lane under nextest**, making the process-per-test assumption true
   everywhere it is already written down.

(1) is the immediate unblock; (2) is the honest fix; (3) is worth considering
separately because the assumption is currently stated in one crate and relied on
in another.

## Provenance

Found 2026-08-15 while restoring tier 1 after issue 0601. The tests are not new
— `packages/core/nros-node/src/executor/types.rs` was last touched by
`3fd70f32a` (phase-359 W6) — and the failure reproduces on a tree with NO local
changes to `packages/core/nros-node`.
