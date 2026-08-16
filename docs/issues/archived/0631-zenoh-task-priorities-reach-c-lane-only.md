---
id: 631
title: "`#626`'s transport-task priorities reached the Zephyr C lane and not the Rust one, and `check-kconfig-knob-forwarding` was red on main for it"
status: resolved
type: bug
severity: medium
area: build, rmw-zenoh, zephyr
related: [issue-0626, issue-0460, issue-0623, issue-0135]
---

## Symptom

`just check` fails on `main`:

```
[FAIL] ZPICO_LEASE_TASK_PRIORITY is forwarded by zephyr/cmake/nros_cargo_build.cmake
       but no Rust build script reads it
[FAIL] ZPICO_READ_TASK_PRIORITY is forwarded by zephyr/cmake/nros_cargo_build.cmake
       but no Rust build script reads it
```

Reproduced in a clean worktree at `e228a8e80` with no other commits, so this is
a red on the mainline rather than an interaction. `just ci` stops at `check`, so
tier 1 could not reach its test phase at all.

## Cause

`e228a8e80` (#626) made the zenoh read/lease task priorities settable on Zephyr
and wired three drop points, but the knobs only ever reached the **C** lane:

```
zephyr/cmake/nros_cargo_build.cmake:178  _nros_resolve_knob(ZPICO_READ_TASK_PRIORITY  …)
zephyr/cmake/nros_rmw_zenoh.cmake:171    ZPICO_READ_TASK_PRIORITY=${NROS_RESOLVED_…}
packages/rmw/zenoh/zpico-sys/c/zpico/zpico.c:195  #ifndef ZPICO_READ_TASK_PRIORITY / #define 16
```

`nros_cargo_build.cmake` publishes knobs with `set(ENV{…})`, which touches only
the configure-time process; zephyr-lang-rust's `rust_cargo_application` builds
its own command and inherits nothing. So a Zephyr **Rust** image compiled the
shim with `zpico.c`'s own `#define` fallback of 16 while Kconfig said something
else — and the failure is silent, because 16 is a perfectly plausible priority.

That is **issue 0460 exactly**, the case CLAUDE.md already records: *"A Kconfig
knob reaches the Zephyr C lane and NOT the RUST one."* 0460 was `MAX_QUERYABLES`
(16 in the cmake TU, 8 in the cargo one), where the disagreement is also an
issue-0135 ABI split. Here it is a scheduling parameter, so the two lanes
produce images that behave differently under load rather than crash — which is
worse to find, and is exactly the question #626 exists to let people ask.

The gate that caught it, `check-kconfig-knob-forwarding`, is 0460's remedy
working as designed. It fired the moment the feature landed.

## Fix (2026-08-16)

Two rows in `nros-zpico-build`'s `KCONFIG_KNOBS`
(`ZPICO_{READ,LEASE}_TASK_PRIORITY` ← `CONFIG_NROS_ZENOH_{READ,LEASE}_PRIORITY`),
two `ShimConfig` fields resolved with `env_usize`, and two `-D`s emitted to the
C shim. Defaults are **16/16, mirroring `zpico.c`'s own `#ifndef` fallback** —
a different number here would not be a tuning choice, it would be the two lanes
disagreeing by a second route.

The defines are emitted **unconditionally**, unlike the `tx_batch` pair which is
genuinely opt-in. Omitting them is not "no opinion": the shim defines its own
default, so a missing `-D` is the C default silently winning over Kconfig.

### `apply_to_cc` was untestable, and that is why this could happen

`cc::Build` (1.2.63) exposes no way to read back what was defined on it, so any
test written against the builder can only assert that the call did not panic.
The define list is now a pure `ShimConfig::defines() -> Vec<(&str, String)>` and
`apply_to_cc` is a loop over it, so the assertion can finally be written:
`transport_task_priorities_reach_the_c_shim` checks both knobs in both
`tx_batch` states, plus that `tx_batch` gates only its own pair.

Mutation-tested by deleting the two `out.push` calls — i.e. reconstructing the
pre-fix state — which fails with `ZPICO_READ_TASK_PRIORITY not defined
(tx_batch=false)`.

Gate after: `kconfig-knob-forwarding OK — 23 forwarded knob(s), each read by the
Rust lane.`

## Not verified here

That a Zephyr image actually schedules at the configured priority. This host has
no Zephyr workspace, so the claim is that the Rust lane now COMPILES the shim
with the same `-D` the C lane passes — checked by the gate and the unit test,
not by watching a task run. #626's own acceptance is the place for that.

**Units caveat, since this is the file for it:** these are NORMALISED 0–31
values mapped down onto the platform's range, not raw RTOS priorities. Issue
0623 records the collision that follows from confusing the two, and CLAUDE.md
carries it.
