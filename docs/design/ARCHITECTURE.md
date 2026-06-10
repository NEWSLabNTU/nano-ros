# nano-ros architecture

The finalized whole-system design. This narrates how the pieces fit; each section links the
RFC(s) that own the detail. When an RFC flips to `Stable`, update the matching section here in
the same commit (the drift rule in [README](README.md)).

Scope: this is the **developer/agent** synthesis. The **user-facing** workflow synthesis lives
in the [book](../../book/src/). Where they overlap (the `nros new → build → deploy` flow), the
book is authoritative for *how to run it* and this doc is authoritative for *why it is shaped
that way*.

---

## 1. Layered crate stack

nano-ros is a `no_std` ROS 2 client. Crates live under `packages/{core,zpico,xrce,dds,boards,
drivers,interfaces,testing,verification,reference,codegen,cli}/`. The stack layers, bottom-up:

1. **Platform layer** — sync/timer/yield ABI per RTOS (`nros-platform-*`), exposed as a stable
   `nros_platform_*` C ABI so transports link against one interface.
2. **RMW layer** — pluggable middleware backends behind one interface.
3. **Node/executor layer** — `nros-node`: `Executor`, `Node`, typed entities, spin model.
4. **Language surfaces** — Rust (mirrors rclrs), C (mirrors rclc), C++ (mirrors rclcpp).

→ RFC-0001 (architecture-overview) is the canonical layer/crate map.

## 2. Three orthogonal axes

Every build is a point in a 3-axis space, compile-time mutually exclusive within each axis,
never cross-implied:

- **RMW**: `rmw-{zenoh,xrce,cyclonedds}` (dust-dds retired).
- **Platform**: `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx}`.
- **ROS edition**: `ros-{humble,iron}`.

`nros` default features are `["std"]` only; the user picks each axis explicitly. **RMW is a
declared, language-agnostic selection** (`system.toml` / deploy override / CLI flag), *lowered* by
the toolchain to a Rust cargo feature or a CMake `-DNANO_ROS_RMW`. Scope is per-deploy-binary
(nodes inherit; in-process multi-RMW only via `[[bridge]]`); the cargo feature is the lowering
target, not the user-facing knob.

→ RFC-0005 (rmw-layer-design), RFC-0006 (portable-rmw-platform-interface), RFC-0031 (RMW
selection & lowering).

## 3. RMW & data plane

The RMW layer is a Rust trait with a parallel C vtable (`nros_rmw_vtable_t`); backends register
explicitly (no constructor/linker-set assumption on Zephyr/native_sim). Slots that a backend
cannot implement fall back in the runtime or return `RET_UNSUPPORTED` — no obligation creep.

- QoS for services/actions, and the gap it closes → RFC-0007, RFC-0008.
- In-binary cross-session topic relay → RFC-0009 (bridge-topic-forwarding).
- Zero-copy loan/commit/borrow/release with arena fallback → RFC-0010.
- PX4 uORB backend → RFC-0011.
- **Single-copy receive** → RFC-0038 (zero-copy-data-transport). The executor's arena
  dispatches subscription callbacks **in-place** from the backend's receive slot via the
  `process_raw_in_place` vtable slot (eliminating the arena staging copy); backends without
  the slot keep the buffered fallback. zenoh-pico routes each subscription to a **size-class**
  receive buffer (small/large by the `rx_buffer_hint` that flows `TopicInfo`→`NrosRmwQos`), so
  receive RAM stops scaling `MAX_SUBS × DEPTH × largest_slot`. Live on zenoh-pico + XRCE.

Backend host-language policy: a backend's host language matches its underlying library's native
language unless overridden (cyclonedds=C++, XRCE=Rust→C, zenoh-pico=Rust).

## 4. Platform, board & toolchain

A board crate composes a transport bridge + driver(s) + platform; platform crates stay free of
networking code. Vendor BSP × board × SDK-variant integration is structured so out-of-tree
boards self-describe their dependencies.

- Vendor BSP integration shape → RFC-0012.
- Out-of-tree board provisioning → RFC-0013.
- `nros setup` as the single toolchain/SDK entrypoint, index-driven from `nros-sdk-index.toml`
  → RFC-0014. (`just <module> setup` recipes are thin callers.)
- Cross-RTOS launch tree + manifest codegen → RFC-0015; per-RTOS scheduling survey → RFC-0016.
- Real-time timer primitive → RFC-0017; the RT executor model → RFC-0002.

## 5. Codegen, workspace & user workflow

Messages are generated from `package.xml` by the in-tree `nros` CLI (`packages/cli/`), never
hand-written. Unmodified ROS 2 message packages build against nano-ros via codegen workspace
discovery → RFC-0023.

The workspace shape (single vs multi-node, Rust/C++/mixed) and its concrete file trees are the
active design front:

- Overall multi-node workspace shape + open questions → RFC-0024 (Draft).
- Concrete per-case file trees → RFC-0025 (Draft).
- Canonical standalone-example layout → RFC-0026.
- User-facing workflow + `nros new` scaffolding → RFC-0027. Nested-sequence message handling
  spike → RFC-0030 (Draft).

Configuration is **language-agnostic and scale-uniform**: `system.toml` (universal system
descriptor, optional for single-node) + per-language manifests (Cargo `[package.metadata.nros.*]`
/ CMake `nano_ros_*`) as native-idiom projections the toolchain lowers; `nros.toml` is narrowed to
the embedded direct-mode runtime file (`config.toml` retired; root `nros.toml` rejected). → RFC-0004
(config) + RFC-0031 (RMW selection).

## 6. Language API surfaces

- C++ surface mirrors rclcpp over typed extern "C" FFI to `nros-node` → RFC-0018.
- nros-c is a thin wrapper that delegates and never re-implements logic → RFC-0019, with the
  compliance audit in RFC-0020.
- Blocking helpers always take an executor handle (no hidden global) → RFC-0021.
- Entity constructors come in two tiers: convenient `fork` + customizable `clone` → RFC-0022.

## 7. Domain & safety

- Safety-critical platform integration analysis → RFC-0028.
- Zonal E/E vehicle architecture and where nano-ros fits → RFC-0029.

---

## Open design fronts (today)

`Draft` RFCs are where the design is still moving: RFC-0003 (rtos-integration-pattern),
RFC-0024 / RFC-0025 (multi-node workspace), RFC-0030 (nested sequences). Everything else is
`Stable` — changes are refinements tracked in each RFC's Changelog.
