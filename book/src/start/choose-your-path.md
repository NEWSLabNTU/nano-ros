# Choose Your Entry

<!-- type: how-to · level: 1 -->

This book is organized around what you are trying to do with nano-ros, not
around who you are. Pick the purpose closest to yours. Each section lists the
pages to read, in order.

## Everyone: your first project

Every purpose starts the same way:

1. **[Install](install.md)** the `nros` CLI.
2. **[First Project](../tutorial/run-a-talker.md)** scaffolds a talker
   for a Cortex-M3 board, runs it in QEMU and shows its messages in ROS 2:

   ```bash
   nros setup mps2-an385-baremetal --rmw zenoh
   nros new sensor_ws --board mps2-an385-baremetal --template talker
   cd sensor_ws
   nros run sensor_node
   ```

   No QEMU or ROS 2? The same chapter runs it on your Linux host.
3. **[Anatomy of What You Just Built](../tutorial/what-you-just-built.md)**
   explains the workspace in nano-ros's own terms. You do not need to open
   the bringup package before you have seen it run.

New to ROS entirely? Two ideas carry the whole book. A **node** is a unit
that computes, and nodes talk through named **topics** (publish/subscribe)
and request/reply **services**. An **RMW** ("ROS MiddleWare") backend is the
transport that moves the bytes. nano-ros is a small implementation of that
model for microcontrollers.

If anything goes wrong in the first steps, see
[Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

## Industrial: reproducible, auditable, offline

You build products where safety and robustness matter. You need the same
inputs to give the same image, every fact on a page, and an install that runs
with no network.

- **What is supported, and how it is checked.**
  [Supported Boards](../reference/supported-boards.md) and
  [Board support levels](../reference/board-support-levels.md) are generated
  from the board registry. `nros doctor --report` prints the status of every
  board your bringup's images name, as a report you can file.
- **Pinning and offline builds.**
  [Pinning a version](../get-started/installation-details.md#pinning-a-version)
  (`nros pin`), and
  [Integrating into a Vendored Tree](../build/vendored-tree.md)
  for a company tree with air-gapped CI. `nros build --offline` fails,
  naming what is missing, rather than fetching it.
- **Your RTOS and your build tool.**
  [How Integration Works](../targets/how-integration-works.md):
  your RTOS keeps its own build tool (west, make, CMake, your IDE) and
  nano-ros plugs into it. [Build as a CMake
  subdirectory](../build/cmake-subdirectory.md) covers a tree
  with no `system.toml`.
- **Real-time and memory bounds.**
  [Execution Model and Two-Layer API](../concepts/two-layer-api.md),
  [Scheduling Models](../realtime/scheduling-models.md) and the generated
  [Scheduling Wiring Matrix](../reference/sched-matrix.md),
  [`no_std`, `alloc`, and `std`](../concepts/no-std.md),
  [Dispatch Strategy](../realtime/dispatch-strategy.md), and the
  [Static Pool Inventory](../reference/static-pool-inventory.md).
- **Safety and adoption gates.**
  [Safety Protocol](../realtime/safety-protocol.md) (E2E CRC, EN 50159
  mapping) and the
  [Production Readiness Checklist](../realtime/production-readiness.md).
- **Your own board or BSP.**
  [Custom Board Package](../extending/custom-board.md),
  [Vendor Overlay](../extending/vendor-overlay.md) and the
  [STM32F4 out-of-tree worked example](../extending/stm32f4-out-of-tree.md).
  A board of yours lives in your tree (`NROS_EXTRA_BOARD_PATH`). A new
  platform (a new RTOS) still means changing nano-ros itself.
- **Comparisons.** [Differences from Standard ROS
  2](../from-ros2/differences.md),
  [nano-ros vs micro-ROS](../from-ros2/vs-micro-ros.md),
  [Choosing an RMW Backend](../tutorial/pick-the-rmw.md) and the
  generated [Per-RMW Feature Matrix](../reference/rmw-feature-matrix.md).

## Field solutions: a working system per customer

You build a system for each customer, often on a board you were handed, and
you need it publishing data that a ROS 2 system can see, quickly.

1. **Your first project** (above), on the host.
2. **Your board.** Find it in
   [Supported Boards](../reference/supported-boards.md); its evidence page
   says where it runs and which chapter covers it. Then read your platform's
   chapter (FreeRTOS, Zephyr, NuttX, ThreadX or bare-metal). Every embedded
   chapter has a QEMU flow that works without hardware.
3. **Run it on the target.** [The target model](../targets/target-model.md)
   and [`nros run`](../targets/run.md) for a host process or an emulator.
4. **Talk to ROS 2.** [Choosing an RMW](../tutorial/pick-the-rmw.md)
   and [ROS 2 Interoperability](../tutorial/talk-to-ros2.md). Cyclone DDS
   talks to `rmw_cyclonedds_cpp` directly, with no router. Zenoh needs the
   ROS 2 router (`rmw_zenoh_cpp`) on the host.
5. **Grow the system.** More nodes, parameters and images:
   [Growing your workspace](../tutorial/grow-your-workspace.md),
   and [Mixed-language workspaces](../build/mixed-language.md) for
   other languages. Bridging several networks:
   [Cross-backend Bridges](../communicate/bridges.md).
6. **Coming from rclcpp or rclpy?**
   [Setup Compared to Standard ROS 2](../from-ros2/setup-compared.md),
   [Porting a ROS 2 C++ node](../from-ros2/porting-a-cpp-node.md),
   [C / C++ multi-node workspaces](../build/cpp-workspaces.md) and
   the [Migration Guide](../from-ros2/migration-guide.md).

## Education: labs that fit a session

You teach a course or run a lab. Every seat needs the same result, the
install must be short, and each lab must fit one session.

1. **Install once per seat**:
   [Installation](install.md). Nothing in the first
   project needs ROS 2, a router or hardware.
2. **The first lab** is the first project (above), in C++ or, on the same
   page, in Rust. The output it prints is the result to check on every seat.
3. **Concepts for the lecture.**
   [Anatomy](../tutorial/what-you-just-built.md),
   [Architecture Overview](../concepts/architecture.md) and
   [Execution Model and Two-Layer API](../concepts/two-layer-api.md).
4. **Embedded labs without hardware.** Each platform chapter runs its
   example under QEMU, for example
   [FreeRTOS](../targets/freertos.md) or
   [bare-metal](../targets/bare-metal.md).
5. **Labs that talk to ROS 2.**
   [Choosing an RMW](../tutorial/pick-the-rmw.md); Cyclone DDS needs no
   router, which keeps a lab machine simple.

## Still not sure?

Read the **[Introduction](welcome.md)** for the one-page overview.

## Design background

- [RFC-0116: adoption purposes and front doors](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0116-adoption-purposes-and-front-doors.md) — the three purposes this page routes by
