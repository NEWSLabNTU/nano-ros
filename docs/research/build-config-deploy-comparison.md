# Build / configuration / deployment workflows — nano-ros vs related work

Status: research notes · Date: 2026-05-28 · Author review of Phase 172 model

Scope: the three *workflow* axes — **how you get a binary** (build), **how you
set RMW / transport / QoS / params / RT** (configuration), **how the binary
ships and runs** (deployment) — across the embedded-ROS / embedded-pub-sub
landscape. Feature-level parity is covered separately in
`book/src/concepts/comparison-vs-microros.md`; UX (project creation,
distribution) in `docs/research/sdk-ux/{micro-ros,platformio-arduino-mbed}.md`.
This doc evaluates against the **Phase 172** nano-ros model (root `nros.toml`,
`nros deploy`, the two-form entry lib, build-ownership axis), which supersedes
the older `find_package(NanoRos)` / `config.toml` workflow those docs describe.

## Peers compared

| System | What it is | Embedded reach |
|---|---|---|
| **nano-ros** | full ROS 2 client (Rust + C/C++), multi-RMW | bare-metal, FreeRTOS, NuttX, ThreadX, Zephyr, ESP-IDF, PX4 |
| **micro-ROS** | full ROS 2 client (`rclc`, C) over Micro-XRCE-DDS | FreeRTOS, NuttX, Zephyr, ESP-IDF; PX4 canonical |
| **Zenoh-pico** | pub/sub lib (no ROS API); the transport nano-ros wraps | any C target; RTOS + bare-metal |
| **embedded DDS** (Cyclone-/Fast-DDS reduced profiles) | full DDS stack on MCU-class | Linux-class / high-end MCU; rarely truly bare-metal |
| **Arduino/PlatformIO ROS libs** (`micro_ros_arduino`, rosserial legacy) | packaged micro-ROS / serial bridge | Arduino-class boards |
| **ros2-rust `rclrs`** | host ROS 2 Rust client | Linux only (no MCU) — baseline, not embedded |

## Axis 1 — Build workflow

| | nano-ros (Phase 172) | micro-ROS | Zenoh-pico | embedded DDS | Arduino-ROS |
|---|---|---|---|---|---|
| Driver | `nros deploy <name>` (one root `nros.toml`) → metadata → plan → entry lib → vendor `build[]` | `create_firmware_ws.sh` → `configure_firmware.sh` → `build_firmware.sh` (per-RTOS meta-build over colcon) | plain CMake `add_subdirectory` / `find_package` | CMake + the DDS vendor's gen + per-RTOS port | drop a library zip into the IDE, hit Build |
| Who owns the toolchain | **explicit axis**: self / vendor-lib / vendor-module | the meta-build owns it per RTOS (opaque) | the consumer | the consumer | the IDE |
| Cross-RTOS uniformity | one command, one config; the generated *entry lib* is the neutral unit, platform startup is deploy-side | one CLI, but each RTOS has its own `config/<rtos>/<board>/{create,configure,build,flash}.sh` recipe | none — the consumer wires each RTOS | none | hidden by the IDE |
| Code generation | launch + component metadata → generated wiring (one binary, all nodes) | none — you hand-write the `rclc` app | none | IDL → type support only | none |
| Distribution form | source (`add_subdirectory`+corrosion) **or** compiled `lib<sys>.a`+header | precompiled `.a`+`.h` per ecosystem (Arduino/IDF/PIO/Zephyr/CubeMX) | source or `.a` | source | precompiled lib zip |

**Read:** micro-ROS's headline strength is the *one-CLI, four-verb* flow with
precompiled per-ecosystem artifacts — lowest friction to first-flash. nano-ros's
distinguishing move is the **orchestration pipeline** (launch graph + component
metadata → a generated entry lib wiring every node into one binary) and the
**explicit build-ownership axis** — micro-ROS has neither (you hand-write the
`rclc` app; the meta-build hides ownership). nano-ros's two-form entry lib
(compiled `.a` for nano-ros-owned toolchains, source+CMake for vendor-owned)
generalizes what micro-ROS does ad-hoc per ecosystem. Where nano-ros still
trails: micro-ROS ships **precompiled artifacts for 5 ecosystems**; nano-ros's
compiled form is proven only host/QEMU, vendor builds are template-stage.

## Axis 2 — Configuration workflow

