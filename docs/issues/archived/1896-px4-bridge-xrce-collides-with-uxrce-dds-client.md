---
id: 1896
title: "PX4 bridge built with `NROS_PX4_BRIDGE_RMW=xrce` links, then SEGVs at boot: PX4's own `uxrce_dds_client` calls into nano-ros's copy of Micro-XRCE-DDS-Client"
status: resolved
resolved: 2026-10-15
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

## Resolution (2026-10-15)

**nano-ros's copy of the client compiles under private names.** Option (a), at
compile time: `packages/rmw/xrce/nros-rmw-xrce-cffi/build_symbol_prefix.rs`
compiles the vendored `uxr` + `ucdr` trees once into a probe directory with the
archive's own flags, reads their ELF symbol tables, and writes a force-included
header `#define <name> nros_<name>` for every global they DEFINE plus every
`uxr_*`/`ucdr_*` they REFERENCE (`uxr_millis`/`uxr_nanos`, which our backend
defines). Every TU of the archive — vendored and backend — is then compiled
with `-include` of it, so the rename is invisible inside the archive and
nothing outside it ever named the client. After the real compile, `verify()`
reads the produced archive and fails the build if any symbol in it still
carries an original name (and if it holds no `nros_uxr_*` at all, so the check
cannot pass vacuously). The CMake lane's CTest binaries link that archive and
`graph_sample.c` calls `ucdr_*` directly, so `nros-rmw-xrce/CMakeLists.txt`
reads the header's path from the pointer file (`prefix_header=`) and puts it
on `nros_rmw_xrce_vendored`'s INTERFACE.

### Measured

* **The overlap.** PX4's `libmicroxrcedds_client.a` + `libmicrocdr.a` define
  527 strong globals; the bridge's `libnros_cpp.a` defined 517 `uxr_*`/`ucdr_*`
  (349 + 168). **512 were defined by both** (344 `uxr_*`, 168 `ucdr_*`) —
  micro-CDR was duplicated too, as suspected. After the fix: **0**.
* **The versions differ, which rules out (b).** PX4 builds client **2.4.0** /
  micro-CDR **2.0.1** with `UCLIENT_PROFILE_CUSTOM_TRANSPORT=OFF`
  (`src/modules/uxrce_dds_client/CMakeLists.txt`); ours is **3.0.1** / **2.0.2**
  with custom transport ON — `nros-rmw-xrce`'s registry transports are built on
  it. Linking PX4's client instead would mean a major-version API change and a
  profile PX4 compiles out, and only for PX4; every other image that meets a
  second client keeps the bug. (c) defeats RFC-0039 Track B and fixes one host.
* **A prefix is not the set.** The first cut renamed every `uxr_*`/`ucdr_*`
  identifier (from the preprocessed sources). The PX4 link then FAILED loudly
  instead of silently: `multiple definition of 'read_submessage_info'`,
  `serialize_get_info_message`, `listen_info_message`, `read_submessage_format`,
  `on_full_output_buffer_fragmented`, `on_full_output_buffer`,
  `get_available_free_slots` — upstream exports 7 unprefixed globals too, and
  once PX4's `uxr_*` references stopped being satisfied by our members, ld
  pulled PX4's and met them. Reading the objects is what makes the set
  complete; it also catches micro-CDR's token-pasted names
  (`ucdr_serialize ## SUFFIX`), which no grep of the sources sees. 523 names
  are renamed on the host build.
* **Every lane.** Host (`x86_64-unknown-linux-gnu`) archive: 0 unprefixed, 517
  `nros_` defined, 0 `nros_*` references left unresolved inside the archive.
  `thumbv7m-none-eabi` (arm-none-eabi-gcc 13.2, the embedded/`UCLIENT_PLATFORM_NO_POSIX`
  shape): 0 unprefixed, 509 defined, 0 unresolved. `just check rmw-xrce`: 5/5
  CTests pass against the prefixed archive.
