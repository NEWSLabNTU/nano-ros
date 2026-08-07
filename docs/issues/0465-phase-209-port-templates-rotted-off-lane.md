---
id: 465
title: "Phase 209's C++ port templates are in NO lane, and the acceptance one exists to prove no longer runs"
status: open
type: bug
severity: medium
area: testing, cpp, cmake
related: [phase-209, issue-0317, issue-0196]
---

## Finding

`examples/templates/cpp-port-minimal-publisher` is phase 209's acceptance
artifact. Its README states the claim plainly:

> The acceptance for 209 lands here: a normal ROS 2 C++ node compiles + links +
> **runs** against nano-ros by swapping the build glue + zero `#include` edits.

Two of those three still hold. It does not run.

```console
$ cmake -S examples/templates/cpp-port-minimal-publisher -B /tmp/p209   # rc=0
$ cmake --build /tmp/p209 -j8                                          # rc=0
$ build/zenohd/zenohd -l tcp/127.0.0.1:7447 &                          # README's own steps
$ /tmp/p209/minimal_publisher
nros: NodeError::Transport(ConnectionFailed)
```

Expected (README): `Publishing: 'Hello, world! 0'` …

## It is the template, not the host

A shipped example against the SAME router, same moment, connects:

```console
$ NROS_LOCATOR=tcp/127.0.0.1:7447 examples/native/cpp/talker/build-zenoh/cpp_talker
nros C++ Talker
===================
Node created: talker
```

Router confirmed up (`ss -ltn` shows 7447; zenohd logs "Zenoh can be reached at
tcp/127.0.0.1:7447").

Also ruled out — the usual `Transport(ConnectionFailed)` cause, a backend that
was never registered (issue 0155's class), is NOT this. Both binaries carry the
backend and the hook:

| binary | `nros_rmw_zenoh` symbols | `nros_app_register_backends` |
| --- | --- | --- |
| `minimal_publisher` (template) | 154 | 1 |
| `cpp_talker` (shipped, works) | 133 | 1 |

Tried and still failing: the baked locator (`tcp/127.0.0.1:7447`, the only one in
the binary), `NROS_LOCATOR` pointing at a router on another port, and both
`NROS_SESSION_MODE=client` and `=peer`.

## Why it rotted: no lane owns it

None of phase 209's three port templates is referenced by any fixture row, test,
or recipe:

```console
$ for t in cpp-port-minimal-publisher rclcpp-compat-smoke topic-state-monitor-port; do
    git grep -l "$t" -- examples/fixtures.toml packages/testing just scripts | wc -l
  done
0
0
0
```

They are named only by docs, `examples/templates/README.md`, and themselves. So
nothing has executed the acceptance since it was written (2026-05-30), and the
phase has read "MVP DONE" ever since.

This is issue 0317's shape (the wake-latency bench rotted off-lane) and the
issue-0196 rule (a gate narrower than the invariant it enforces) — here there is
no gate at all. `just check` compiles examples, which is why the build half of
the claim stayed true while the run half died silently.

## Diagnosis (2026-08-07) — the rclcpp shim needs TWO sessions, the pool ships with ONE

`NROS_RMW_TRACE_OPEN=1` on both binaries, same router, same moment:

```
TEMPLATE (fails)
  [nros-rmw-cffi] open: locator="" mode=0 ret=0  backend_data=0x5e17c77d6000
  [nros-rmw-cffi] open: locator="" mode=0 ret=-1 backend_data=0x0
  nros: NodeError::Transport(ConnectionFailed)

CONTROL (works)
  [nros-rmw-cffi] open: locator="tcp/127.0.0.1:7447" mode=0 ret=0 backend_data=0x5c68118f3030
```

The template opens the session **twice**, and the SECOND open fails. Not a
connection problem at all — the first open succeeds against the same router.

Both opens are by design, and `rclcpp_compat.hpp` says so in its own comments:

* `rclcpp::init(argc, argv)` → `::nros::init()` — session #1 (`:238`).
* the `Node` shim "owns its own `nros::Executor` (the typical
  single-node-per-process pattern) … each currently gets its own Executor"
  (`:256-260`), whose `Executor::create` (`:330`) opens session #2.

`ZPICO_MAX_SESSIONS` defaults to **1** (`nros-zpico-build/src/runner.rs:50`,
`env_usize("ZPICO_MAX_SESSIONS", 1)`). So the second open exhausts the pool and
returns `-1`, which surfaces as `Transport(ConnectionFailed)` — the same error
text a genuine connection failure produces, which is why it read as one.

**Confirmed by rebuilding the template unchanged, with a 2-session pool:**

```console
$ ZPICO_MAX_SESSIONS=2 cmake -S examples/templates/cpp-port-minimal-publisher -B /tmp/p465
$ ZPICO_MAX_SESSIONS=2 cmake --build /tmp/p465 -j8
$ build/zenohd/zenohd -l tcp/127.0.0.1:7447 &
$ /tmp/p465/minimal_publisher
  open: locator="" ret=0 backend_data=0x…e77000
  open: locator="" ret=0 backend_data=0x…e78480      <- both succeed now
[INFO] …/minimal_publisher.cpp:48 Publishing: 'Hello, world! 0'
[INFO] …/minimal_publisher.cpp:48 Publishing: 'Hello, world! 1'
```

Exactly the output the README promises. The C++ source is still unmodified and
the three glue lines are unchanged — so 209's *porting* claim holds; what does
not hold is that the shipped defaults can run it.

### Why it worked in May and not now

The double-open is inherent to the shim's one-Executor-per-Node design, so this
needed `ZPICO_MAX_SESSIONS >= 2` from the start. The pool itself arrived with
phase-328 / issue 0348 (multi-session support, default 1); before that the shim
presumably shared a session. Either way the acceptance has not been executed
since, so nothing noticed the default stopped being sufficient.

### Fix options

1. **Make the shim share one session.** `Executor::open_over_session` already
   exists for exactly this (the borrowed-tier pattern), and the shim comment
   flags the current behaviour as a default rather than a requirement. Cheapest
   for users: no build knob, one session per process, matches rclcpp's own
   process-level context model.
2. **Raise the default to 2.** Costs pool memory on every embedded target to fix
   a hosted-porting path — the wrong trade for a `no_std` project.
3. **Document the knob in the template's CMakeLists.** Honest but leaves a
   stock rclcpp node failing out of the box, which is the friction 209 exists to
   remove.

(1) is the one that keeps 209's promise. Whichever lands, the lane from the
section above is what stops it rotting again — and note a build-only row would
NOT have caught this: the template built and linked cleanly throughout.

## Fix direction

Two parts, in order:

1. **Diagnose the runtime failure.** The build glue is three lines
   (`add_subdirectory` of the checkout, `NrosRclcppCompat.cmake`,
   `nros_generate_interfaces`), so the divergence from the working `cpp_talker`
   is narrow. `NROS_RMW_TRACE_OPEN=1` on both and diff is the obvious first
   step; the compat path's session/config construction is the suspect, since
   linkage and registration are identical.
2. **Give the templates a lane**, or the next repair rots the same way. The
   cheapest honest option is a `compile-check` row plus a runtime cell for the
   minimal publisher — it is a native posix binary and a router, the same shape
   `matrix::CELLS` already runs. A build-only row would re-create exactly the
   blind spot that hid this.

Until then phase 209's status line overstates: the MVP builds, it does not run.
