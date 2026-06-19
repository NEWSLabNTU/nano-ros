# Phase 263 — Complete the workspace examples (feature demos + tests)

Status: **Planned (2026-06-19)** · Informs RFC-0024 (multi-node layout), RFC-0026
(examples), RFC-0027 (ROS 2 user workflow) · Book: `getting-started/workspace-*.md`.

> **Goal.** Turn the four product-shaped workspaces (`examples/workspaces/{rust,c,
> cpp,mixed}`) from pub/sub-only connectivity baselines into (a) polished **starters**
> that demonstrate the everyday ROS feature set in all four languages, and (b) a set of
> separate **advanced** workspaces that each demonstrate one nano-ros differentiator —
> with runtime tests asserting every feature actually works.

## Why (the 2026-06-19 review)

Grounded review of `examples/workspaces/*` + the `multi_pkg_workspace_*` fixtures:

- **All four workspaces demonstrate only pub/sub + timer + multi-node.** Rust is the
  flagship (9 entry pkgs across native/freertos/nuttx/esp32/threadx/zephyr, a
  `multihost.launch.xml`, 5 deploy targets, 2 e2e tests). **C / C++ / mixed are
  native-only, single-entry, single-launch, and have NO runtime test** (fixture-built
  but never asserted).
- **Every advanced feature is absent from every workspace** — services, actions,
  parameters (+ param services), QoS overrides, lifecycle, scheduling tiers, E2E
  safety/CRC, cross-RMW bridges, logging, custom `.msg`, composable nodes, advanced
  launch (conditionals / includes / remaps / params / env). Each exists ONLY in a
  single-node `examples/native/*` app or a stub test fixture. A user wanting to see a
  feature *in a real multi-package system* has no example.
- The `multi_pkg_workspace_*` test fixtures (esp_idf / zephyr / nuttx / platformio /
  px4) are mostly **stubs** (codegen-shape checks; px4 `#[ignore]`d; pre-§212.L shape)
  — a different role (toolchain-integration smoke), not feature demos. Out of scope here.
- Housekeeping: the `build*/` `target*/` dirs are **already gitignored + untracked** —
  no cleanup needed.

## Decisions (locked 2026-06-19)

1. **Feature scope: ALL** — the ROS quartet (services, actions, parameters, lifecycle),
   the nano-ros differentiators (scheduling tiers, E2E safety, cross-RMW bridge, QoS),
   advanced launch, and logging + an in-workspace custom-msg package.
2. **Structure: starters + separate advanced workspaces.** Extend the existing four
   workspaces into **good starters** carrying the everyday ROS feature set, kept
   approachable (a minimal default launch + a `showcase` launch). The **differentiators
   and advanced features go to new, separate, single-purpose workspaces** so the starter
   stays a clean onboarding path.
3. **Language parity: all four.** `rust`, `c`, `cpp`, `mixed` reach the same starter
   feature set + tests (not native-only).

## Coverage target (feature → where it lands)

| Feature | Starter (rust/c/cpp/mixed) | Advanced workspace |
| --- | --- | --- |
| pub/sub, timer | ✓ (have) | — |
| services (server + client) | ✓ add | — |
| actions (server + client) | ✓ add | — |
| parameters (declare/get/set + param services) | ✓ add | — |
| lifecycle (managed node) | ✓ add | — |
| logging | ✓ add | — |
| scheduling tiers (RFC-0015) | — | `ws-realtime-<lang>` |
| E2E safety / CRC | — | `ws-safety-<lang>` |
| cross-RMW bridge | — | `ws-bridge` |
| QoS overrides | — | `ws-qos-<lang>` |
| advanced launch (cond/include/remap/env) | minimal in showcase | `ws-launch` (full) |
| custom `.msg`/`.srv` in-workspace | — | `ws-custom-msg-<lang>` |
| multi-platform entries + multihost | ✓ (rust has; add to c/cpp/mixed) | — |

## Tracks & waves

### Track A — Starter workspaces (extend the existing four)
Per language, add the everyday-ROS feature node-pkgs and a `showcase` launch that
composes them; keep `system.launch.xml` (talker+listener) as the untouched minimal
default. Sequence so each wave is shippable on its own.

- **A1 — services.** `service_server_pkg` + `service_client_pkg` (AddTwoInts via
  `example_interfaces`), wired into `showcase.launch.xml` + `system.toml` components.
  Port from `examples/native/{rust,c,cpp}/service-*`.
- **A2 — parameters.** `param_pkg` (declare/get/set; enable `[param_services]` /
  `features=["param_services"]` so external get/set works). Port from
  `examples/native/cpp/parameters`.