| | nano-ros (Phase 172) | micro-ROS | Zenoh-pico | embedded DDS |
|---|---|---|---|---|
| Config home | one **root `nros.toml`** (SSOT) + optional per-component `[component]` | `colcon.meta` / `app-colcon.meta` (static sizing) + `configure_firmware.sh` flags | C `#define`s + `z_config_t` at runtime | XML profile (`CYCLONEDDS_URI`, Fast-DDS `*.xml`) + IDL |
| RMW selection | `[system].rmw` (zenoh/xrce/cyclonedds), per-`[deploy]` override; build-time | fixed (XRCE only) | n/a (is the transport) | fixed (that DDS) |
| Transport | `[[transport]]` (ethernet/wifi/serial/can; ip/mac/gw/ssid/locator) | `configure_firmware.sh -t udp/serial -i <ip> -p <port>` | `z_config` keys | DDS XML transport descriptors |
| Static sizing | derived from the plan (callback/node/param counts) + env knobs | hand-tuned `colcon.meta` (`MAX_NODES`, `MAX_PUBLISHERS`, …) | compile `#define`s | profile-driven |
| Params / remaps | `[overlays.<inst>]` (params/remaps/namespace), launch overlays | none at config level (hand-coded) | n/a | n/a |
| RT scheduling | `[[scheduling.contexts]]` → `SchedContext` (FIFO/EDF/Sporadic/TT) | rclc executor priority callbacks (no EDF/TT) | n/a | n/a |
| Multi-domain / bridge | `[[domain]]` / `[[bridge]]` (design; routing pending K.5) | n/a (single XRCE session) | manual | DDS domains (native) |

**Read:** nano-ros centralizes *everything* in one declarative file with a true
single-source-of-truth for RMW + domain, and uniquely carries RT-scheduling +
params/remaps + multi-domain *as config* (the orchestration plan consumes them).
micro-ROS splits config across `colcon.meta` (static memory) + shell flags
(transport) + hand-coded app (params/QoS) — simpler per piece, no unifying file,
and **hand-tuned static sizing is a known micro-ROS footgun** (wrong
`MAX_*` → silent runtime failure). nano-ros deriving sizing from the plan is a
genuine ergonomic edge. The Cargo-style manifest revision (one `nros.toml`,
section-discriminated, `[component]` folds in `component_nros.toml`) closes
nano-ros's last config-shape wart; micro-ROS has no equivalent unification.

## Axis 3 — Deployment workflow

| | nano-ros (Phase 172) | micro-ROS | Zenoh-pico | embedded DDS |
|---|---|---|---|---|
| Ship step | `nros deploy <name>` runs vendor `build[]`/`package[]` (var-substituted) | `flash_firmware.sh` (OpenOCD/board probe auto-detect) | consumer's flash | consumer's flash |
| Ownership models | **3 explicit**: self (nano-ros owns), vendor-lib (link a vendor `.a`, e.g. Orin SPE), vendor-module (vendor build owns, e.g. PX4/Zephyr) | one implicit (the meta-build owns flash per board) | none | none |
| **Broker / agent on the wire** | **none for Zenoh P2P / Cyclone DDS**; agent only for XRCE | **always** — the Micro-XRCE-DDS **Agent** must run on the host, bridging XRCE↔DDS | none (Zenoh router optional, P2P-capable) | none (DDS is brokerless) | always (the agent, like micro-ROS) |
| Host-side prereq | a `zenohd` router (Zenoh) **or** nothing (Cyclone P2P) **or** the XRCE Agent (XRCE) | the Agent process, always | optional `zenohd` | nothing | the agent |
| Multi-binary / gateway | `[[bridge]]` (in-binary) + `nros-bridge.toml` (separate deployable) | n/a | manual | DDS routing service |
| Reflash to change transport | no — re-`deploy` from config | no — re-`configure_firmware.sh` + reflash | edit + reflash | edit XML (some runtime) |

