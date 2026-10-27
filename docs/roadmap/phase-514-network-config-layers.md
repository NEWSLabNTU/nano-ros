# Phase 514 — Network and middleware configuration in three layers

**Status (2026-10-25). W1, W2, W4, W6 landed; W3a and W3b in review; W5
in review, its cells passing.** W1: every board descriptor states `[board.net] stack` and
`links`, from one link vocabulary (`LINK_KINDS`, a superset of the image
`TRANSPORT_KINDS`). W2: an image's address lives in the board network file
its `os_config` names; all 78 in-tree rows moved, each resolving
byte-identically. W4a + W6: `[network.*]` and the inline `[image.<id>.net.*]`
are refused naming their homes; a network's subnet/gateway/dhcp moved into
the board network files, its router into the image's `locator`, its attach
into `[image.<id>.run] nic`. W4b: the link is derived from the locator
(`[image] transport` refused), `conf` is an alias of `os_config` (75 tracked
lines moved), D9 check 6 refuses a layer-1 file its board's network owner
does not read, and NuttX's address default is its kernel `defconfig`.
W3a: a bringup's `rmw/zenoh.json5` is lowered on the host, baked into every
road's entry and installed before the session opens; `$ZENOH_SESSION_CONFIG_URI`
replaces it on hosted builds. W3b: the file is named (`[image] rmw_config`,
`[system] rmw_config`, `[[domain]] config`) and one resolver answers every
road, a bridged domain's file travels as that session's properties, and the
`bridge-cyclonedds` example is U5. W5 needs a host with `vcan0` to verify.
Implements
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

**W3a — the default domain's file, by convention (in review).** A bringup's
`rmw/zenoh.json5` (beside its `system.toml`, as `rmw/cyclonedds.xml` already
was) applies to every zenoh image of it; there is no key naming it yet.

- One lowering, `nros-zenoh-session-config` (a hand-written json5 reader —
  the `json5` crate would have added `pest` to eleven tracked locks): a leaf
  path zenoh-pico has a property for lowers to it, anything else is REFUSED
  naming the key. `nros_orchestration_ir::rmw_config` finds the file and runs
  the D9 checks: the retired flat `rmw/zenoh.conf` is refused naming the
  json5, an image `locator` beside a file is refused (a `[system]` locator is
  simply unused), and a Cyclone `<Domain id>` other than `any` or the
  image's domain is refused.
- Every road bakes the lowered table and installs it before the first session
  opens: `nros::main!` (a static installed by the deploy-overlay expression,
  plus the Zephyr arm), and the C/C++ entry (`nros codegen entry
  --rmw-config`, passed by `nano_ros_entry` when the file exists;
  `nros_cpp_set_session_properties`). The registry is `nros-node`'s
  (`with_session_properties`, beside `launch_remap`), so every executor open
  path reads it. With a file the entry bakes no locator: the file is the
  wiring.
- The shim treats `mode: "peer"` and a `connect` list as the session's own:
  a `connect` replaces the default router instead of being dialed beside it.
- Hosted builds read `$ZENOH_SESSION_CONFIG_URI` through the same lowering
  (D6a); it REPLACES the baked table. The backend reads it through the C
  library (`getenv` + `fopen`), not `std`: the C/C++ road links the backend
  with `platform-posix` and no `std` (phase-359 W10), and a `std::env` reader
  left the variable unread there, so the C talker dialled the default router.
  `nros-zenoh-session-config` is `no_std` + `alloc` for the same reason, and
  the zenoh descriptor's `[rmw.capabilities] alloc = "alloc"` forwards the
  image's heap to the backend (the C/C++ image crate otherwise names only
  `platform-posix` on it).

Acceptance as built differs from the line above in one respect, on purpose:
a `listen` endpoint reached by a peer needs zenoh-pico's multicast transport,
which the shipped shim compiles out (issue 0682), so the test cell proves the
other half — `native_zenoh_session_config_file_is_the_route` (rust + C): the
talker gets no locator and a router on a unique port, so the file's `connect`
is its only route. The bake itself is pinned by
`a_zenoh_rmw_file_bakes_its_session_properties_and_no_locator`.

A Rust Zephyr west application has no carrier for the lowered text yet, so
`nros build` refuses one whose bringup ships the file (issue 2051).

**W3b — naming the file; a domain's own (done, RFC-0109 D8 amendment W3b).**

