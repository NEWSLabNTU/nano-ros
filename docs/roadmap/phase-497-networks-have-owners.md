# Phase 497 -- networks have owners: board interfaces, project networks, image addresses

**Status (2026-10-16). W1, W2 and W4 landed; W3 open.** Re-planned against D10 as
amended 2026-10-15 (D10a–D10g). Implements
[RFC-0103 D10](../design/0103-one-owner-per-fact.md). Follows phase-484 W4b-2,
which moves every QEMU board onto one default network first.

## Why

Before RFC-0103 D10, a board's network identity was one per board. That
model cannot express:

- **a bridge**: many images, or many instances of one board, on ONE network;
- **one board on several networks**: a QEMU run, a lab bridge, a docker
  network;
- **a board with several NICs**.

The tree had already worked around it in three different ways:

- per-board SLIRP subnets (`10.0.2.30` / `.40` / `.50` entry defaults beside
  `192.0.3.10` presets);
- a `docker` cargo feature on `mps2-an385` carrying a third network;
- a per-image `NROS_ENTRY_IP_LAST` / `NROS_APP_NET_IP_LAST` override, which is
  the only thing keeping two QEMU nodes of a test pair from sharing an IP+MAC.
  Sharing one gives identical zenoh ZIDs and silent no-delivery (phase-287 W6).

D10 gives each fact its owner:

- the board states **interfaces** (`[board.net]`);
- the project states **networks** (`system.toml` `[network.<name>]`);
- the image states its **address** on each (`[image.<id>.net.<ifname>]`).

## Work items

### W1 -- the schema, and retiring the single-NIC spellings

- `system.toml` `[network.<name>]` (D10a): `subnet`, `gateway`, `router`,
  `dhcp`, and `attach` (`"slirp"`, `{ bridge = "<dev>" }`, `{ tap = "<dev>" }`;
  absent for a physical-only network). The built-in `qemu` network exists
  without being written.
- `[image.<id>.net.<if>]` (D10b): `network` (default: the board's
  `[board.net] network`), then exactly one of `address`, `host` or
  `address = "dhcp"`, and an optional `mac`.
- `[board.net.<if>]` (D10c): `kind = ethernet | wifi | serial | custom` and
  `mac_prefix`; `[board.net] network` stays as the board's default network.
- Host-OS boards (D10e): the descriptor predicate is read from
  `supported_netstacks = []`; threadx-linux gets the one statement it lacks
  (its `netxduo` is NSOS over the host).
- D10g: migrate every tracked `[image.<id>] ip` / `gateway` / `netmask` (38
  `system.toml` files on 2026-10-15) and `[board.net] ip` / `netmask` /
  `gateway` / `mac`, then refuse each, naming its new home.

**Acceptance:** the descriptor and `system.toml` loaders parse and validate
the tables (`deny_unknown_fields`). Unit tests: the default network, an
explicit physical network (no `attach`), a leased address, a refused
board-level subnet, a refused flat `[image] ip`, an address on a `serial`
interface refused, `[image.*.net]` on `linux` refused. No tracked file uses a
retired spelling.

**Status: landed 2026-10-15.**
- One reader, `nros_orchestration_ir::system_net`, parses `[network.*]` and
  `[image.<id>.net.<if>]` and lowers an image's address. Every road takes its
  answer: `leaf_system` (the cargo and cmake leaves and `nros::main!`) and the
  CLI schema (`SystemToml.network`, `ImageBlock.net`,
  `validate_image_networks`, run by `nros build` and the planner).
- `[board.net] interfaces` and `stack = "host"` are checked against an image
  on every road (`check_image_net_on_board`).
- Migrated: all 38 `system.toml` files and the 8 book snippets. For all 37
  cargo leaves among them, `nros sync` renders the same `NROS_ENTRY_IP` as
  before; leaves that omitted `netmask` or `gateway` now carry the `qemu`
  network's (`/24`, `.2`), which are those boards' own defaults.
- `just build-test-fixtures lane=tier2` builds all 7 families.

Three changes from the plan above (RFC-0103 changelog, 2026-10-15):
- **Host-OS boards** are named by `[board.net] stack = "host"`, not by
  `supported_netstacks = []`: NuttX and Zephyr list `[]` too.
