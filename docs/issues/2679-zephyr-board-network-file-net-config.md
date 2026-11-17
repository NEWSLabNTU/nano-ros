---
id: 2679
title: "A board network file is refused on Zephyr; render `[eth0]` and `[network.<n>]` into Zephyr's `NET_CONFIG_*` instead"
status: open
type: enhancement
area: [zephyr, orchestration, cli, boards]
severity: medium
found: 2026-11-17
related: [phase-478, phase-497]
---

**Home:** phase-478 I31 (found by the safety island). GitHub #4011.
**State: DESIGN, Q1-Q3 ruled by the nano-ros manager 2026-11-17; awaiting the
user's approval** (it adds public config surface: a Zephyr board's
`[board.net] interfaces`, a generated Kconfig fragment, and an amended
RFC-0103 D10d).

## Symptom

`book/src/reference/system-toml.md`, "Board network file": a board network
file is refused on a board that declares no interfaces, and no Zephyr board
declares any ("Zephyr boards take their address from Kconfig"). So an image
states its address in a `prj-*.conf` on Zephyr and in a board network file
everywhere else.

The safety island keeps its own bring-up for that reason:

- `actuation_module/src/asi_common/src/common/net/network_config.cpp`: a
  static address per interface from its own `CONFIG_NET_IFACE{1,2}_ADDR/GW/
  NETMASK/VLAN`, or a DHCP start and a wait on a semaphore;
- `board_network_hook.cpp`: a sleep of `CONFIG_NET_DHCPV4_INITIAL_DELAY_MAX`
  (10 s on every FVP boot);
- `prj-fvp-tap.conf`: `CONFIG_NET_DHCPV4=n` and `CONFIG_NET_IFACE1_*` with
  192.168.10.2/24 via 192.168.10.1, and `prj-actuation.conf`:
  `CONFIG_NET_DHCPV4=y`.

Both of its Zephyr images use one interface (`IFACE1`). The second interface
and the VLAN tag are used only on the S32Z.

## Proposed design

Technical questions Q1-Q3 were answered by the nano-ros manager on
2026-11-17 and are folded in below (D3, D4, D7). The design as a whole still
needs the user's approval before implementation.

D1. **A Zephyr board opts in by declaring its interface.** The board's
`[board.net]` gains the same `interfaces` row the FreeRTOS boards have, e.g.
`fvp-aemv8r-smp`:

    [board.net]
    stack = "os"
    links = ["tcp", "udp"]
    interfaces = { eth0 = { kind = "ethernet", dhcp = true } }

A Zephyr board without the row keeps today's refusal, with its text changed to
say the board declares no interface yet. No Zephyr board gets a default
`network`: the image's file names it (the FVP's user-mode network is not QEMU's
`qemu`).

D2. **`nros build` renders the resolved interface into a Kconfig fragment.**
The resolver is the one every road already uses
(`orchestration/image_net.rs`, `build/nros/net/<image>.toml`). On a Zephyr
image it also writes `build/nros/net/<image>.conf`, and the Zephyr builder
appends it LAST to `EXTRA_CONF_FILE`, after the board's and the image's
`os_config` fragments:

