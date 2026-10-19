---
id: 1982
title: "Every executor carves one extra session per node slot, on an image that links one RMW"
status: resolved
type: enhancement
area: [memory, zephyr]
severity: low
found: 2026-10-19
resolved: 2026-10-19
resolved_in: "branch fix/1982-extra-sessions-from-backend-count"
related: [1971, 1970, 0436]
---

## What happens

`Executor::extra_sessions` (and `extra_session_ids`) hold the sessions a node
opens on an RMW backend other than the primary's (`NodeBuilder::rmw`,
`Executor::open_multi`, issue 0436). Every executor backing carved them one per
node slot (`nodes`), whatever the image links: on thumbv7em with zenoh a slot
is the backend's `ConcreteSession`, 528 B, plus a 168 B `ExtraSessionId`.

An image that registers one backend never pushes one: a node naming the
primary's backend binds slot 0 (`resolve_session_slot`), zenoh-pico's session
is a process singleton, and a backend the image does not register cannot be
opened. Measured on the Autoware safety island (RFC-0107, nano-ros d5fe58a518):
2,112 B of `extra_sessions` plus 672 B of ids carved and never used on the
one-tier image, 2,640 B plus 840 B on the two-tier split.

## Direction

Size the tables from the backends the image registers: the sizing
descriptor's `[image] backend_count`, the same fact `nros-rmw-cffi` sizes its
backend registry from (and trusts: a short registry is a registration
failure). An image that registers B backends needs at most B - 1 extra
sessions per executor, and never more than its node slots.

A second session on the SAME backend (another locator or domain, on a backend
that allows it) is not a backend count; it needs a stated number.

## Resolution

- `nros-node/build/extra_sessions.rs` (`extra_session_cap`, included by
  `build.rs` and by `tests/extra_sessions.rs`): `backend_count - 1` when the
  descriptor states it, no cap below the node slots when it does not (the
  old size, byte for byte).
- `config::MAX_EXTRA_SESSIONS`, and `storage::extra_session_slots(sizing)` =
  `min(nodes, MAX_EXTRA_SESSIONS)` sizes both tables per executor (and so per
  tier); `nros-executor-layout::Counts` gains `extra_sessions`.
- Knob `NROS_EXECUTOR_EXTRA_SESSIONS` (`-1` derives) states a count, for a
  second session on one backend. A full table stays
  `NodeError::NodeTableFull`.

A one-RMW image (the island) carves no extra session: 528 B + 168 B per node
slot per executor saved.
