# Phase 310 — multi-edition rich E2E (pubsub + service + action, both directions)

**Status (2026-07-26): Draft.** Extends the [phase-309](phase-309-multi-edition-ros-harness.md)
multi-edition harness ([RFC-0058](../design/0058-multi-edition-ros-test-harness.md))
from one thin E2E slice (nano-ros PoseStamped publish → `domain_bridge`) to a
**rich** matrix: pub/sub, service, and action, in **both directions**, between a
real nano-ros CycloneDDS node and a live ROS 2 edition peer, over **direct
same-domain** RTPS (no bridge).

## Problem

The only product E2E against a non-host edition today is
`ros_editions_nano_interop.rs`: nano-ros *publishes* one topic through a
`domain_bridge`. It exercises neither the receive direction, nor services, nor
actions. The host-humble suite (`interop_e2e.rs`, `services.rs`, `actions.rs`)
covers all of those — but only against the single host edition. A ROS 2 edition
can change service/action wire details (the RIHS type hash, `_Event` synthesis,
XCDR), so "nano-ros services/actions interoperate with jazzy/iron" is currently
unverified.

## Approach

Reuse the existing native Rust example nodes as the nano-ros side, built with
`--features rmw-cyclonedds` into a **per-edition, per-example build dir**
(`target-ros-edition-<distro>`) so the default/host builds are untouched. The
ROS 2 side runs inside the phase-309 `DockerRosEnv` container (`ros2` CLI +
the rclpy servers already scripted in `nros-tests::ros2`). nano-ros (host) and
the ROS peer (container, `--network host`) share a `ROS_DOMAIN_ID` and discover
over RTPS — the same host↔container cyclone path issue 0267 verified.

**Nano-ros side is built against each example's committed (host) generated
bindings.** The interop types here (`std_msgs/Int32`,
`example_interfaces/{AddTwoInts,Fibonacci}`) are edition-stable, so
host-generated == edition-generated on the wire; this avoids dirtying every
example's committed `generated/`. Per-edition regeneration (as phase-309's
PoseStamped fixture does, where jazzy vs humble genuinely differ) can be layered
on later for a divergent type without changing the lane structure.

### The six direction-pairs

| # | Direction | nano-ros side (host bin) | ROS side (container) | Type | Assert on |
|---|-----------|--------------------------|----------------------|------|-----------|
| 1 | pub → | `talker` (`NROS_PUB_TYPE=int32`) | `ros2 topic echo` | `std_msgs/Int32` | echo output |
| 2 | → sub | `listener` (`NROS_SUB_TYPE=int32`) | `ros2 topic pub` | `std_msgs/Int32` | listener stdout |
| 3 | client → | `service-client` | rclpy `add_two_ints` server | `AddTwoInts` | client stdout (sum) |
| 4 | → server | `service-server` | `ros2 service call` | `AddTwoInts` | call output (sum) |
| 5 | client → | `action-client` | rclpy `fibonacci` server | `Fibonacci` | client stdout (result) |
| 6 | → server | `action-server` | `ros2 action send_goal` | `Fibonacci` | send_goal output |

Each test allocates a unique `ROS_DOMAIN_ID` (`unique_ros_domain_id`) so
concurrent lanes don't collide on the RTPS ports.

## Work items

### W1 — `DockerRosEnv` E2E peer helpers
- Container-side, direct-cyclone-domain helpers mirroring the host
  `nros-tests::ros2` cyclone helpers: `topic_pub`/`topic_echo` (Int32),
  `add_two_ints_server` (rclpy) + `service_call`, `fibonacci_server` (rclpy) +
  `action_send_goal`. Each sources the distro + exports
  `RMW_IMPLEMENTATION=rmw_cyclonedds_cpp` + `ROS_DOMAIN_ID`. Reuse the rclpy
  server scripts verbatim from `ros2.rs`.
- **Acceptance:** a gated unit-ish smoke spawns the rclpy `add_two_ints` server
  in the jazzy container and a `ros2 service call` (both cyclone, same domain)
  and sees the reply — proving the in-container server/client helpers work.

### W2 — per-edition example build recipe
- `just ros_editions build-e2e-fixtures <distro>`: build `talker`, `listener`,
  `service-server`, `service-client`, `action-server`, `action-client` with
  `--features rmw-cyclonedds --target-dir target-ros-edition-<distro>`.
- **Acceptance:** the recipe produces all six binaries under each example's
  `target-ros-edition-<distro>/debug/`; a missing prereq fails loud.

### W3 — pub/sub E2E, both directions
- `ros_editions_e2e_pubsub.rs`: pairs 1 (nano `talker` → `ros2 topic echo`) and
  2 (`ros2 topic pub` → nano `listener`), on a shared domain, asserting the
  Int32 value crosses each way. Skips without the fixtures/docker/image.
- **Acceptance:** both directions deliver the value against jazzy.

### W4 — service E2E, both directions
- `ros_editions_e2e_service.rs`: pairs 3 (nano `service-client` → rclpy server)
  and 4 (`ros2 service call` → nano `service-server`), `AddTwoInts`, asserting
  the sum both ways.
- **Acceptance:** both directions return the correct sum against jazzy.

### W5 — action E2E, both directions
- `ros_editions_e2e_action.rs`: pairs 5 (nano `action-client` → rclpy fibonacci
  server) and 6 (`ros2 action send_goal` → nano `action-server`), `Fibonacci`,
  asserting the result sequence both ways.
- **Acceptance:** both directions complete the goal + return the sequence
  against jazzy.

### W6 — CI wiring + docs
- Fold `build-e2e-fixtures` + the three e2e test binaries into
  `just ros_editions ci <distro>` (still opt-in, still deselected from the
  default `test-all` sweep by the `not binary(~ros_editions)` filter). Document
  the matrix in AGENTS.md.
- **Acceptance:** `just ros_editions ci jazzy` and `ci iron` run all lanes
  (harness + codegen + the 6 e2e pairs) green; `just ci` still never touches
  docker.

## Done when

`just ros_editions ci <distro>` runs the full matrix — pub/sub, service, action,
both directions — between real nano-ros nodes and a live ROS 2 edition peer over
direct same-domain cyclone, green for jazzy + iron, and the default `just ci`
sweep remains docker-free.
