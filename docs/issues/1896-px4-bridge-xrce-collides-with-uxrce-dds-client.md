---
id: 1896
title: "PX4 bridge built with `NROS_PX4_BRIDGE_RMW=xrce` links, then SEGVs at boot: PX4's own `uxrce_dds_client` calls into nano-ros's copy of Micro-XRCE-DDS-Client"
status: open
type: bug
severity: medium
area: [px4, rmw-xrce, build]
related: [issue-0436, issue-1893, phase-325]
found: 2026-10-14
---

## Measured

phase-325 W3.3 ticked "the same source builds against a second backend" on a
BUILD receipt for `NROS_PX4_BRIDGE_RMW=xrce`. It still builds (re-measured
2026-10-14 while landing W3.4: `rc=0`, cmake `nros_uorb_bridge: outward backend
= xrce`, the generated `nros_app_register_backends.c` registering exactly
`uorb, xrce`). The image it produces does not boot:

```
INFO  [uxrce_dds_client] init UDP agent IP:127.0.0.1, port:8888
timeout: the monitored command dumped core          (rc=139)
```

Backtrace (gdb, PX4's signals passed through):

```
Thread 74 "uxrce_dds_clien" received signal SIGSEGV
#0  0x0000000000000000 in ?? ()
#1  uxr_ping_agent_attempts ()
#2  UxrceddsClient::setupSession (...) at src/modules/uxrce_dds_client/uxrce_dds_client.cpp:212
```

`px4_sitl_default` enables PX4's stock `uxrce_dds_client` (that is RFC-0039
Track B's whole premise), which links its own Micro-XRCE-DDS-Client. The
bridge's `libnros_cpp.a`, built with `rmw-xrce-cffi`, carries a SECOND one:

```
.../uxrce_dds_client/lib/libmicroxrcedds_client.a:ping.c.o: T uxr_ping_agent_attempts
target/release/libnros_cpp.a:b8ec23b66878645f-ping.o:        T uxr_ping_agent_attempts
```

Two archives defining the same strong symbols link without a diagnostic — the
first archive member that satisfies an undefined reference wins — so PX4's
client ran against nano-ros's copy, built with a different transport/profile
configuration, and called a NULL function pointer. Same class as issue 0436's
first defect (two zenoh-pico copies in one image), one backend over.

So with xrce the "second backend" is a link that succeeds and an image that
dies before the startup script returns; `Px4Sitl::boot_in` reports
`BootTimeout { timeout_secs: 15 }`. The W3.4 test does not run against it (it
drives a ROS 2 peer over rmw_zenoh and checks the image bridges to zenoh), so
nothing automated sees this.

## What would close it

One Micro-XRCE-DDS-Client per image. Either the bridge's xrce build links
PX4's client instead of bundling one (configuration must then match what
nros-rmw-xrce expects — profile, stream sizes, transports), or the nano-ros
copy is symbol-prefixed so the two coexist, or an xrce bridge image disables
PX4's `uxrce_dds_client` (which defeats running both tracks in one SITL). The
acceptance is a BOOT, plus a sample reaching a ROS 2 subscriber through the
XRCE agent — not a link.
