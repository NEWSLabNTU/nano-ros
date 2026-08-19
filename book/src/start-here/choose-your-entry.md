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

## 🚀 I want to get started shipping something

You've decided to use nano-ros and want a working system on Linux
first, then move it to your target.

1. **[Install](../getting-started/installation.md)** — two commands:
   `./scripts/bootstrap.sh`, then `nros setup native --rmw cyclonedds`.
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
6. Talking to a ROS 2 system needs the zenoh backend —
   [Choosing an RMW](../user-guide/rmw-choosing.md).

## 🔬 I'm evaluating capabilities

You're a senior engineer or tech lead assessing nano-ros for
adoption. You want to see scope of coverage, performance bounds,
verification status, and trade-offs before committing.

- **[Architecture Overview](../concepts/architecture.md)** — the
  three-layer model.
- **[Execution Model and Two-Layer API](../concepts/two-layer-api.md)**
  — poll vs callback discipline.
- **[Choosing an RMW Backend](../user-guide/rmw-backends.md)** —
  capability matrix per backend, including QoS coverage and
  multi-backend bridges.
- **[Real-Time Analysis](../internals/realtime-analysis.md)** +
  **[Scheduling Models](../internals/scheduling-models.md)** —
  RT scheduling story.
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
