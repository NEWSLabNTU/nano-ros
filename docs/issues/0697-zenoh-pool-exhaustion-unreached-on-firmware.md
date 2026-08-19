---
id: 697
title: "The zenoh session-pool exhaustion path is hardened for firmware and reached only on native — no embedded build raises `ZPICO_MAX_SESSIONS`, and nothing tests the `Full` error"
status: open
type: tech-debt
area: rmw/zenoh
related: [issue-0348, issue-0465, issue-0589, issue-0393]
---

## What is missing

`zpico.rs`'s pool-exhaustion arm returns `ZpicoError::Full` and logs the one
message that explains the failure. Issue 0589 deliberately moved that log from
`std::eprintln!` to `nros_log` **so it would reach `no_std` targets**, noting
that the old `cfg(feature = "std")` arm

> left the pool exhaustion mute — and firmware is where a fixed-size pool
> actually fills.

The pool is a fixed-size static array (`ZPICO_MAX_SESSIONS`, default 1;
phase-328 / issue 0348). So the reasoning is right: firmware is where it fills.

But nothing on firmware ever reaches it.

- **No embedded build raises the knob.** `grep -rn ZPICO_MAX_SESSIONS zephyr/
  packages/boards/ config/` is empty; so is every `[[fixture]]` row and every
  example `CMakeLists.txt`. Only `just test-zpico-multisession` sets it, and
  that lane is native posix + zenoh.
- **Nothing tests the `Full` error on any platform.** The single reference
  outside the backend is a doc comment in `zenoh_integration.rs` explaining
  that the test SKIPS when the pool is 1 — i.e. the exhaustion arm is what the
  test avoids, not what it asserts.

So a diagnostic was hardened for a target class that never executes it, and the
error path it guards has no test at all.

## What this is NOT

Worth stating, because the obvious remedies are both wrong:

- **Not a matrix axis.** Multi-session is the BRIDGE shape, not a configuration
  users deploy — the backend's own message says "a non-bridge application opens
  exactly ONE session". An axis would multiply every coordinate in the fixture
  space for a configuration nothing ships.
- **Not a raised default.** Issue 0465 considered exactly that and rejected it:
  "costs pool memory on every embedded target to fix a hosted-porting path — the
  wrong trade for a `no_std` project." That issue's real consumer (the phase-209
  rclcpp shim, which opened two sessions) was fixed by sharing one session
  instead, so no shipped shape needs ≥2 today. Checked while filing: the
  zenoh→cyclone bridge opens one ZENOH session plus a Cyclone participant, a
  different backend and a different pool.

The existing single lane is the right size for the native probes. What it cannot
cover is the `no_std` arm.

## Direction

ONE embedded cell, not a lane family: a firmware image built with
`ZPICO_MAX_SESSIONS=1` that deliberately opens a second session, asserting the
`Full` return AND that the message actually appears on the console. The second
half is the point — 0589's fix was about the message reaching a target where
`std` stdio is fatal, and only a target can prove it does.

Cheapest home is probably the FreeRTOS or ThreadX runtime cell that already
boots a zenoh image, since the assertion is a console grep on a path the image
takes deliberately.

Until then, note the honest state: 0589 hardened a diagnostic nobody has
observed on the platform it was hardened for.
