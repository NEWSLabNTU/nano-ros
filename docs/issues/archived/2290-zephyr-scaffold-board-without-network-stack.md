---
id: 2290
title: "Two Zephyr boards give a scaffolded IP image no network stack: qemu-cortex-a53 keeps its network in a bringup, nucleo-f767zi-zephyr turns IP off and states no host_serial"
status: resolved
type: bug
area: zephyr, boards, scaffold
related: [phase-531, 2177, 1782]
found: 2026-11-06
resolved: 2026-11-07
resolved_in: "fix-issue-2290 branch: each board's package states its own link"
---

## What

`nros new <ws> --board <b>` writes an image with no `os_config` and, unless
the board states `[board.net] host_serial`, no `locator`. On Zephyr, that
image's network stack therefore comes only from the board package: its
`prj.conf`, plus the snippets `[board.zephyr] snippets` names. CONFIG_NETWORKING
is the root of every other net symbol. A board conf that leaves it out has
every net assignment dropped as an unmet dependency, and the image dials its
Kconfig default (`tcp/…:7447`) with no sockets.

The book session hit this on `zephyr` (native_sim):
`west build` failed to link zephyr.exe on `z_impl_sys_rand_get`, because
with no NETWORKING there is no entropy driver either. Measured locally the
same day on `mps2-an385-zephyr`: the image linked (that board states
`CONFIG_TEST_RANDOM_GENERATOR`), booted, and failed every connect with
`ConnectionFailed`. Both are fixed in the PR that filed this issue: the
`nros-nsos` snippet and `nros-board-mps2-an385-zephyr/prj.conf` now state
`CONFIG_NETWORKING`, `CONFIG_NET_IPV4` and `CONFIG_NET_SOCKETS`. A scaffolded
C talker on each matched `Publishing:` under `nros run --expect`.

Two siblings remain. `leaf_system::tests::a_zephyr_board_whose_default_link_is_ip_states_a_network_stack`
names both as known exceptions pointing at this issue:

- **`qemu-cortex-a53`** has `links = ["tcp", "udp"]`, but its network
  (NET_CONFIG addresses, the driver, NETWORKING itself) lives in the one
  in-tree bringup that uses it,
  `examples/workspaces/realtime-c/src/smp_bringup/boards/qemu_cortex_a53_qemu_cortex_a53_smp/`,
  not in `packages/boards/nros-board-qemu-cortex-a53/prj.conf`. The fix is to
  move the board-level part into the board package. Not done here because it
  was not measured: no QEMU a53 image ran in this session.
- **`nucleo-f767zi-zephyr`** turns IP off (`CONFIG_NETWORKING=n`) because its
  link is USART3 serial, yet states no `[board.net] host_serial`. So a
  scaffolded image gets no `serial/` locator and dials the TCP Kconfig default
  with no stack. The fix is likely a `host_serial` (the board's Renode twin
  carries the link on `usart3`), so the scaffold writes the serial locator
  `[image.nucleo_serial]` already states by hand.

## Next

Fix each board, then delete its row from the test's exception list.

## Resolution

Both boards are fixed in their packages, and the test's exception list is gone.

- **`qemu-cortex-a53`.** `packages/boards/nros-board-qemu-cortex-a53/prj.conf`
  now states the board's network and runtime needs: NETWORKING, IPv4, UDP, TCP,
  sockets, the ethernet L2, the static QEMU user-mode addresses, the router at
  `tcp/10.0.2.2:7447`, TEST_RANDOM_GENERATOR, and the aarch64 stack sizes with
  the stack guard. The NET_CONFIG addresses, the random generator and the stacks
  moved out of the SMP bringup's board conf. `nros build` merges the board
  package's conf into that generated application too, with the same values. The
  stacks were not in the issue. A scaffolded talker at Zephyr's 16 KiB main
  stack booted and printed nothing, and with the bringup's 32 KiB it published.

  Measured: `nros new a53ws --board qemu-cortex-a53 --lang c`, then `nros build`,
  linked. Its `.config` has NETWORKING, ETH_E1000, NET_QEMU_ETHERNET and the
  10.0.2.15 address. Booted under `qemu-system-aarch64 -machine virt,gic-version=3
  -nic user,model=e1000 -smp cpus=2` with `rmw_zenohd` on the host, it printed
  `Publishing: 'Hello World: 1'` and onward. `nros run` itself did not reach the
  boot: it ran `west build -t run` outside the west workspace (`west: unknown
  command "build"`), the west-workspace-root bug tracked separately. Zephyr's
  own run target for this board also uses `-nic tap`, which needs
  `/dev/net/tun`. The nros-tests harness uses user-mode networking instead.
- **`nucleo-f767zi-zephyr`.** Its `[board.net]` states
  `host_serial = "serial@40004800#baudrate=115200"` (USART3), so `nros new`
  writes the serial locator `[image.nucleo_serial]` states by hand. The Renode
  smoke cell already boots that image with that locator.
- **`rpi-pico-w-zephyr`**, the same class, found by the new test: a serial-only
  board whose conf turns IP off and which stated no host port. It now states
  `host_serial = "uart@40034000#baudrate=115200"` (UART0), the locator its
  overlay's header names. Not measured: the board has no emulator.

Tests, in `packages/cli/nros-cli-core/src/cmd/leaf_system.rs`:
`a_zephyr_board_whose_default_link_is_ip_states_a_network_stack` has no
exception list now, and requires `qemu-cortex-a53` among the boards it checks.
`a_zephyr_board_whose_conf_turns_ip_off_states_its_host_serial` is new: every
Zephyr board whose conf sets `CONFIG_NETWORKING=n` must carry a serial link and
a `host_serial`. Both failed before the board changes, naming `qemu-cortex-a53`,
`nucleo-f767zi-zephyr` and `rpi-pico-w-zephyr`, and pass after them.
