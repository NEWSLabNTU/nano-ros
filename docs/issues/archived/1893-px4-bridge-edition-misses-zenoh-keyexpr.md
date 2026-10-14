---
id: 1893
title: "PX4 bridge: `build-bridge-example <topics> <edition>` reaches the generated TYPE_HASH but not the zenoh backend, so a `jazzy` image publishes Humble data keys"
status: resolved
type: bug
severity: medium
area: [px4, rmw-zenoh, build]
related: [issue-0362, issue-0436, issue-1896, phase-325]
found: 2026-10-14
resolved: 2026-10-15
---

## Measured

Found while landing phase-325 W3.4 (the bridge's real-ROS-2-subscriber test,
`packages/testing/nros-px4-sitl-test/tests/px4_bridge_ros2_e2e.rs`).

`just px4 build-bridge-example debug_key_value jazzy` and `... humble` were both
built and both run against the same ROS 2 **Humble** `ros2 topic echo`
(`rmw_zenoh_cpp` 0.1.9). Both delivered — the Humble subscriber decoded
`key: velx`, advancing `value`, from either image:

```
# image generated with --ros-edition jazzy
TYPE_HASH = "RIHS01_9f784632ce2d700b9ea22685b6849e45d3f6028e81e6655b74c7d92b303b6198"   (in bin/px4)
ROS 2 humble subscriber decoded 3 bridged samples, first Sample { timestamp: 1668000, key: "velx", value: 3.0 }, ...
```

That is not "rmw_zenoh ignores the hash". It is the archive ignoring the
edition. `nros-rmw-zenoh`'s DATA key expression is chosen by its OWN cargo
feature, not by the message's `TYPE_HASH`
(`packages/rmw/zenoh/nros-rmw-zenoh/src/keyexpr.rs`, `to_key`):

* `not(any(ros-iron, ros-jazzy))` → `<domain>/<topic>/<type>/TypeHashNotSupported`
* `ros-iron | ros-jazzy` → `<domain>/<topic>/<type>/<type_hash>`

and the recipe's step [3/4] builds `libnros_cpp.a` with
`--features std,rmw-<rmw>-cffi,bridge,platform-posix` — no edition. Nor would
adding `ros-jazzy` there help: `nros-cpp/ros-jazzy` forwards to `nros` and
`nros-c` only, and the only crates forwarding an edition to `nros-rmw-zenoh`
are `nros-board-linux` and `nros-rmw-zenoh-staticlib` — the second archive the
bridge deliberately stopped linking (issue 0436: two zenoh-pico copies in one
image broke `z_open`).

So the `edition` argument (default `jazzy`) changes the hash the generator
writes into the header — and therefore what the liveliness token advertises —
while the data key stays Humble's. An image built with the README's default
command is a Humble publisher wearing a Jazzy hash.

## Not measured

Whether a real Jazzy (`rmw_zenoh_cpp` 0.2+) subscriber receives from the
default `jazzy` image. This host has no Jazzy install. The expectation from the
key shape is that it does NOT (it subscribes on its own RIHS01 segment), which
would make the README's default command produce an image no Jazzy peer sees.

## What would close it

The edition is one value with three destinations — the generator's
`--ros-edition`, the zenoh backend's `ros-*` feature, and whatever nros-cpp
itself keys on — and today it reaches one. Forward an edition feature from
`nros-cpp` to `nros-rmw-zenoh` (as `nros-board-linux` does), pass
`ros-<edition>` in step [3/4], and measure against a Jazzy subscriber. Until
then the W3.4 test pins `humble`, which is the one pairing whose two halves
agree.

## Resolution (2026-10-15)

Two faults, and the measurement only cleared once both were fixed.

**1. The edition did not reach the backend — and not only on the bridge.**
Phase 104.A made `nros/ros-*` inert at `nros-node` and told callers to name the
edition on their direct `nros-rmw-<name>` dep. Two kinds of crate stand between
a selected edition and that dep, and neither did:

* `nros-c` and `nros-cpp`, which `cmake/NanoRosFeatureSet.cmake` hands
  `ros-<edition>` to on the C/C++ road. Measured with `cargo tree -e features -i
  nros-rmw-zenoh` for `nros-c --features rmw-zenoh,ros-jazzy`: the backend
  received `platform-posix` and no edition. So every C and C++ zenoh image
  built for iron or jazzy published Humble keys, not just the bridge.
* The five embedded board crates (`esp32-qemu`, `freertos`, `mps2-an385`,
  `nuttx`, `threadx`) hardcoded `ros-humble` on the dep and declared no `ros-*`
  feature. `nros build` forwards the edition only to a board that declares it
  (`facade.rs`, `crate_declares_feature`), so an embedded Rust image had
  nowhere to carry it.

