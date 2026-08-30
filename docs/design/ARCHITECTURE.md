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

- **RMW**: `rmw-{zenoh,xrce,cyclonedds,uorb}` (uorb is the PX4 C++-only port; dust-dds retired).
- **Platform**: `platform-{posix,zephyr,bare-metal,freertos,nuttx,threadx}` (plus an esp-idf
  platform integration for ESP32 targets).
- **ROS edition**: `ros-{humble,iron,jazzy}` (`jazzy` is the delivered default —
  `just ros_editions ci`; `rolling` planned) — a per-distro **interop profile**
  (type hash, wire encoding/extensibility, interface set), RFC-0056. Unlike
  RMW/platform, edition is a *functional* feature ordinary crates may branch on
  (e.g. `nros-rmw-zenoh::keyexpr` selects the type-hash tail). Edition is a
  PER-RUN global (`NROS_ROS_EDITION`), not a per-cell test-matrix axis (issue
  0327; see `nros-tests::matrix::Cell`). Carve-out: humble/iron ship no
  `rmw_zenoh_cpp` apt package, so the zenoh interop lane runs only on jazzy —
  recorded in `examples/README.md`'s coverage matrix.

`nros` declares an EMPTY `default`; the user picks each axis explicitly, and the standard
library with it (see the `std`/`alloc` contract below). **RMW is a
declared, language-agnostic selection** (`system.toml` / deploy override / CLI flag), *lowered* by
the toolchain to a Rust cargo feature or a CMake `-DNANO_ROS_RMW`. Scope is per-deploy-binary
(nodes inherit; in-process multi-RMW only via `[[bridge]]`); the cargo feature is the lowering
target, not the user-facing knob.

### The `std` / `alloc` contract (phase-361 W1)

Normative. Every crate feature table points here rather than restating it.

**Read the direction first, because it decides how the rules below are applied.**
The terminal state of the core crates is **`core` and `core+alloc`**. `std` is
being DELETED from them (phase-359) rather than managed: it is not a convenience
layer over the platform but a second implementation of one, and
`nros-platform-posix` already provides every primitive it was used for. So
`alloc` is a permanent axis and **`std` is a transitional one with a scheduled
end**. Three consequences that come up constantly:

> **New code writes `core::` and `alloc::`, never `std::`.** A `std::` path in a
> crate that targets embedded is a new site for phase-359 to unwind.
> `check-std-census` is a ratchet that only turns one way and its target is
> zero.
>
> **A capability the CONSUMER chooses requires `std`; it does not grant it.**
> `env` is `ExecutorConfig::from_env`, i.e. `std::env` — it emits a
> `compile_error!` naming the feature to add, and the consumer names `std`.
> `env = ["std"]` is the spelling phase-359 W10 removed: it puts the standard
> library into an image through a door nobody declared. Issue 0687 then moved
> the capability itself: `env` lives on `nros` (`nros::env`, the hosted edge)
> and NOT on the core, which takes resolved values (`ExecutorConfig::
> resolve_with` + `EnvRung`). A capability whose facility exists on one platform
> family belongs at the edge, not behind a per-port ABI stub.
>
> **A purely INTERNAL requirement is the other way round, and the distinction is
> the whole rule.** `metadata-mode` is `["std"]` because `metadata_mode.rs`
> itself uses `std::sync::Mutex`, `Box::leak` and `format!` — nothing about that
> is the consumer's choice, so making them name `std` by hand would turn an
> implementation detail into a consumer-facing flavour. Ask *whose* requirement
> it is: the caller's, or the code's.

**Corollary worth remembering.** A capability that grants what it needs can never
reach its own guard — `metadata-mode` carried a `compile_error!` for two phases
that could not fire, because the manifest edge satisfied the condition it tested.

