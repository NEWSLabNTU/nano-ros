---
id: 864
title: "the board presents the SAME zenoh id on every boot — CONFIG_TEST_RANDOM_GENERATOR
  makes the zid deterministic, which confounds every reconnect measurement"
status: resolved
type: bug
area: platform, rmw
related: [issue-0852, issue-0839]
---

## Problem

Two consecutive resets of the same image, reading the liveliness key the board
registers:

```
boot 1 zid: @ros2_lv/10/1322740661b45746fa29b1803f32f5eb
boot 2 zid: @ros2_lv/10/1322740661b45746fa29b1803f32f5eb
```

Byte-identical. A third capture from an earlier session in the same day carries
the same value, so this is stable across power cycles and reflashes, not just
back-to-back resets.

## Cause

```
CONFIG_TEST_RANDOM_GENERATOR=y
CONFIG_TIMER_RANDOM_GENERATOR=y
```

No hardware entropy is configured, so Zephyr supplies the timer-backed stand-in.
zenoh-pico draws the zid at a fixed point early in startup, and the boot path to
that point is deterministic, so the "random" seed is the same tick count every
time. The name of the Kconfig says what it is: a stand-in for tests.

## Why it matters beyond neatness

A zenoh id is the identity a router uses to tell one peer from another and to
hold a session's state for a lease. A board that reboots into the SAME zid is,
from the router's point of view, a peer that vanished and came back — while the
router may still be holding the previous session, its declarations and its
liveliness tokens under that identity.

This makes every reconnect-shaped measurement depend on **what the router
remembers**, not only on what the board does now. Concretely, it invalidated an
A/B during [issue 0852](0852-zephyr-serial-rx-is-polled-and-overruns.md):
polled RX and interrupt-driven RX appeared to differ in whether the ROS graph
populated, and the two runs were not comparable because both boards claimed the
same identity against routers with different histories. The same confound sits
underneath [issue 0839](0839-action-image-session-expires-every-20s.md), whose
whole subject is what happens across a session expiry and reopen.

Two boards of the same model running the same image would also collide
outright.

## Fix direction

Seed from something actually unique to the part. The S32K344 has a factory
device ID in its UTEST/SIUL area; hashing that with a boot counter or the
low bits of a free-running timer gives a per-part, per-boot value without
needing a TRNG.

Prefer wiring a real entropy source (`CONFIG_ENTROPY_GENERATOR`) if the SoC
exposes one on this package — then `CONFIG_TEST_RANDOM_GENERATOR` can go away
rather than be improved.

Either way the acceptance test is the one above: reset twice, read the zid
twice, and require them to differ.

## Note

`CONFIG_TEST_RANDOM_GENERATOR` was already recorded as a production gap. What
is new here is that it has a measurable functional consequence today, in the
one area the serial campaign is trying to characterise, rather than being a
security-hardening item to fix before shipping.

## Resolved (2026-08-28)

`nros-platform-zephyr` now seeds a xoshiro128** PRNG and serves
`nros_platform_random_*` from it — but **only** when
`CONFIG_TEST_RANDOM_GENERATOR` is set and `CONFIG_ENTROPY_GENERATOR` is not. A
board with a real entropy source keeps using it; this must not shadow good
entropy with a PRNG.

The seed mixes, through splitmix32:

- a `__noinit` word carried across reset (Zephyr does not zero `.noinit`), which
  is the previous boot's state on a warm reset and SRAM power-up noise on a cold
  one — this is what makes reset-to-reset differ
- `k_cycle_get_32()` and `k_uptime_get_32()`
- the timer generator's own reading, which is at least not worse

A different value than the one used for this boot is carried forward, so a reset
that happens before any random is drawn still moves the state. The all-zero
xoshiro state is a fixed point and is guarded against.

`nros_platform_random_fill` draws from the SAME stream rather than falling
through to `sys_rand_get` — a `fill` that disagrees with the scalar draws about
which generator is in use is exactly how a zid stays deterministic while
everything else looks fine.

### Acceptance

The test this issue asked for, three times:

```
boot 1 zid: 06f6a004dc9321cfda5fb1359a10e61e
boot 2 zid: ac181f9c54a245950bca25d7ee814faa
boot 3 zid: 9b1311b02b0e4c6a64fb5544281b75dc
```

Previously `1322740661b45746fa29b1803f32f5eb` on every boot.

Cost: 24 B of RAM.

### What this is NOT

Not a CSPRNG, and the code says so at the top of the block. It makes an
IDENTITY unique; it does not make a secret unguessable. The production gap —
this part has no hardware entropy wired — is unchanged, and anything needing
cryptographic randomness still needs it fixed.
