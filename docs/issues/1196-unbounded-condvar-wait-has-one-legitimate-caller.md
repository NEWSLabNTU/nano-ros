---
id: 1196
title: "`nros_platform_condvar_wait` blocks forever by construction and has exactly
  one caller that should — the guard is a gate, not a comment, because
  `condvar_wait` is the spelling the next port will reach for"
status: open
type: tech-debt
area: [platform, core, rmw]
related: [phase-436, 1198]
---

## What

The platform ABI carries two condition-variable waits, and only one of them has
a deadline:

```c
int8_t nros_platform_condvar_wait(void *cv, void *m);
int8_t nros_platform_condvar_wait_until(void *cv, void *m, uint64_t deadline_ms);
```

The unbounded one is unbounded in **every** port, and each spells it with that
port's own forever constant — `TX_WAIT_FOREVER` on ThreadX, `portMAX_DELAY` on
FreeRTOS, `K_FOREVER` on Zephyr, a bare `pthread_cond_wait` on POSIX. That is
not an implementation detail one port could tighten: it is what the function
means.

## Why this is debt rather than a bug

There is nothing to fix in the current tree. Measured on the phase-436 W5
branch, the only call outside the ports' own definitions is
`packages/rmw/zenoh/zpico-sys/c/zpico/platform_aliases.c`, which implements
zenoh-pico's `_z_condvar_wait` — an upstream primitive whose **contract** is an
unbounded wait. Bridging an unbounded upstream call to an unbounded platform
call is the correct mapping, and narrowing it there would silently change
zenoh-pico's semantics rather than improve ours.

The executor does not use it at all. Its own wait is the bounded
`nros_platform_wake_wait_ms`, which is what phase-436 made deadline-driven.

So the debt is the SHAPE, not a site: a forever-blocking primitive sits in the
ABI beside a bounded sibling with a nearly identical name, and the shorter name
is the one a reader reaches for first.

## Why a gate and not a comment

`condvar_wait` is the obvious spelling. A new backend or a new port adds one
call, the bound is lost, and nothing says so — there is no diagnostic for
"blocked forever", only a hang that reads as a deadlock somewhere else. A
marked header states the rule; only a gate keeps it, and the same class has
recurred here whenever the rule lived only in prose.

`scripts/check-no-unbounded-condvar-wait.sh` (phase-436 W5) refuses any call
outside the one allowed path, across `packages/{core,rmw,api}`. The exemption
is BY PATH, deliberately, so a second unbounded caller anywhere else has to be
argued for rather than inherited. The match excludes the `_until` sibling and
the declarations, so the ports' own definitions do not trip it.

## What would close this

Either the unbounded entry point goes away — every remaining caller bridged
through a deadline the caller owns — or zenoh-pico gains a bounded
`_z_condvar_wait_until` upstream and the last legitimate caller disappears with
it. Until one of those happens the gate is the answer, and this issue is the
record of why the exemption exists.