> **`alloc` is the heap axis, and it is the only spelling of it.** Every
> heap-gated item is `#[cfg(feature = "alloc")]`. `cfg(any(feature = "alloc",
> feature = "std"))` is not an alternative spelling — see below.
>
> **`std` implies `alloc`, in one manifest line per crate** (`std = ["alloc",
> …]`) and nowhere else. A hosted consumer therefore never has to name `alloc`;
> an embedded one never acquires a heap without asking.
>
> **No feature other than `std` may enable `alloc`, and none may enable `std`.**
> A capability, backend or platform feature that needs the heap says so with
> `compile_error!` naming the feature the user must add — it does not turn it on
> for them. Selecting a BACKEND or a PLATFORM must not change whether the image
> has a heap.
>
> **No `no_std`-capable crate declares a non-empty `default` containing `std` or
> `alloc`.**
>
> Orthogonally: **`malloc` is unified per platform; `panic` is the image's.**
> Exactly one `#[global_allocator]` and exactly one `#[panic_handler]` per image
> — chosen by different layers, which phase-366 separated.
>
> The **allocator** is selected by the `platform-<rtos>` feature, which selects
> the provider and nothing else. Not a convention: the Rust heap and the C side
> (`z_malloc`, `ddsrt_malloc`) must reach ONE arena, so the port owns it and an
> image gets no knob that could split it (RFC-0034 D6/D7).
>
> The **panic handler** is the image's, because an ending is a policy and the
> platform knows only the mechanism. The port implements `nros_platform_panic`;
> the image decides whether to reach it, halt, or bring its own — said as
> `nros::main!(panic = "platform" | "halt" | "own")` in a Rust entry, or
> `nano_ros_entry(… PANIC …)` for a C/C++ one, whose whole Rust surface is the
> `nros-c`/`nros-cpp` staticlib. Saying nothing gets `platform`.
>
> Which surface carries the choice depends on **who links the final image**, and
> that is the question, because rustc's notion of a final artifact is not the
> system's: rustc demands the lang item wherever it emits a `staticlib`, even
> when west or CMake will link that archive into an ELF it does not own. Where
> another build system brings its own runtime the ending is already supplied —
> Zephyr's, per `zephyr-lang-rust` — and the image says `own` (RFC-0077).
>
> Libraries never provide one. Node packages in a workspace never provide one:
> a node cannot know which image it lands in.

**Why the implication lives in the manifest and not in `cfg`s.** The one-line
form was briefly replaced by independent axes, with the implication respelled at
the use sites as `cfg(any(feature = "alloc", feature = "std"))`. The semantics
were identical — a `std` build has a heap either way — but it put one fact in 123
places and added 88 `std`-mentioning branches for phase-359 (which DELETES `std`
from these crates) to unwind, 66 of them in `nros-node`, its largest target. A
rule the code must route around is not a contract. With the manifest edge, every
heap gate is already in its final form: dropping `std` needs no `cfg` edit.

**What this is not.** It does not say a `std` consumer should think about
`alloc`; it says the crate author writes the implication once. Nor does it
survive on trust — `check-feature-contract` (W4) asserts each clause, and until
that gate exists each is a measurement rather than an invariant.

**Where a consumer gets `std` from is not uniform, and that hides sweeps.** The
seventeen `examples/native/rust/*` reach `nros` through `nros-board-linux`,
which names `std`; the `nros-tests/bins/*` depend on `nros` directly and must
name it themselves. A sweep that only READS leaf feature lists therefore looks
complete while half the tree is red — when `env` became require-not-grant, six
bins broke and the seventeen examples did not. Build the candidates.

→ issue 0598 (the defect that produced the rule), issue 0594 (the 34 implicit
enables), phase-361, phase-359 (the campaign that removes `std` entirely).

**Agnosticism contract.** The `platform-*` / `rmw-*` axis features are lowering targets that
belong ONLY to (a) **board crates** — the selection point that brings the concrete backend +
platform impl into the link graph (carrying the backend force-link), and (b) the backend/platform
crates themselves. Codegen lowers `system.toml` `[system].rmw` to the **board's** `rmw-X` feature
(RFC-0031). The `nros` umbrella is itself agnostic — it consumes only the `nros-rmw-cffi` /
`nros-platform-cffi` vtable shims (phase-248 C5, decided 2026-06-14). These features must NOT be
declared on, nor `#[cfg]`-branched inside:
- **core packages** (`nros-core`, `nros-node`, `nros-params`, `nros-log`, `nros-serdes`,
  `nros-orchestration`),
- **user-facing libraries** (`nros`, `nros-c`, `nros-cpp` — the umbrella included),
- **user node/component packages**.

