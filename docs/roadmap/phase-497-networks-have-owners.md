# Phase 497 -- networks have owners: board interfaces, project networks, image addresses

**Status (2026-10-12). Not started.** Implements
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

### W1 -- the schema

- `system.toml` `[network.<name>]`: `mode` (`slirp` | `bridge` | `tap` |
  `none`), `subnet` (CIDR), `gateway`, and for `bridge`/`tap` the host device.
  A built-in `qemu` network (QEMU's own SLIRP plan: `10.0.2.0/24`, gateway
  `10.0.2.2`, guest `10.0.2.15` — phase-484 W4b-2 already names it with
  `[board.net] network = "qemu"`)
  exists without being written.
- `[image.<id>.net.<ifname>]`: `network` (default `qemu` for a QEMU board),
  `address` or `host` (the last octet), optional `mac`.
- `[board.net]` keeps the interface facts: interface names, MAC prefix. The
  subnet, gateway and address keys it carried after phase-484 W4b-1 become
  network/image facts and are refused there with a pointer to the new home.

**Acceptance:** the CLI's descriptor and `system.toml` loaders parse and
validate the tables (`deny_unknown_fields`). Unit tests cover the default
network, an explicit network, and a refused board-level subnet.

### W2 -- resolution and the collision refusal

- One resolver (`nros_board_common::board_net` grows into it) answers
  `(image, interface) -> {ip, netmask, gateway, mac}`, and every road takes the
  same answer: `Config::default()` constants, the cargo-road C
  `NROS_APP_CONFIG`, `nros_board_net_c` on the cmake road.
- Two images on one network with the same address or MAC is a build error
  naming both.

**Acceptance:** a two-image bridge fixture builds. Swapping one address onto
the other fails at sync, naming both images.

### W3 -- the launcher reads the network

- The QEMU launchers (`nros_tests::qemu`, `nros_tests::esp32`, the `just` run
  recipes) take the NIC arguments from the image's resolved network:
  - SLIRP: `-nic user,net=<subnet>,host=<gateway>`;
  - bridge: `-nic bridge,br=<dev>`;
  - TAP: `-nic tap,ifname=<dev>`.
- They stop hard-coding a NIC per board.
- `mps2-an385`'s `docker` feature becomes a `[network.docker]` row in the
  example that uses it.

**Acceptance:** the QEMU tier-2 cells run unchanged on the default network.
One bridge pair runs on a host with the bridge provisioned, or is reported NOT
VERIFIED when no bridge exists.

### W4 -- multi-NIC

A board with two interfaces states both. An image attaches each to a network.

**Acceptance:** a two-NIC configuration resolves and renders, with a unit
test. A runtime test only where a two-NIC board exists.

## Not in this phase

- The router/locator default. RFC-0103 §3 keeps it an `[image]` fact over the
  platform default. One network makes it `tcp/10.0.2.2:<port>` on every QEMU board,
  which is most of issue 1793.
