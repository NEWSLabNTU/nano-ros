---
id: 1634
title: "An embedded Cyclone session dies when the user's config enables tracing
  without naming a file: Cyclone opens `cyclonedds.log`, which an RTOS cannot"
status: resolved
type: bug
area: [rmw, cyclonedds, embedded]
severity: medium
found: 2026-10-02
related: [1624, phase-206]
---

## What

Found while booting `workspace-cpp-mps3-an536-freertos` for issue 1624. The
image reached `Network ready` and then:

```
         2.431000 [0]        app: cyclonedds.log: cannot open for writing
[ERROR] nros: [    2.445000] RMW session open failed — Backend("rmw_ret error")
[ERROR] nros: [    2.447000] nros: NodeError::Transport(Backend("rmw_ret error"))
```

Same on QEMU slirp and on a `-net socket,mcast` LAN, so not the network.

## Cause

`examples/workspaces/cpp/src/demo_bringup/rmw/cyclonedds.xml` (phase-206 W2,
the bringup's own config, baked into every image that bringup builds) says
`<Tracing><Verbosity>warning</Verbosity></Tracing>` and names no file.
Cyclone's default `<OutputFile>` is `cyclonedds.log`, and `q_init.c` opens it
whenever any trace category is on; on a target with no filesystem `fopen`
fails, `dds_create_domain` fails, and `session_create` returns a bare
`NROS_RMW_RET_ERROR`. The message that names the cause goes to the console
once, above an error that names the transport.

So any user config that merely turns tracing up kills every embedded Cyclone
image — FreeRTOS, ThreadX, native_sim compose the baked baseline the same way.
The an536 C++ row (`examples/fixtures.toml`: "this one RUNS") had not booted
since phase-206 W2 shipped that sample; it is BuildOnly-in-practice because no
test runs it (issue 1624 / 1635).

## Resolution (2026-10-02)

The embedded baseline (`cyclone_config.hpp`, `kEmbeddedCycloneConfig`) now
states `<Tracing><OutputFile>stderr</OutputFile></Tracing>`. `stderr` is one of
the two names Cyclone special-cases without `fopen`, and it is the console on
every platform the baseline is compiled for. A scalar, so a user who DOES name
a file still wins (later tokens override; phase-206 W1's composition).

**Measured.** `cyclone_config_compose` gained
`test_verbosity_without_a_file_keeps_the_session`: from a read-only directory
(the host analogue of "no filesystem"), the user fragment alone FAILS
`dds_create_domain` (control — refuses to pass vacuously as root), and baseline
+ that fragment comes up. Negative control: removing the baseline line fails the
test with `cannot open for writing` twice. On target: the an536 image, rebuilt,
opens its session (see issue 1624's resolution for the boot log).

**Not measured.** ThreadX and native_sim images were not booted with the
change; they compose the same baseline string.
