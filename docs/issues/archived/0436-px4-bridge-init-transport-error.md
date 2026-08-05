---
id: 436
title: "PX4 bridge module: `nros::init()` returns TransportError(-100) when a networked backend is registered alongside uORB"
status: resolved
type: bug
area: rmw
related: [issue-0362, phase-325]
resolved_in: one-zenoh-pico + nros_cpp_init_multi
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

## RESOLVED-AS-DIAGNOSED 2026-08-06 — not one bug: a misuse, two bugs, and a gap

Answering the question directly (design issue / gap / bug): **the design is sound
and the bridge shape is a first-class, already-implemented feature. My module used
the wrong API, and on the way there it hit two real defects and one integration
gap.**

**1. MISUSE (mine).** nano-ros has a multi-RMW bridge surface —
`nros::MultiExecutor` + `SessionSpec` (`nros-cpp/include/nros/bridge.hpp`, phase-128
F.5) over `nros_init_multi`, backed by `Executor::open_multi_in` + `extra_sessions`
+ per-node `session_idx` (phase-104 C.3, landed). Sessions are opened UP FRONT from
a spec list; `NodeBuilder::rmw(name)` then binds a node to one that ALREADY exists.
The module followed the phase-325 W3 note (`nros::init()` + `NodeBuilder`), which is
the SINGLE-session shape — so there was never a zenoh session for the outward node
to bind to. That is why the outward bind failed even with a router up.

