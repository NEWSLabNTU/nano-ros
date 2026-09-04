# Phase 311 — RMW × ROS-edition interop matrix

**Status (2026-07-27): W1/W3/W3.5/W4/W5/W6/W7 landed — XRCE full (jazzy+iron) +
Zenoh (jazzy) both green.** Delivered matrix: **{jazzy, iron} × {cyclone, xrce}**
(4 cells, pub/sub+service+action both ways) **+ {jazzy} × zenoh** (6/6; #0292 fixed). All via `just ros_editions ci <distro>`.

**W5 (zenoh) resolved the #0291 premise (2026-07-27):** the zenoh version gap
was a red herring — the wire is proto `0x09` on both sides (zpico 1.7.2 handshakes
with a stock jazzy 1.11.2 router, live-proven). The real blocker was the RIHS01
keyexpr type-hash tail (fixtures built `ros-humble` → placeholder). Fixed by
selecting the edition on the examples like the RMW (`ros-<edition>` passthrough
feature) + regenerating msgs per edition; NO zenoh bump. Only jazzy ships
`rmw_zenoh_cpp` (iron/humble skip the lane loudly). Completes phase-304
W4-remaining (the nano↔jazzy zenoh wire lane). Version divergence → future work.

> **SUPERSEDED by this doc's own status line — kept because the reasoning is
> the useful part.** The paragraph below says the zenoh lane is BLOCKED and the
> delivered matrix excludes zenoh; the status line two paragraphs up says W5
> landed, zenoh (jazzy) is 6/6, and the version gap was a red herring. The
> status line is the true one, and it was written LATER on the same day — this
> paragraph is the pre-W5 framing that never got struck. It is a worked example
> of the "reversed premise" class phase-419 W3 names: a confident blocker,
> refuted by the measurement it asked for, still reading as live.
>
> Two of its facts have also expired since. zpico no longer pins zenoh-pico
> **1.7.2** — phase-415 moved the patch line to **1.8.0** on 2026-09-04 — and
> **#0291 did not close by a version bump**; W5 closed it by proving the wire is
> proto `0x09` on both sides, which is why no bump was needed for interop.

*Pre-W5 framing, 2026-07-27, refuted the same day:* The zenoh lane (W2/W5) is
blocked on a version gap the source check
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

**Wire-compat (decided, then REVERSED — 2026-08-19).** This said the image's apt
`rmw_zenoh_cpp` is "NOT guaranteed to match zpico's pinned 1.7.2", so the images
would bake a pinned overlay from source. That was never implemented: the
Dockerfile installs the apt package and left the source pin as a
`WITH_ZENOH_PIN` layer that exists only in a comment. Issue 0291 then refuted the
premise — zenoh's wire is proto-`0x09`-stable across 1.x, so zpico 1.7.2
interoperates with a far newer distro RMW, and the real finding was the keyexpr
type-hash. The overlay and its `third-party/zenoh/rmw_zenoh` submodule are now
deleted (RFC-0075, amended). The images use apt; only the micro-XRCE Agent is
still a source build (`scripts/xrce-agent/build.sh`).

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
  install `rmw_zenoh_cpp` from apt (the from-source plan was reversed; see above)
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

> **W3 RESOLVED (2026-07-27).** The lib-packaging TODO is moot — the nros SDK
> store ALREADY ships a **relocatable** Agent at
> `~/.nros/sdk/xrce-agent/<ver>/bin/MicroXRCEAgent` (a launcher that sets
> `LD_LIBRARY_PATH` to its bundled `../lib` Fast-DDS/fastcdr/microxrcedds_agent
> `.so`s), provisioned by `nros setup … --rmw xrce`. The lane runs it **on the
> host** (host-native ABI, no container), not the 24.04 image build. Because
> every container peer is `--network host`, the Agent's UDP side (nano-ros
> client) and Fast-DDS side (the `rmw_fastrtps_cpp` container peer) share host
> loopback. `ros_env::host_xrce_agent_bin()` locates it; `spawn_xrce_agent(port,
> domain)` runs `MicroXRCEAgent udp4 -p <port>` with `ROS_DOMAIN_ID=<domain>`.
> The abandoned 24.04-in-image build is not used.