* **The bug, reproduced, then gone.** With the force-include disabled (a
  temporary edit, reverted) the image links and `bin/px4` dies at boot,
  `px4 exit: 139` right after `INFO [uxrce_dds_client] init UDP agent
  IP:127.0.0.1, port:8888`. With it, the same image holds both copies
  (`nm bin/px4`: `T uxr_ping_agent_attempts` and `T nros_uxr_ping_agent_attempts`)
  and boots: `INFO [px4] Startup script returned successfully`.

### Acceptance — a boot and TWO deliveries

`packages/testing/nros-px4-sitl-test/tests/px4_bridge_xrce_ros2_e2e.rs`
(`just px4 test-bridge-xrce-ros2`): one MicroXRCEAgent on udp/8888, PX4's
stock `uxrce_dds_client` and the bridge both dial it, and a ROS 2 humble
`ros2 topic echo` over `rmw_fastrtps_cpp` with the `px4_msgs` overlay decodes

* 3 bridged `px4_msgs/msg/DebugKeyValue`, keys `["velx", "velx", "velx"]`
  (PX4's stock `px4_mavlink_debug` → uORB → nano-ros xrce bridge), and
* 3 `/fmu/out/timesync_status` from PX4's OWN client in the same image — the
  half the collision broke, so a fix that silenced PX4's client would fail here.

```
PASS [ 15.575s] nros-px4-sitl-test::px4_bridge_xrce_ros2_e2e xrce_bridge_and_px4_uxrce_dds_client_share_one_image
one PX4 image, two XRCE clients: ROS 2 humble decoded 3 bridged px4_msgs/msg/DebugKeyValue (keys ["velx", "velx", "velx"]) and /fmu/out/timesync_status from PX4's uxrce_dds_client
```

Before boot the test asserts the image contains `nros_uxr_create_session`, so a
regression of the prefix pass is named rather than read as a `BootTimeout`.

### Two things the test had to learn, measured

* **Pin PX4's participant too (issue 1009's rule, a half it did not reach).**
  The agent's loopback profile binds participants created from a BINARY or
  REFERENCE request (the bridge's); PX4's client sends an XML STRING carrying
  its own transports, which overrides the default. With `UXRCE_DDS_PTCFG=0`
  the loopback-pinned subscriber listed 1 `/fmu/out` topic (the bridge's) and
  none of PX4's 26; the same run unpinned on both sides listed all 27. The test
  sets `UXRCE_DDS_PTCFG 1` (PX4's own localhost-only participant) and restarts
  the client before any subscriber.
* **PX4 SITL instance 0 is HOST-global** (`/tmp/px4-sock-0`). While another
  session ran a SITL test, this boot printed `PX4 server already running for
  instance 0` and exited, which `Px4Sitl::boot_in` reports as an anonymous
  `BootTimeout`. The test now refuses up front, naming the socket.

### The orphaned SITL processes (in scope, partly fixed)

Measured: when the daemon dies (SEGV here), `sh -c /bin/sh etc/init.d-posix/rcS 0`,
`/bin/sh etc/init.d-posix/rcS 0` and `px4-param --instance 0 greater -s
ZENOH_ENABLE 0` survive it in its process group, blocked on a server nobody
will answer — the shape that leaked for 13 hours. SIGTERM to them was measured
to kill all three, and `Px4Sitl`'s `Drop` SIGTERMs the whole group, so the
orderly and the panicking paths are clean (every run above: no PX4/agent
process left). What `Drop` cannot cover is a SIGKILLed test process (a nextest
timeout, a cancelled lane): `PR_SET_PDEATHSIG` reaches the daemon only. The new
test records the daemon's group (and its two `ros2` peers) in the issue-0659
orphan ledger, and both `test-bridge-*` recipes now run `nros-peer-sweep` first.
Not done: the zenoh sibling does not record its group, and `Px4Sitl` itself
(in the `px4-rs` submodule, `tests/sitl/src/process.rs`) still returns early
from `graceful_kill` when the leader is already reaped, so it never follows the
SIGTERM with a SIGKILL — harmless today because SIGTERM suffices, worth a
`px4-rs` change that exposes the pid and escalates regardless.
