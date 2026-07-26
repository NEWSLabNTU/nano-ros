# Phase 311 — RMW × ROS-edition interop matrix

**Status (2026-07-26): W1 landed; zenoh lane DEFERRED (issue #0291); XRCE in
progress.** The zenoh lane (W2/W5) is blocked on a version gap the source check
surfaced: zpico pins zenoh **1.7.2** / vendored rmw_zenoh **1.7.1**, but a stock
jazzy `rmw_zenoh_cpp` is **1.11.2** — a 4-minor-version gap, near-certainly
wire-incompatible. Building the pinned-1.7.1 overlay would test a *non-default*
jazzy zenoh, not a stock peer, so it is NOT the meaningful test; the real fix is
a zpico version bump, filed as **issue #0291**. Zenoh is gated on that. The
matrix this phase delivers is therefore **{jazzy, iron} × {cyclone, xrce}**, with
zenoh added once #0291 lands.

Extends the multi-edition harness
([RFC-0058](../design/0058-multi-edition-ros-test-harness.md), phase-309/310)
from **cyclone-only** to the full **RMW axis** — so nano-ros's Zenoh (zpico) and
XRCE paths are verified against live ROS 2 peers across ROS editions, not just
CycloneDDS. Also closes the phase-41 residual (the zenoh keyexpr type-hash tail
accepted by a live `rmw_zenoh_cpp` peer).

## Problem

The 309/310 lanes all use CycloneDDS: nano-ros `rmw-cyclonedds` ↔ a
`rmw_cyclonedds_cpp` ROS peer on a shared RTPS domain. nano-ros's two other RMWs
are unverified against a non-host edition:

- **Zenoh** (`zpico`, pinned zenoh **1.7.2**): nano-ros client ↔ a `zenohd`
  router ↔ a `rmw_zenoh_cpp` ROS client. The type hash rides the **zenoh
  keyexpr** (not RTPS typeinfo), so this is the path phase-41's RIHS01 work
  actually affects on the wire.
- **XRCE** (micro-XRCE-DDS client): nano-ros client → a **micro-XRCE Agent**
  (Fast-DDS) ← DDS → a `rmw_fastrtps_cpp` ROS peer.

## Approach

Add an `Rmw` dimension {Cyclone, Zenoh, Xrce} to the harness, crossed with the
edition axis. Target the **full matrix** {jazzy, iron} × {cyclone, zenoh, xrce}
(6 cells). The nano-ros side is the native example nodes built with the matching
`rmw-*` feature per (edition, rmw); the ROS side + intermediary run in the
edition container.

| RMW | nano-ros | intermediary (container) | ROS peer |
|-----|----------|--------------------------|----------|
| Cyclone | `rmw-cyclonedds` | — (shared domain, RTPS) | `rmw_cyclonedds_cpp` |
| Zenoh | `rmw-zenoh` (zpico 1.7.2) | `rmw_zenohd` router (host-net) | `rmw_zenoh_cpp` |
| XRCE | `rmw-xrce` | micro-XRCE Agent (UDP) | `rmw_fastrtps_cpp` |

**Wire-compat (decided):** the image's apt `rmw_zenoh_cpp` is 0.2.9 (some zenoh
1.x) — NOT guaranteed to match zpico's pinned 1.7.2. So the images **bake the
pinned zenoh 1.7.2 + `rmw_zenoh_cpp` overlay from source** (replicating
`just rmw_zenoh setup`: colcon `zenoh_cpp_vendor rmw_zenoh_cpp` from
`third-party/zenoh/rmw_zenoh`), and the micro-XRCE Agent from source
(`scripts/xrce-agent/build.sh`). These are heavy per-edition source builds — the
RFC-0058-deferred overlay work, now required for a real zenoh lane.

## Work items

### W1 — RMW axis in the harness
- `nros_tests::ros_env`: an `Rmw` enum {Cyclone, Zenoh, Xrce}; `test_rmw()` reads
  `NROS_RMW` (default `cyclone`). `e2e_setup` + `nano_node_cmd` take the RMW so
  the nano-ros node is spawned with the right env (cyclone: ROS_DOMAIN_ID; zenoh:
  RMW_IMPLEMENTATION=zenoh + locator; xrce: agent addr). `DockerRosEnv` grows the
  matching ROS-peer helpers per RMW (zenoh: `rmw_zenoh_cpp` + start `rmw_zenohd`;
  xrce: start the Agent + `rmw_fastrtps_cpp`).
- **Acceptance:** unit tests for the env/command composition per RMW; existing
  cyclone lanes unchanged (RMW defaults to cyclone).

### W2 — pinned zenoh 1.7.2 + rmw_zenoh overlay in the edition image
- Extend `docker/ros-editions/Dockerfile` (or a follow-on build step) to colcon-
  build `zenoh_cpp_vendor` + `rmw_zenoh_cpp` from `third-party/zenoh/rmw_zenoh`
  (the pinned 1.7.2 source) into `/opt/nros-overlay`, sourced alongside the ROS
  setup. Include `rmw_zenohd`.
- **Acceptance:** `image-check <distro>` shows `rmw_zenoh_cpp` from the overlay
  (not apt) + `rmw_zenohd`; a zenoh handshake between two overlay nodes works.