- **The board-level `ip` / `netmask` / `gateway`** retire in W2, with `mac`.
  Their copies live outside the descriptor: the riscv64 board's C `cfg_ip` and
  its cmake `NROS_APP_NET_IP_LAST` default.
- **`network` is required next to an address** until W2, because the macro
  reader has no board catalog to take a default from.

Parsed and validated, but REFUSED until a later wave renders it, so nothing
is accepted and then dropped:
- a leased address and a per-image `mac` (W2);
- a network's `router` (W2);
- a bridge or TAP `attach` (W3);
- more than one interface (W4).

The SystemModel transport's `ip` / `mac` / `gateway`, a third home read by
nothing, are refused at ingest and gone from the plan schema.

### W2 -- one resolver, one output, and the collision refusal

- One resolver (`nros_board_common::board_net` grows into it) answers
  `(image, interface) -> { kind, ip | dhcp, netmask, gateway, mac }`, writes
  `<build>/nros/net/<image>.toml` (D10f), and every carrier renders from it:
  the Rust `BOARD_NET_*` constants (per interface), the C
  `nros_app_config_t.network`, the cmake board module.
- Refused at sync, naming both images (D10b): equal static addresses on one
  network, an address equal to the gateway or outside the subnet, equal MACs
  on one network (leased interfaces included), `dhcp` on a network without
  it. An unstated MAC is derived from `mac_prefix` + `(network, image,
  interface)`.
- Router default (D10d): derived on `qemu`; elsewhere the network's `router`,
  else the image's `locator`, else a sync error.
- Retire the board-level `[board.net] ip` / `netmask` / `gateway` / `mac` (moved
  from W1), together with their copies in the riscv64 board's C and cmake.
- `network` defaults to the board's `[board.net] network`, now that one
  resolver can tell every road.
- The per-image last-octet override (`NROS_ENTRY_IP_LAST` /
  `NROS_APP_NET_IP_LAST`, 23 `fixtures.toml` rows) is replaced by `host =` and
  retired.

**Acceptance:**
- A two-image bridge fixture builds.
- Swapping one address onto the other fails at sync, naming both images; so
  does a duplicated MAC.
- The test pairs that used the last-octet override build from `host =`
  instead, and still deliver.

**Status: landed 2026-10-16.**
- **One resolver**, `nros_cli_core::orchestration::image_net`, takes the
  image's interfaces, the project's networks and the board's `[board.net]`
  and answers per interface `{ kind, network, address | dhcp, prefix,
  gateway, mac }`. It runs on every road: the cargo leaf (`leaf_settings`),
  the workspace cargo and cmake roads (`cmd::build`, over EVERY image of the
  bringup), and the standalone cmake leaf (`nros ws leaf-system`).
- **One output**, written to `<build>/nros/net/<image>.toml` and carried as
  ONE text, `NROS_IMAGE_NET` (`nros_board_common::image_net`: a `no_std`
  decoder and one encoder). Three carriers read it:
  - `nros::main!`, which bakes it verbatim into `DeployOverlay::net`, which
    replaced `ip` / `gateway` / `netmask`. The macro no longer resolves
    anything, so it no longer needs a board catalog, and `network` is optional
    again;
  - `nros board net --image-net`, which renders the C
    `nros_app_config_t.network` for `nros_board_net_c`. The cmake configure
    takes it from `-DNROS_IMAGE_NET` (the workspace road) or the top-level
    leaf's read;
  - the riscv64 board's build script, because its `startup.c` hands NetX
    the address its C TU bakes before the kernel runs. Measured while
    wiring it: on the cargo road an image's address had NEVER reached NetX
    there, W1 included, because the TU was the board default and the Rust
    overlay is applied after.
- **Refused at sync**, naming both images on a duplicate: two equal static
  addresses or two equal MACs on one SHARED network, an address outside its
  subnet or equal to the gateway, `dhcp` on a network without `dhcp = true`
  or on a board interface with no DHCP client, and an image off `qemu` with no
  locator (D10d). `qemu` is never shared: every `-nic user` is a private SLIRP
  copy per VM.
