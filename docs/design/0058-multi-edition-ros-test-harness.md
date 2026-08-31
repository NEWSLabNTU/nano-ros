---
rfc: 0058
title: "Multi-edition ROS test harness: docker-backed RosEnv provider for codegen + interop"
status: Draft
since: 2026-07
last-reviewed: 2026-07
implements-tracked-by: [phase-309]
supersedes: []
superseded-by: null
---

# RFC-0058 — Multi-edition ROS test harness

## Summary

nano-ros must generate code for, and interoperate with, **several ROS 2
editions** (humble/iron/jazzy/rolling — the RFC-0056 axis). But a single host
can install only **one** edition (humble targets Ubuntu 22.04, jazzy 24.04 — the
apt trees do not coexist). Today every ROS-dependent test path assumes the one
host edition (`activate.sh` sources `/opt/ros/humble`; `nros-tests/src/ros2.rs`
defaults `humble`). So the other editions are exercised only by **offline golden
fixtures** (`fixtures/ros-editions/jazzy/srv-hashes.txt`), never against a live
peer or a live codegen run.

This RFC introduces a **test-only** `RosEnv` provider with two backends — the
host ROS install (default edition) and a **per-edition docker container**
(`nano-ros-ros:<distro>`) — behind one interface. Extra editions run in
**opt-in internal lanes** that cover the two axes that actually vary per edition:
**codegen** (does `nros generate-rust` produce the right bindings for that
edition's message set + type hashes?) and **testing** (does nano-ros, built
against that edition's generated code, interoperate with that edition's live
ROS 2 peer + `domain_bridge`?).

## Scope / non-goals

- **Internal test + dev infrastructure only.** Nothing user-facing changes.
  Users build nano-ros against **their** host ROS and **their** message
  packages — that stays their responsibility. No Dockerfile enters the ship
  path; users never need docker. The `RosEnv` provider lives entirely in
  `packages/testing/nros-tests` and is never referenced by the product crates,
  the `nros` CLI, or an example's build.
- **Default dev inner loop is unchanged.** Host humble, no docker,
  `activate.sh` untouched. Docker is reached only for editions the host does not
  have, and only in lanes that are opt-in (never part of default `just ci`).
- **Not a code-shipping mechanism.** The per-edition images are a way to *run*
  ROS for a test, not to distribute nano-ros.

## Design

### One provider, two backends

```
trait RosEnv                     // "for edition E, do a ROS thing"
  fn available(&self) -> bool     // host: /opt/ros/<d> sources + `ros2` works
                                  //  docker: image built + `docker` on PATH
  fn edition(&self) -> &str
  fn run_ros2(&self, args) -> Output          // ros2 topic/service/node CLI
  fn spawn_peer(&self, PeerSpec) -> PeerHandle // pub/sub/service/action; RAII kill
  fn run_domain_bridge(&self, BridgeCfg) -> BridgeHandle
  fn generate(&self, GenSpec, out_dir)         // codegen in that edition's env

HostRosEnv    // sources /opt/ros/<distro>/setup.bash (+ pinned zenoh overlay).
              // Absorbs today's ros2.rs (is_ros2_distro_available, ros2_env_setup*).
              // The DEFAULT edition when the host has it.
DockerRosEnv  // docker run --network host  nano-ros-ros:<distro>
              // EXTRA editions. Image bakes distro + zenoh 1.7.2 + rmw_zenoh
              // overlay + domain_bridge + codegen deps.
```

`RosEnv::for_edition(distro)` resolves a backend: host if the host distro matches
and is available, otherwise docker. A test asks for an edition and gets a uniform
surface; it never branches on host-vs-docker.

### Per-edition images — built locally on demand

One parametric `docker/ros-editions/Dockerfile` (`ARG DISTRO`, `FROM
ros:${DISTRO}-ros-base`) bakes the pinned zenoh 1.7.2 + `rmw_zenoh_cpp` overlay
(wire-compat with `packages/rmw/zenoh`), `domain_bridge`, and the codegen toolchain.
`just ros-edition-image <distro>` builds it locally (docker layer cache; the
first build is slow — zenoh is a source build). No registry, no auth, no
publish flow — matches the "build locally on demand" decision. CI that wants
speed can cache the image layer.

### The two axes

**Codegen axis.** `DockerRosEnv(E).generate(pkgs)` runs `nros generate-rust`
(and the cyclone IDL path) inside edition E, writing to an **edition-scoped**
`generated-editions/<E>/` (gitignored, ephemeral) so an extra-edition run never
clobbers the committed default-edition `generated/`. The output is diffed
against committed golden fixtures `fixtures/ros-editions/<E>/` (extending
today's `srv-hashes.txt`) — a deterministic, build-free codegen check.

**Testing axis.** The host builds an interop fixture against
`generated-editions/<E>/` into a **per-edition build tree**
(`build/ros-editions/<E>/`, isolated like the platform-sweep lane dirs so it
never stales the default tree), then runs it against a live `DockerRosEnv(E)`
peer — pub/sub, service, action, and `domain_bridge` republish — asserting the
values survive. This is a true per-edition interop test: nano-ros's
edition-specific generated code against that edition's real peer.

### Data flow (one edition-E lane)

```
just ros-edition-image E                      # once, cached
  → DockerRosEnv(E).generate(pkgs)            # codegen axis
      → generated-editions/E/  ── diff ──▶ fixtures/ros-editions/E/ (golden)
  → host build fixture against generated-editions/E/  → build/ros-editions/E/
  → fixture  ↔  DockerRosEnv(E) peer / domain_bridge  # testing axis
      → assert values intact
```

### Contract

- `available()` false (no image / no docker / host distro absent) ⇒
  `nros_tests::skip!` — never a silent pass (CLAUDE.md fail-loud rule). Same
  contract the QEMU lanes already use.
- Peer + bridge handles are **RAII** — dropped handles kill the child (reuse the
  `patter[n]` self-match guard; no orphan `ros2` daemons survive a test).
- Per-edition build-tree isolation keeps the fixture-mtime treadmill off the
  default tree.
- The docker lanes are **gated**: excluded from default `just ci` (they need
  docker + a slow image build); a `just ros_editions ci` composite runs them
  for internal/CI use.

## Alternatives considered

- **Host installs per edition** — impossible on one box (distro Ubuntu bases
  differ); would force one CI machine per edition. Rejected.
- **Prebuilt images pushed to GHCR** — faster first run, but adds a publish
  flow + registry auth for a test-only artifact. Deferred; the Dockerfile is
  registry-ready if that changes.
- **Host-default nano-ros vs edition peers (no per-edition rebuild)** — cheap
  but only tests wire compat, missing edition-specific generated-code drift.
  Rejected: the codegen axis is half the point.
- **Uniform docker for the default edition too** — cleaner single mechanism but
  slows the everyday loop and forces docker on all ROS work. Rejected: host
  fast-path for the default edition is kept.

## Implementation

Tracked by **phase-309**. Related: RFC-0056 (the edition axis this exercises),
issue 0267 (the depth-2 nested bug whose live verification motivated a real
multi-edition peer harness).
