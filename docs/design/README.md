# nano-ros design RFCs

This directory is the **design source of truth**. Each file is a numbered, living RFC:
a design decision record that is edited in place as the shape settles. Phase docs in
`docs/roadmap/` are *work breakdowns* — they implement RFCs, they do not own design
rationale. **New design rationale goes in an RFC, never only in a phase doc.**

- **Finalized whole-system view:** [ARCHITECTURE.md](ARCHITECTURE.md) — narrates the system
  end-to-end and links into the RFCs below.
- **New RFC:** copy [0000-template.md](0000-template.md) to `NNNN-slug.md`, next free number.
- **Status field** tells you stable vs evolving: `Draft` (moving) · `Stable` (settled) ·
  `Superseded` (retired by a hard reversal; see `archived/`).
- **Drift rule:** flipping an RFC to `Stable` requires updating the matching
  [ARCHITECTURE.md](ARCHITECTURE.md) section in the same commit.

Each RFC carries frontmatter: `rfc`, `title`, `status`, `since`, `last-reviewed`,
`implements-tracked-by` (phase slugs), `supersedes`, `superseded-by`.

## Index

### Foundations & architecture

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0001 | [architecture-overview](0001-architecture-overview.md) | Stable | layered crate stack + RMW + executor + board composition |
| 0002 | [rt-execution-model](0002-rt-execution-model.md) | Draft | real-time executor model (RTOS subsections pending) |
| 0003 | [rtos-integration-pattern](0003-rtos-integration-pattern.md) | Draft | universal embedded RTOS adapter pattern |
| 0004 | [configuration-and-transports](0004-configuration-and-transports.md) | Stable | unified config across single-node/workspace; `system.toml` + metadata; transports |

### RMW & data plane

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0005 | [rmw-layer-design](0005-rmw-layer-design.md) | Stable | middleware abstraction (delink zenoh-only) |
| 0006 | [portable-rmw-platform-interface](0006-portable-rmw-platform-interface.md) | Stable | Rust trait + C vtable dual API |
| 0007 | [service-qos](0007-service-qos.md) | Stable | service/action QoS design |
| 0008 | [service-qos-gap](0008-service-qos-gap.md) | Superseded | gap analysis → resolved by RFC-0007 (Phase 189.M3.3) |
| 0009 | [bridge-topic-forwarding](0009-bridge-topic-forwarding.md) | Stable | in-binary `[[bridge]]` raw-CDR topic relay |
| 0010 | [zero-copy-raw-api](0010-zero-copy-raw-api.md) | Stable | zero-copy raw publish/subscribe API |
| 0011 | [px4-rmw-uorb](0011-px4-rmw-uorb.md) | Stable | PX4 uORB RMW backend |
| 0031 | [rmw-selection-and-lowering](0031-rmw-selection-and-lowering.md) | Stable | RMW declared (system.toml/flag) + lowered per language; per-deploy |
| 0035 | [rmw-vtable-abi](0035-rmw-vtable-abi.md) | Draft | frozen 34-slot RMW vtable ABI + append-only rule + NULL contract + `abi_version` |
| 0038 | [zero-copy-data-transport](0038-zero-copy-data-transport.md) | Stable | single-copy receive: size-class buffers + in-place dispatch; removes copy #1 + arena buffer (issue #8 / Phase 231) |
| 0039 | [px4-integration-architecture](0039-px4-integration-architecture.md) | Draft | PX4 umbrella: in-firmware uORB vs companion XRCE; 1.16 message versioning; px4-rs revision opportunities (RFC-0011 = uORB detail) |

### Platform, board & toolchain

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0012 | [board-bsp-integration-architecture](0012-board-bsp-integration-architecture.md) | Stable | vendor BSP × board × SDK-variant integration shape |
| 0013 | [custom-board-provisioning](0013-custom-board-provisioning.md) | Stable | out-of-tree boards self-describe deps to `nros setup` |
| 0014 | [nros-setup-toolchain-management](0014-nros-setup-toolchain-management.md) | Stable | `nros setup` as single toolchain entrypoint |
| 0015 | [rtos-orchestration](0015-rtos-orchestration.md) | Stable | launch tree + manifest codegen across RTOSes |
| 0016 | [rtos-scheduling-features](0016-rtos-scheduling-features.md) | Stable | per-RTOS scheduling feature survey |
| 0017 | [platform-timer](0017-platform-timer.md) | Stable | `PlatformTimer` + `AtomicSporadicState` |
| 0034 | [platform-layer-split](0034-platform-layer-split.md) | Draft | enforce `nros_platform_*` as sole system boundary; alloc-first unification; scalar vs opaque-struct ABI |
| 0042 | [platform-build-determinism](0042-platform-build-determinism.md) | Draft | one canonical `<nros/platform.h>`; capability-driven config SSoT in `nros-board.toml`; generated deterministic link manifest (one register path, no `--allow-multiple-definition`/`-u`); merge-time platform×lang gate |

