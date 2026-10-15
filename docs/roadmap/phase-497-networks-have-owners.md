# Phase 497 -- networks have owners: board interfaces, project networks, image addresses

**Status (2026-10-15). Not started; re-planned against D10 as amended
2026-10-15 (D10a–D10g).** Implements
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
- The per-image last-octet override (`NROS_ENTRY_IP_LAST` /
  `NROS_APP_NET_IP_LAST`, 23 `fixtures.toml` rows) is replaced by `host =` and
  retired.

**Acceptance:**
- A two-image bridge fixture builds.
- Swapping one address onto the other fails at sync, naming both images; so
  does a duplicated MAC.
- The test pairs that used the last-octet override build from `host =`
  instead, and still deliver.

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