The fix differs by road, because nros-c and nros-cpp may not name a backend
(RFC-0071 — `check-rmw-agnostic` refused the first attempt, which added
`nros-rmw-zenoh?/ros-*` to both):

* **C/C++ road.** The backend declares its own edition feature,
  `[rmw.link] edition_feature = "ros-{edition}"` in zenoh's `nros-rmw.toml`;
  `nros ws rmw-dispatch` emits it as `NROS_RMW_EDITION_FEATURE`, and
  `nros_feature_set()` appends `<rlib_dep>/<feature>`. Measured: the jazzy C
  talker's cargo commands carry `nros-rmw-zenoh/ros-jazzy`. A dependency's
  `features = [...]` may not contain a slash, so the workspace runtime crate
  (`NanoRosRuntimeCrate.cmake`) turns that entry into a direct dependency on
  the bundled rlib instead. The PX4 recipe reads the same dispatch row.
* **Cargo road.** The five embedded boards forward `ros-<edition>` to
  `nros-rmw-zenoh?/ros-<edition>` and no longer hardcode one, so `nros build`
  has a feature to forward to. The two NuttX kernel-bundle crates
  (`nros-nuttx-ffi`, `nros-nuttx-riscv-ffi`) keep their hardcoded `ros-humble`:
  they have no `[features]` table, so nothing selects an edition through them.

Gate: `check-feature-set-ssot` §9 — a board crate must forward each edition and
hardcode none, and a bundled rlib with `ros-*` features must declare
`edition_feature`. Self-tested (missing forward, hardcoded edition, missing
descriptor field, correct ones not reported) and mutation-checked (dropping
`edition_feature` from zenoh's descriptor, and reverting
`nros-board-freertos`).

**2. The type hash was wrong for a `char` field.** With fault 1 fixed, a `jazzy`
bridge image against a Jazzy subscriber (`nano-ros-ros:jazzy` from `just
ros_editions image jazzy`, `rmw_zenoh_cpp` 0.2.11, `px4_msgs` built in that
container from the PX4 tree) still delivered **0** samples. `ros2 topic info -v`
showed why:

```
publisher  (bridge)  Topic type hash: RIHS01_9f784632ce2d700b9ea22685b6849e45d3f6028e81e6655b74c7d92b303b6198
subscriber (Jazzy)   Topic type hash: RIHS01_e89a9760f505a9dfed23a149d7eed32d03f8dd4fdd90e3ba2ae385e4f77d3e63
```

`DebugKeyValue` has a `char[10] key`. `rosidl-resolve/src/rihs.rs` hashed a
`.msg` `char` as REP-2011 `CHAR` (13); rosidl_adapter rewrites `char` to `uint8`
on the way to `.idl`, and the hash is computed from the `.idl`, so rosidl hashes
it as `UINT8` (3). The same file's comment on `service_event_info_msg` already
said so for `client_gid`. Fixed in `primitive_id`; only the `.msg` parser
produces that variant. Golden test `msg_char_hashes_as_uint8_like_rosidl` pins
both `px4_msgs/msg/DebugKeyValue` (from the live subscriber) and
`std_msgs/msg/Char` (from Jazzy's shipped `Char.json`), and fails with the old
mapping.

**Receipt.** Same setup, both fixes, `NROS_PX4_BRIDGE_RMW=zenoh just px4
build-bridge-example debug_key_value jazzy`:

```
bridging /fmu/out/debug_key_value (uorb) -> /fmu/out/debug_key_value (zenoh)
samples: 47
timestamp: 10520000
key: [118, 101, 108, 120, 0, ...]      # "velx"
```

This was a manual run: the host has no Jazzy, and `px4_bridge_ros2_e2e` spawns
its subscriber on the host. The automated test still pins `humble` on both
sides.

**Not closed here: C and C++ message codegen.** The same check on the C road
(`examples/linux/c/talker`, jazzy) reached the RIHS01 key but advertised
`RIHS01_0000…0000`: the C and C++ generators still emit the phase-304
placeholder instead of computing the hash, so they match no Iron+ peer.
Filed as issue 1909. The PX4 bridge is unaffected — `nros generate-px4-msgs`
computes the hash.

**Found on the way: every `just px4 test-*` recipe died on a fresh checkout.**
`nros_cargo_nextest_args` emits `--locked` (phase-485 W4, on the premise that
every caller runs in the root workspace), and the three PX4 recipes `cd` into
`packages/testing/nros-px4-sitl-test`, which is out of workspace with a
GITIGNORED `Cargo.lock`. With no lock, `cargo metadata --locked` refuses to
create one (`cannot create the lock file … because --locked was passed`). They
now use `nros_cargo_nextest_args_unlocked`. After that, `just px4
test-bridge-ros2` (humble image, Humble subscriber) passed: 2 tests, 20.7 s.
