# Phase 122 — Unify C / C++ / Rust API code paths

**Goal:** Eliminate code-path divergence between Rust user-facing API
and the path C/C++ wrappers invoke internally. Reduce test-surface
mismatch.
**Status:** Investigation pass complete. Implementation TBD.
**Priority:** Medium.
**Depends on:** Phase 120 (root cause for the action-server divergence
identified — see `phase-120-baseline-failures.md`).

## Motivation

Phase 120.3 revealed that `Node::create_action_server` (manual-poll)
has a deterministic post-handshake crash on threadx-rv64, while
`Executor::add_action_server` (callback + arena) — the path the
nros-c thin wrapper invokes — does NOT. Switching the Rust example to
the callback model fixed the test.

Naive expectation: "C is a thin wrapper, so identical to Rust." Truth:
C wrapper delegates to a DIFFERENT Rust API entry point than the Rust
example uses. **Same wire-level behavior, different code paths,
different bug surface.**

Unifying the paths means C/C++ tests exercise the same Rust code the
Rust example does. A regression in either path then trips both
language tests — easier to catch.

## Code-path divergences (current state)

| Example | Rust example uses | C wrapper internal path |
|---|---|---|
| **talker** (publisher) | `node.create_publisher` (`session.create_publisher` → returns `EmbeddedPublisher` value) | `session.create_publisher` directly in `nros-c/src/publisher.rs:244` |
| **listener** (subscription) | `node.create_subscription` (manual-poll, returns `Subscription`) | `executor.add_subscription_raw_with_qos_sized` (callback + arena) |
| **service-server** | `executor.add_service` (callback) ✓ | `executor.add_service_raw_sized` (callback) ✓ **already aligned** |
| **service-client** | `node.create_client` (manual-poll, returns `EmbeddedServiceClient`) | `executor.add_service_client_raw_sized` (callback + arena) |
| **action-server** | `executor.add_action_server` (callback) ✓ **fixed in Phase 120.3** | `executor.add_action_server_raw_sized` (callback) ✓ **aligned** |
| **action-client** | `node.create_action_client` (manual-poll, returns `ActionClient`) | `executor.add_action_client_raw` (callback + arena) |
| **timer** | `executor.add_timer` (callback) ✓ | `rust_exec.add_timer` ✓ **aligned** |
| **guard condition** | `executor.add_guard_condition` (callback) ✓ | `rust_exec.add_guard_condition` ✓ **aligned** |

**Diverging entities (manual-poll Rust vs callback C):**

1. **publisher** — Rust uses `node.create_publisher` returning value;
   C uses `session.create_publisher` directly. Different storage
   (Rust returns by value, C stores in `Publisher` struct).
2. **subscription / listener** — Rust manual-poll vs C callback +
   arena.
3. **service-client** — Rust manual-poll vs C callback + arena.
4. **action-client** — Rust manual-poll vs C callback + arena.

**Asymmetry in Rust examples themselves:**

Across `examples/{qemu-riscv64-threadx,qemu-arm-freertos,qemu-arm-nuttx,
threadx-linux,zephyr}/rust/zenoh/`, examples DON'T agree internally:

- Zephyr `listener` uses `executor.add_subscription` (callback).
- All other listener examples use `node.create_subscription` (manual-poll).
- Zephyr `talker` uses `executor.add_timer` (callback-driven).
- All other talker examples use `node.create_publisher` directly.
- Service-server examples uniformly use `executor.add_service` (callback).
- Action-server post-Phase-120.3-fix on rv64 uses
  `executor.add_action_server` (callback); other platforms still on
  `node.create_action_server` (manual-poll).

## Proposal

**Standardize on callback + arena model in ALL Rust examples,
matching what nros-c invokes internally.**

Rationale:

1. **Matches C/C++ wrapper path** — same Rust code under test for both
   language ABIs.
2. **More robust on resource-constrained targets** — the rv64 crash
   shows the manual-poll path can hit issues the callback path doesn't.
   Whether or not we ever fully fix the manual-poll path, callback
   model is the safer default.
3. **Better fits ROS 2 ergonomics** — rclrs 0.7.0 and rclcpp use
   callback dispatch. Manual-poll is a niche API for users who
   explicitly want it.