Those crates carry only *functional* features (`std`/`alloc`/`no_std`, `param-services`, `lending`,
ROS edition) and reach platform/RMW exclusively through the **vtable seams**: `nros-rmw` +
`nros-rmw-cffi` (RMW) and `nros-platform-api` + `nros-platform-cffi` (platform). Workspace
selection is config-file-driven (`system.toml` `[system].rmw` / `[deploy.<id>]` rmw+board); a user
never edits a `platform-*`/`rmw-*` cargo feature on their node package. (Convergence to this
contract is tracked by issue #60 / phase-248; a `just`-level grep guard over core/user-lib
`Cargo.toml`s can enforce it once converged.)

**Entry macros are framework API, not platform-impl (phase-248 C7, RESOLVED 2026-06-14).** `nros`
carries NO `platform-*` feature — `platform-zephyr` was the last one and is now deleted (C7). One
nuance the contract should make explicit: **entry macros that emit per-target boot code are
framework API and legitimately live in `nros`/`nros-macros`**, NOT platform-impl. `nros::main!`
(nros-macros) already emits the Zephyr `rust_main` entry, gated on the `Framework` enum resolved
from board/deploy metadata (not a feature); the single-node `nros::zephyr_component_main!` macro
(nros/lib.rs) is the same category, gated only on `rmw-cffi` (it needs `Executor`). The platform
*impl* they call — `nros_platform::zephyr::wait_network` — lives in `nros-platform`, and the
concrete platform/RMW still enter via deps, not `nros` features. So the "no platform code in core
src" rule means no platform-IMPL (`#[cfg(platform-*)]` wake/alloc/socket branches), which `nros`
has none of. (Long-term ideal — fold the single-node zephyr entry into `nros::main!` for one
uniform entry macro — is impractical today: `zephyr_component_main!` is a `macro_rules!` that
can't move into the `main!` proc-macro crate, and zephyr examples are lib-only `staticlib` crates
that don't fit `main!`'s bin-based Form-1. Tracked as a future entry-macro unification, not a
blocker.)

→ RFC-0005 (rmw-layer-design), RFC-0006 (portable-rmw-platform-interface), RFC-0031 (RMW
selection & lowering), RFC-0004 (config home), issue #60 / phase-248 (agnosticism convergence).

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
  receive buffer (small/large by the `rx_buffer_hint` that flows `TopicInfo`→`rmw_subscription_options_t`, phase-301), so
  receive RAM stops scaling `MAX_SUBS × DEPTH × largest_slot`. Live on zenoh-pico + XRCE.

- **Callback by default; poll is an opt-in, not an RMW requirement** → RFC-0041
  (Principle). Every callback-capable entity — subscription, timer, service
  server/client, action server/client — is callback-driven: the executor pumps
  the transport once per `spin_once` and dispatches the user callback. The pump is
  **per-session, not per-entity**, so a poll-backend (XRCE `uxr_run_session_time`)
  and a wake-backend (zenoh-pico MT, Cyclone) converge at the same dispatch path —
  poll-vs-wake only changes *when* `drive_io` returns, never the user API. Poll
  (`try_recv_*`, `Promise`, `polling_action_*`) is for **user-owned scheduling**
  (RTIC / Embassy / task-per-entity), not an RMW constraint. To be callback-driven
  an entity must be **arena-registered** (`spin_once` runs its `try_process`); a
  merely-created entity has no pump (the action-client trap → issue-0047).

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
- That index is the SSoT for EVERY dependency class, not just toolchains: `[prereq.<key>]` over
  four providers, setup and doctor both deriving from it so remedies are computed rather than
  hand-written, and a `[tool.*]`'s dist declaring the host libraries it needs — measured from the
  dist, not listed by hand → RFC-0062. rosdep is not consulted anywhere.
- WHERE a tool comes from follows from what it does, not from whether we patched it
  (RFC-0062 amendment 2): a build input is pinned whatever the host has, a tool that
  must match something the host already runs comes from the system — shipping our own
  is the drift RFC-0075 removed for the zenoh router — and the remainder prefers the
  system once a version constraint can say "good enough" (phase-404).
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