- `[[domain]] config`, `[image.<id>] rmw_config = "<file>" | { <domain> =
  "<file>" }` (merged over `[image_defaults]`, a table key by key) and
  `[system] rmw_config`. Default session: image, then system, then the
  conventional file. Checked at sync (`SystemToml::check_rmw_files`): a named
  file that is absent, a file for a backend with none (XRCE), a domain with
  `config` and `locator`, a Cyclone `<Domain id>` against the DOMAIN's id, an
  override naming no declared domain, and a domain `config` that no
  `[[bridge]]` opens (it would be read by nothing).
- A bridged domain's file is its session's properties
  (`RmwFile::session_properties`): planner endpoints and transports carry
  them, `nros-bridge.toml` `[[node]] properties` and `SessionSpec::properties`
  deliver them, and `NROS_BRIDGE_<N>_LOCATOR` replaces them whole. Cyclone
  takes one `config` property — a whole document — in place of its baked one
  (`nros_rmw_cyclonedds_session_config_property`, mutation-checked: dropping
  the property fails it); the cffi seam heap-copies a value past 256 bytes on
  `alloc` builds.
- The CMake road asks `nros ws rmw-config` (`nros_rmw_user_config_file`),
  with each generated entry's `IMAGE`, for the file and its format: a
  `session-json5` one goes to `--rmw-config`, a `verbatim-xml` one to the
  bake. No CMake file spells an RMW file name any more.
- U5: `examples/workspaces/bridge-cyclonedds` names `rmw/zenoh-zen.json5` and
  `rmw/cyclonedds-dds.xml` on its two domains and drops `[system] locator`;
  `declarative_zenoh_to_cyclonedds_bridge_to_nano_listener` drives the zenoh side with
  `ZENOH_SESSION_CONFIG_URI` and the Cyclone side opens with the domain's XML.

Not done, stated: an image's `rmw_config = { <domain> = … }` is checked and
then refused, because the bridge configuration is per bringup (issue 2062);
a C++ `NrosCppSessionSpec` carries no properties (a C/C++
multi-session image has no per-domain file), and a lone `[[domain]]` is not
yet an image's default session (spelled `[system] rmw_config` meanwhile).

### W4 — retire the keys RFC-0109 moves

`[[transport]]`, `[image]` / `[system] locator`, `[image] transport` (derived),
`[network.<name>]`, `[board.net]` addresses: each refused with a remedy naming
its new home. `conf` becomes an alias of `os_config`. NuttX's `[board.net]`
moves to its `defconfig`.

Taken while implementing (RFC-0109 D6 amended 2026-10-25): `[image]` /
`[system] locator` is NOT retired — it is the implicit default domain's
`locator`, refused only beside an `rmw_config` (W3). Split in two:

- **W4a (with W6)** — `[network.<name>]`, inline `[image.<id>.net.*]` /
  `[image_defaults.net]` and a top-level `[[transport]]` refused by name
  (`nros_orchestration_ir::retired_tables`), for every reader (the CLI schema,
  the leaf reader, `nros::main!`). Done.
- **W4b** — `[image] transport` derived from the locator scheme
  (`leaf_system::link_kind_of_locator`; `custom://` is a link kind now,
  `nros_platform::LinkKind::Custom`, which the mps2 board's XRCE-over-UART
  keys on), with one predicate for the byte-stream implications
  (`is_byte_stream_link`); `conf` an alias of `os_config` (concatenating
  over `[image_defaults]`, a table stating both refused), 75 tracked lines
  moved; D9 check 6 (`BoardNet::check_os_config`, swept over every tracked
  image); NuttX `BOARD_NET_*` generated from `nuttx-config/<arch>/defconfig`
  (`board_net::from_kconfig`), its `[board.net]` reduced to `stack`, and the
  arm defconfigs moved `10.0.2.30` → `10.0.2.15` so the generated value is
  the address the board already applied. Done. Two things deliberately left,
  each an issue: the direct-mode `config.toml` `[[transport]]` that seven
  board crates' `Config::from_toml` read (a board API for apps with no
  `system.toml`, issue 2047), and the NuttX board's run-time `SIOCSIFADDR`
  push that overrides the kernel it now reads (issue 2048).

**Done when** no tracked file uses a retired key and each refusal has a test.

### W5 — CAN controllers are started by layer 1

The zenoh-pico fork's ISO-TP link stops calling `can_start()`; nano-ros's
Zephyr board glue starts the controller; CAN locators refuse `bitrate` /
`dbitrate` (RFC-0081/0083 amendments). Fork-branch workflow.

