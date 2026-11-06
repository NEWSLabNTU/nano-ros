---
id: 2290
title: "Two Zephyr boards give a scaffolded IP image no network stack: qemu-cortex-a53 keeps its network in a bringup, nucleo-f767zi-zephyr turns IP off and states no host_serial"
status: open
type: bug
area: zephyr, boards, scaffold
related: [phase-531, 2177, 1782]
found: 2026-11-06
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
