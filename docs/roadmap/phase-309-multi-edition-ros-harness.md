# Phase 309 — multi-edition ROS test harness

**Status (2026-07-26): W1–W6 + the W5 residual landed.** The `RosEnv` provider,
per-edition docker image, peer + `domain_bridge` backend, in-container codegen,
the nano-ros publisher fixture, and the opt-in `just ros_editions ci <distro>`
composite are implemented + green against a live jazzy image. Lanes (all skip
cleanly without docker/image, none in `just ci`): peer smoke, codegen,
stock-publisher `domain_bridge`, and — the completed residual — a REAL nano-ros
CycloneDDS node (`bins/ros-edition-pose-pub`, built against the edition's
regenerated `geometry_msgs` via `just ros_editions build-fixture <distro>`)
publishing PoseStamped through the jazzy `domain_bridge` with every depth-2
nested value intact. The per-edition rebuild proved non-vacuous: jazzy's
`geometry_msgs` carries **33** messages vs humble's 32, so "rebuild against the
edition's defs" is a real difference, not a no-op.

Implements [RFC-0058](../design/0058-multi-edition-ros-test-harness.md)
(the docker-backed `RosEnv` provider). Exercises the [RFC-0056](../design/0056-ros-edition-axis.md)
ROS-edition axis against **live** peers + **live** codegen, not just the offline
golden fixtures that exist today. Motivated by issue 0267, whose fix could only
be verified by standing up a stock ROS 2 Jazzy peer by hand
(`scripts/ros/domain-bridge-repro.sh`) — this phase turns that one-off into
reusable infrastructure.

## Problem

A host installs exactly one ROS 2 edition (humble → 22.04, jazzy → 24.04; the
apt trees do not coexist). So every live ROS test path assumes the host edition
(`activate.sh` sources `/opt/ros/humble`; `nros-tests/src/ros2.rs` defaults
`humble`), and iron/jazzy/rolling are covered only by offline golden hashes.
Two things that genuinely vary per edition go untested against a real peer:

1. **Codegen** — `nros generate-rust` bindings + type hashes for that edition's
   message set.
2. **Interop** — nano-ros (built against that edition's generated code) talking
   to that edition's live ROS 2 graph + `domain_bridge`.

## Approach

A **test-only** `RosEnv` provider (RFC-0058) with a `HostRosEnv` backend (default
edition, host install, today's behavior) and a `DockerRosEnv` backend (extra
editions, `nano-ros-ros:<distro>` built locally on demand). Extra editions run in
**opt-in** lanes; the default dev loop and the user-facing build are untouched.

## Why it is phased, not one wave

W1 (refactor the host path behind the trait) must land green with **zero
behavior change** before any docker code exists — it is the safety net that
proves the abstraction did not regress the humble path. The docker image (W2) is
a slow, self-contained artifact worth isolating. The two axes (W4 codegen, W5
interop) each need the image + backend (W3) first, and W5 additionally needs the
per-edition build-tree plumbing. Each step has an independent acceptance signal.

## Work items

### W1 — `RosEnv` trait + `HostRosEnv` (refactor, no behavior change)
- New `packages/testing/nros-tests/src/ros_env.rs`: the `RosEnv` trait +
  `HostRosEnv` absorbing `ros2.rs` (`is_ros2_distro_available`,
  `ros2_env_setup*`, `require_ros2`).
- Port existing interop tests to obtain their peer via `RosEnv::for_edition`.
- **Acceptance:** the current host interop suite (`interop_e2e`,
  `xrce_ros2_interop`, `bridge_*`, `qos_zephyr_ros2_interop_e2e`) stays green on
  host humble, driven through the trait. No new skips, no wire change.

### W2 — per-edition image + build recipe
- `docker/ros-editions/Dockerfile` (`ARG DISTRO`, `FROM ros:${DISTRO}-ros-base`):
  pinned zenoh 1.7.2 + `rmw_zenoh_cpp` overlay, `domain_bridge`, codegen deps.
- `just/ros-editions.just`: `ros-edition-image <distro>` (local build, cached).
- **Acceptance:** `just ros-edition-image jazzy` builds; the image runs
  `ros2 --help`, `ros2 pkg list | grep -q rmw_zenoh_cpp`, and
  `ros2 pkg prefix domain_bridge` all succeed.

### W3 — `DockerRosEnv` backend
- Implement `available()` (image present + `docker` on PATH), `run_ros2`,
  `spawn_peer` (pub/sub/service/action), `run_domain_bridge` — all
  `docker run --network host`. Handles RAII-kill on drop (no orphan daemons).
- **Acceptance:** a gated smoke test spawns a jazzy peer, `run_ros2` echoes one
  sample, and after drop no `ros2`/container child survives (`available()` false
  ⇒ `skip!`, never silent pass).

### W4 — codegen axis
- `DockerRosEnv::generate(pkgs) -> generated-editions/<E>/` (gitignored); diff
  vs golden `fixtures/ros-editions/<E>/` (extend beyond `srv-hashes.txt` to the
  generated Rust surface / type hashes).
- Seed the jazzy golden from a verified in-container run.
- **Acceptance:** `generate(jazzy)` output is diff-clean vs the committed jazzy
  golden; a deliberate edition mismatch (humble defs vs jazzy golden) fails loud.

### W5 — per-edition interop lane (jazzy reference)
- Host builds an interop fixture against `generated-editions/jazzy/` into
  `build/ros-editions/jazzy/` (isolated lane tree).
- Interop vs `DockerRosEnv(jazzy)` peer: pub/sub + `domain_bridge` republish,
  asserting values survive (fold `scripts/ros/domain-bridge-repro.sh` in as the
  bridge helper). Reuse the 0267 shapes (PoseStamped / Control) as coverage.
- **Acceptance:** nano-ros PoseStamped both direct-echoes and survives the jazzy
  `domain_bridge` with all fields intact, driven by the harness (not the manual
  script).

### W6 — CI composite + docs
- `just ros-editions-ci` composite (image + codegen + interop for the wired
  editions); explicitly **excluded** from default `just ci`.
- Document in AGENTS.md (Testing) + book: how to add an edition, image build
  cost, the gating.
- **Acceptance:** `just ros-editions-ci` runs the jazzy lane end-to-end; `just
  ci` does not invoke it; the "add an edition" recipe is one documented step
  (iron/rolling reuse W2–W5 by distro arg).

## Done when

`just ros-editions-ci` builds the jazzy image, runs codegen (golden-clean) +
interop (PoseStamped/Control survive the live jazzy peer + `domain_bridge`), the
host humble suite is unchanged, and a second edition (iron or rolling) is wired
by distro arg alone to prove the harness is edition-parametric.
