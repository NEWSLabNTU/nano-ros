# Phase 252 — declared capability/feature axes: board-crate lowering + registry

Status: **Planned (2026-06-16)** · Implements
[RFC-0031 § "Generalization (Phase 250 / issue 0072)"](../design/0031-rmw-selection-and-lowering.md)
· Closes the board-path remainder of [issue 0072](../issues/0072-safety-e2e-backend-feature-not-lowered.md)
· Follows [phase-250](phase-250-safety-params-feature-dimension.md) (the `[safety]` /
`[param_services]` axes).

## Why

A declared capability axis (`[safety]`, `[param_services]`, future) must reach the **backend's
own** build feature, not just `nros/<feat>` on the entry — Cargo features do not propagate
upward, so `nros/safety-e2e` alone leaves `nros-rmw-zenoh/safety-e2e` off and the CRC attach/
validate dead (issue 0072 root cause). Phase-250 + the issue-0072 native fix close two of the
three lowering targets:

| Target | Status |
| --- | --- |
| Entry `nros/<feat>` | DONE (phase-250 Waves 1/3) |
| Direct backend dep (board-less native) | DONE (issue-0072 native) |
| **Board-crate feature (board-backed / embedded)** | **this phase** |

The board is the RMW/platform selection point (RFC-0031 C5b); a capability axis lowers the
same way — to a board-crate feature that forwards to the board's backend. This phase builds
that, behind a **capability registry** SSoT so RMW + the capability axes share one table, and
a **board-descriptor gate** so a board without the feature is skipped+warned, never a Cargo
error (no blind 14-board edit).

## Scope / non-goals

- **In:** the Rust board-crate lowering for `safety` (the live consumer); the
  `resolve_capability` registry; the descriptor capability advertisement; the per-board
  `safety-e2e` feature (incrementally, starting with the host-testable `nros-board-native`).
- **Out:** the C/C++ / CMake path (`safety-e2e` is Rust-only today — no `NROS_SAFETY` define,
  the CRC machinery is feature-gated inside the zpico Rust shim). Tracked as its own issue.
- **Out:** new config surface — the typed `[safety]` / `[param_services]` blocks stay; a
  generic declared-feature list is possible future sugar over the same registry.

## Waves

- **Wave 1 — capability registry (`resolve_capability`).** Add the SSoT mapping a declared
  axis → `{ nros_feature, board_feature, backends_supporting, cmake_token?, c_define? }`,
  parallel to `resolve_rmw`. Refactor the existing native + entry lowering (the ad-hoc
  `if safety && backend == "zenoh"` in `backend_features`, and the `generated_default_features`
  safety/param pushes) to read it — **no behaviour change**, byte-identical generated output.
  Unit tests assert the table + that the refactor preserves output.
- **Wave 2 — descriptor capability advertisement.** Add a `supported_capabilities` (or
  `[board.capability_features]`) field to the board descriptor + parse from `nros-board.toml`.
  A board lists the capability features it forwards (e.g. `["safety-e2e"]`). Absent ⇒ none.
- **Wave 3 — board-feature threading (generate.rs).** In `render_platform_dependencies`,
  beside `board_rmw_features`, build a `capability_feats` list from `plan.safety` (via the
  registry) and append to the board dep's feature list — **only** for capabilities the
  descriptor advertises; else skip + `log::warn!` ("board X doesn't support safety-e2e").
  Unit tests: advertised board → feature emitted; unadvertised → skipped + warned; off →
  byte-identical.
- **Wave 4 — per-board `safety-e2e` feature.** Add `safety-e2e = ["nros-rmw-zenoh?/safety-e2e"]`
  (or the board's own backend forwarding) + the descriptor advertisement, **one board at a
  time**, each reviewed against its own deps. Start `nros-board-native` (host-buildable +
  testable — wire an orchestration `[safety]` native-board build into an e2e). Then the
  embedded boards (freertos, stm32, threadx-family→overlay, nuttx, esp32, rtic, …) as their
  toolchains allow; family crates forward to their overlay, xrce/cyclone-only boards declare
  `safety-e2e = []` (inert).
- **Wave 5 — C/C++ path.** Separate issue: a CMake/C `#define NROS_SYSTEM_SAFETY_E2E` (the
  registry's `cmake_token` / `c_define` slots, mirroring `-DNANO_ROS_RMW`) + a zpico-C safety
  gate. Scoped + filed, not built here.

## Acceptance

- A declared `[safety]` lowering on a board-backed entry enables the backend's `safety-e2e`
  (CRC validates over zenoh), via the board-crate feature — not just `nros/safety-e2e`.
- RMW + capability axes share the `resolve_capability` / `resolve_rmw` registry shape (one
  SSoT, no ad-hoc per-capability checks left in `generate.rs`).
- A board that does not advertise a capability is **skipped + warned**, never a Cargo error.
- `nros-board-native` `[safety]` build validates CRC end-to-end (the host-testable proof).
- Generated output for non-safety plans stays byte-identical (additive, skip-when-absent).

## Risks

- **Embedded boards aren't locally buildable** (cross-toolchains). Mitigated by the
  per-board + descriptor-gated design (each change is local + reviewable against that board's
  deps) and by landing/validating `nros-board-native` first.
- **Heterogeneous backend wiring** across the 14 boards (optional vs direct vs overlay). Each
  board's feature line owns its own forwarding — correct-by-construction, not a global edit.
- **Registry refactor must be behaviour-preserving** — guard with byte-identical-output tests
  before adding the board target.