**Read:** the sharpest deployment difference is the **mandatory broker**.
micro-ROS *always* needs the Micro-XRCE-DDS Agent running host-side — a process
to provision, monitor, and (in the field) keep alive; it's the single biggest
operational tax of the XRCE model. nano-ros inherits that tax **only** on its
XRCE backend; with Zenoh it's peer-to-peer through an optional router, and with
Cyclone DDS it's fully brokerless RTPS — the device is a first-class DDS
participant with no bridge. That multi-RMW choice is nano-ros's strongest
deployment differentiator. The **ownership axis** (self/vendor-lib/vendor-module)
is also novel: micro-ROS's meta-build implicitly owns the flow per board, which
is frictionless when your board is in its catalog and a cliff when it isn't;
nano-ros makes ownership explicit so an out-of-catalog vendor target (Orin SPE,
a custom PX4 module) is a `[deploy]` table, not a fork of the build system.
On flashing, the model differs by ownership: where nano-ros owns the deployment
(`self`/`vendor-lib`) the flash is a `package[]` step `nros deploy` can drive;
where it's vendored (`vendor-module`) flashing follows the vendor's own
`flash`/`upload` target — not re-implemented. Where nano-ros trails is in
*proof*, not model: micro-ROS's `flash_firmware.sh` **flashes a board catalog
today**, while nano-ros's vendor deploys are still template + dry-run with no
real hardware boot in CI (Phase 172 W.4).

## First-image effort — time + space

How much does a *user* spend to deliver and run the first image? micro-ROS
supports FreeRTOS (esp32, 4× Nucleo, crazyflie21, olimex-e407), Zephyr, host,
plus Arduino / ESP-IDF / PlatformIO / STM32CubeMX via ecosystem packages.
Measured against the repo's clones (`external/`):

| | micro-ROS — `micro_ros_setup` (FreeRTOS Nucleo) | micro-ROS — Arduino (precompiled) | nano-ros (Phase 172) |
|---|---|---|---|
| Steps to first image | ~8: ROS 2 install · colcon ws + clone `micro_ros_setup` · `colcon build` · `create_firmware_ws.sh` · `configure_firmware.sh` · `build_firmware.sh` · `flash_firmware.sh` · build + run Agent | ~5: install Arduino IDE/CLI · add `micro_ros_arduino` lib · compile sketch · upload · build + run Agent | clone repo · `direnv allow` · `just setup` · `nros new` · `nros deploy` (~5, but `just setup` dominates) |
| Fetch scope | **board-scoped** — only that board's `freertos_apps` + CubeMX extension | **one precompiled `.a`** (`libmicroros.a`: 14 MB cortex-m3, 22 MB esp32) | **workspace-wide** — every platform's SDK |
| Host disk (deps) | ROS 2 ~1–3 GB + the board's `freertos_apps` (~0.5 GB of the 477 MB tree) + arm-none-eabi toolchain ~0.5–1 GB | Arduino IDE + the 14–22 MB lib + ROS 2 (for the Agent) | repo + `third-party/` = **7.4 GB** (qemu 2.7 GB, esp32 1.4 GB, px4 1.2 GB, zenoh 813 MB, nuttx 655 MB, threadx 389 MB, …) |
| Time to first image (ROS 2 already installed) | ~15–40 min (downloads + firmware build + Agent build) | ~10–20 min (precompiled `.a` links fast) | `just setup` **20–60+ min** (it *builds* QEMU from source) + cargo build |
| Target flash (int32 talker) | ~30–50 KB | ~30–50 KB | ~75 KB (XRCE) / ~100 KB+ (Zenoh) |
| Host runtime prereq | XRCE **Agent** | XRCE **Agent** | none (Zenoh P2P / Cyclone) · or `zenohd` · or Agent (XRCE) |