### Language APIs

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0018 | [cpp-api-design](0018-cpp-api-design.md) | Stable | C++ surface mirroring rclcpp over typed FFI |
| 0019 | [nros-c-thin-wrapper-discipline](0019-nros-c-thin-wrapper-discipline.md) | Stable | nros-c delegates, never re-impls |
| 0020 | [thin-wrapper-audit](0020-thin-wrapper-audit.md) | Stable | nros-c / nros-cpp thin-wrapper compliance audit |
| 0021 | [blocking-api-rules](0021-blocking-api-rules.md) | Stable | every blocking helper takes an executor handle |
| 0022 | [entity-api-tiers](0022-entity-api-tiers.md) | Stable | convenient `fork` + customizable `clone` entity ctors |
| 0036 | [ros2-api-divergences](0036-ros2-api-divergences.md) | Draft | authoritative catalog of nano-ros vs rclrs/rclcpp/rclc divergences + rationale |
| 0037 | [rust-c-user-api-surface](0037-rust-c-user-api-surface.md) | Draft | records the Rust (`nros-node`) + C (`nros-c`) user surfaces; C++ is 0018 |

### Codegen, workspace & user workflow

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0023 | [codegen-workspace-discovery](0023-codegen-workspace-discovery.md) | Stable | unmodified ROS 2 msg pkg builds against nano-ros |
| 0024 | [multi-node-workspace-layout](0024-multi-node-workspace-layout.md) | Draft | overall multi-node workspace shape + open questions |
| 0025 | [workspace-layout-by-case](0025-workspace-layout-by-case.md) | Draft | concrete file trees per case (single/multi × rust/cpp) |
| 0026 | [example-directory-layout](0026-example-directory-layout.md) | Stable | canonical `examples/<plat>/<lang>/<example>/` shape |
| 0027 | [ros2-user-workflow](0027-ros2-user-workflow.md) | Stable | user-facing workflow + `nros new` scaffolding |
| 0030 | [sequence-of-nested](0030-sequence-of-nested.md) | Draft | nested-sequence message handling (Phase 212 spike) |
| 0032 | [entry-codegen-pipeline](0032-entry-codegen-pipeline.md) | Draft | how `main()` is emitted: proc-macro + CLI mirror, BoardEntry run/run_tiers, tier resolver |
| 0033 | [message-field-capacity-config](0033-message-field-capacity-config.md) | Draft | per-field, language-agnostic message capacity via `nros-codegen.toml`; owned/heap/borrowed modes |
| 0040 | [distribution-and-scaffolding-deps](0040-distribution-and-scaffolding-deps.md) | Draft | no-crates.io source-release model; `nros new` dep convention: in-tree + out-of-tree both use the ws-sync patch-block (git deps opt-in); CLI install line |
| 0041 | [unified-callback-receive-model](0041-unified-callback-receive-model.md) | Draft | service/action clients move from poll/`Promise`-single-buffer to callback-at-spin + QoS-depth `BufferStrategy` (triple/ring), unifying with subscriptions; dual-mode (Promise kept); ROS-aligned (KEEP_LAST 10, rclcpp callbacks); RT-safe + poll-backend (XRCE) compatible |

### Domain & safety

| RFC | Doc | Status | One-liner |
| --- | --- | --- | --- |
| 0028 | [e2e-safety-protocol-integration](0028-e2e-safety-protocol-integration.md) | Stable | safety-critical platform integration analysis |
| 0029 | [zonal-vehicle-architecture](0029-zonal-vehicle-architecture.md) | Stable | zonal E/E architecture + nano-ros fit |

## Superseded

Retired RFCs live in [archived/](archived/) with `status: Superseded` and a forward pointer.

## How a decision moves: RFC → roadmap → code

1. **Draft RFC** captures the option space + open questions.
2. Discussion (PR/issue/phase doc) resolves the open questions.
3. A **phase doc** in `docs/roadmap/` carries the work items + acceptance tests and names the
   RFC in its `Implements:` header.
4. Implementation lands; the RFC flips to **Stable** and ARCHITECTURE.md is updated in the
   same commit.
