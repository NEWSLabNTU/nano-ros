---
id: 2879
title: "`s32z270-rtu0-zephyr` declares no `[board.net]`, and a Zephyr board network file renders one interface with no VLAN, so the S32Z image keeps a hand-written network bring-up"
status: resolved
type: enhancement
severity: low
area: [zephyr, boards, network]
related: [phase-478, 2679, 2631]
found: 2026-11-23
resolved: 2026-11-25
resolved_in: "feat(zephyr boards, issue 2879): s32z270-rtu0-zephyr declares its interface, and a board network file states a VLAN tag"
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
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

## Resolved, 2026-11-25

Both wanted items, for the one interface; the second interface is an open
decision (below). Not measured: no S32Z270 board or twin was used, so the
tagged bring-up on the wire stays phase-512 / phase-524 work, and the
acceptance's "measured" row is open.

### Checked first: Zephyr's own VLAN symbol

Zephyr 3.7.2 and 4.4.0 both have `CONFIG_NET_CONFIG_MY_VLAN_ID` (`depends on
NET_VLAN`, `subsys/net/lib/config/Kconfig`), and `net_config_init_by_iface`
calls `setup_vlan(iface)` before `setup_ipv4` / `setup_dhcpv4`
(`subsys/net/lib/config/init.c`). It does not give a tagged address:

- Zephyr's VLAN has been a virtual interface since 3.7: `net_eth_vlan_enable`
  attaches one of `NET_VLAN_COUNT` virtual interfaces (`l2/ethernet/vlan.c`,
  `enable_vlan_iface`) to the Ethernet interface, and those are created
  `NET_IF_NO_AUTO_START`.
- `init_app` picks the first auto-started interface (the Ethernet one), tags
  it, and then adds the address, or starts DHCPv4, on that same untagged
  interface. The VLAN interface gets no address and is never brought up.
  Zephyr's own samples (`samples/net/sockets/echo_client/src/vlan.c`) address
  the virtual interface by hand.

So rendering `NET_CONFIG_MY_VLAN_ID` alone would build, tag nothing useful and
leave the address untagged. It is not rendered, and a build that sets it with
the nros tag is refused (`#error` in `net_vlan.c`).

### Decisions

D1. **`s32z270-rtu0-zephyr` declares one interface.** `[board.net] interfaces
= { eth0 = { kind = "ethernet", dhcp = true, vlan = true } }`. The RTU0
devicetree (Zephyr 3.7.2 `s32z2xxdc2_s32z270_rtu0.dts` over
`nxp_s32z27x_r52.dtsi`) enables one ENETC port, `enetc_psi0`; the VSIs
(`enetc_vsi1`..`7`) stay disabled. An image without `vlan` gets the issue-2679
rendering unchanged, `NET_CONFIG_NEED_IPV4` included.

D2. **`vlan = <tag>` on an interface of the board network file**, 1-4094,
beside `network` / `address`. It is taken only where the board's interface
states `vlan = true` (a new key of the `[board.net] interfaces` table, like
`dhcp`); anywhere else the file is refused naming the board key, so a board
whose road does not render the tag never drops it silently. Only
`s32z270-rtu0-zephyr` states it: the NETC driver reports `ETHERNET_HW_VLAN`
under `CONFIG_NET_VLAN` (`drivers/ethernet/eth_nxp_s32_netc.c`). 0 (priority
only) and 4095 (reserved) are refused. The tag is on the interface, not the
network, because it is what the island's `CONFIG_NET_IFACE2_VLAN` was.

D3. **A tagged interface keeps Zephyr's address rows and hands the bring-up to
the nros module.** The fragment keeps `NET_CONFIG_SETTINGS=y`, `NET_DHCPV4` and
`NET_CONFIG_MY_IPV4_*` exactly as for an untagged one, and replaces
`NET_CONFIG_AUTO_INIT=y` / `NET_CONFIG_NEED_IPV4=y` with
`NET_CONFIG_AUTO_INIT=n`, `NET_VLAN=y`, `NET_IF_MAX_IPV4_COUNT=2` (Zephyr's
default with one VLAN is one, which the untagged interface could take first)
and `NROS_NET_VLAN_TAG=<tag>`. The module compiles
`packages/platform/nros-platform-zephyr/src/net_vlan.c` when the tag is set:
at the APPLICATION init level, where Zephyr's `init_app` runs, it tags the
first Ethernet interface, clears the VLAN interface's `NO_AUTO_START`, makes
it the default interface (so the RMW's sockets, `nros_platform_zephyr_wait_network`
and off-link routes use it), brings it up and calls Zephyr's own
`net_config_init_by_iface(vlan, NULL, NET_CONFIG_NEED_IPV4,
NET_CONFIG_INIT_TIMEOUT)`. So the address, the DHCPv4 lease and the wait are
Zephyr's code, on the tagged interface; nano-ros adds about 40 lines and no
new address symbol. The untagged interface stays up with no address.

