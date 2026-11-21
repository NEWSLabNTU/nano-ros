---
id: 2785
title: "A Zephyr Cyclone image with the stock node interfaces (rosout, sim_time) boots and then emits no nros log record; with node_interfaces = \"none\" it runs"
status: resolved
type: bug
severity: high
area: [zephyr, rmw-cyclonedds, logging]
related: [phase-556, issue-2786]
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(zephyr): refuse a socket service stack too small for its poll array"
---

**Home: [phase-556](../../roadmap/phase-556-ros2-compat-audit.md) W13**, which made the
stock node interfaces every image's default (6229085857).

## What happens

A consumer's Zephyr controller image on `fvp_baser_aemv8r` with Cyclone DDS
(the safety island's `[image.fvp]`, nano-ros pin 1866c0594f -> 7ca11830f9)
gains `CONFIG_NROS_CAPABILITY_ROSOUT=y` and `CONFIG_NROS_CAPABILITY_SIM_TIME=y`
from the new default. It prints

    nros: sched task priority set sched_task=`control` prio=9

and then no nros log record at all for 500 s: not its stack-headroom warning,
not "Starting Controller Node". It reproduces on a rerun. The same image at
1866c0594f, before the default changed, is Live in about 1.2 s.

With `node_interfaces = "none"` on the image, at 7ca11830f9, it boots: the
parameter store is carved (164 slots) and it is Live at 1.28 s model time. So
the hang comes with the interfaces, most likely rosout (the first nros log
record never appears); sim_time is not ruled out yet.

The FreeRTOS POSIX and AN536 (Cyclone, FreeRTOS) images of the same consumer
at the same pin are not affected.

## Why the consumer cannot work around it

- `node_interfaces = "none"` also removes `param_services`, so `ros2 param`
  stops working on the image.
- A conf fragment with `CONFIG_NROS_CAPABILITY_ROSOUT=n` is overridden: the
  built `.config` still has `=y` (issue 2786).

## Direction

1. Build the image with rosout on and sim_time off, and the reverse, to tell
   which interface hangs.
2. Find where the boot stops (the rosout publisher's creation or its first
   publish on the Zephyr Cyclone road, or the log sink path).
3. Fix the hang. A per-interface opt-out is worth having on its own, but it
   is not the fix.

Logs: the consumer's lane logs `s1-fvp.log` (hang) and `fvp-exp-run.log`
(`none`, boots), kept with the consumer's draft.

## Resolution

Neither interface hangs anything, and the boot did not stop. The image ran,
and every nano-ros log record after the first was silenced.

**Reproduction.** The consumer's FVP lane at the 7ca11830f9 pin (no in-tree
image reproduces it: the in-tree FVP workspace image enables no socket
service thread). Same symptom: the printk lines, then no nros record.

**Where it stopped.** `nros: sched task priority set` is printed after the
boot level's `setup` has constructed the controller, whose constructor logs
"Starting Controller Node". So construction ran and its record vanished.
Instrumenting `nros-log` showed `DISPATCH_GUARD` already `true` at the first
dispatch, before anything had acquired it: every record took the "another
delivery is in progress" arm and was held forever. The bytes around the guard
were AArch64 frame records (fp, lr into `z_add_timeout` / `k_spin_unlock`).
Listing the threads: `net_socket_service` has a 0x1000-byte stack at
0xa92f30, and its saved stack pointer was 0xa90d20, 8.7 KB below its base,
just above the guard at 0xa90cb8.

**Root cause.** Zephyr 3.7 `zsock_poll_internal`
(`subsys/net/lib/sockets/sockets.c:2322`) puts
`struct k_poll_event poll_events[CONFIG_NET_SOCKETS_POLL_MAX]` on the
caller's stack. The socket service thread (selected by DHCPv4) polls through
it, and the image sets `CONFIG_NET_SOCKETS_POLL_MAX=256` with
`CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=4096`: 10,240 bytes of array plus
2,576 of other frames (the DHCP callback and its log record run on that
thread). Nothing guards that stack on the FVP, so it overran into .bss. The
rosout queue's statics moved `nros_log::DISPATCH_GUARD` under the overflow;
with `node_interfaces = "none"` the same overflow landed on bytes nothing
read afterwards. So rosout vs sim_time is not the question: the image is
Live at 1.28 s with both on once the stack fits (measured with
`CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE=16384`).

**Fix.** `zephyr/cmake/nros_socket_service_stack.cmake`, called from
`zephyr/CMakeLists.txt` for every IP image: a configure whose socket service
stack is below `POLL_MAX * sizeof(k_poll_event) + margin` (40 B and 3,072 B on
64-bit, 20 B and 1,024 B on 32-bit; `CONFIG_ZVFS_POLL_MAX` on 4.x; not on
native_sim) is refused, naming both knobs and the floor. The consumer image
is refused with "It needs at least 13312 bytes".

The consumer has to raise `CONFIG_NET_SOCKETS_SERVICE_STACK_SIZE` (13,312 or
more) or lower its poll maximum.

Check: `just check zephyr-socket-service-stack`
(`tests/zephyr-socket-service-stack-tests.sh`).
