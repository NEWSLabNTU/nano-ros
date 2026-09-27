---
id: 1505
title: "check-kconfig-knob-forwarding's coverage arm cannot see the 27 knobs forwarded by _nros_resolve_derivable_knob"
status: open
area: build
severity: medium
phases: [468]
rfcs: [0049]
related: [0460, 0751, 1490, 0196]
---

# `check-kconfig-knob-forwarding`'s coverage arm cannot see the 27 knobs forwarded by `_nros_resolve_derivable_knob`

## The measurement

The gate harvests the knobs it checks with

```bash
grep -oE '_nros_resolve_knob\(([A-Z0-9_]+)' "$CMAKE"
```

`zephyr/cmake/nros_cargo_build.cmake` has a second forwarding helper,
`_nros_resolve_derivable_knob()` — "`_nros_resolve_knob` plus rungs 3 and 4 of
the ladder", for a knob whose Kconfig option documents `-1` as *derive*. The
string `_nros_resolve_knob(` is **not** a substring of
`_nros_resolve_derivable_knob(`, so every knob forwarded that way is invisible
to the harvest, silently.

27 knobs, including the ones issue 0460 was measured on:

```
NROS_EXECUTOR_MAX_CBS          NROS_MAX_QUERYABLES
NROS_EXECUTOR_MAX_NODES        NROS_MAX_PUBLISHERS
NROS_EXECUTOR_ACTION_CLIENTS   NROS_MAX_SUBSCRIBERS
NROS_EXECUTOR_MAX_MONITORS     NROS_MAX_LIVELINESS
NROS_EXECUTOR_MAX_AGE_MONITORS NROS_MAX_PARAMETERS
NROS_SUBSCRIBER_BUFFER_SIZE    NROS_MAX_PARAM_NAME_LEN
NROS_SUBSCRIPTION_BUFFER_SIZE  NROS_MAX_STRING_VALUE_LEN
NROS_SUBSCRIBED_TYPE_BOUNDS    NROS_MAX_ARRAY_LEN
NROS_RMW_SUBSCRIBER_SLOTS      NROS_MAX_BYTE_ARRAY_LEN
NROS_DECLARED_TL_PUBLISHERS    NROS_XRCE_MAX_SERVICE_SERVERS
ZPICO_MAX_LARGE_SUBSCRIBERS    NROS_XRCE_MAX_SUBSCRIBERS
ZPICO_SUBSCRIBER_LARGE_SIZE    NROS_XRCE_SERVICE_REPLY_BUFFER_SIZE
ZPICO_TL_RETAIN_BYTES          NROS_XRCE_SERVICE_REQUEST_BUFFER_SIZE
                               NROS_XRCE_SUBSCRIBER_BUFFER_SIZE
```

The gate's own success line reads "47 forwarded knob(s)". The cmake module
forwards 74.

This is issue 0196's shape — a gate whose REACH is narrower than the rule it
enforces — and it is what let two live splits sit unreported:
`ZPICO_MAX_LARGE_SUBSCRIBERS` and `ZPICO_SUBSCRIBER_LARGE_SIZE` are forwarded
from `CONFIG_NROS_MAX_LARGE_SUBSCRIBERS` / `CONFIG_NROS_SUBSCRIBER_LARGE_SIZE`
and had a row in neither of the two per-crate `KCONFIG_KNOBS` tables, so on a
Zephyr Rust image both were read env-only. They multiply into the largest pool
the tree has (`ZPICO_MAX_LARGE_SUBSCRIBERS × ZPICO_SUBSCRIBER_RING_DEPTH ×
ZPICO_SUBSCRIBER_LARGE_SIZE` — 131 072 bytes at the defaults, the arithmetic
issue 0271 cost ~145 KB to).

## What phase-468 W4 already did, and what is left

W4 rewrote the gate around what the one reader cannot express. Its **pairing**
arm reads BOTH spellings, because checking that a knob's env name maps to the
right Kconfig symbol needs no reader to exist — and that arm is what caught the
two knobs above, which now have `KCONFIG_PAIRS` rows.

The **coverage** arm — "some Rust build script reads this knob" — still reads
`_nros_resolve_knob(` only. Widening it is not a one-line change: several of
the 27 are forwarded under a `NROS_*` spelling that no Rust crate reads, because
a second `ZPICO_*` export of the same symbol is what the Rust lane takes
(`NROS_MAX_PUBLISHERS` beside `ZPICO_MAX_PUBLISHERS`). Each needs either a
reader or a `NO_RUST_READER` entry **with a reason**, and an entry written
without measuring which lane consumes it would be an exemption that means
nothing.

## Acceptance

- [ ] The harvest reads both `_nros_resolve_knob(` and
      `_nros_resolve_derivable_knob(`, and the success line names the real
      count.
- [ ] Every knob the widened harvest adds has a reader or a `NO_RUST_READER`
      reason naming the lane that consumes it instead.
- [ ] The negative control covers the widened harvest: a `derivable` knob with
      no reader must FAIL.