D4. **The conflict check covers the new rows.** An `os_config` fragment that
sets `NET_VLAN`, `NET_CONFIG_AUTO_INIT` or `NET_IF_MAX_IPV4_COUNT` beside a
tagged file is refused as issue 2679 D4 refuses any rendered symbol, so the
island's hand-written `CONFIG_NET_CONFIG_SETTINGS` / `CONFIG_NET_DHCPV4` in
`prj-s32z.conf` must go when it names the file.

D5. **No board hook.** With D1-D3 nothing S32Z-specific is left for a
`nros_board_network_wait`: the untagged image waits in Zephyr's `init_app`
(`NET_CONFIG_NEED_IPV4`), the tagged one in `net_vlan.c`, and the weak no-op
default stays. The second interface is the only part a hook could still own,
and it waits on the owner (below).

### Tests

- Unit, `orchestration/image_net.rs`: a tagged static address renders the
  rows of D3 in order and no `MY_VLAN_ID` / `NEED_IPV4`; a tagged lease renders
  `NET_DHCPV4=y` and the empty address; a tag on a board interface without
  `vlan = true`, and the tags 0 and 4095, are refused; a fragment that sets
  `NET_VLAN` beside a tagged file is refused.
- Compile check: `[image.s32z270_rtu0]` of `examples/workspaces/cpp`
  (the `west_s32z270_rtu0_zephyr` row, `just zephyr build-module-fixtures`)
  names `net-s32z270-rtu0.toml`, a static address on VLAN 20, so the row
  configures with the rendered fragment and links `net_vlan.c` for the R52.

### Open decision (owner)

- **A second interface.** Issue 2679 D3 still refuses one on Zephyr. On the
  RTU0 a second interface means enabling a NETC VSI in the image's overlay
  (the board enables only PSI0), and its address would need a per-interface
  rendering that Zephyr's `NET_CONFIG` does not have (a generated `SYS_INIT`
  over the interfaces, like `net_vlan.c` but for N). No in-tree island
  configuration sets `CONFIG_NET_IFACE2_ADDR` today, so nothing waits on it.
  If it is wanted, the smallest shape is a second `[ethN]` table rendered into
  nros-owned symbols and configured by the same file; if not, the island
  states its second interface in its own application code.
- **The tag on the network instead of the interface.** D2 puts it on the
  interface; a `[network.<n>] vlan` would make every image on one network
  agree by construction. Either is a small change while the S32Z is the only
  user.

### What the island changes

The S32Z image names a board network file, e.g.
`actuation_module/src/controller_bringup/zephyr/net-s32z.toml`, first in
`[image.s32z] os_config`:

    [eth0]
    network = "s32z"
    address = "dhcp"        # or address = "a.b.c.d"
    # vlan  = <tag>         # only if the switch port is tagged

    [network.s32z]
    subnet = "<the lab subnet>/24"
    dhcp   = true           # gateway = "..." for a static address

Then:

- `prj-s32z.conf` drops `CONFIG_NET_CONFIG_SETTINGS=y` and `CONFIG_NET_DHCPV4=y`
  (both rendered; D4 refuses them). `CONFIG_NET_DHCPV4_INITIAL_DELAY_MAX` may
  stay (not rendered), and with `NEED_IPV4` it shortens the boot wait rather
  than adding a sleep.
- `board_network_hook.cpp` goes: its DHCP sleep is replaced by Zephyr's
  `NEED_IPV4` wait (untagged) or `net_vlan.c`'s (tagged), and
  `configure_network()` does nothing on the S32Z (no `CONFIG_NET_IFACE2_ADDR`
  is set anywhere, and `NET_CONFIG_AUTO_INIT` is on, so its DHCP branch never
  ran). The weak nano-ros default takes over.
- `network_config.cpp`, `network_config.hpp` and the `NET_IFACE2_*` Kconfig
  symbols in `asi_common/zephyr/Kconfig` go, and with them the last
  `PLATFORM_ZEPHYR` reader, unless the owner keeps a second interface in the
  application (open decision above).
