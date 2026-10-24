# Phase 514 — Network and middleware configuration in three layers

**Status (2026-10-24). Not started.** Implements
[RFC-0109](../design/0109-network-config-layers.md). Opened with the RFC so
its work and phase-206's leftovers have one home.

**Prior:** [phase-206](archived/phase-206-multi-homing-transport-interfaces.md)
(the RMW user-config passthrough, W1–W5 landed; its two open acceptance items
are replaced here), [phase-497](archived/phase-497-networks-have-owners.md)
(built RFC-0103 D10, whose surviving parts this phase re-sources).

## Work items

### W1 — board descriptors state who owns layer 1, and the links they carry

`[board.net] stack = "host" | "os" | "board"` on every descriptor;
`links = [...]` (`tcp`, `udp`, `serial`, `isotp`, `can`, `custom`). Zephyr and
NuttX boards are `"os"`; the reference FreeRTOS / ThreadX / bare-metal boards
are `"board"`; `supported_netstacks` is unchanged.

**Done when** every tracked descriptor states both, and a gate refuses a
descriptor that states neither.

### W2 — the reference board file, parsed at sync

One format in `nros-board-common` (RFC-0109 D4), parsed by `nros sync` for
every image and rendered into `NROS_IMAGE_NET`. D10b's collision refusal and
MAC derivation move onto it. Each reference board package ships a default file
(the `qemu` network).

**Done when** the tracked `[image.<id>.net.*]` uses have moved into board
files, the fixture rows that pass a per-image IP read them, and an image with
no `os_config` on a reference board still boots on QEMU user-mode networking.

### W3 — `[[domain]] config` and the RMW's own file

`[[domain]] config`, `[image] rmw_config`, the implicit default domain, and
the checks in RFC-0109 D9. Cyclone's file is carried as today (phase-206 W2);
zenoh json5 is lowered to `zp_config_insert` properties on the host, replacing
`cmake/NanoRosRmwUserConfig.cmake`'s flat `zenoh.conf` path; hosted builds link
the same lowering for `ZENOH_SESSION_CONFIG_URI` (D6a).

**Done when** a native C bringup with a `rmw/zenoh.json5` stating a `listen`
endpoint is reached by a peer in a test cell (phase-206's open item), and a
json5 key zenoh-pico cannot take is refused at sync.

### W4 — retire the keys RFC-0109 moves

`[[transport]]`, `[image]` / `[system] locator`, `[image] transport` (derived),
`[network.<name>]`, `[board.net]` addresses: each refused with a remedy naming
its new home. `conf` becomes an alias of `os_config`. NuttX's `[board.net]`
moves to its `defconfig`.

**Done when** no tracked file uses a retired key and each refusal has a test.

### W5 — CAN controllers are started by layer 1

The zenoh-pico fork's ISO-TP link stops calling `can_start()`; nano-ros's
Zephyr board glue starts the controller; CAN locators refuse `bitrate` /
`dbitrate` (RFC-0081/0083 amendments). Fork-branch workflow.

**Done when** the ISO-TP cells pass with the start moved.

### W6 — the runner's NIC

`[image.<id>.run] nic` read by the QEMU launcher and the test harness,
replacing `[network.<name>] attach`.

**Done when** the bridge/TAP QEMU cells run from it.

## Acceptance

The RFC-0109 examples that the tree can build (Linux, Zephyr native_sim and
mps2-an385-zephyr, NuttX QEMU, the FreeRTOS / ThreadX / bare-metal reference
boards) build with the new shape, and the U5 bridge shape (two domains, two
RMW files, one bridge) builds on Linux.
