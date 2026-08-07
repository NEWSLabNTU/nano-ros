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
