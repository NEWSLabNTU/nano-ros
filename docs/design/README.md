# nano-ros design documents

`docs/design/` holds LIVE design documents that iterate alongside roadmap phase docs in `docs/roadmap/`. Treat phase docs as work breakdowns; treat design docs as source of truth on shape decisions.

## Index

### LIVE (iterating)

- [multi-node-workspace-layout.md](multi-node-workspace-layout.md) — overall workspace shape + open questions for Phase 212
- [workspace-layout-by-case.md](workspace-layout-by-case.md) — concrete file trees per case (single/multi × rust/cpp + mixed)
- [rtos-integration-pattern.md](rtos-integration-pattern.md) — universal embedded RTOS adapter pattern

### Companion roadmap phase

- [phase-212-ux-cargo-native-and-file-consolidation.md](../roadmap/phase-212-ux-cargo-native-and-file-consolidation.md) — work breakdown

### Stable architecture (not Phase 212 specific)

| File | One-liner |
| --- | --- |
| [architecture-overview.md](architecture-overview.md) | layered crate stack + RMW + executor + board composition |
| [blocking-api-rules.md](blocking-api-rules.md) | every blocking helper takes executor handle |
| [board-bsp-integration-architecture.md](board-bsp-integration-architecture.md) | vendor BSP × board × SDK-variant integration shape |
| [bridge-topic-forwarding.md](bridge-topic-forwarding.md) | in-binary `[[bridge]]` raw-CDR topic relay across sessions |
| [codegen-workspace-discovery.md](codegen-workspace-discovery.md) | unmodified ROS 2 msg pkg builds against nano-ros |
| [configuration-and-transports.md](configuration-and-transports.md) | `nros.toml` manifest + transports + node binding |
| [cpp-api-design.md](cpp-api-design.md) | C++ surface mirroring rclcpp over typed FFI |
| [custom-board-provisioning.md](custom-board-provisioning.md) | out-of-tree boards self-describe deps to `nros setup` |
| [e2e-safety-protocol-integration.md](e2e-safety-protocol-integration.md) | safety-critical platform integration analysis |
| [entity-api-tiers.md](entity-api-tiers.md) | convenient `fork` + customizable `clone` entity ctors |
| [example-directory-layout.md](example-directory-layout.md) | canonical `examples/<plat>/<lang>/<example>/` shape |
| [nros-c-thin-wrapper-discipline.md](nros-c-thin-wrapper-discipline.md) | nros-c delegates, never re-impls |
| [nros-setup-toolchain-management.md](nros-setup-toolchain-management.md) | `nros setup` as single toolchain entrypoint |
| [phase-110-e-platform-timer.md](phase-110-e-platform-timer.md) | `PlatformTimer` + `AtomicSporadicState` |
| [portable-rmw-platform-interface.md](portable-rmw-platform-interface.md) | Rust trait + C vtable dual API review |
| [px4-rmw-uorb.md](px4-rmw-uorb.md) | PX4 uORB RMW backend |
| [rmw-layer-design.md](rmw-layer-design.md) | middleware abstraction (delink zenoh-only) |
| [ros2-user-workflow.md](ros2-user-workflow.md) | user-facing workflow + `nros new` scaffolding |
| [rt-execution-model.md](rt-execution-model.md) | RT executor model live doc |
| [rtos-orchestration.md](rtos-orchestration.md) | launch tree + manifest codegen across RTOSes |
| [rtos-scheduling-features.md](rtos-scheduling-features.md) | per-RTOS scheduling feature survey |
| [service-qos-gap.md](service-qos-gap.md) | gap: `create_service_*` no-QoS path |
| [service-qos.md](service-qos.md) | service/action QoS design closing the gap |
| [thin-wrapper-audit.md](thin-wrapper-audit.md) | Phase 83 nros-c / nros-cpp compliance audit |
| [zero-copy-raw-api.md](zero-copy-raw-api.md) | zero-copy raw publish/subscribe API |
| [zonal-vehicle-architecture.md](zonal-vehicle-architecture.md) | zonal E/E architecture + nano-ros fit |

## How decisions move from design → roadmap

LIVE design doc captures option space + open question → discussion in PR/issue or Phase doc → LOCKED in phase doc work item + acceptance test → implementation lands and updates the design doc to mark settled.

## Open questions today (Phase 212)

From [multi-node-workspace-layout.md §8](multi-node-workspace-layout.md#8-open-questions):

1. [Orchestration pkg `Cargo.toml`?](multi-node-workspace-layout.md#8-open-questions) — Path A no-toml vs Path B stub-toml; blocked on `cargo nros plan <dir>` walk-outside-members spike.
2. [Multi-system shared config](multi-node-workspace-layout.md#8-open-questions) — duplicate vs `include =` vs workspace-root `[defaults]`; wait for real pain.
3. [`nros launch` vs `ros2 launch`](multi-node-workspace-layout.md#8-open-questions) — host-side launcher independent of ament, or shell to `ros2 launch`?
4. [C++ workspaces — `cmake nros` subcommand?](multi-node-workspace-layout.md#8-open-questions) — no cmake plugin idiom; C++ invokes `nros plan`/`deploy` directly; confirm asymmetry.
5. [`system.toml` location](multi-node-workspace-layout.md#8-open-questions) — orchestration pkg vs workspace root; leaning bringup pkg.
6. [`[system].components` schema](multi-node-workspace-layout.md#8-open-questions) — flat list vs `{name, role, qos_overrides}` tables; leaning simple list.
7. [Mixed-language workspace bootstrap](multi-node-workspace-layout.md#8-open-questions) — first-time `cargo build` against C++-containing workspace; leaning document.
8. [Codegen interface package shape](multi-node-workspace-layout.md#8-open-questions) — where `my_interfaces/` `.msg`-only pkg sits in multi-pkg workspace.
9. [Embedded MCU + multi-pkg workspace](multi-node-workspace-layout.md#8-open-questions) — one west app composing N components vs per-component apps.