- **A3 — lifecycle.** `lifecycle_pkg` (managed node; `[lifecycle] autostart`). Port
  from `examples/native/rust/lifecycle-node`.
- **A4 — actions.** `action_server_pkg` + `action_client_pkg` (Fibonacci). Port from
  `examples/native/{rust,…}/action-*`.
- **A5 — logging.** Add structured logging to one node per workspace (the `nros-log`
  facade), documented in the README.

### Track B — Advanced workspaces (new, single-purpose, separate dirs)
Each is a minimal product-shaped workspace demonstrating ONE differentiator end-to-end.

- **B1 — `ws-safety-<lang>`:** E2E CRC. `features = ["safety"]` (phase-261 W4/W5 wired
  it for all languages); a talker + a validating listener (`try_recv_validated`).
  Builds on the `examples/native/{c,cpp}/safety-listener` + the phase-261 surface.
- **B2 — `ws-realtime-<lang>`:** scheduling tiers (RFC-0015) — `[tiers.*]` + node
  `callback_groups` + `[[node_overrides]]`, on a multi-tier executor (freertos/threadx).
- **B3 — `ws-bridge`:** cross-RMW gateway (zenoh ↔ xrce/cyclonedds), from
  `examples/bridges/*`, but as a workspace bringup (`[[bridge]]` in system.toml).
- **B4 — `ws-qos-<lang>`:** QoS overrides (reliability / durability / deadline) +
  status events (deadline-missed / liveliness), from the book's documented surface.
- **B5 — `ws-launch`:** advanced launch — conditionals, includes, remaps, params, env
  in the bringup XML; exercises the planner end-to-end.
- **B6 — `ws-custom-msg-<lang>`:** an in-workspace `.msg`/`.srv` interface package
  (`nros generate-rust` / `nros_generate_interfaces`), from `examples/native/rust/custom-msg`.

### Track C — Platform parity (C / C++ / mixed)
Give the starter C/C++/mixed workspaces the embedded entries Rust already has
(freertos / nuttx / zephyr / esp32 / threadx) + a `multihost.launch.xml` + robot1/2
deploy targets. Reuses the Rust workspace's per-platform Entry pattern.

### Track D — Tests (close the C/C++/mixed gap)
A runtime e2e test per workspace + per feature, asserting behaviour (not just a build):
- starter: service call returns, action goal completes, param get/set round-trips,
  lifecycle transitions, log line appears.
- advanced: CRC catches a corrupted frame, tiers schedule by priority, bridge forwards
  across RMWs, QoS deadline-missed fires.
- Each as a build-stage fixture (`examples/fixtures.toml`) + a `nros-tests` consumer
  (no compile-in-test — prebuilt fixture, per AGENTS.md). C/C++/mixed currently have
  zero runtime tests; this is the biggest correctness win.

## Sequencing

A1 → A2 → A3 → A4 → A5 (starter, rust first as the reference, then c/cpp/mixed per
wave), interleaving Track D tests as each feature lands. Track B advanced workspaces
are independent — pick up after the starter quartet (B1 safety first; it's already
wired by phase-261). Track C parity last (most build-infra, least feature-novel).

Each wave: implement for Rust (reference), then project to c/cpp/mixed; add the fixture
+ runtime test; update the book `getting-started/workspace-*.md` + the workspace README
coverage matrix; `just ci`.

## Acceptance

- Each starter workspace boots its `showcase.launch.xml` with services + actions +
  parameters + lifecycle + logging working, in all four languages, with a runtime test
  per feature.
- Each advanced workspace demonstrates its one differentiator end-to-end with a test.
- The README in `examples/workspaces/` carries a feature × language × workspace matrix
  so a user can find "feature X in a real workspace" in one place.
- No feature remains demonstrated ONLY in a single-node example or a stub fixture.

## Risks / notes

- **Scale.** This is large (≈6 starter feature-pkgs × 4 languages + 6 advanced
  workspaces + tests). Waves are sized to ship independently; the starter quartet (A1–A4
  for Rust) is the highest-value first slice.
- **C/C++ feature maturity.** Some features are richer in Rust today (lifecycle,
  actions). Where the C/C++ API lags, the wave surfaces the gap as a tracked issue
  rather than faking the demo (tests must fail-loud on an unimplemented path).
- **Don't bloat the starter.** The minimal `system.launch.xml` stays talker+listener;
  features live behind `showcase.launch.xml` so the onboarding path is still small.
- **Reuse, don't duplicate.** Port node logic from the existing `examples/native/*`
  single-node apps into reusable node-pkgs; the workspace is the composition, not new
  node code.
