---
id: 697
title: "The zenoh session-pool exhaustion path is hardened for firmware and reached only on native — no embedded build raises `ZPICO_MAX_SESSIONS`, and nothing tests the `Full` error"
status: resolved
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

## Resolution

A cell on `threadx-linux` — `packages/testing/nros-tests/bins/pool-exhaustion-threadx-linux`,
asserted by `tests/pool_exhaustion_firmware.rs`. Both halves, measured:

```
[ERROR] nros: zenoh session pool exhausted — this build allows ZPICO_MAX_SESSIONS=1…
[INFO]  nros: pool-exhaustion: second session refused with Full
```

The first line is the backend's diagnostic reaching the console of a `no_std`
image — the thing issue 0589 hardened and nothing had ever executed. The second
is `Full`, distinguished from a transport error (issue 0465).

### Two design choices worth the lines

**No router, and that is deliberate.** The obvious shape — open a real first
session, then a second — cannot work here: `Context::new` takes a slot before any
I/O and RELEASES it on failure, so a first session that cannot reach a router
leaves the pool EMPTY and the second open succeeds for the wrong reason. Tried it
first; on this board the network is NetX's stack rather than the host loopback and
the first open failed with `Session`. The fixture instead exhausts the pool with a
raw `zpico_session_acquire()` — the precondition — and still reaches the arm under
test through `Context::new`. It is deterministic and needs nothing running.

**It refuses to report a false pass.** If the raw acquire fails, the pool is not
full and the run proves nothing; the image says so and exits non-zero rather than
printing a verdict. That is the arm that fired during development.

### It could not have passed before 0708

This family's boot funnel never published an `nros_log` sink list, so the record
was constructed, dispatched and dropped before any console. That is why the cell
this issue asked for did not exist, and it is now a second thing the cell guards.

### Mutation-checked, both assertions

* mute the backend's `nros_error!` (the 0589 regression) → FAIL
* mute `init_default()` in all four ThreadX funnels (the 0708 regression) → FAIL
* restore → PASS

### Also folded in

The exhaustion block was duplicated BYTE FOR BYTE in `Context::new` and
`Context::with_config` — 25 lines each, comments included. Now one
`acquire_session_slot()`, and the message is a named constant
(`SESSION_POOL_EXHAUSTED_MARKER`, mirrored as
`nros_tests::output::ZENOH_SESSION_POOL_EXHAUSTED`) so the grep is not a literal.
Two copies of a message a test pins is two things to keep in step, and the test
would have pinned only one.

### Not done

Only `threadx-linux` carries the cell — it is the one host-runnable `no_std`
board, so it is the one where this is cheap to keep honest. FreeRTOS and NuttX
run the same arm through the same code and are not separately asserted.

## Direction (as filed)

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
