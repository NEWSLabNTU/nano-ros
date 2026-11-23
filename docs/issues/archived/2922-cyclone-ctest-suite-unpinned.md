---
id: 2922
title: "The Cyclone C++ ctest suite runs on Cyclone's own interface pick, so a host whose first interface loops no multicast back hangs it in discovery"
status: resolved
type: bug
severity: medium
area: [rmw, cyclonedds, testing, ci]
found: 2026-11-24
resolved: 2026-11-24
related: [1009, 1137, 1139]
---

**Owner:** unassigned.

## What happens

`just check rmw-cyclonedds` (the `build::check::rmw-cyclonedds` gate) timed out
at its 2400 s limit on one host, on every branch. The ctest log stops at test 2:

```
 1/40 Test  #1: nros_rmw_cyclonedds_register_smoke .....   Passed    0.03 sec
      Start  2: nros_rmw_cyclonedds_session_smoke
error: recipe `test` was terminated on line 136 by signal 15
```

Run by hand with Cyclone tracing on, the binary picks the public ethernet
interface and never finishes discovery:

```
using network interface enp0s31f6 (udp/115.43.150.48) selected arbitrarily from: enp0s31f6, docker0, br-...
```

The same binary with `CYCLONEDDS_URI` naming the loopback interface prints
`OK` at once, and the whole suite passed 40/40 in 137 s that way.

## Why

Issue 1009 confined every DDS test bus to loopback with a profile FILE, and
issue 1137 / 1139 carried it to our own Rust spawns and to the two shell e2e
cells. The C++ ctest suite in `nros-rmw-cyclonedds/tests` starts Cyclone in
every test, from CMake, and was neither: its 39 binary tests ran on Cyclone's
own interface pick. Whether that pick loops multicast back to the same host is
a property of the host's network, so the suite's result was too.

There were also two copies of the loopback profile: the Rust string in
`nros_tests::dds_isolation` and a hand-kept mirror in `ros2_e2e_common.sh`.

## Resolution

Fixed 2026-11-24.

- The loopback profile is one committed file,
  `packages/testing/nros-tests/dds/cyclone-loopback.xml`. The Rust harness reads
  it with `include_str!`; the shell pin (`nros_export_cyclone_config`, moved to
  `tests/dds_loopback.sh`) points `CYCLONEDDS_URI` at it, and rewrites a copy
  only for the `NROS_E2E_LOOPBACK_*` negative control.
- Every ctest is registered through `nros_rmw_cyclonedds_add_test`, which runs
  it under `tests/dds_loopback_exec.sh`. The launcher applies the Rust rules:
  `NROS_DDS_ALLOW_LAN` pins nothing, an operator's `CYCLONEDDS_URI` is kept,
  otherwise the committed profile.
- Gate: `check-dds-isolation-symmetry` has a ctest arm. A tracked CMake file
  that names `nros_rmw_cyclonedds` and registers tests with a bare `add_test`
  is refused, and so is a helper that does not route through the launcher.
  Self-test cases cover both, plus a non-Cyclone suite and a commented-out
  registration.
- The launcher pins through `CYCLONEDDS_URI`, which Cyclone reads only when a
  domain is created without a config. `config_compose` calls
  `dds_create_domain` with its own strings, so it stayed unpinned under the
  launcher: 12 runs in 40 hung in `dds_delete` -> `rtps_stop` ->
  `wait_for_receive_threads`, joining a `recvMC` thread whose wake-up never
  looped back, and the full `ci gate` hit the 2400 s timeout there. Its three
  call sites now pass a buffer from `nros_test_pinned_cyclone_config`
  (`tests/nros_test_domain.h`), which puts the pin first so the test's own
  fragments still override it. Measured: 0 hangs in 60 runs under the launcher,
  6 in 20 with `NROS_DDS_ALLOW_LAN=1`. The gate's ctest arm also reads tracked
  C/C++ sources under `tests/`: a `dds_create_domain` whose config is not a
  helper-filled buffer (or null) is refused, with self-test cases for both,
  a null config, an expression config, a comment and a string literal.