**Done when** the ISO-TP cells pass with the start moved.

Done; the cells pass (2026-10-26, `vcan0` + `can-isotp`). The fork's two Zephyr
CAN links (multicast AND ISO-TP — the multicast one had to move too, because a
board-started controller made its `can_start()` return `-EALREADY` and fail
the open) assume a started controller, refuse a stopped one at open
(`link/transport/can_zephyr.h`) and read CAN FD from the controller; Linux's
multicast link reads FD from the interface MTU. nano-ros starts the
`zephyr,canbus` controller from an APPLICATION `SYS_INIT`
(`nros-platform-zephyr/src/can_start.c`). One rule refuses the rate keys,
`nros_zenoh_session_config::refuse_device_facts`, asked by every CLI road;
the Kconfig road carries a cross-referenced copy in `nros_rmw_zenoh.cmake`.
Found on the way: that file passed `CONFIG_NROS_ZENOH_LOCATOR` through a CMake
LIST, so the `;` of every ISO-TP locator split the definition and no Zephyr
ISO-TP image could build; `;` is escaped as `\073` beside issue 1955's `#`.
Measured without `vcan0`: the image boots, `nros_can_start` reports the
controller not ready, and the session open fails — the expected negative.
Measured with it: `isotp-zephyr-tier2.sh` PASS (59 `/chatter` messages,
Zephyr native_sim talker -> ISO-TP -> zenoh-rs `z_sub`, `nros_can_start`
logging the controller started); `isotp-pico-interop.sh` PASS (pub/sub 11, the
query 1 reply); two pico peers over the multicast `can/vcan0` link on a CAN FD
`vcan0` (MTU 72), 7 of 7. That last run is what caught the Linux link's MTU
probe clobbering `ifr_ifindex` (one `ifreq` union) so `bind()` failed with
`ENODEV` — the ISO-TP cells never reach that function. A pico library proves
nothing about a link unless built with `-DZ_FEATURE_LINK_CAN=1
-DZ_FEATURE_LINK_ISOTP=1` in `CMAKE_C_FLAGS`: `config.h.in` hard-codes both to
0, so the cache options alone build a TCP-only library. Not run: the
`isotp-ros-*` cells (they need a zenoh-c built by `scripts/can/build-zenohc-can.sh`;
the code they exercise, Linux ISO-TP, did not change). Multicast CAN on a
classic-MTU `vcan0` (16) fails to open on both sides, on the pre-W5 fork commit
too: the 7-byte batch cannot hold a JOIN, which is not fragmentable —
issue 2091, which also corrects RFC-0080's claim that classic CAN works at
MTU 7.

Follow-up (2026-10-27). The `isotp-ros-*` cells now ran, against a zenoh-c
1.8.0 built with the ISO-TP link: service server and client, action server,
client and cancel, and parameters through `rmw_zenohd` — all six PASS. Two
things stood in the way. `isotp-ros-interop.sh` handed the ROS side's
`ZENOH_SESSION_CONFIG_URI` to the nano-ros nodes as well, which since RFC-0109
D8 refuse the zenoh-rs keys in it, so no node session opened; and its
`count_in` helper had been deleted with four call sites left. Both are fixed.
The cells still have no supported BUILD: nothing derives `zpico-sys`'s
`link-isotp` feature from an image's locator or its board's `links`, so the
nodes were built with the feature added by hand to the generated manifests —
issue 2096.

Resolved the same day: the link is implied from the image's wiring (its
`locator`, or its RMW file's endpoints) on every road, through one table in
`nros_board_common::policy` that both the CLI and zenoh-pico's build read; a
leaf that states no endpoint asks with `NROS_LINK_ISOTP=1`. The six cells
pass with their nodes built that way.

### W6 — the runner's NIC

`[image.<id>.run] nic` read by the QEMU launcher and the test harness,
replacing `[network.<name>] attach`.

**Done when** the bridge/TAP QEMU cells run from it. The launchers read the
attach from `NROS_IMAGE_NET` (unchanged text), which the resolver now fills
from `run.nic` (`ImageRun::attach_for`), so they do; the workspace road's test
asserts the multihost pair resolves to `bridge=qemu-br`.

## Acceptance

The RFC-0109 examples that the tree can build (Linux, Zephyr native_sim and
mps2-an385-zephyr, NuttX QEMU, the FreeRTOS / ThreadX / bare-metal reference
boards) build with the new shape, and the U5 bridge shape (two domains, two
RMW files, one bridge) builds on Linux.
