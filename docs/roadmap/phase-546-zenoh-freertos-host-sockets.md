# Phase 546 — zenoh on the FreeRTOS POSIX board: a host-socket FreeRTOS arm

**Status (2026-11-07). IN PROGRESS.** W1 to W4 are implemented and measured
in the box; they land with the zenoh-pico fork commit W2 needs (the fork's
`nano-ros-1.10.1` branch, then a pin bump). W5 waits on the book's zenoh
variant of the FreeRTOS POSIX starter, W6 on the issue landing on `main`.
Implements [RFC-0109 Amendment 2026-11-07](../design/0109-network-config-layers.md#amendment-2026-11-07--the-stack-owner-selects-the-zenoh-pico-socket-arm-issue-2342)
(D11). Closes
[issue 2342](../issues/2342-zenoh-freertos-host-netstack-port.md).

Maintainer ruling 2026-11-07: add a host-socket FreeRTOS arm for zenoh, do
not refuse zenoh on the board.

## Why

The `freertos-posix` board runs the FreeRTOS kernel inside a host process
(the `ThirdParty/GCC/Posix` port). Its tasks are FreeRTOS tasks, each a host
pthread the port schedules, and its network is the host's own BSD sockets
(`[board.net] stack = "host"`). zenoh-pico has no platform for that pair:
`[port.freertos]` chose `ZENOH_FREERTOS_LWIP` (lwIP's sockets) or
`ZENOH_ORIN_SPE` (no sockets), so every zenoh image on the board failed at
`lwipopts.h`. The board's Cyclone image works, and the starter scaffolds the
zenoh default.

## Work items

### W1 — The board fact: `host_stack`, derived from `[board.net] stack`

- `BoardEntryFacts::resolved_capabilities` adds the RFC-0086 capability
  `host_stack` from `[board.net] stack` (`"host"` is true, `"os"` and
  `"board"` false, unstated absent). A `[board.capabilities] host_stack` row
  is refused: one fact, one spelling.
- `nros-platform-freertos` declares `host_stack = false` (the family brings its
  own stack), so the lwIP boards keep their arm and say nothing new.

Acceptance: `host_stack_is_derived_from_the_board_net_stack` and
`the_freertos_posix_board_states_a_host_stack`
(`nros-platform-config`). **Done.**

### W2 — The third arm of `[port.freertos]`

- A conditional define may carry a VALUE (`ConditionalDefine::value`).
- The host arm defines zenoh-pico's own custom-platform hook,
  `ZP_SYSTEM_PLATFORM_HEADER = "nros_zenoh_freertos_host.h"`
  (`zpico-sys/c/platform/freertos-host/`): `freertos/lwip.h`'s system types
  beside `unix.h`'s socket type, and `ZP_PLATFORM_SOCKET_POSIX`, so the core
  `link` group compiles `link/transport/{tcp,udp}/*_posix.c`. It compiles
  `system/unix/network.c` for the socket wait and close half, and
  `system/freertos/system.c` for the tasks, mutexes, condvars and clock. The
  lwIP rows gain `host_stack = false`.
- `Z_RUNTIME_IDLE_READ_TASK_SLEEP = 2` on the host arm. A FreeRTOS task
  blocked in a host `recv` is still RUNNING to the kernel, so a read task that
  never yields starves every lower-priority task (measured without the rest:
  the image connected and printed no `Publishing:` line in 20 s, while
  `strace` showed the read task's `recvfrom` returning `EINTR` back to back).
  The POSIX port's 1 ms tick interrupts the `recv` (`sa_flags = 0`,
  no `SA_RESTART`), which bounds one poll; the 2 ms rest returns the CPU.
- The zpico shim keys its FreeRTOS KERNEL arms (spin semaphore, task
  attributes, clock-seeded ZID) on `ZPICO_FREERTOS_KERNEL`, set for lwIP and
  for the host arm; the lwIP-only arms (semihosting printk, stack report) stay
  on `ZENOH_FREERTOS_LWIP`.
- The ABI size probe takes the library's resolved conditional defines instead
  of hardcoding the lwIP arm (issue 0135: the probe measures what the library
  compiles).
- `check-capability-conditionals` partitions only the rows that select a
  platform header, and reads the custom header through the
  `include_paths_conditional` row gated like its define.
- **Fork patch** (zenoh-pico `nano-ros-1.10.1`, `cf228437`):
  `system/freertos/system.c` stopped with `#error "... no compatible network
  stack"` under any header but its three; a custom platform header now takes
  the no-include arm and the SPE's RNG arm.

Acceptance: a zenoh C talker and listener on `freertos-posix` build with
`nros build` and deliver through `rmw_zenohd`, to each other and to a ROS 2
`ros2 topic echo`. **Done in the box** (13 of 13 samples to the listener, the
echo printing `data: 'Hello World: N'`); lands with the fork pin.

### W3 — A host-stack board dials the host's loopback router

- `nros board net --format cmake` prints `NROS_BOARD_NET_STACK`;
  `nros_board_net_c(... STACK_VAR)` returns it, and the entry-locator resolver
  sends a `"host"` board to `tcp/127.0.0.1:7447`
  (`NROS_LOCATOR_ENTRY_HOST_STACK`) instead of its gateway rung, which
  answered the platform default `192.0.3.1`.

Acceptance: a scaffolded zenoh talker on `freertos-posix` with no locator
prints `locator tcp/127.0.0.1:7447 (baked)`. **Done.**

### W4 — A lane builds and runs it

- `examples/workspaces/c` image `freertos_posix_zenoh` (locator
  `tcp/127.0.0.1:11130`, `alloc::port_of(FreertosPosix, C, EntryPubsub)`),
  fixture row `workspace-c-freertos-posix-zenoh`, cell
  `(FreertosPosix, C, Zenoh, EntryPubsub, Workspace, Runtime)` and
  `freertos_posix_c_entry_delivers_over_zenoh` (a router on the baked port,
  the in-image listener after the talker).

Acceptance: `just freertos build-fixtures-posix` builds the row and the test
passes. **Done in the box**; lands with the fork pin.

### W5 — The book starter

- When the book's zenoh variant of the FreeRTOS POSIX starter probe track
  carries `awaits` naming issue 2342, drop the `awaits` (registry, then
  regenerate). NOT STARTED: that variant is not on `main` yet.

### W6 — Close issue 2342

- Issue 2342 `status: resolved`, archived, `changelog.d/2342.feat.md`.
  NOT STARTED: the issue lands with PR #3257.
