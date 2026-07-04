---
id: 137
title: "Embedded declarative action clients are send-only — no feedback/result seam, client result line unobservable"
status: resolved
type: enhancement
area: codegen
related: [phase-277]
resolved_in: "2026-07-04 — examples switched to create_action_client_with_callbacks_for_name"
---

## Resolution

Not a missing runtime seam — the seam already shipped as
`DeclaredNode::create_action_client_with_callbacks_for_name` (Phase
212.M-F.23): the executor auto-drives accept → feedback stream → result during
spin and dispatches `ExecutableNode::on_callback` with the named result /
feedback callbacks. The zephyr / threadx / threadx-linux action-client
examples already used it; the freertos, nuttx, and baremetal-RTIC ones were
still on the plain `create_action_client_for_name` (send-only) with an empty
`on_callback` seam comment.

Fix: the three lagging examples now use the with-callbacks builder + fill
`on_callback` with the `action_tutorials` wording
(`Next number in sequence received:` / `Result received:`) and log
`Goal accepted by server, waiting for result` after a successful send —
matching the zephyr reference. Baremetal-RTIC keeps its `nros_log`/`nros_info!`
idiom.

Verified: `test_rtos_action_e2e` NuttX/Rust PASS (8.2s) — client now observes
`Goal accepted` + `Result received` (was `accepted=false, completed=false`).
Server side was always fine. NuttX-rust rtos_e2e is now fully green across
pub/sub, service, and action (the #132 log-sink makes the `log::info!` lines
visible on NuttX).
