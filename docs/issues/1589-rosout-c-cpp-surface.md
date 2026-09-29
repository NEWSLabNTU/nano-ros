---
id: 1589
title: "`/rosout` exists in Rust only — the C and C++ surfaces reach none of it,
  and the ledger row that used to carry that work has left the `gap` queue"
status: open
type: gap
area: [api, log]
severity: medium
found: 2026-09-29
related: [0460, 0589, 0710, 1303, 1378]
---

## What is true now

phase-467 Q4's follow-through landed the `/rosout` bridge. A nano-ros node can
publish `rcl_interfaces/msg/Log` on `/rosout`, and a stock
`ros2 topic echo /rosout` was measured seeing it (interop cell
`native-logging-rust-zenoh-n2r`, `packages/testing/nros-tests/tests/rosout_interop.rs`).

It is reachable from **Rust only**:

* `nros_log::rosout` — the queue and the `LogSink`, behind `nros-log/rosout`.
* `nros_node::rosout` — `TOPIC`, `qos()`, `qos_bounded()`, `pump()`, behind
  `nros-node/rosout`.
* `nros::rosout` — the umbrella re-export, behind `nros/rosout`.

## What is missing, and why this file exists rather than a ledger row

Three things:

1. **No C predicate.** rcl's `rcl_logging_rosout_enabled()`
   (`/opt/ros/humble/include/rcl/rcl/logging.h:127`) has no counterpart in
   `packages/api/nros-c/include/nros/log.h`. `nros_log::rosout::enabled()` is
   one forwarder away and answers truthfully, including `false` for an image
   built without the feature — which is exactly upstream's contract.
   The one thing to decide first is the SPELLING, and it is not free: this
   module's convention is `nros_log_*`, which normalises to `log_rosout_enabled`
   and therefore does **not** correlate with upstream's
   `logging_rosout_enabled`. A row that reads `rename` for no reason but a
   prefix is worse than the absence.
2. **No C or C++ way to install the bridge.** The sink is a Rust `LogSink`;
   `nros_log_add_sink` exists on the C side (`c:logging_output_handler_t`) but
   there is no C entry point that installs the `/rosout` one, and no pump.
3. **`rclcpp::NodeOptions::enable_rosout` is still REFUSE-LOUD**
   (`packages/api/nros-cpp/include/nros/options.hpp`), and correctly so — the
   bridge is not automatic even in Rust, so `enable_rosout(true)` still has
   nothing to switch on. Closing item (2) is what would make that refusal worth
   revisiting, not this issue on its own.

The ledger row `c:logging_rosout_enabled` (log.json) carried this work while it
was the whole of the gap. It has moved `gap`+`absent` → `divergence`+`absent`,
because the topic now exists and the row's verdict was asserting that it did
not. **That takes it out of the campaign's `gap` queue, which is the one thing
phase-467 Q4 warned about** — a verdict that leaves the queue is not re-asked.
So the residue is filed here instead of being implied by a row that no longer
says it.

## Also unbuilt, and deliberately

**Automatic wiring.** Upstream republishes from every rcl node with no user
action; here the application creates the publisher, calls `rosout::enable()`
and calls `rosout::pump()` from its spin loop. That is a design decision, not
an oversight — a publisher is an ENTITY and reaches `EntityInventory::derive`,
the zenoh pools and the sizing descriptor, so one the runtime conjures below
the declaration is issue 1341's shape exactly. Automating it means giving
`nros::main!` a declared endpoint, not giving the executor a hidden publisher.
Recorded here so the difference from `enable_rosout = true` is written down
somewhere that is re-read.

## Acceptance

A C or C++ program can turn `/rosout` on and an operator's `ros2 topic echo
/rosout` sees its records — measured against a live peer, the way the Rust half
was, not reasoned about from the headers.