### W3.5 — example rmw-xrce build — RESOLVED (2026-07-27)
- **It was NOT a feature-flag or code bug — it was a `ros-launch-manifest`
  submodule DRIFT.** `nros-macros` (a core crate) depends on
  `ros-launch-manifest-model` by path (the submodule at
  `packages/cli/third-party/ros-launch-manifest`) and calls
  `NodeInstance::resolved_params` (`main_macro.rs:644`). The submodule was checked
  out at a **divergent** commit `db91f2bad` (heads/main, from a parallel agent)
  that lacks `resolved_params`, while the superproject records `0612574f4` (which
  HAS it, added in "project params_files YAML into resolved parameters", #276).
  A fresh `nros-macros` compile (triggered by the xrce build's distinct
  target-dir) hit the stale checkout; the cyclone builds passed only on a cached
  `nros-macros`.
- **Fix:** sync the submodule to the recorded commit —
  `git -C packages/cli/third-party/ros-launch-manifest checkout 0612574f4`
  (or `git submodule update`). Then ALL six xrce example nodes build with
  `--no-default-features --features rmw-xrce` (talker/listener/service-{server,
  client}/action-{server,client}, verified). No code change; a submodule-sync
  hygiene fix (the CLAUDE.md submodule-drift rule).

### W4 — per-(edition, rmw) example fixtures — DONE (2026-07-27)
- `build-e2e-fixtures <distro> <rmw>` gained the `rmw` arg (`cyclone|zenoh|xrce`
  → `rmw-cyclonedds|rmw-zenoh|rmw-xrce`), building the six nodes into
  `target-ros-edition-<distro>-<rmw>/`. **Path convention unified:** the
  phase-310 cyclone lanes now also use the `-<rmw>` suffix (`example_bin`
  delegates to `example_bin_rmw(.., Cyclone)`) — one scheme, no duplication.
- **Verified:** cyclone + xrce fixture sets build for jazzy + iron.
- **Footgun found + fixed — the `NROS_RMW` canonical-name trap.** `just
  ros_editions ci` exports `NROS_RMW=<token>` as the lane selector; that env
  LEAKS into the spawned nano-ros node. `Executor::open` (spin.rs) resolves the
  backend by the env's value against the backend's REGISTERED name — and cyclone
  registers as **`cyclonedds`**, not the harness token `cyclone`. So an ambient
  `NROS_RMW=cyclone` mis-resolved → `Transport(ConnectionFailed)` and every
  cyclone cell went red (empty, silent, discovery-looking failure). Fix:
  `Rmw::nros_rmw_name()` returns the canonical selector (`cyclonedds`/`zenoh`/
  `xrce`), and BOTH `nano_node_cmd` + `nano_node_cmd_rmw` now set `NROS_RMW`
  explicitly to it — self-contained selection, immune to the ambient leak.

### W5 — zenoh interop lane (both directions) — DONE (2026-07-27, jazzy)
- `tests/ros_editions_zenoh.rs`: nano-ros zpico node ↔ `rmw_zenohd`
  (`spawn_zenoh_router`, readiness-polled) ↔ `rmw_zenoh_cpp` peer
  (`Middleware::Zenoh { domain_id }`, `--network host`). Six tests: pub/sub,
  service, action, both directions.
- **The #0291 investigation** proved transport-compat (proto `0x09`) and pinned
  the blocker to the RIHS01 keyexpr tail — the fix is building the fixture with
  the `ros-<edition>` feature, not a zenoh bump. Examples now carry
  `ros-humble`/`ros-iron`/`ros-jazzy` passthrough features (mirroring the RMW
  block); `build-e2e-fixtures … zenoh` regenerates msgs for the edition.
- **The native zpico zenoh node uses a COMPILE-TIME domain (0)** — it ignores
  `ROS_DOMAIN_ID`/`NROS_DOMAIN_ID` at runtime (unlike cyclone), and the domain is
  the first keyexpr segment, so the peer runs on domain 0 too. The lane is serial
  (one host router on tcp/7447).
- **Verified GREEN:** jazzy **6/6** — pub/sub, service, action, both directions.
  (ROS→nano action SERVER needed the #0292 fix: per-session entity-id + the
  send_goal/get_result SERVICE hashes — now landed.)
  iron/humble ship no `rmw_zenoh_cpp` → the lane `skip!`s loudly.
- **Acceptance met** for the pub/sub minimum both ways vs a live `rmw_zenoh_cpp`
  jazzy peer, using the real RIHS01 keyexpr (closes the phase-41 residual on the
  wire); completes phase-304 W4-remaining.

### W6 — XRCE interop lane (both directions) — DONE (2026-07-27)
- `tests/ros_editions_xrce.rs`: nano-ros `rmw-xrce` node → host Agent
  (`spawn_xrce_agent`) → `rmw_fastrtps_cpp` container peer (`Middleware::FastRtps`
  env, `--network host`, same domain). Six tests — pub/sub, service, action, each
  both directions. `e2e_setup_xrce` guards on fixture + Agent + docker + image
  (skips, never a silent pass).
- **Verified GREEN:** jazzy 6/6, iron 6/6 (`NROS_RMW=xrce`).

### W7 — matrix CI + docs — DONE (2026-07-27, xrce+cyclone; zenoh deferred)
- `just ros_editions ci <distro>` builds the cyclone + xrce per-rmw fixtures and
  runs the cyclone lanes (`NROS_RMW=cyclone`) then the xrce lane
  (`NROS_RMW=xrce`); prints the zenoh-deferred note (#0291). Still opt-in, still
  deselected from `just ci` (`not binary(~ros_editions)` covers the new lane).
- AGENTS.md documents the RMW × edition matrix + the `NROS_RMW` canonical-name
  footgun.
- **Acceptance met:** `ci jazzy` + `ci iron` run {cyclone, xrce} green; zenoh
  gated on #0291; `just ci` stays docker-free.

## Done when

The harness runs pub/sub (min) both directions for the delivered cells
({jazzy, iron} × {cyclone, xrce} + {jazzy} × zenoh) against live ROS 2 peers —
cyclone over RTPS, xrce through a host micro-XRCE Agent to `rmw_fastrtps_cpp`,
zenoh through a `rmw_zenohd` to `rmw_zenoh_cpp` — green via
`just ros_editions ci <distro>`. **Met (2026-07-27).**

**The zenoh cells DID land** (jazzy, 6/6) — issue #0291's investigation refuted the
version-gap premise (zenoh proto `0x09` is stable across 1.x; zpico 1.7.2 interops
with jazzy 1.11.2), so the fix was the RIHS01 keyexpr tail (build the fixture
`ros-jazzy`), not a zpico bump. #0292 fixed the action-server service-hash tail.
This also closed the phase-41 RIHS01-keyexpr residual on the wire. Only **iron +
humble zenoh** stay N/A — those editions ship no `rmw_zenoh_cpp` (the lane skips
loudly). Zenoh **version** divergence (relevant only if a future edition bumps the
proto past `0x09`) remains future work under #0291.
