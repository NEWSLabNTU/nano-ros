---
rfc: 0109
title: "Network and middleware configuration in three layers: the OS owns the
  devices, the RMW's own file owns the wiring, nano-ros owns only ROS-level facts"
status: Draft
since: 2026-10
last-reviewed: 2026-10-24
implements-tracked-by: [phase-514]
supersedes: []
superseded-by: null
---

# RFC-0109 — Network and middleware configuration in three layers

**Status:** Draft (2026-10-24). **Amends** RFC-0004 (§"No NIC axis", the
2026-09-04 amendment, §6 `[[transport]]`), RFC-0103 D10 / D10a–D10g, and
RFC-0098 D4/D11 (`conf`). **Closes** the design question phase-206 left open
(phase-206 is archived when this lands).

## Summary

On Linux, a ROS 2 user never writes a NIC into a node. The operating system
brings the devices up (`ip`, netplan, `ip link set can0 type can bitrate …`),
the middleware's own file wires sessions to them (`CYCLONEDDS_URI`, a zenoh
json5), and ROS states only ROS-level facts (`ROS_DOMAIN_ID`, launch). Node
code owns none of it, which is what keeps it portable.

nano-ros adopts the same three layers on every platform, including RTOS and
bare metal:

| layer | holds | owner | nano-ros |
| --- | --- | --- | --- |
| **1 devices** | which NICs / CAN / UARTs exist; IP or DHCP; bitrate; baud; Wi-Fi association | **the OS** — on bare metal and on kernels with no network config of their own, **the board package** | reads facts only |
| **2 RMW wiring** | `connect` / `listen`, peers, which interface a session binds, scouting, middleware tuning | **the RMW's own file** (`cyclonedds.xml`, `zenoh.json5`) | carries it verbatim; lowers it where the target has no parser |
| **3 ROS level** | rmw per domain, domain id, which node joins which domain, bridges, QoS | **`system.toml`** | owns it |

One more class sits beside them: **runner** facts (how an emulator attaches
— test and launch tooling, not firmware). Secrets (Wi-Fi passwords, TLS keys)
are a fourth, named here and not designed: no in-tree road consumes one
today.

The consequence that matters: **nano-ros config states no device fact and no
wiring fact.** Every such key in `system.toml` today is either moved to the
layer that owns it or retired.

## Problem

Four documents describe overlapping pieces, and they disagree:

1. **Two session spellings.** RFC-0004 §6 describes `[[transport]]` (`ip`,
   `mac`, `interfaces`, `locator`, …). The schema still parses it; the plan
   refuses its address keys (`RETIRED_TRANSPORT_ADDRESS_REMEDY`); no example
   uses it. The live spelling is `[[domain]]` (`name`, `rmw`, `id`,
   `locator?`) plus `[[bridge]]`.
2. **Singular image fields against a two-session image.** RFC-0098 gives an
   image one `rmw`, one `transport`, one `locator`. A bridge image (CAN zenoh
   + Ethernet DDS) has two sessions on two links.
3. **Wiring stated twice.** RFC-0103 D10d derives a router from
   `[network.<name>] router`; `[image] locator` and `[system] locator` state
   it too; phase-206's verbatim `rmw/zenoh.conf` can say `connect = …`, which
   zenoh-pico APPENDS. Nothing says which owner wins.