Keep `Node::create_*` (manual-poll) APIs in the library — they're
useful and other crates may depend on them — but examples + tests
default to the callback model.

## Work items (2026-05-13 design pass)

After the initial investigation, the design pass added new
constraints that grew the phase scope:

- **Two-layer API rule.** L1 (primitive, caller polls) + L2
  (callback, executor-managed) with verb discipline:
  `Node::create_*` (L1) + `Executor::register_*` (L2).
- **Cross-language consistency.** The same two-layer surface in
  Rust, C, and C++.
- **Thin-wrapper discipline.** nros-c / nros-cpp delegate to Rust;
  no duplicated bookkeeping. Detailed in
  `docs/design/nros-c-thin-wrapper-discipline.md`.

Sub-items:

- [x] **122.1 — Rust `add_*` -> `register_*` rename.** Landed
  2026-05-13 (commit `c3c56cc7`). 123 call sites across 37 files.
- [x] **122.2 — C/C++ `nros_executor_add_*` -> `register_*` rename.**
  Landed 2026-05-13 (commit `68e9eef5`). 53 files.
- [~] **122.3.a — nros-c thin-wrapper audit + discipline doc.**
  In flight 2026-05-13. Output: `docs/design/nros-c-thin-wrapper-discipline.md`.
  Audit result: 4 of 11 entities follow opaque-thin pattern today;
  7 are field-mirror (subscription / service / client / action_server
  / action_client / timer / guard_condition partial).
- [ ] **122.3.b — subscription thin-wrapper refactor + L1 entry
  points.** Template for the remaining four executor-registered
  entities. Adds `nros_subscription_init_polling` (L1 init)
  + `nros_subscription_try_recv_raw` (L1 op). Refactors L2 init
  + register to match the same opaque shape.
- [ ] **122.3.c — service / service_client / action_server /
  action_client thin-wrapper refactor + L1 entry points.** Per
  the pattern in 122.3.b. Per-entity L1 ops listed in the
  discipline doc.
- [ ] **122.3.d — nros-cpp wrapper sync.** Mirror the refactored
  C struct shape into the C++ headers. Add L1 constructor +
  `try_recv` method per entity.
- [ ] **122.4 — Rust example migration L1 -> L2 (callback).**
  Mechanical rewrite of non-RTIC example main.rs files. Migration
  list below. RTIC examples stay on L1.
- [ ] **122.5 — docs.** Update book pages, porting guide, and the
  RT positioning page with the unified two-layer story.

## Migration list

Convert these Rust example files to `executor.add_*`:

- `examples/{qemu-riscv64-threadx,qemu-arm-freertos,qemu-arm-nuttx,threadx-linux}/rust/zenoh/listener/src/main.rs` → `executor.add_subscription`
- `examples/{*}/rust/zenoh/talker/src/main.rs` — keep publisher but drive via `executor.add_timer` for periodic publish (like Zephyr already does)
- `examples/{*}/rust/zenoh/service-client/src/main.rs` → `executor.add_service_client` (verify API exists; may need same callback-model shape as action-client)
- `examples/{*}/rust/zenoh/action-client/src/main.rs` → `executor.add_action_client`
- Same set for `examples/{*}/rust/xrce/...` and `examples/{*}/rust/dds/...`

## Verification

After migration, every threadx-rv64 / threadx-linux / freertos / nuttx
/ zephyr Rust example exercises the same internal Rust path as the
matching C example. A regression in either path fails both test
families. Phase-120-style "Rust-only" bugs become much harder to
introduce silently.

## Open question — the manual-poll bug itself

This phase **does not fix** the underlying `Node::create_action_server`
crash on rv64. It only routes around it. The crash signature
(deterministic JALR to `0x80251630` = `nx_bsd_socket_pool_memory + 8`)
is documented in `phase-120-baseline-failures.md`. Anyone who needs
the manual-poll API on rv64 will still hit it.

Tracking as a separate open item (not part of this phase): find the
actual STORE that writes `0x80251630` into a function-pointer field
during the manual-poll spin-loop pattern. Watchpoint or
print-on-callback-registration around `zpico_declare_queryable` /
`zpico_declare_subscriber` is the natural next step.