**The finding — scope, not steps.** Step *count* is similar (~5–8 either way),
but micro-ROS's onboarding is **board-scoped**: `create_firmware_ws.sh freertos
nucleo_f767zi` fetches only that board's deps (~0.5 GB), and the Arduino path is
a single 14–22 MB precompiled archive. nano-ros's `just setup` is a
**workspace-developer** action that pulls **all** platform SDKs — **7.4 GB**,
and *builds QEMU from source* — even for a user who only wants one board. So
nano-ros's *time and space to first image are roughly an order of magnitude
heavier* than micro-ROS's per-board path, despite comparable step counts. The
target flash side is the known floor gap (~30–50 KB vs ~75–100 KB).

**Actionable gap (candidate W.5): a board-scoped first-image path.** nano-ros
needs the equivalent of `create_firmware_ws.sh <board>` — a `nros setup <board>`
/ `just setup board=<x>` that fetches only that target's SDK (FreeRTOS+lwIP for
a Nucleo, not qemu+esp32+px4+…), and ideally a prebuilt or vendored QEMU instead
of a 2.7 GB source build. The Phase 172 ownership model already scopes the
*build*; setup/fetch is the missing scoping. This is the single biggest
first-image UX delta vs micro-ROS — larger than flash floor or precompiled libs.

## Summary — where each wins

**nano-ros wins:** multi-RMW (no mandatory agent; brokerless option); the
orchestration pipeline (launch→plan→generated one-binary wiring; nothing else
here generates the app); one-file declarative config with SSOT RMW/domain + RT +
params; the explicit build-ownership axis for out-of-catalog targets; Rust-first
safety + Kani/Verus + E2E CRC.

**micro-ROS wins (today):** lowest first-flash friction (one CLI + per-ecosystem
precompiled artifacts) and proven real-hardware flashing across a board catalog;
mature community + commercial support + Jazzy/Iron; smaller flash floor (~30 KB
rclc+XRCE vs nano-ros's ~75 KB XRCE / ~100 KB+ Zenoh). Two of these are
**deliberate-tradeoff** differences, not capability gaps for nano-ros: the
precompiled matrix (`target × arch × RTOS × RMW`) is a maintenance sink nano-ros
declines (source-only by choice, accepting the UX cost), and flashing is
vendor-specific — owned deploys flash via `package[]`, vendored ones follow the
vendor's flash target. The real catch-up items are *proving* one owned-target HW
flash and (later) lowering the flash floor.

**The others:** Zenoh-pico is the transport, not a peer framework (no ROS API,
no config/deploy story of its own — nano-ros is the framework over it).
Embedded DDS gives brokerless RTPS but rarely fits truly bare-metal and has no
codegen/orchestration. Arduino-ROS = packaged micro-ROS, so it inherits the
agent tax with the lowest possible build friction. `rclrs` is host-only.

## Gaps vs micro-ROS — and which are deliberate

Flashing and packaging are **vendor-specific**, so the nano-ros position
follows the Phase 172 build-ownership axis rather than chasing micro-ROS's
one-size meta-build:

1. **Flashing follows ownership (a gap in *proof*, not in *model*).** Flashing
   is a `package[]` step, and where nano-ros **owns the whole deployment**
   (`self` / `vendor-lib`) it can and should drive the flash for the best UX —
   a `package[]` line shelling out to the board's flasher, surfaced through
   `nros deploy`. Where the build is **vendored** (`vendor-module` — PX4 `make`,
   Zephyr `west`, ESP-IDF `idf.py`), flashing **follows the vendor's
   convention** (their `flash`/`upload` target), exactly as the 172 ownership
   model dictates — nano-ros does not re-implement it. So this is not a missing
   capability; the real gap is that **no owned-target HW flash is proven
   end-to-end yet** (Phase 172 W.4). micro-ROS's `flash_firmware.sh` covers a
   board catalog today; nano-ros must demonstrate the owned-flash path on at
   least one real board.
2. **Source-only distribution is a deliberate tradeoff, not a gap.** A
   precompiled matrix is `target × arch × RTOS × RMW` — combinatorially large
   and a maintenance sink. nano-ros sticks to **source distribution**
   (`add_subdirectory` + corrosion, or the generated source entry lib): it
   works for *most* platforms with one artifact and zero per-ecosystem
   packaging. The cost is UX — no drop-in Arduino/IDF zip — and we accept it for
   now. (micro-ROS pays the opposite tax: it maintains per-ecosystem packaged
   artifacts.) Revisit only if a specific high-volume target justifies a
   prebuilt.
3. **Flash floor — acknowledged future work.** rclc+XRCE's ~30 KB floor
   undercuts nano-ros (~75 KB XRCE / ~100 KB+ Zenoh); shrinking it is planned,
   not yet scheduled. For the tightest targets micro-ROS still wins on size
   today.

## See also

- `book/src/concepts/comparison-vs-microros.md` — feature parity (some Build-row
  entries predate Phase 172; corrected alongside this doc).
- `docs/research/sdk-ux/micro-ros.md` — the one-CLI / precompiled-artifact UX gap.
- `docs/roadmap/phase-172-orchestration-deferred.md` — the deployment model this
  doc evaluates (W.4 = the real-hardware-deploy gap above).