| Resolved | Rendered |
| --- | --- |
| any address | `CONFIG_NET_CONFIG_SETTINGS=y`, `CONFIG_NET_CONFIG_AUTO_INIT=y`, `CONFIG_NET_CONFIG_NEED_IPV4=y` |
| `address = "a.b.c.d"` or `host = n` | `CONFIG_NET_DHCPV4=n`, `CONFIG_NET_CONFIG_MY_IPV4_ADDR="a.b.c.d"`, `CONFIG_NET_CONFIG_MY_IPV4_NETMASK` from the subnet's prefix |
| `[network.<n>] gateway` | `CONFIG_NET_CONFIG_MY_IPV4_GW="..."` |
| `address = "dhcp"` (needs `dhcp = true` on the network and on the board's interface, as today) | `CONFIG_NET_DHCPV4=y`, `CONFIG_NET_CONFIG_MY_IPV4_ADDR=""` |

`NET_CONFIG_AUTO_INIT` is rendered because `NET_CONFIG_NEED_IPV4` exists only
under it, and its default is `n` on a USB network device. `NET_DHCPV4=n` on a
static address is required, not tidy: with DHCPv4 on, net_config adds the
static address as `NET_ADDR_OVERRIDABLE` and a lease replaces it. What
Zephyr then does at boot, from the sources below:

- `init_app` runs as `SYS_INIT(..., APPLICATION, CONFIG_NET_CONFIG_INIT_PRIO)`
  and configures the FIRST auto-started interface (`iface_find_cb`, skipping
  `NET_IF_NO_AUTO_START`), not `net_if_get_default()`, which is only the
  fallback of a direct `net_config_init_by_iface(NULL, ...)` call.
- A static address is added, the netmask set by address and the gateway set,
  and `NEED_IPV4` is ready at once (no ACD).
- With DHCPv4 the client is started, and `init_app` blocks the APPLICATION
  init level until `NET_EVENT_IPV4_ADDR_ADD` or `CONFIG_NET_CONFIG_INIT_TIMEOUT`
  (default 30 s), then logs "Timeout while waiting network setup" and boots
  on. So the application needs no wait of its own. The client's own RFC 2131
  random initial delay, 1 s up to `CONFIG_NET_DHCPV4_INITIAL_DELAY_MAX`
  (default 10, range 2-10), still applies inside that wait; an image that
  wants a short boot states `NET_DHCPV4_INITIAL_DELAY_MAX=2` in its own
  fragment (not a rendered symbol, so D4 does not refuse it).
- With neither an address nor DHCPv4 on a native IPv4 stack the build fails
  with `#error "You need to define an IPv4 address or enable DHCPv4!"`, which
  the rendered rows never leave.

D3. **One interface in the first cut (ruled).** Zephyr's `NET_CONFIG_*`
configures one interface, the first auto-started one (D2), in 3.7.2 and in
4.4.0 alike. A Zephyr board's `[board.net]` that declares a second interface,
or an image file that addresses one, is refused: "Zephyr's NET_CONFIG
configures one interface (the first auto-started); `<if>` is a second --
configure it in the application, or remove it from the file". Per-interface
rendering (a generated `SYS_INIT` that walks the interfaces, as the island's
`network_config.cpp` does) is a follow-up if a consumer needs it.

D4. **Refuse a conflicting fragment, naming both sources (ruled).** An
`os_config` fragment that also sets one of the rendered symbols is refused,
naming the fragment, the symbol, its value there and the board network file
whose rendered value it contradicts. A warning with one value winning is the
issue-0876 class (a stated value silently losing), so it is not offered.

D5. **MAC and VLAN are not rendered.** A Zephyr Ethernet driver takes its MAC
from devicetree (`local-mac-address`) or generates one; a stated `mac` on a
Zephyr image is refused, and a derived MAC is not rendered. The collision
check across images still runs on addresses. A VLAN tag stays a board hook
(Zephyr has `CONFIG_NET_CONFIG_MY_VLAN_ID`, but it is not part of this cut).

D6. **Unchanged:** every refusal the resolver already makes (an address
outside its subnet or equal to the gateway, a network no file defines, two
images with one static address on a shared network).

D7. **Amends RFC-0103 D10d (ruled).** The resolver refuses an image off `qemu`
that states no `locator`. That rule is for an RMW that dials a router or an
agent, and a Cyclone DDS image has neither. It now applies only when the
image's RMW descriptor (`nros-rmw.toml`) states a host peer, `[rmw.host] peer`
other than `"none"` (today `zenoh-router` for zenoh and `xrce-agent` for XRCE).
It is read from the descriptor, not from a list of RMW names, so an
out-of-tree RMW takes the rule its descriptor states. Cyclone DDS and uORB
(`peer = "none"`) images are not refused. The book's `system-toml.md` and the
RFC-0103 D10d text say so.

## Source verification

Checked 2026-11-17 against `zephyrproject-rtos/zephyr` at tags `v3.7.2` (the
pin in `west.yml`) and `v4.4.0`. Paths under `subsys/net/lib/`.

| Fact | v3.7.2 | v4.4.0 |
| --- | --- | --- |
| `menuconfig NET_CONFIG_SETTINGS` | config/Kconfig:12 | config/Kconfig:12 |
| `NET_CONFIG_AUTO_INIT`, default `n` on a USB network device | config/Kconfig:25-28 | config/Kconfig:25-27 |
| `NET_CONFIG_NEED_IPV4`, inside `if NET_CONFIG_AUTO_INIT` | config/Kconfig:56 (if 34-64) | config/Kconfig:55 (if 33-63) |
| `NET_CONFIG_INIT_TIMEOUT`, default 30 | config/Kconfig:66-68 | config/Kconfig:65-67 |
| `NET_CONFIG_MY_IPV4_ADDR` / `_NETMASK` (default 255.255.255.0) / `_GW`, inside `if NET_IPV4` | config/Kconfig:99, 104-106, 111 (if 97) | config/Kconfig:98, 103-105, 110 (if 96) |
| `NET_DHCPV4`; `NET_DHCPV4_INITIAL_DELAY_MAX` default 10, range 2-10 | dhcpv4/Kconfig:8, 23 | dhcpv4/Kconfig:8, 23 |
| `#error` with neither address nor DHCPv4 | config/init.c:137-139 | config/init.c:138-140 |
| empty `MY_IPV4_ADDR` skips the static address | config/init.c:156 | config/init.c:157 |
| static address `OVERRIDABLE` under DHCPv4, else `MANUAL` | config/init.c:177, 179 | config/init.c:178, 180 |
| lease (`NET_EVENT_IPV4_ADDR_ADD`) marks IPv4 ready | config/init.c:108-112 | config/init.c:109-113 |
| `setup_dhcpv4` starts the client | config/init.c:58-62 | config/init.c:59-63 |
| `setup_ipv4` then `setup_dhcpv4` on the chosen interface | config/init.c:457-458 | config/init.c:448-449 |
| wait until ready or timeout, then "Timeout while waiting network setup" | config/init.c:471-476 | config/init.c:462-467 |
| first auto-started interface (`iface_find_cb`) | config/init.c:489-498, 521 | config/init.c:480, 512 |
| `NEED_IPV4` flag and the `INIT_TIMEOUT` wait | config/init.c:538-544 | config/init.c:529-535 |
| `SYS_INIT(init_app, APPLICATION, CONFIG_NET_CONFIG_INIT_PRIO)` | config/init.c:581 | config/init.c:565 |

## What the island would write

`zephyr/net-fvp-tap.toml`, named first in `[image.fvp-tap] os_config`,
replaces `CONFIG_NET_DHCPV4=n` and `CONFIG_NET_IFACE1_*`:

    [eth0]
    network = "tap"
    address = "192.168.10.2"

    [network.tap]
    subnet  = "192.168.10.0/24"
    gateway = "192.168.10.1"

and `zephyr/net-fvp.toml` for the user-mode images replaces
`CONFIG_NET_DHCPV4=y` and the 10 s sleep:

    [eth0]
    network = "fvp"
    address = "dhcp"

    [network.fvp]
    subnet = "172.20.51.0/24"   # the FVP user-mode network; to be measured
    dhcp   = true

## Test (when implemented)

- Unit, `image_net.rs`: the fragment text for a static and a DHCP interface;
  a second interface (board or image file), a stated MAC and a conflicting
  fragment refused, the last naming both sources.
- Unit, D7: an image off `qemu` with no `locator` is refused under a
  descriptor with `[rmw.host] peer = "zenoh-router"` and accepted under one
  with `peer = "none"` (an out-of-tree descriptor, so no RMW name decides).
- `zephyr.rs` builder test: the fragment is last in `EXTRA_CONF_FILE`.
- One Zephyr QEMU image (`mps2-an385-zephyr` on `qemu`) built with a board
  network file, `build/zephyr/.config` carrying the rendered address, and the
  image reaching its router.
