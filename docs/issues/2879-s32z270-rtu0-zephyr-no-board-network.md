---
id: 2879
title: "`s32z270-rtu0-zephyr` declares no `[board.net]`, and a Zephyr board network file renders one interface with no VLAN, so the S32Z image keeps a hand-written network bring-up"
status: open
type: enhancement
severity: low
area: [zephyr, boards, network]
related: [phase-478, 2679, 2631]
found: 2026-11-23
---

**Home:** [phase-478](../roadmap/phase-478-island-developer-experience-findings.md)
I39. Consumer: the safety island (autoware-safety-island fork), branch
`nano-ros-next`, nano-ros pin `8d4e70a382`.

## What happens

Issue 2679 (phase-478 I31) lets a Zephyr image state its address in a board
network file, rendered into `NET_CONFIG_*`. Its cut was deliberate: one
interface (the first auto-started), and "a VLAN tag stays a board hook"
(2679 D5); a second interface is refused. The `s32z270-rtu0-zephyr` board
(issue 2631, PR 4086) declares no `[board.net]` at all
(`packages/boards/nros-board-zephyr/boards/s32z270-rtu0-zephyr/nros-board.toml`),
so an S32Z image cannot name a board network file even for its first
interface.

The S32Z deployment is where the second interface and the VLAN tag live: the
NETC switch carries the island's traffic on a tagged port.

## Island workaround

The last `PLATFORM_*` reader in the island's platform layer is this path:

- `actuation_module/src/asi_common/src/common/net/network_config.cpp`
  (`#if !defined(PLATFORM_ZEPHYR) #error`): brings up a second interface from
  the island's own `CONFIG_NET_IFACE2_ADDR/_GW/_NETMASK/_VLAN` symbols, applies
  the VLAN tag, and waits on a semaphore for a DHCPv4 lease when
  `NET_CONFIG_AUTO_INIT` is off.
- `actuation_module/src/asi_common/src/zephyr/board_network_hook.cpp`: a strong
  `nros_board_network_wait()` that sleeps `CONFIG_NET_DHCPV4_INITIAL_DELAY_MAX`
  (10 s on the S32Z) when the image does not render `NET_CONFIG_NEED_IPV4`, then
  calls `configure_network()`.
- `actuation_module/src/controller_bringup/zephyr/prj-s32z.conf` states
  `CONFIG_NET_CONFIG_SETTINGS`, `CONFIG_NET_DHCPV4` and the delay by hand.

The FVP images already dropped their half (the island's phase-15 P6): they
name `net-fvp.toml` / `net-fvp-tap.toml`.

## Wanted

1. `s32z270-rtu0-zephyr` declares its interfaces in `[board.net]` (the NETC
   ports the RTU0 devicetree enables), so an S32Z image names a board network
   file for its first interface and gets `NET_CONFIG_NEED_IPV4`.
2. A board network file can state a VLAN tag on an interface, rendered into
   Zephyr's `CONFIG_NET_VLAN` / `CONFIG_NET_CONFIG_MY_VLAN_ID` (or a
   post-init hook nano-ros owns), and, if the owner agrees, a second
   interface. If either stays out of scope, the board package owns the hook
   (a weak `nros_board_network_wait` in the board, not in the consumer).

## Acceptance

- A configure-only build of an S32Z image naming a board network file
  renders `build/nros/net/<image>.conf` with the address or lease, and with
  the VLAN tag if stated; the image links.
- Measured on S32Z270 hardware (phase-512) or the S32Z twin (phase-524): the
  interface comes up tagged with no island code.
- The island deletes `network_config.cpp`, the `NET_IFACE2_*` Kconfig symbols
  and the DHCP sleep in `board_network_hook.cpp`; `asi_common` no longer
  defines `PLATFORM_ZEPHYR` / `PLATFORM_FREERTOS`.