- **Defaults:** `network` is the board's; an interface on `qemu` with no
  address takes `10.0.2.15` (QEMU's first guest); a MAC the image does not
  state is derived from the interface's `mac_prefix` (default `02:00:00`) and
  `(system, image, network, interface)`. The system name joined the key
  because every single-package FreeRTOS leaf calls its image
  `mps2-an385-freertos`, so a pair is two projects with one image id.
- **Router (D10d):** the image's `locator`, else its first network's
  `router`, else `[system] locator`.
- **Retired:** `[board.net] ip` / `netmask` / `gateway` / `mac` (refused when a
  descriptor is read); the riscv64 C board's `cfg_ip` / `cfg_mac` defaults
  (zero until `nros_threadx_set_config`, gated by `check-board-net-one-home`);
  `NROS_ENTRY_IP` / `_GATEWAY` / `_NETMASK`, `NROS_LEAF_IP` / `_GATEWAY` /
  `_NETMASK`, `nros board net --ip-last` / `--mac-last`, and the
  `NROS_ENTRY_IP_LAST` / `NROS_APP_NET_{IP,MAC}_LAST` overrides. The 23
  fixture rows that passed them now state `host` in the leaf; the riscv64
  Cyclone pair also states `mac`, because its test passes the same MAC to
  QEMU's virtio device, which NetX reads back.
- **Behaviour change** for an image that states no address: FreeRTOS C/C++
  images move from `10.0.2.10` to `10.0.2.15`, esp32 and riscv64 from `.50` /
  `.40`. All on `qemu`, where no two images meet.
- **Zephyr boards** declare no interfaces (their address is Kconfig's), so
  `[image.*.net]` on one is refused rather than dropped.

Acceptance, as measured:
- The two-image shared-network case and both duplicate refusals are unit
  tests (`two_images_on_a_shared_network_resolve_and_a_duplicate_is_refused_naming_both`).
  A two-image bridge FIXTURE needs a bridge `attach`, which W3 renders, so it
  moved there.
- The former override pairs build from `host =`; tier 2 below.

### W3 -- the launcher reads the network

- The QEMU launchers (`nros_tests::qemu`, `nros_tests::esp32`, the `just` run
  recipes) take the NIC from the image's network `attach`:
  - SLIRP: `-nic user` (on `qemu`, no `net=`/`host=`);
  - bridge: `-nic bridge,br=<dev>`;
  - TAP: `-nic tap,ifname=<dev>`.
- They stop hard-coding a NIC per board. An emulated image on a network with
  no `attach` is refused before launch.
- `mps2-an385`'s `docker` feature becomes a `[network.docker]` row in the
  example that uses it.

**Acceptance:** the QEMU tier-2 cells run unchanged on the default network.
One bridge pair runs on a host with the bridge provisioned, or is reported NOT
VERIFIED when no bridge exists.

### W4 -- multi-NIC

A board with two interfaces states both. An image attaches each to a network,
and the per-interface output (W2) carries both.

**Acceptance:** a two-NIC configuration resolves and renders, with a unit
test. A runtime test only where a two-NIC board exists.

This is the board half of multi-homing.
[Phase-206](phase-206-multi-homing-transport-interfaces.md) owns the
middleware half: the user's own Cyclone `<Interfaces>` or zenoh config
reaches the backend verbatim, and nano-ros models no NIC there. Its open
hosted lo-plus-NIC acceptance is the runtime counterpart of this item.
Forwarding between two NICs inside one image is not in either phase (D10e).

**Status: landed 2026-10-16, with W2.** The resolver addresses every
interface the image states, each on its own network and with its own derived
MAC, and the `NROS_IMAGE_NET` text carries all of them, primary first
(`a_two_nic_image_resolves_and_renders_both_interfaces`). A single-NIC board
applies the first. No in-tree board declares a second interface, so there is
no runtime test, as the acceptance allows.

## Not in this phase

- How a host configures the network it MAKES (D10e). A Linux box that bridges
  its ports, runs dnsmasq or hosts the router is set up with Linux tools;
  `system.toml` describes the resulting network for the boards on it.
- Host-OS boards' addressing (`linux`, `threadx-linux`, `freertos-posix`,
  Zephyr `native_sim`): the host kernel owns it (D10e).
- Forwarding between NICs inside one image, and which interface a middleware
  session binds (phase-206).
- IPv6, and Wi-Fi credentials, which are not network-address facts.

The router default moved INTO this phase with the 2026-10-15 amendment (D10d,
W2). [Issue 1793](../issues/1793-board-default-locator-differs-by-road.md)'s
two-roads disagreement is what W2's single output removes; close it there
only once the ThreadX boards it names are measured on both roads.
