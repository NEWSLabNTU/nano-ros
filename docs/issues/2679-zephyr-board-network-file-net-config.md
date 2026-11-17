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
**State: DESIGN, awaiting a ruling** (it adds public config surface: a Zephyr
board's `[board.net] interfaces`, and a generated Kconfig fragment).

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
| any address | `CONFIG_NET_CONFIG_SETTINGS=y`, `CONFIG_NET_CONFIG_NEED_IPV4=y` |
| `address = "a.b.c.d"` or `host = n` | `CONFIG_NET_DHCPV4=n`, `CONFIG_NET_CONFIG_MY_IPV4_ADDR="a.b.c.d"`, `CONFIG_NET_CONFIG_MY_IPV4_NETMASK` from the subnet's prefix |
| `[network.<n>] gateway` | `CONFIG_NET_CONFIG_MY_IPV4_GW="..."` |
| `address = "dhcp"` (needs `dhcp = true` on the network and on the board's interface, as today) | `CONFIG_NET_DHCPV4=y`, `CONFIG_NET_CONFIG_MY_IPV4_ADDR=""` |

Zephyr's `net_config` then applies the address at boot, or starts DHCP and
waits for the lease up to `CONFIG_NET_CONFIG_INIT_TIMEOUT`, so an application
needs no wait of its own. The symbol names and the DHCP wait are to be
checked against Zephyr v3.7.2 `subsys/net/lib/config/init.c` before the
implementation lands.

D3. **One interface.** Zephyr 3.7's `NET_CONFIG_*` configures the default
interface only. A file that addresses a second interface on a Zephyr image is
refused, naming the interface, and saying a second interface stays the
application's to configure. Per-interface rendering (a generated `SYS_INIT`
that walks `net_if_get_by_index`, as the island's `network_config.cpp` does)
is a follow-up if a consumer needs it.

D4. **No silent last-wins.** An `os_config` fragment that also sets one of the
rendered symbols is refused, naming the fragment, the symbol and both values.
The image states its address in one place.

D5. **MAC and VLAN are not rendered.** A Zephyr Ethernet driver takes its MAC
from devicetree (`local-mac-address`) or generates one; a stated `mac` on a
Zephyr image is refused, and a derived MAC is not rendered. The collision
check across images still runs on addresses. A VLAN tag stays a board hook.

D6. **Unchanged:** every refusal the resolver already makes (an address
outside its subnet or equal to the gateway, a network no file defines, two
images with one static address on a shared network).

## Open questions for the ruling

- Q1. D4 refuses a conflicting fragment. A warning with the rendered value
  winning is the softer choice; refusal is proposed because the fragment is
  the place the island would otherwise forget to clean up.
- Q2. The resolver refuses an image off `qemu` that states no `locator`
  (RFC-0103 D10d). A Cyclone DDS image has no router. Is D10d to be scoped to
  router RMWs, or does a Cyclone image on Zephyr keep stating its address in
  Kconfig until it is?
- Q3. Is the single-interface limit (D3) acceptable for a first cut? It covers
  both of the island's Zephyr images today.

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
  a second interface, a stated MAC and a conflicting fragment refused by name.
- `zephyr.rs` builder test: the fragment is last in `EXTRA_CONF_FILE`.
- One Zephyr QEMU image (`mps2-an385-zephyr` on `qemu`) built with a board
  network file, `build/zephyr/.config` carrying the rendered address, and the
  image reaching its router.
