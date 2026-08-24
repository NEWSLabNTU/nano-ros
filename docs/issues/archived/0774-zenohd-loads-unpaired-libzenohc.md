---
id: 774
title: "`rmw_zenohd` loads whatever `libzenohc.so` the loader finds, so a router
  that resolves fine SEGVs at startup — 13 tests red on a host that has ROS"
status: resolved
type: bug
area: testing
related: [rfc-0075, issue-0609, issue-0653]
resolved_in: "phase-374"
---

## Symptom

Thirteen router-backed tests fail, all with the same message and none of it
about zenoh versions:

```
thread 'eventfd_write_unblocks_spin_once' panicked at
packages/testing/nros-tests/src/fixtures/zenohd_router.rs:475:19:
the ROS zenoh router failed to start: Process failed: zenohd exited before
listening on tcp/127.0.0.1:32873 with signal: 11 (SIGSEGV) (core dumped)
```

`nros-tests::{signal_fd_wake, wake_latency, component_runtime,
component_dispatch, component_param, trigger_conditions, tier_filter}` — the
whole `check-required-features-tests` lane, 13 of 20.

Reproduces on a host where ROS **is** installed and `rmw_zenohd` **is** found.
Starting the same router by hand works, which is what makes it confusing.

## Cause

`rmw_zenohd` links `libzenohc.so` by SONAME, so which one it loads is decided
by the loader, not by which router the fixture resolved. ROS ships its own at
`<prefix>/opt/zenoh_cpp_vendor/lib/libzenohc.so`, and that directory is on
`LD_LIBRARY_PATH` only if the caller sourced `setup.bash` — or this repo's
`activate.sh`, which adds the same entry.

Without it, any other `libzenohc.so` on the default search path wins. On the
host this was found, a stray `/lib/libzenohc.so` (17 MB, owned by no dpkg
package, four months older than the ROS one) took precedence. A zenoh C library
the router was not built against does not fail to load — it SEGVs partway
through startup.

So the fixture's own rule ("a host with no ROS router SKIPS; a router that is
present and will not start is a real fault") classified this correctly as a
fault, and was still unhelpful: the fault was in the environment, and nothing
in the message said so.

The resolver answers "where is a router binary?". Running one needs a second
property — "will it load the zenoh it was built against?" — and only the first
was ever checked. Same drift RFC-0075 exists to prevent (issue 0609 measured it
for the router's own version pin), arriving one layer down through the loader
instead of through a pin.

This is also why the lane looked like a code red on `main` while `just check`
run from a shell that HAD sourced `activate.sh` was green: the failure tracked
the caller's environment, not the tree.

## Fix

`zenohd_router::paired_zenoh_library_dir` derives the vendored zenoh directory
from the resolved router path (`<prefix>/lib/rmw_zenoh_cpp/rmw_zenohd` ->
`<prefix>/opt/zenoh_cpp_vendor/lib`) and the fixture prepends it to the child's
`LD_LIBRARY_PATH`. The pairing is pinned by the fixture instead of inherited
from whoever invoked it.

Absent directory means the inherited environment stands: a layout with no
vendored zenoh beside the router is not ours to second-guess.

Verified by running the lane from a shell that never sourced `activate.sh` —
the exact condition that produced the SEGV: 20/20 pass, where 13 failed before.
`paired_zenoh_library_dir` has a unit test over a synthetic prefix, so it
asserts the derivation rather than "this host happens to have one".

## Notes

An orphaned `rmw_zenohd` holding the default 7447 for nearly three hours turned
up alongside this — the class `kill_listeners_on_port` already handles, but only
for the port a test actually asks for, so an orphan on 7447 survives a run whose
routers all take ephemeral ports. Cleared manually; no code change.