**2. BUG — uORB registered under the wrong name (FIXED).** `nros_rmw_uorb_register`
used `nros_rmw_cffi_register`, the `#[deprecated]` unnamed shim, which registers the
literal name `"default"` ("use `nros_rmw_cffi_register_named` with the backend's
canonical name" — its own note). Every other backend uses the named form. So uORB
could not be selected as `"uorb"` by EITHER handle: `NodeBuilder().rmw("uorb")` or
`$NROS_RMW=uorb`. Fixed to `register_named("uorb", &kVtable)`. Single-backend images
are unaffected (`resolve_backend` returns the sole entry regardless of name).
VERIFIED: after the fix `NROS_RMW=uorb` resolves and the inward uORB bind succeeds.

**3. BUG — a selection outcome reported as a transport failure (FIXED).**
`Executor::open_in` mapped every non-`Single` resolution — `Ambiguous`, `NoBackend`,
`Unknown` — to `Transport(ConnectionFailed)`. Two registered backends and no
selector is "you must disambiguate", not a network error: it reads as a router
problem and was chased as one (zenohd on two ports, `NROS_LOCATOR`, all irrelevant).
Now `Transport(InvalidConfig)` plus a std-gated line naming the outcome and the
remedy. The `-100` catch-all at the C++ seam is likewise widened to print the real
`NodeError`.

**4. GAP (the remaining work).** `MultiExecutor` requires linking `libnros_bridge.a`
(the `packages/rmw/bridge` crate), and `nros_px4_add_module` has no notion of it —
zero mentions. So a PX4 module cannot use the supported bridge API today. That is
the actual phase-325 W3 gap: not the runtime, not the codegen (issue 0362, done),
but the PX4 link helper. It is small: teach the helper to resolve + link the bridge
archive the way it already does for `libnros_cpp.a`, `libnros_platform_posix.a` and
the zenoh platform archive.


## Update 2026-08-06 (b) — gap closed, module ported, frontier moved into PX4

**The GAP is closed.** `nros-cpp` gained a `bridge` feature (+ a force-link anchor,
because the rlib's `#[no_mangle]` exports were DCE'd out of the staticlib — the
CLAUDE.md FORCE_LINK class; verified: the archive first carried `U nros_init_multi`
with nothing to satisfy it). `libnros_cpp.a` now DEFINES `nros_init_multi`,
`nros_fini_multi`, `nros_pubsub_bridge_create`, so `<nros/bridge.hpp>`'s
`MultiExecutor` is reachable from a PX4 module — previously the header existed but
no archive carried its symbols.

**The module is ported** to the supported shape (`MultiExecutor` + `SessionSpec`,
both sessions up front, `create_node_on` / `NodeBuilder` per node). It compiles and
LINKS into PX4 SITL.

Three more things fixed on the way, each the same "collapsed error" class:
* `nros_init_multi` discarded the cause (`Err(_) => NROS_RMW_RET_ERROR`) — now names
  it (std-gated).
* `nros-cpp` `std` did not forward to `nros-bridge`, so those diagnostics compiled
  out; forwarded via `nros-bridge?/std`.
* `<nros/bridge.hpp>` is gated on `NROS_CPP_STD` (deliberately, issue 0332), which
  a PX4 module does not define — the module now opts in explicitly.

**Also found, worth its own fix:** `nros_init_multi` does NOT call the generated
`nros_app_register_backends()`, while `nros_cpp_init` (i.e. `nros::init()`) does. So
on the MultiExecutor path the registry is EMPTY unless the caller registers first.
The module now calls it explicitly; the asymmetry should probably be removed inside
`nros_init_multi`.

**Frontier: the failure is now inside PX4, not in nano-ros's API.** With backends
registered, both sessions named, `mode = Client` and a real locator
(`tcp/127.0.0.1:7447`, `zenohd` up), `nros_init_multi` returns
`Transport(ConnectionFailed)` — i.e. a SESSION OPEN failed. uORB's `session_create`
cannot meaningfully fail (it mallocs and stashes), so it is the zenoh open. zenoh
demonstrably works on this host (the native talker/listener pair published over a
router minutes earlier), so **PX4 is the remaining variable** — most likely its
lockstep simulated clock or the work-queue thread context that zenoh-pico's connect
runs in. That is the next thing to test (e.g. open the same session from a plain PX4
task vs a work-queue item, and with lockstep disabled).


## Update 2026-08-06 (c) — isolated to zenoh-in-PX4, with a strong lead

Added a single-session probe (`NROS_BRIDGE_PROBE=uorb|zenoh`) to the module, which
splits the question cleanly:

| probe | result |
| --- | --- |
| `uorb` only | MultiExecutor OPENS fine (fails later only at the zenoh bind, as expected) |
| `zenoh` only | `nros_init_multi failed — Transport(ConnectionFailed)` |

So it is **not** the bridge API and **not** multi-session: a zenoh session simply
cannot be opened inside PX4. `NROS_RMW_TRACE_OPEN=1` gives the backend's own code:

```
[nros-rmw-cffi] open: locator="tcp/127.0.0.1:7447" mode=0 ret=-18 backend_data=0x0
```

`-18` is `NROS_RMW_RET_CONNECTION_FAILED`, `mode=0` is `Client`, the locator is
right, and `zenohd` is listening. **The router logs no incoming connection at all**,
so zenoh-pico never reaches the network.

Ruled out, each by test rather than argument:
* **Stack** — `STACK_MAIN 32768` changes nothing.
* **Missing platform layer** — all 1429 zenoh-pico symbols are in `bin/px4`,
  including `_z_open_socket` / `_z_open_link`.
* **Compile config** — the staticlib's `zenoh_generic_config.h` has
  `Z_FEATURE_LINK_TCP 1`, and is IDENTICAL (`Z_FEATURE_MULTI_THREAD 0`) to the one
  the WORKING native talker uses.
* **The host / the router** — a native nros client connects to the same router
  seconds later ("nros: session open").

**Strong lead: the image links TWO complete copies of zenoh-pico.**

```
libnros_cpp.a          : 1465 z_* symbols, defines _z_open_socket
libnros_rmw_zenoh_staticlib.a : 1576 z_* symbols, defines _z_open_socket
```

`libnros_cpp.a` carries the zenoh backend (built `--features rmw-zenoh-cffi`) AND
its bundled zenoh-pico; the separate platform archive carries another. The linker
picks one definition per symbol, but each copy has its own `static` state — the
`#48` split-registry class, one layer down, and exactly the hazard CLAUDE.md
records for the zpico shim ("shim + library MUST share the generated config … a
mismatched TU is a silent ABI break"). A session whose config/state lives in copy A
while the socket call resolves to copy B would fail to connect while emitting
nothing on the wire, which is precisely what is observed.

Next: confirm by checking whether the two copies' objects differ (they are built by
different cargo invocations with different feature sets), then decide the fix —
most likely the PX4 link should take zenoh-pico from ONE archive, i.e. either the
umbrella carries it and the platform archive is dropped, or the umbrella is built
without the bundled copy. Note `nros_px4_add_module` currently REQUIRES the separate
zenoh archive (its guard names it), so the fix is a helper change too.



## RESOLVED 2026-08-06 — the bridge works end to end

```
INFO  [nros_uorb_bridge] bridging /fmu/out/debug_key_value (uorb) -> /fmu/out/debug_key_value (zenoh)
INFO  [nros_uorb_bridge] forwarded 100 samples (key=velx value=99.0)
```

Live uORB samples (from `px4_mavlink_debug`) read in-firmware, translated
field-by-field into the generated CDR `px4_msgs` type, and published on zenoh — the
phase-325 W3 goal, on the codegen issue 0362 delivered.

**The last blocker: two incompatible executor handles behind one `void*`.**
`nros_cpp_node_create_with_options` casts its handle to `*mut CppContext`
(`{ executor, domain_id, in_dispatch, backing }`), while a `MultiExecutor` handle is
the `ExecutorBox { executor, _spec_strings: Vec<String> }` that `nros_init_multi`
boxed. `CppExecutor` IS `Executor<'static>` and both structs put it at offset 0
(CppContext documents "keep `backing` last so `executor` stays at offset 0"), so the
cast reads correctly — and then writes `domain_id`/`in_dispatch` over the bridge
box's `Vec`. PX4 dumped core during construction.

Fixed by giving the C++ surface its own multi-init: `nros_cpp_init_multi(specs,
len, storage)` opens one session per spec into the CALLER'S storage with the same
contract as `nros_cpp_init`, producing a real `CppContext`. Every existing C++ path
— `nros::Node`, `NodeBuilder().rmw(name)`, publishers, subscriptions — then works
against a multi-session executor unchanged, and there is no second handle type to
confuse. It also calls `nros_app_register_backends()`, which `nros_init_multi` does
not (recorded above as its own trap).

`<nros/bridge.hpp>`'s `MultiExecutor` remains valid for the C bridge API
(`nros_pubsub_bridge_*`, raw sample forwarding); it is simply NOT the handle the C++
Node API takes. Worth a follow-up: give the two handles a type tag so mixing them is
a clean error rather than memory corruption.

## Update 2026-08-06 (d) — the duplicate WAS the bug; two fixes landed, one blocker left

**Confirmed and FIXED: two copies of zenoh-pico.** The umbrella carried zenoh-pico's
1465 CORE symbols but none of the 111 PLATFORM ones (`z_clock_*`, `_z_condvar_*`,
`_z_task_*`, socket shims) — because it was built without a platform feature. The
separate `libnros_rmw_zenoh_staticlib.a` supplied those, but it is a COMPLETE
zenoh-pico (1576), so the image got two cores. Each copy owns its statics, so a
session opened against one made socket calls resolving into the other: `z_open`
returned CONNECTION_FAILED having put nothing on the wire.

The SSoT fix is a single source, not a second archive — build the umbrella WITH a
platform feature and its zenoh-pico is complete:

    cargo build -p nros-cpp --no-default-features \
        --features std,rmw-zenoh-cffi,platform-posix --release   # → all 1576

`nros_px4_add_module` no longer links the second archive (it warns if
`NROS_ZENOH_ARCHIVE` is still set), and `just px4 build-bridge-example` builds the
complete umbrella. **VERIFIED: a zenoh session now OPENS inside PX4** —
`[nros-rmw-cffi] open: locator="tcp/127.0.0.1:7447" mode=0 ret=0` where it was
`ret=-18`. This is the duplicate-symbol class the project bans outright, one layer
below the archives `check-no-allow-multiple-def` inspects.

**Also FIXED: `open_multi`'s extra sessions were anonymous.** `NodeBuilder::rmw(name)`
could only recognise an extra session by finding a NodeRecord already bound to it, so
the FIRST node naming a backend always missed and fell through to "open a new
session" — a SECOND session against a singleton backend, with an empty locator.
Executor now carries `extra_session_ids` (the extras' `(rmw, locator)`, mirroring
`primary_rmw_name`/`primary_locator`), populated by `open_multi_in` and by the
open-a-new-session path, and consulted first in `resolve_session_slot`.

**Remaining blocker — the C++ bridge and Node surfaces do not compose.**
`nros_cpp_node_create_with_options` does `&mut *(executor_handle as *mut CppContext)`,
but a `MultiExecutor` handle is the `ExecutorBox` that `nros_init_multi` boxed. Both
happen to start with an `Executor`, so the cast "works" far enough to proceed and
then corrupts memory: PX4 now **dumps core** during MultiExecutor construction
instead of failing cleanly. `<nros/bridge.h>` documents the bridge handle as input to
`nros_create_node_on` (the C entry point), so the C++ `nros::Node` / `NodeBuilder`
path — which assumes a `CppContext` — cannot be used with it.

Direction: give the two surfaces one handle type (or make the C++ Node path accept a
bridge handle explicitly), and add a type tag so mixing them is a clean error instead
of a core dump. Until then a C++ uORB→RMW bridge cannot create nodes on its sessions,
and `examples/px4/cpp/bridge` stays a documented work-in-progress.

## Investigation trail — the error is named, and two mechanisms are confirmed

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
