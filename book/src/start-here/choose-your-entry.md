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

## 🔌 I have an ESP32 on my desk right now

Already have hardware? Two-step path:

1. **Linux first** — [First Node — Rust](../getting-started/first-node-rust.md)
   on your host to verify the stack in ~10 minutes
   (`just setup` then `cargo run`).
2. **Then ESP32** — once Linux works, follow
   [ESP32 (esp-hal)](../getting-started/esp32.md) for the Rust
   cross-compile path. You need a second machine (or the host
   itself) running `zenohd` on your Wi-Fi network — the board
   needs network reach to the router. For a C-only path use
   [ESP32 (ESP-IDF component)](../getting-started/integration-esp-idf.md)
   if you already have ESP-IDF set up.

## 🚀 I want to get started shipping something

You've decided to use nano-ros and want a working talker on Linux
first, then maybe move to an MCU.

1. **[Install + first build](../getting-started/installation.md)**
   — `just setup` (one-shot fetch + build of every module) then
   `source ./setup.bash`.
2. **First Node** in your language:
   [Rust](../getting-started/first-node-rust.md) ·
   [C](../getting-started/first-node-c.md) ·
   [C++](../getting-started/first-node-cpp.md).
3. **[Troubleshooting — First 10 Minutes](../getting-started/troubleshooting-first-10-min.md)**
   if anything goes sideways.
4. Cross-compile for an RTOS via the
   [Embedded Starters](../getting-started/freertos.md) section.

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
