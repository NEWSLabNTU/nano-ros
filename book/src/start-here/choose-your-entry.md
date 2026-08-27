# Choose Your Entry

Different readers want different paths through this book. Pick the
shoe that fits and jump straight to the right page.

## 🧪 I'm taking a glance

You heard about nano-ros and want to know in 5 minutes whether
it's worth playing with.

- Start at the **["Can I use nano-ros right now?"
  matrix](../introduction.md#quick-board-check--does-it-work-on-the-board-i-have-today)**
  in the intro. One row per dev board you might have on your desk.
- Then the **[Project Status](../introduction.md#project-status)**
  paragraph for the maturity signal.
- If you stay interested, jump to one of the starters below.

## 🌱 I've never used ROS

New to ROS entirely? Two ideas carry the whole book: a **node** is a
process-like unit that computes; nodes talk through named **topics**
(publish/subscribe), plus request/reply **services**. ROS 2 is the
ecosystem standardizing those; an **RMW** ("ROS MiddleWare") backend
is the transport that moves the bytes. nano-ros is a small
implementation of that model for microcontrollers. That's enough to
start:

1. **[Install](../getting-started/installation.md)** — host toolchain,
   no ROS 2 required.
2. **[First Project](../getting-started/first-project.md)** — scaffold
   a two-node workspace and watch one node hear the other.
3. **[Anatomy](../getting-started/anatomy.md)** explains what you
   built, in nano-ros's own terms.

## 🔧 I want it running on the board in my hand tonight

Honest expectations first: the fastest real win is on your **Linux
host** (~10 minutes, no ROS 2, no daemon — `--rmw cyclonedds`), and
every embedded chapter has a QEMU flow that works without hardware.
Real-hardware flashing is currently documented only for out-of-tree
boards ([STM32F4 worked example](../porting/stm32f4-out-of-tree.md)).
The zenoh embedded flows need a ROS 2 install on the host for the
router.

1. Check your board's row in **[Supported
   Boards](../reference/supported-boards.md)**.
2. Take the host win: [First Project](../getting-started/first-project.md).
3. Then your platform's chapter under **Bring Your Own RTOS**
   (FreeRTOS / Zephyr / NuttX / ThreadX / ESP32 / bare-metal).

## 🤖 I have rclcpp/rclpy nodes and a colcon workspace

Experienced ROS 2 developer porting existing nodes to an MCU:

1. **[Setup Compared to Standard ROS 2](./setup-compared-to-ros2.md)**
   — what stays familiar (package.xml, launch XML, `find_package`)
   and what changes (no install prefix, compile-time RMW).
2. **[Porting a ROS 2 C++ node](../getting-started/porting-a-cpp-node.md)**
   — the rclcpp-compat shim and its limits.
3. **[C / C++ multi-node workspaces](../getting-started/workspace-cpp.md)**
   — the colcon-shaped workspace flow.
4. **[ROS 2 Interoperability](../getting-started/ros2-interop.md)** —
   wire your MCU node into your existing graph. Zenoh needs
   `ros-jazzy-rmw-zenoh-cpp` (Humble ships no apt package);
   Cyclone DDS interops with `rmw_cyclonedds_cpp` directly, no router.
5. **[Migration Guide](./migration-guide.md)** — concept-to-concept map.

## 🔌 I have a board (or a vendored SDK tree) on my desk

Already have hardware — an ESP32, an STM32Cube or MCUXpresso project, a
Zephyr or NuttX workspace? Two-step path:

1. **Linux first** — [First Project](../getting-started/first-project.md)
   on your host verifies the whole stack in ~10 minutes, with no daemon
   and no ROS 2 install (`nros setup native --rmw cyclonedds`, one
   scaffold command, `cmake`).
2. **Then your target** — start at
   **[How Integration Works](../getting-started/how-integration-works.md)**:
   your RTOS keeps its own build tool (west, make, idf.py, your IDE) and
   nano-ros plugs into it. One chapter per host build system follows it.
3. **Company tree, own BSP, forked RTOS?** Start at
   [Integrating into a Vendored Tree](../getting-started/vendored-tree.md)
   (pinning, air-gapped CI, the patch set you carry), then the
   Porting Guide: [Build as a CMake
   subdirectory](../getting-started/build-as-subdirectory.md),
   [Custom Board Package](../porting/custom-board.md),
   [Vendor Overlay](../porting/vendor-overlay.md), and the
   [STM32F4 out-of-tree worked example](../porting/stm32f4-out-of-tree.md).
   Know up front: adding a *platform* (new RTOS) or an in-catalog
   *board* still means carrying a small patch set inside your vendored
   nano-ros checkout — plan for a fork-with-rebase workflow against
   the `nros-v<X.Y.Z>` tags.

## 🚀 I want to get started shipping something

You've decided to use nano-ros and want a working system on Linux
first, then move it to your target.

1. **[Install](../getting-started/installation.md)** — three commands:
   `./scripts/bootstrap.sh`, `source ./activate.sh` (every new shell),
   then `nros setup native --rmw cyclonedds`.
2. **[First Project](../getting-started/first-project.md)** — one
   scaffolded workspace, C++ and CMake, publishing with nothing else
   running. Rust variant on the same page.
3. **[Anatomy of What You Just Built](../getting-started/anatomy.md)**
   — the three package roles and the one configuration file; every
   later addition is another instance of these.
4. Growing: nodes, parameters, more deploy targets — the
   [Multi-Node Projects](../getting-started/workspace-from-app-node.md)
   group; other languages —
   [Rust, C, and Mixed](../getting-started/workspace-languages.md).
5. **[Troubleshooting — First 10 Minutes](../getting-started/troubleshooting-first-10-min.md)**
   if anything goes sideways.
6. Talking to a ROS 2 system — zenoh (router-based) or Cyclone DDS
   (direct, routerless) both interop:
   [Choosing an RMW](../user-guide/rmw-choosing.md).

## 🔬 I'm evaluating capabilities

You're a senior engineer or tech lead assessing nano-ros for
adoption. You want to see scope of coverage, performance bounds,
verification status, and trade-offs before committing.

- **[Architecture Overview](../concepts/architecture.md)** — the
  three-layer model.
- **[Execution Model and Two-Layer API](../concepts/two-layer-api.md)**
  — poll vs callback discipline.
- **[Per-RMW Feature Matrix](../reference/rmw-feature-matrix.md)** —
  generated from the backend sources: services, events, QoS per
  backend. [Backend Reference](../user-guide/rmw-backends.md) for
  architecture and footprint;
  [Support Status](../reference/support-status.md) for versions,
  pins, and CI tiers.
- **[Scheduling Wiring Matrix](../reference/sched-matrix.md)** —
  generated per-platform truth: which classes and kernel capabilities
  are wired where. [Scheduling Models](../internals/scheduling-models.md)
  is the narrative behind it ([Real-Time
  Analysis](../internals/realtime-analysis.md) the lint/tooling
  catalogue).
- **[Static Pool Inventory](../reference/static-pool-inventory.md)** +
  **[Opaque Storage Sizing](../internals/opaque-storage-sizing.md)** —
  memory footprint knobs and their single source of truth.
- **[`no_std`, `alloc`, and `std`](../concepts/no-std.md)** +
  **[Dispatch Strategy](../internals/dispatch-strategy.md)** — the
  execution/allocation constraints.
- **[Formal Verification](../internals/verification.md)** — Kani
  + Verus harness coverage.
- **[Safety Protocol](../internals/safety.md)** — E2E CRC,
  EN 50159 mapping.
- **[Production Readiness Checklist](../internals/production-readiness.md)**
  — concrete adoption gates.
- **[nano-ros vs micro-ROS](../concepts/comparison-vs-microros.md)**
  — head-to-head with the closest peer project.

## 💼 I'm scoping nano-ros for a fleet / product line

You're a PM, CTO, or technical buyer. You want license terms,
supplier reach, deployment patterns, and risk signals before you
write the memo.

- **[Setup Compared to Standard ROS 2](./setup-compared-to-ros2.md)**
  — the elevator pitch + what stays familiar vs what changes.
- **[Differences from Standard ROS 2](../concepts/ros2-comparison.md)**
  — feature deltas in plain prose.
- **[Supported Boards](../reference/supported-boards.md)** — the
  procurement matrix (vendor × board × MCU × RTOS × status).
- **[Choosing an RMW Backend](../user-guide/rmw-backends.md)** —
  decision tree.
- **[Cross-backend Bridges](../user-guide/cross-backend-bridges.md)**
  — multi-RMW fleets.
- **[Safety Protocol](../internals/safety.md)** — E2E CRC
  framework + standards mapping.
- **[Production Readiness Checklist](../internals/production-readiness.md)**
  — what you'd ask your pilot team to validate.
- **[nano-ros vs micro-ROS](../concepts/comparison-vs-microros.md)**
  — license / governance / commercial support comparison.

## Still not sure?

Read the **[Introduction](../introduction.md)** for the one-page
overview. Every section above branches from there.
