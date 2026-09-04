---
id: 1037
title: "`nros-log` carries FOUR pick-one Cargo-feature families in `packages/core/`,
  three of which encode \"off\" — RFC-0086 D5 forbids both and its audit missed them"
status: open
type: bug
area: config, api
related: [rfc-0086, rfc-0049, phase-400, phase-417, issue-0503, issue-0710]
---

## The rule, and the claim that is wrong

RFC-0086 D5:

> * Cargo features may **pull code in**. They may not express "off", and they may
>   not encode a pick-one family.
> * Any knob whose correct value is sometimes "off", and any exclusive choice,
>   belongs in the ladder with a lane front-end that can carry `0` as well as `1`.
> * **No migration is outstanding. The audit found no exclusive or negative
>   configuration expressed as a Cargo feature in core or api.**

`packages/core/nros-log/Cargo.toml` has four:

| family | members | encodes "off"? |
| --- | --- | --- |
| `max-level-*` | trace, debug, info, warn, error, **off** | yes — `max-level-off` |
| `early-records-<N>` | **0**, 8, 16 | yes — `early-records-0` |
| `dynamic-loggers-<N>` | **0**, 8, 32 | yes — `dynamic-loggers-0` |
| `buffer-size-<N>` | 128, 256, 512, 1024 | no, but pick-one |

Sixteen features across four exclusive families, in `core`, three of them with a
member whose whole meaning is "off". Every one is a knob a user picks, and every
one is invisible to `nros config explain`.

`platform-clock` is the fifth and a different shape: it legitimately *pulls code
in*, so D5's first clause allows it — but its ABSENCE silently changes observable
behaviour in all three languages (records carry `timestamp_ns: 0`, and
`nros_log_throttle_admit` has no time base so a 200 ms window admits every
record — measured 40 of 40 without, 5 of 40 with). That is the negative half of
D5 arriving through the back door.

## Why the audit missed it

D5's audit looked for configuration expressed as a Cargo feature. These read as
*sizing* — `buffer-size-256` looks like a build detail rather than a policy — and
`nros-log` was not one of the tenants phase-400 W6 migrated (`executor`,
`memory`, `params`, `rmw`, `transport`). Logging simply was not on the list, so
nothing pointed at it.

The general shape: **a knob looks like a Cargo feature exactly when the crate
that owns it is not yet a tenant.** The five migrated tenants have the same kind
of knob and none of them is a feature.

## How phase-417 made it worse, twice

1. W4.d added `dynamic-loggers-{0,8,32}` — a **new** pick-one family with an
   "off" member, in core, after D5 was written.
2. Integration wired `nros-log/platform-clock` through `nros-c`'s five
   `platform-*` features. That was a fix for a real bug (setting it on the
   *dependency* turned it on workspace-wide, and test binaries link no platform
   port, so `nros-log`'s own tests failed on `undefined reference to
   nros_platform_clock_ns`) — but the correct answer was never a Cargo feature.

## What it should be

`capabilities` is a `BTreeMap<String, bool>` — an **open vocabulary**
(`platform_config.rs:92`), so the clock needs no schema change:

```toml
# packages/platform/nros-platform-{posix,zephyr,freertos,nuttx,threadx}/nros-platform.toml
[capabilities]
clock = true          # nros_platform_clock_us is exported by this port
```

and the sizing knobs become a `logging` tenant beside the five phase-400 W6
migrated, resolved builtin < platform < board < env, reaching C, C++ and Rust
from one declaration and printed by `nros config explain`:

```toml
[knobs.logging]
max_level     = "info"
buffer_size   = 256
early_records = 4
dynamic_loggers = 16
```

The Cargo features do not all have to disappear. D5 permits one that only pulls
code in, and a build script may still *derive* a `cfg` from the resolved ladder —
that is internal logic reading a public declaration, which is the distinction
that matters. What must stop is a HUMAN writing `features = ["max-level-info"]`
in a manifest to choose product behaviour.

## Why this is filed rather than fixed

It reaches the config ladder, five platform manifests, `nros-log`'s build, the
Kconfig `imply` path and every consumer that currently names a feature. It also
needs one decision that is not obvious: whether a `logging` tenant's knobs are
per-image (like `memory`) or per-logger, since a named logger's threshold is
already a runtime value (`nros_logger_set_level`) and a compile-time ceiling
that contradicts it would be two sources of truth for one number.

Meanwhile the shipped state works and is not a regression — `platform-clock`
reaches every real image through the platform features, which is why the C
throttle functions. It is debt with a stated shape, not a broken build.

**D5's third bullet should be corrected**: a migration IS outstanding, and the
"enforced at review" claim did not hold — a new violating family landed after
the rule was written.