> **W2 investigation notes (2026-07-26 — probe, not yet landed).** Findings from
> attempting the lighter "host zenohd as shared router" shortcut, kept so the
> next attempt starts informed:
> 1. **`rmw_zenoh_cpp` REQUIRES its own `rmw_zenohd` router** — it does NOT
>    auto-connect to an arbitrary `zenohd`. A stock `ros2 topic echo` under
>    `rmw_zenoh_cpp` warns *"Unable to connect to a Zenoh router … start one with
>    `ros2 run rmw_zenoh_cpp rmw_zenohd`"*. To use nano-ros's pinned 1.7.2 zenohd
>    as the shared router, the ROS side needs an explicit `ZENOH_ROUTER_CONFIG_URI`
>    / connect-endpoint config pointing at it (and the apt-0.2.9 client must
>    version-negotiate with the 1.7.2 router — the compat question, still open).
> 2. **Orchestration:** `zenohd` needs `--listen tcp/127.0.0.1:7447` (bare
>    `zenohd` doesn't listen on 7447) and exits immediately when detached via a
>    chained shell `&` or the harness background mechanism (works foreground /
>    under `timeout`) — start it under a process manager that keeps a live
>    session, or via the `nros_tests` process helpers, not a bare `&`.
> 3. **Two viable paths, ranked:** (a) the router shortcut above — cheap if
>    apt-0.2.9 ↔ pinned-1.7.2-router negotiates, needs the ROS-side connect
>    config + one compat test; (b) the full overlay build (this W2 as written) —
>    guaranteed wire-match, heavy (rust + colcon `zenoh_cpp_vendor`). Resolve the
>    compat question first (path a's one test) before committing to path b.

### W3 — micro-XRCE Agent in the edition image
- Build `MicroXRCEAgent` from source (`scripts/xrce-agent/build.sh` shape) into
  the image; expose it on PATH.
- **Acceptance:** `image-check` runs `MicroXRCEAgent --help`; a smoke XRCE↔DDS
  bridge stands up.

> **W3 progress (2026-07-27).** Built `MicroXRCEAgent` (24.04-ABI) by mounting
> `third-party/xrce/agent` into the jazzy image (cmake/g++ ARE already present)
> → `build/ros-editions/jazzy/xrce-agent/MicroXRCEAgent`. It runs (usage:
> `MicroXRCEAgent <udp4|tcp4|serial|…> <args>`). **TODO:** only the executable was
> copied — it dynlinks the built Fast-DDS/fastcdr/microxrcedds_agent `.so`s, so
> the runtime lane must copy the whole `build/*/lib` install (or set
> `LD_LIBRARY_PATH`), not just the binary. Low XRCE version-gap risk (client +
> agent are co-pinned submodules; the DDS side is standard RTPS).

### W3.5 — example rmw-xrce build (blocker found)
- The example nodes do NOT build with a naive `cargo build --no-default-features
  --features rmw-xrce` — `nros-macros` errors `no method resolved_params for
  &NodeInstance` (`main_macro.rs:644`). The working fixture build (host xrce
  tests) uses the `examples/fixtures.toml` xrce rows (`features = ["rmw-xrce"]`,
  `target_dir = "target-xrce"`) via the build-stage recipe — so a feature the
  naive flags drop provides `resolved_params`. **Next attempt must reuse the
  fixtures.toml xrce feature set (via `build_example_rmw`/the build recipe), not
  hand-rolled flags.** (Pre-existing to this phase; unrelated to the edition/
  rolling work.)

### W4 — per-(edition, rmw) example fixtures
- Extend `build-e2e-fixtures` to build the six example nodes with the selected
  `rmw-*` feature into `target-ros-edition-<distro>-<rmw>/`.
- **Acceptance:** all (edition, rmw) fixture sets build.

### W5 — zenoh interop lane (both directions)
- nano-ros zpico node ↔ `rmw_zenohd` ↔ `rmw_zenoh_cpp` ROS peer, per edition,
  pub/sub (+ service/action as feasible). Asserts delivery AND — closing the
  phase-41 residual — that the keyexpr type-hash tail is accepted at discovery.
- **Acceptance:** pub/sub survives both ways vs a live `rmw_zenoh_cpp` edition
  peer; iron/jazzy use the real RIHS01 keyexpr.

### W6 — XRCE interop lane (both directions)
- nano-ros XRCE node → Agent → `rmw_fastrtps_cpp` ROS peer, per edition, pub/sub
  (+ service/action as feasible).
- **Acceptance:** pub/sub survives both ways through the Agent vs a live
  `rmw_fastrtps_cpp` edition peer.

### W7 — matrix CI + docs
- `just ros_editions ci <distro>` runs all three RMWs (env-driven `NROS_RMW`);
  build the per-rmw fixtures. AGENTS.md documents the RMW × edition matrix. Still
  opt-in, still deselected from `just ci`.
- **Acceptance:** `just ros_editions ci jazzy` and `ci iron` run the full
  {cyclone, zenoh, xrce} matrix green; `just ci` stays docker-free.

## Done when

The harness runs pub/sub (min) both directions for all six cells
({jazzy, iron} × {cyclone, zenoh, xrce}) against live ROS 2 peers — cyclone over
RTPS, zenoh over a pinned-1.7.2 `rmw_zenohd`, xrce through a micro-XRCE Agent —
green via `just ros_editions ci <distro>`, with the zenoh lane exercising the
real RIHS01 keyexpr (closing the phase-41 residual).
