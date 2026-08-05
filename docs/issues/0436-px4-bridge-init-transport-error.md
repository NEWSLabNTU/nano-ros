---
id: 436
title: "PX4 bridge module: `nros::init()` returns TransportError(-100) when a networked backend is registered alongside uORB"
status: open
type: bug
area: rmw
related: [issue-0362, phase-325]
---

## Symptom

The phase-325 W3 bridge module (`examples/px4/cpp/bridge`) builds, links and
registers, but fails at startup:

```
pxh> nros_uorb_bridge start
ERROR [nros_uorb_bridge] nros::init() failed: code=-100     # TransportError
ERROR [nros_uorb_bridge] Task start failed (-1)
```

It fails in `nros::init()` — BEFORE the module creates either node, so before any
of the two-session (`NodeBuilder(...).rmw(...)`) code runs.

## What is ruled out

- **Not the link.** `bin/px4` carries the module, the generated CDR symbols
  (`nros_cpp_publish_px4_msgs_msg_debug_key_value`) and BOTH backends' register
  symbols. The W3 gate (one module, two backends) still holds.
- **Not a missing router.** Same `-100` with `zenohd` listening and
  `NROS_LOCATOR=tcp/127.0.0.1:47501` exported into the PX4 process.
- **Not registration order.** The generated stub registers uORB FIRST:
  ```c
  void nros_app_register_backends(void) {
      (void)nros_rmw_uorb_register();
      (void)nros_rmw_zenoh_register();
  }
  ```
  so the default session should be the uORB one — which is exactly what the W2
  demo (`examples/px4/cpp/firmware`, `BACKENDS uorb` only) opens successfully.

## Investigation 2026-08-06 — the error is named, and two mechanisms are confirmed

**The real error is `NodeError::Transport(ConnectionFailed)`.** `-100` is
documented in `node_error_to_cpp_ret` as the catch-all for UNMAPPED variants, so
the C++ caller could not tell a genuine transport failure from anything else.
Widened that seam (std-gated `eprintln!` of the real variant — the issue-0428
move, one layer out); the message above is what it prints.

**Both backends register successfully.** Instrumented the module to call and print
each register's return code before `init()`:

```
INFO  [nros_uorb_bridge] register codes: uorb=0 zenoh=0
```

so the "uORB's register silently fails, leaving zenoh in slot 0" theory is dead.
Note the generated stub DOES discard these codes (`(void)nros_rmw_uorb_register();`)
— worth fixing on its own merits, but not the cause here.

**Registry order is NOT the stub's argument order.** `nros_rmw_register_backend!`
(`nros-rmw-cffi/src/section.rs:55`) installs a `.init_array` ctor on every HOSTED
target (`#[cfg(not(target_os = "none"))]`), and PX4 SITL is posix/hosted — so
zenoh self-registers BEFORE `main`, while uORB registers later, inside
`nros_cpp_init`'s generated stub. `default_vtable()` is literally slot 0
(`cffi/src/lib.rs:973`), and `nros::init()` opens the default session through it.
The two backends also register asymmetrically: uORB via `nros_rmw_cffi_register`
(the literal name `"default"`), zenoh via `nros_rmw_cffi_register_named`.

**A reachable router does NOT fix it.** Tested with `zenohd` on the default 7447
and on a custom port with `NROS_LOCATOR` exported into the PX4 process: same
`Transport(ConnectionFailed)`. So "zenoh is slot 0 and cannot reach a peer" is not
a complete explanation either — the next step is to instrument INSIDE
`CppExecutor::open_in` / the cffi `Session::open` to log which vtable is selected
and where `ConnectionFailed` originates.

## Also found: the bridge and the W2 demo cannot coexist

Building the bridge rebuilds the SHARED `target/release/libnros_cpp.a` with
`--features rmw-zenoh-cffi`. The W2 demo (`BACKENDS uorb`) then fails to LINK
against that same archive — 74 undefined zenoh-pico platform symbols
(`z_clock_*`, `_z_condvar_*`), because a uORB-only module never links
`libnros_rmw_zenoh_staticlib.a`. One path, two incompatible feature variants: the
issue-0360 class that issue 0362 explicitly predicted for this work. Whatever
fixes 0360 should cover the PX4 archive too.

## The original suspicion, now superseded

The difference from the working W2 demo is that a SECOND (networked) backend is
registered. `nros::init()` opens a process-default session; with two backends
registered, something in that default-session path takes the networked transport
(or a per-backend init runs and the zenoh one fails inside PX4's posix/work-queue
context). That is a hypothesis — the error code is the only evidence so far.

A clean discriminator was attempted (rebuild with `BACKENDS uorb` alone) and is
NOT conclusive: the module publishes on the networked backend, so dropping it
fails to link for an unrelated reason. Discriminating properly needs either a
build where the module's outward half is `#if`-ed out, or tracing which backend
`nros_cpp_init` selects.

## Why it matters

This is the last step between the bridge scaffolding and a ROS 2 peer test. The
build, the link, the codegen (issue 0362) and the type hash are all verified; the
module simply cannot start.

## Direction

1. **Instrument `CppExecutor::open_in` / cffi `Session::open`** to log the selected
   vtable and the origin of `ConnectionFailed`. Everything outside that call is now
   accounted for; this is the one unobserved step.
2. **The API gap this exposes, independent of the bug.** `nros::init()` always opens
   a DEFAULT session through slot 0, and cffi's own doc says multi-backend (bridge)
   binaries should use `open_named`. The C++ init path never got that treatment, so
   a bridge cannot say "open no default session" or "open the default session on
   THIS backend" — it inherits whichever backend won the `.init_array` race. That
   is the shape to fix, whatever the immediate cause turns out to be.
3. **Make registration order deterministic, or stop depending on it.** A hosted
   backend self-registering pre-`main` while another registers inside `init()` means
   slot 0 depends on link order, not on the `BACKENDS` list the author wrote.
4. Consider having the generated stub CHECK the register return codes it currently
   discards.