4. **Devices stated by nano-ros where the OS already owns them.** RFC-0103
   D10 states interfaces and addresses for every non-host board, including
   NuttX, whose `defconfig` / netinit already owns them. Zephyr images
   already state networking in `prj*.conf` (passed through `conf`), so a
   Zephyr image with D10 rows would have two owners — Zephyr's own config is
   applied last-wins (issue 0876's class).

And one design pressure the earlier documents did not have: **multi-NIC is
real**, mostly at bridges between two RMW backends (a vehicle control unit on
CAN and Ethernet), and in robots on two links of one network (Wi-Fi mesh +
wired dock).

## Practice this is drawn from (measured 2026-10-24)

| platform | who brings the network up | where the settings live | "ready" signal |
| --- | --- | --- | --- |
| Linux | the OS | `ip`, netplan, NetworkManager; `ip link set can0 type can bitrate 500000` (ros2_socketcan's README: create the interface BEFORE the node, which only names `can0`) | — |
| Zephyr | Zephyr | devicetree (`can0`, `bus-speed`), Kconfig `NET_CONFIG_*` (one interface; multi-interface samples hand-write one Kconfig symbol per interface) | `net_mgmt` events |
| NuttX / PX4 | NuttX | `defconfig` `CONFIG_NETINIT_*`; PX4 keeps the agent address in its own parameters (`UXRCE_DDS_AG_IP`, `UXRCE_DDS_CFG`), not in netinit | netinit |
| ESP-IDF | ESP-IDF | `esp_netif` + `menuconfig` (micro-ROS's component puts Wi-Fi SSID/password AND agent IP there) | `IP_EVENT_STA_GOT_IP` |
| FreeRTOS + FreeRTOS-Plus-TCP | **the application** | `FreeRTOSIPConfig.h` (stack options); addresses are arguments to `FreeRTOS_FillEndPoint()` before `FreeRTOS_IPInit_Multi()` | `vApplicationIPNetworkEventHook_Multi()` |
| FreeRTOS + lwIP | **generated BSP code** | CubeMX `.ioc` → `MX_LWIP_Init()` → `netif_add()`; `lwipopts.h` | netif link/status callback |
| ThreadX + NetX Duo | **app / BSP** | `nx_ip_create(…, nx_stm32_eth_driver, …)`, `nx_dhcp_create()` in `tx_application_define` (CubeMX X-CUBE-AZRTOS → `app_netxduo.c`) | address-change notify |
| bare metal (Embassy, RTIC + smoltcp, esp-hal) | **the app's `main`** | `embassy_net::Config::{dhcpv4, ipv4_static}` | `stack.wait_config_up()` |

Middleware wiring, everywhere it exists, is separate from device setup and
names devices by the platform's own name: zenoh `tcp/192.168.0.1:7447#iface=eth0`,
Cyclone `<NetworkInterface name="eth0"/>`, our zenoh-pico fork's
`isotp/<device>#tx_id=…;rx_id=…`. Fleets share one Cyclone template and fill
per-robot values through `${ENV}` or name peers by hostname, because DHCP.

## Design

### D1 — Three layers, one owner each

The table in the Summary is normative. A fact belongs to exactly one layer,
and only that layer's owner may state it. Node code states none of them.

### D2 — Who "the OS" is, per platform

| platform family | layer-1 owner | where it is written | descriptor `[board.net] stack` |
| --- | --- | --- | --- |
| Linux, Zephyr `native_sim`, `freertos-posix`, `threadx-linux` | the host OS | outside nano-ros | `"host"` (RFC-0103 D10e, unchanged) |
| Zephyr (real boards) | Zephyr | the user's `prj*.conf` + devicetree overlay | `"os"` |
| NuttX | NuttX | a `defconfig` fragment | `"os"` |
| FreeRTOS (lwIP, FreeRTOS-Plus-TCP) | **the board package** | the board package's own config file (D4), or the user's own bring-up inside the board package (D5) | `"board"` |
| ThreadX (NetX Duo) | **the board package** | as above | `"board"` |
| bare metal (smoltcp, esp-hal, Embassy) | **the board package** | as above | `"board"` |

`[board.net] stack` is RFC-0103 D10e's predicate, extended from one value
(`"host"`) to three: who owns layer 1. It is deliberately NOT read off
`supported_netstacks`, which keeps its meaning (which stack crates a
`"board"`-stack board can be built with: `["smoltcp"]`, `["lwip"]`,
`["netxduo"]`) and its `[]` on Zephyr and NuttX, where `policy.rs` relies on
`[]` not moving the transport policy (D10e's 2026-10-15 correction).

ESP-IDF is not a row: RFC-0065 D3 deleted the `idf.py` road and esp32 builds
bare-metal (`nros-board-esp32-qemu`, smoltcp). Should an ESP-IDF road return,
it is an `"os"` row whose layer-1 file is `sdkconfig.defaults`.

### D3 — What nano-ros reads (the whole contract)

- **Build time, from the board descriptor:** `[board.net] stack` (D2),
  `supported_netstacks`, and a new `links` — the link kinds the board can
  carry, from ONE vocabulary, `LINK_KINDS` in `nros-platform-config`:
  `serial`, `tcp`, `udp`, `can`, `isotp`, `ivc`, `raweth`, `custom`. It is a
  superset of the image-side `TRANSPORT_KINDS` (`serial | tcp | udp |
  custom`, pinned by a test) and maps onto zenoh-pico's `link-*` features (`tcp` / `udp` →
  `link-ip`). *(Landed: phase-514 W1.)* These choose which
  zenoh-pico / XRCE link code is compiled (today's `[image] transport`
  implication, now derived: D6). An image whose wiring names a link the board
  lacks is refused at sync. Cyclone's link is always `udp`; a
  `nros_transport_ops_t` transport is `custom`.
- **Run time:** nothing new. Bring-up and "the network is ready" stay
  `BoardEntry::run`'s job, before the executor opens a session. Phase-206 W4
  deleted the `NetworkWait` / `TransportBringup` traits for a reason that still
  holds: "skip if the board does not implement it" cannot be expressed in Rust
  (coherence), and the board already owns the boot sequence. D5 is therefore
  a board package, not a hook.
- **Never, as a statement in nano-ros config:** an address, netmask, gateway,
  MAC, interface list, bitrate, baud or SSID. On a `"board"`-stack board
  these live in the board package's file (D4); nano-ros READS them through
  that package's parser.

### D4 — Where nano-ros is the OS, the board package owns a small config file

On FreeRTOS, ThreadX and bare metal there is no OS-level network config to
defer to. The board package plays that role, and its configuration is the
board package's own file, in the board package's own format. nano-ros core
does not define that format; it reads the file only through the board
package's own parser.

- The image names it with `os_config` (D6), exactly as a Zephyr image names
  its `prj.conf`.
- The in-tree reference boards (mps2-an385, mps2-an385-freertos,
  mps3-an536-freertos, esp32-qemu, threadx-qemu-riscv64, s32z270-freertos)
  share ONE format. *As built (phase-514 W2)* it is one table per interface
  carrying the fields the retired inline `net` entry had, read by
  `nros_orchestration_ir::system_net::read_board_net_file`:

  ```toml
  # board-net.toml, beside system.toml; named by [image.<id>] os_config
  [eth0]
  network = "qemu"              # the network it sits on (until W4/W6 move it)
  host = 10                     # or: address = "192.0.3.10" | "dhcp"
  mac = "02:00:00:00:00:10"     # optional; derived when absent
  ```

  The netmask and gateway still come from the network the interface names;
  they move into this file when W4 retires `[network.<name>]`. A CAN or
  serial interface's device facts (`[can0] bitrate`, `[uart1] baud`) join the
  same file when W5 moves controller bring-up out of the link.
- **Where it is parsed.** `nros sync` and `nros build` parse every image's
  file (`system_net::fold_board_net_files`, on the leaf and workspace roads
  both) and render the result
  into the ONE carrier phase-497 built, `NROS_IMAGE_NET` (RFC-0103 D10f), on
  all three roads. So the C-startup board (`threadx-qemu-riscv64`, which bakes
  its address from that carrier), the QEMU launcher (which must pin the same
  virtio MAC the firmware uses) and the Rust board crates keep reading one
  resolved answer, and no road gains a new carrier (issue 0460/0491's class).
- **Cross-image checks stay**, because sync sees every image: two equal
  static addresses or MACs on one shared bridge, and a MAC derived from
  `(system, image, interface)` when the file states none (RFC-0103 D10b's
  rules, moved here unchanged).
- **A default ships with the board.** A reference board package carries a
  default file (today's `qemu` network: `10.0.2.15/24`, gateway `10.0.2.2`),
  so an image that names no `os_config` on such a board is legal and boots on
  QEMU user-mode networking exactly as it does today.
- An out-of-tree board package may use any format it likes, or none (D5); it
  then supplies its own answer to `NROS_IMAGE_NET`'s consumers, or brings the
  network up from its own code.

### D5 — A user BSP is a board package

A production board usually already has its bring-up code, generated or hand
written. The user writes a board package (RFC-0012) whose boot sequence calls
it and waits on that stack's own ready signal before running the entry; no
`os_config` is needed:

```c
/* in the user's board package, inside its run / boot sequence */
MX_LWIP_Init();                                   /* CubeMX owns the IP */
while (!(netif_is_up(&gnetif) && netif_is_link_up(&gnetif))) { osDelay(10); }
nros_board_run_entry();                           /* then nano-ros starts */
```

A Rust bare-metal user who owns an Embassy stack does the same in Rust: build
the stack, `stack.wait_config_up().await`, then run the entry with that stack.
The spelling of handing a user-built stack to the entry is left to the
builder.

### D6 — `system.toml` shape

- **`[[domain]]` is the only spelling of a session.** Fields: `name`, `rmw`,
  `id`, and `config = "rmw/<file>"` (the RMW's own file, layer 2).
- **An image with no `[[domain]]` has one implicit default domain**, built
  from `[system] rmw` / `domain_id` and the image's own `rmw` / `domain_id`
  overrides (RFC-0103 §3, unchanged), whose wiring is `[image] rmw_config`
  or `[system] rmw_config` when either is given. So the single-session images
  that make up nearly the whole tree change nothing; `[[domain]]` is needed
  only for a second session. In a multi-domain project every component joins
  the default domain unless it names one (RFC-0004 §"Runtime mapping": a node
  binds to exactly one transport; N > 1 ⇒ each names its own).
- **`locator` on a domain** is permitted only when that domain has no
  `config` — the case for a backend with no file format (XRCE), or a quick
  start. A domain with both is refused. The implicit default domain's
  `locator` is spelled `[image.<id>] locator` / `[image_defaults] locator` /
  `[system] locator` (most specific first), and is refused beside an
  `rmw_config` the same way (amendment 2026-10-25, phase-514 W4: 88 tracked
  single-session images state one locator line; moving each into an RMW file
  would add a file per image and gain no single owner it lacks today).
- **`[[bridge]]`** is unchanged (RFC-0009).
- **`[image.<id>]`** keeps `board`, `rmw` (its default domain's backend),
  `env`, `knobs`, and gains:
  - `os_config = [ … ]` — layer-1 files for that platform; generalises today's
    Zephyr-only `conf` (D11), which becomes an alias then retires (alias
    since phase-514 W4: read before `os_config`'s own entries, a table stating
    both refused; lists concatenate over `[image_defaults]`);
  - `rmw_config = "<file>"` for the default domain, or
    `{ <domain> = "<file>" }` — a per-image override of a domain's `config`,
    for the case where wiring differs per image (two ECUs with different CAN
    ids, a router that differs between a QEMU run and the lab).
- **`[image.<id>.run]`** — runner facts: `nic = "slirp" | { tap = "<dev>" } |
  { bridge = "<dev>" }`. Read by the launcher and the test harness only.
- **Retired** (each refused with a remedy naming its new home):
  `[[transport]]` (whole table), `[image] locator` / `[system] locator`
  **on an image that names an `rmw_config`** (→ that file; amended
  2026-10-25, see `locator` above), `[image] transport` (→ derived from the
  domain's wiring + the board's `links`; since phase-514 W4, the scheme of the
  default domain's locator, `custom://` naming link kind `custom`), `[image.<id>.net.*]` and `[board.net]` addresses (→ D4's file),
  `[network.<name>]` (→ `router` to the image's `locator` or its RMW file,
  `attach` to `[image.<id>.run] nic`, subnet/gateway/dhcp to a
  `[network.<name>]` table in D4's file, beside the interface on it; two
  images' files naming one network must agree).
- **Kept:** `[system] domain_id` / `rmw` and their `[image]` overrides
  (layer 3).

### D6a — Run-time overrides on hosted builds

On a hosted build (Linux, `native_sim`, `*-posix`, `threadx-linux`) the
environment overrides the baked wiring at run time, as it does for every ROS 2
process: `NROS_LOCATOR`, `ROS_DOMAIN_ID`, `ZENOH_SESSION_CONFIG_URI` (a json5
read at run time by the same lowering code D8 runs at sync, which hosted
builds link) and the `ZENOH_*` keys the zenoh shim already reads. These are
RFC-0103 D2's "override, not a source": they win over the file and are never
a second authored home. Test harnesses rely on exactly this to give each test
its own router and domain. Embedded builds have no environment; their wiring
is what was baked.

### D7 — Interface and device names are the platform's own

nano-ros does not invent a portable interface vocabulary (RFC-0004's reason
stands: `eth0` is a Linux name, Zephyr names a devicetree node, smoltcp has
one unnamed interface). Layer-2 files use the platform's own names, as they do
on Linux. A wiring file therefore targets a platform; where names differ, the
project keeps one file per platform and selects it per image (`rmw_config`).
On a board package that is the OS (D4), the board package's file names its
interfaces, and wiring uses those names.

### D8 — Layer 2 is carried verbatim, and lowered on the host where needed

- **CycloneDDS:** the file's bytes are handed to `dds_create_domain` as one
  source (unchanged from phase-206 W2). `<Domain id>` must be `any` or equal
  to the domain's `id`; anything else is refused.
- **zenoh (json5):** the format is upstream zenoh's session config, as
  rmw_zenoh's `ZENOH_SESSION_CONFIG_URI` uses. On a target with zenoh-pico,
  `nros sync` parses it ON THE HOST and lowers each key to a
  `zp_config_insert` property (the phase-206 W3 seam). A key zenoh-pico does
  not support is refused, never dropped. **There is no json5 parser on an
  embedded target**; hosted builds link the same lowering code for D6a. The
  flat `rmw/zenoh.conf` format retires (the tree tracks none; its bake path in
  `cmake/NanoRosRmwUserConfig.cmake` is replaced, which is new work rather
  than a rename).
- **XRCE:** no file format exists; the domain's `locator` is the sole owner.
- **Amendment (phase-514 W3a, 2026-10-25) — how the default domain finds its
  file, and what the file displaces.** Until `[[domain]] config` lands, the
  default domain's file is found BY CONVENTION beside the `system.toml`:
  `rmw/zenoh.json5` for a zenoh image, `rmw/cyclonedds.xml` for a Cyclone one
  (the backend is declared, so the name follows; RFC-0087). Three rules the
  lowering needed that the text above left open, taken as calls:
  1. **The file is the wiring.** With a file, the image bakes no locator (a
     `[system] locator` is unused; an image's own is refused, D9-2), and the
     zenoh shim treats the file's `connect` as REPLACING the backend's default
     router, never as a second endpoint dialed beside it. `mode: "peer"` in
     the file makes the session a peer whatever the API said — which on the
     shipped shim is refused, because peer mode needs zenoh-pico's multicast
     transport (issue 0682).
  2. **`ZENOH_SESSION_CONFIG_URI` replaces the baked table, whole** (D6a). It
     is not merged key by key: rmw_zenoh reads it as a complete session
     config, and so do we.
  3. **One lowering, one reader** — `nros-zenoh-session-config`, a small
     hand-written json5 reader (the `json5` crate pulls `pest` into every
     board lock). The build and the hosted runtime call the same `lower`.

- **Amendment (phase-514 W3b, 2026-10-25) — naming a file, and a domain's
  own.** Calls taken while implementing:
  1. **Precedence for the default session** is `[image.<id>] rmw_config =
     "<file>"` (over `[image_defaults]`), then `[system] rmw_config`, then the
     conventional file. A NAMED file that does not exist is refused, never
     read as "no file"; a backend with no file format (XRCE) refuses one,
     naming `locator`.
  2. **A `[[domain]]` is a session a `[[bridge]]` opens.** The examples above
     that show a lone `[[domain]] main` as an image's session are spelled
     `[system] rmw_config` until a lone domain becomes the default session;
     a `[[domain]] config` that no bridge opens is refused, because nothing
     would read it. A named domain has no conventional file (the convention
     is the default session's). `rmw_config = { <domain> = "<file>" }`
     overrides one domain's file and must name a declared domain — and is
     refused at sync until it has a carrier (issue 2062, see 4).
  3. **A domain's file travels as its session's properties**, so one image
     can open two domains with two files: a `session-json5` file lowered, a
     `verbatim-xml` one as its whole text under the one neutral key
     `config` (`nros_orchestration_ir::session_properties::DOCUMENT`).
     Cyclone takes that key in place of its baked document, under the
     baseline and under `CYCLONEDDS_URI`; every other key is still refused.
     Composition is Cyclone's own: a scalar the environment states wins,
     but a LIST element (`<Interfaces>`, `<Peers>`) appends to the file's —
     measured on the U5 example, where a loopback file plus the test
     harness's loopback `CYCLONEDDS_URI` stopped delivery, and a scalar-only
     file did not.
     The cffi seam copies a value past its 256-byte stack buffer to the heap
     on an `alloc` build and refuses it without one.
  4. **The bridge's own runtime overrides** — `NROS_BRIDGE_<N>_LOCATOR`
     replaces a domain file's wiring whole, as `ZENOH_SESSION_CONFIG_URI`
     replaces a baked table (D6a); kept, the file's `connect` would outrank
     it and the override would be silently unread. The bridge config is one
     per bringup, so an image's per-domain override reaches a bridge only
     through the bringup's `[[domain]] config`.
  5. **One resolver on every road.** The CMake road asks
     `nros ws rmw-config <bringup> --rmw <backend> [--image <id>]` (the
     generated workspace root passes each entry's `IMAGE`); it no longer
     spells `rmw/zenoh.json5` or `rmw/cyclonedds.xml` itself. Without an
     image, the images on that backend must agree on one file or the
     configure is refused, naming `-DNROS_IMAGE`.

### D9 — Checks at `nros sync`

1. A retired key (D6) is refused, naming its new home.
2. A domain with both `config` and `locator` is refused.
3. Cyclone `<Domain id>` other than `any` / the domain's `id` is refused.
4. A zenoh json5 key zenoh-pico cannot take is refused (zenoh-pico targets).
5. Wiring that names a link kind the board's `links` lacks is refused.
6. A layer-1 statement in the wrong place is refused — e.g. a D4 file on a
   Zephyr board, `os_config` naming a `prj.conf` on a bare-metal board.
7. `nros config explain` reports, per session, which file and line its
   endpoint came from.

### D10 — Moves out of the middleware

Our zenoh-pico fork's CAN / ISO-TP link calls `can_start()` itself, and its
locators may carry `bitrate` / `dbitrate` (RFC-0081, RFC-0083). Bringing a
controller up and setting its rate are layer 1:

- On Linux the rate is already out of band (`ip link`); RFC-0081 calls the
  locator keys advisory there. They become refused in wiring.
- On Zephyr, devicetree states the controller and its `bus-speed`, but no
  configuration-only way STARTS it. The `can_start()` call moves from the link
  into nano-ros's Zephyr board glue, which runs before any session opens.
- On a `"board"`-stack board, the board package starts it, from `[can0]
  bitrate` in its D4 file.

The link then assumes a started controller and reports a clear error when it
is not. The fork change follows the vendored-fork branch workflow.

## Examples by platform

Each example is the smallest complete statement for that platform. The RMW
file is shown once where it is the same everywhere.

**Linux (host OS owns everything below ROS).**
```toml
[[domain]]
name = "main"
rmw = "cyclonedds"
id = 3
config = "rmw/cyclonedds.xml"

[image.host]
board = "linux"
```
```xml
<CycloneDDS><Domain id="any"><General><Interfaces>
  <NetworkInterface name="eth0"/>
</Interfaces></General></Domain></CycloneDDS>
```

**Zephyr `native_sim` (NSOS: the host owns the network).** Same as Linux;
`board = "zephyr"`, `os_config = ["prj.conf"]` for Zephyr's own non-network
settings. No network lines in it.

**Zephyr on a real board.**
```toml
[image.node]
board = "mps2-an385-zephyr"
os_config = ["prj.conf", "prj-net.conf"]     # Zephyr owns eth0
rmw = "zenoh"

[[domain]]
name = "main"
rmw = "zenoh"
config = "rmw/zenoh.json5"
```
```
# prj-net.conf — Zephyr's own language
CONFIG_NET_CONFIG_SETTINGS=y
CONFIG_NET_CONFIG_MY_IPV4_ADDR="192.0.3.20"
CONFIG_NET_CONFIG_PEER_IPV4_ADDR="192.0.3.1"
```
```json5
// rmw/zenoh.json5
{ mode: "client", connect: { endpoints: ["tcp/192.0.3.1:7447"] } }
```

**NuttX.**
```toml
[image.node]
board = "nuttx-qemu"
os_config = ["defconfig.net"]                # CONFIG_NETINIT_IPADDR=0xc0000314 …
```

**esp32 (bare metal, esp-hal + smoltcp — today's road).**
```toml
[image.esp]
board = "esp32"                              # nros-board-esp32-qemu, stack = "board"
os_config = ["board-net.toml"]               # or none: the board default
```

**FreeRTOS + lwIP, in-tree reference board (QEMU).**
```toml
[image.talker]
board = "mps2-an385-freertos"
os_config = ["board-net.toml"]               # D4: board format, read by nros sync
```

**FreeRTOS + lwIP, user's CubeMX board.** `board = "my_stm32h7"`, no
`os_config`; the board package wraps `MX_LWIP_Init()` (D5).

**FreeRTOS + FreeRTOS-Plus-TCP, user's board.** The user's board package
calls the app's `FreeRTOS_FillEndPoint()` + `FreeRTOS_IPInit_Multi()` in its
boot sequence and waits for the flag `vApplicationIPNetworkEventHook_Multi()`
sets before running the entry (D5).

**ThreadX + NetX Duo, in-tree reference (RISC-V QEMU).**
```toml
[image.node]
board = "threadx-qemu-riscv64"
os_config = ["board-net.toml"]               # → NROS_IMAGE_NET → the board's nx_ip_create()
```

**ThreadX on Linux (`threadx-linux`).** Host OS; like Linux.

**Bare metal + smoltcp, in-tree reference (mps2-an385).**
```toml
[image.talker]
board = "mps2-an385"
os_config = ["board-net-talker.toml"]

[image.listener]
board = "mps2-an385"
os_config = ["board-net-listener.toml"]
```

**Bare metal, user's own Embassy stack.**
```rust
// in the user's board package (D5); handing the stack to the entry is the builder's spelling
let stack = embassy_net::new(driver, Config::dhcpv4(Default::default()), res, seed);
stack.wait_config_up().await;
```

## Examples by use case

**U1 — one Ethernet MCU, static IP, zenoh client to a router.** The
FreeRTOS / bare-metal examples above; the router is in `rmw/zenoh.json5`.

**U2 — Wi-Fi with DHCP.** Layer 1 states `dhcp = true` (D4's file, or the
RTOS's own config). Wiring names the router by address or hostname; nothing
needs the device's own address. Wi-Fi credentials are a secret and are NOT
decided here (below).

**U3 — serial only, no IP.** The board's `links` include `serial`; the device
(UART, baud) is layer 1 (`[uart1] baud = 115200` in D4's file, or the RTOS's
devicetree). The wiring file connects `serial/uart1`.

**U4 — a CAN-only node (zenoh over ISO-TP).**
```json5
{ mode: "client", connect: { endpoints: ["isotp/can0#tx_id=0x101;rx_id=0x181"] } }
```
`can0` and its bitrate are layer 1 (Zephyr devicetree `bus-speed`, Linux
`ip link`, or `[can0] bitrate` in D4's file).

**U5 — vehicle control unit: CAN to the ECUs, Ethernet to the planner,
bridged** (two NICs, two purposes, two domains).
```toml
[[domain]]
name = "ecu"
rmw = "zenoh"
id = 0
config = "rmw/zenoh-can.json5"

[[domain]]
name = "ctl"
rmw = "cyclonedds"
id = 5
config = "rmw/cyclonedds-eth.xml"

[[bridge]]
name = "gw"
from = "zenoh:ecu"
to = "cyclonedds:ctl"
topics = ["/vehicle/speed", "/cmd"]

[image.ccu]
board = "s32k344"
os_config = ["prj-net.conf", "boards/s32k344.overlay"]
```

**U6 — one domain over Wi-Fi and Ethernet (multipath / ad-hoc mesh).** One
domain; its Cyclone file lists both interfaces:
```xml
<Interfaces>
  <NetworkInterface name="wlan0" priority="1"/>
  <NetworkInterface name="eth0"  priority="2"/>
</Interfaces>
<Peers><Peer address="robot-dock.local"/></Peers>
```
On zenoh-pico this is FAILOVER only (`connect` endpoints are alternatives in
client mode; in peer mode extra endpoints are TCP/UDP peers of one primary
link). Stated, not promised.

**U7 — two Ethernet NICs, separate networks, same RMW.** Two domains, each
`config` binding its own interface (`#iface=eth0`, `#iface=eth1`), joined by a
`[[bridge]]` if anything crosses.

**U8 — XRCE agent.** No file format; the domain's `locator` is the owner:
```toml
[[domain]]
name = "main"
rmw = "xrce"
id = 0
locator = "udp/192.168.1.5:8888"
```

**U9 — one image, QEMU run vs lab deployment.** `[image.<id>.run] nic =
"slirp"` for the run; the router differs, so the lab image names a different
wiring file: `rmw_config = { main = "rmw/zenoh-lab.json5" }`.

**U10 — a fleet sharing one wiring file.** One `config` for every image; per-
robot differences are hostnames (peers) or DHCP, never addresses nano-ros owns.

**U11 — several ECUs on one CAN bus.** CAN ids are wiring, and differ per
image: each image's `rmw_config` names its own file. Bitrate is shared and
layer 1.

## What this changes in other documents

- **RFC-0004:** §6 `[[transport]]` is deleted; "No NIC axis" stays and points
  here; the 2026-09-04 amendment's "nano-ros must not add a JSON5 parser"
  narrows to "no parser on the target" (D8).
- **RFC-0103 D10:** D10/D10a–D10d, D10f, D10g are superseded by D2–D6 here.
  D10e (host-OS boards own no network facts) generalises to every board whose
  OS owns its network. D10b's collision refusal moves into
  `nros-board-common`'s D4 parser (one image set, one board format).
- **RFC-0098:** D4's `transport` implication becomes derived (D6); D11's
  `conf` generalises to `os_config`.
- **phase-206:** archived; its two open acceptance items are replaced by this
  RFC's work (the hosted multi-homing case is U6, the middleware's own feature).

## Decisions taken while drafting (2026-10-24)

Recorded so a reader can tell a decision from a default. Each was weighed
against the alternative named:

1. Reference board files are parsed by `nros sync` and carried by
   `NROS_IMAGE_NET` (not: a new per-road carrier read by each board build) —
   keeps cross-image collision checks and the launcher/firmware MAC pairing.
2. An image with no `[[domain]]` has an implicit default domain (not: every
   image migrates to `[[domain]]`).
3. Hosted builds keep run-time environment overrides (D6a) (not: baked wiring
   only) — every native test depends on them.
4. Reference boards ship a default D4 file (not: every example states one).
5. No new board-ABI hooks; bring-up stays `BoardEntry::run`'s (not: weak C
   symbols) — phase-206 W4's coherence finding.
6. `[board.net] stack = "host" | "os" | "board"` beside an unchanged
   `supported_netstacks` (not: overloading `supported_netstacks`).
7. ESP-IDF is not a supported row; secrets are named, not designed.
8. `can_start()` moves to nano-ros's Zephyr board glue (not: left in the
   link) — Zephyr offers no configuration-only start.
9. `links` includes `custom`; Cyclone's link is `udp`.
10. Per-image wiring differences use a per-image `rmw_config` file (not:
    `${…}` substitution into a shared file) — with device facts out of
    nano-ros config, substitution would have almost nothing to substitute.

## Left to the builder

- The exact name and location of the reference board config file
  (`board-net.toml` above) and its field spellings, provided ONE format is
  shared by the in-tree reference boards.
- The order in which retired keys turn from warnings into refusals, and the
  migration of every tracked use (fixture rows that pass a per-image IP, the
  38 `system.toml` files D10g migrated, NuttX's `[board.net]`).
- The spelling of handing a user-built Embassy stack to a Rust entry (D5).

## Not decided here

- Run-time rewiring (a micro-ROS-style API, a parameter store): out of scope.
- Zephyr multi-interface bring-up beyond what `NET_CONFIG` does: the user's
  Zephyr configuration, not nano-ros.
- Secrets (Wi-Fi credentials, TLS material): their carrier is designed when
  a road consumes one. Until then they never appear in `system.toml`, a D4
  file or an RMW file that is tracked.
- IPv6.
