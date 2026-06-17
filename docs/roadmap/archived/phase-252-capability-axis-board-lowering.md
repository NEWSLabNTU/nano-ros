# Phase 252 — declared capability/feature axes: board-crate lowering + registry

Status: **COMPLETE (Waves 1–4, 2026-06-16; Wave 5 filed as issue 0073)** · Implements
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

- **Wave 1 — capability registry — DONE (2026-06-16).** `cargo-nano-ros`
  `capability_resolver` (beside `rmw_resolver`): a `Capability { declared, nros_feature,
  backend_feature, backends_supporting }` table + `capability(axis)` lookup, re-exported via
  `nros-cli-core` orchestration. The entry lowering (`generated_default_features`) and the
  native backend lowering (`backend_features`) now read it instead of hardcoded strings — no
  behaviour change (the existing `safety_axis_lowers_to_nros_feature`,
  `param_services_axis_lowers_to_nros_feature`, `safety_axis_reaches_zenoh_backend_feature`
  tests stay green = byte-identical output). Registry tests in `capability_resolver`.
- **Wave 2 — descriptor capability advertisement — DONE (2026-06-16).**
  `BoardDescriptor.capability_features: Vec<String>` (`board_descriptor.rs`), parsed from
  `nros-board.toml` (`#[serde(default)]`). A board lists the capability features it forwards
  (e.g. `["safety-e2e"]`); absent ⇒ none.
- **Wave 3 — board-feature threading (generate.rs) — DONE (2026-06-16).** In
  `render_platform_dependencies`, beside `board_rmw_features`, `board_capability_features(plan, &p)`
  builds the list from `plan.safety` via the registry and appends to the board dep's feature
  list (both the RtosOwned and normal branches) — **only** for capabilities the descriptor
  advertises; else skip + `eprintln!` warn ("board X does not declare 'safety-e2e' …"). Tests:
  `board_capability_features_gated_on_advertisement` (advertised → emitted; unadvertised →
  skipped; off → empty/byte-identical).

  **Correction:** the earlier "start with `nros-board-native`" premise was wrong — `native` /
  `posix` is **board-less** (`packages/boards/posix/nros-board.toml`, no `board_crate`), so
  native orchestration already lowers through the direct-backend path (issue-0072 native fix,
  done). **The board-dep path is embedded-only** (stm32 / esp32 / freertos / threadx / nuttx /
  rtic) — none host-buildable here, so per-board edits are validated by the descriptor-resolve
  + the gate unit test, not an embedded build.
- **Wave 4 — per-board `safety-e2e` feature — DONE (2026-06-16).** Every embedded descriptor
  `board_crate` that lowers a backend now forwards `[safety]` to its zenoh backend +
  advertises it:
  - **Direct-zenoh** (`safety-e2e = ["nros-rmw-zenoh?/safety-e2e"]`): `nros-board-stm32f4`,
    `-esp32-qemu`, `-esp32s3`, `-mps2-an385`.
  - **Family crates** (own forwarding, re-exported by overlays): `nros-board-freertos`,
    `nros-board-nuttx` → `["nros-rmw-zenoh?/safety-e2e"]`.
  - **Family-forwarders** (overlay → family, non-optional dep, no `?`):
    `nros-board-mps2-an385-freertos` → `["nros-board-freertos/safety-e2e"]`;
    `nros-board-nuttx-qemu-{arm,riscv}` → `["nros-board-nuttx/safety-e2e"]`.
  - **Descriptor advertisement** (`capability_features = ["safety-e2e"]`): stm32f4, esp32-qemu,
    esp32s3, mps2-an385, mps2-an385-freertos, nuttx-qemu-{arm,riscv}.
  - **`nros-board-native`** also carries the feature (host crate) — `cargo check -p
    nros-board-native --features safety-e2e` **compiles**, host-validating the exact forwarding
    pattern the embedded boards use.
  - **Skipped:** `threadx-{linux,qemu-riscv64}` expose no `rmw-zenoh` board feature (non-standard
    backend wiring) → not advertised, so the gate skips + warns (safe). The non-descriptor
    crates (`rtic-*`, `embassy-*`, `fvp-aemv8r-smp`, `s32z270dc2-r52`) are not orchestration
    `board_crate`s — the axis never reaches them via codegen; a hand-written entry enables the
    feature directly if needed.

  Embedded board crates are cross-toolchain (unbuildable here); validated by descriptor-resolve
  (`stm32f4_advertises_safety_capability_feature`), the gate unit test, all forwarding targets
  shown to exist, `?`-vs-direct matched to each dep's optionality, and the host `nros-board-native`
  check.
- **Wave 5 — C/C++ path — FILED (not built).** Tracked as
  [issue 0073](../issues/0073-safety-e2e-c-cpp-cmake-path-missing.md): a CMake/C
  `#define NROS_SYSTEM_SAFETY_E2E` (the registry's `cmake_token` / `c_define` slots, mirroring
  `-DNANO_ROS_RMW`) + a zpico-C safety gate. Deeper, separate.

**Phase 252 — COMPLETE** (Waves 1–4; Wave 5 filed as issue 0073). The Rust capability-axis
lowering (entry + native backend + board) is done; the embedded board path is no longer the
open remainder of issue 0072.

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
