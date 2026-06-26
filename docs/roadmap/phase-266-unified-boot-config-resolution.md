# Phase 266 — Unified boot-config resolution across languages and platforms

Implements **[RFC-0045](../design/0045-unified-boot-config-resolution.md)**. Closes
**[#101](../issues/0101-board-boot-config-not-unified.md)** and the all-boards remainder of
**[#98](../issues/0098-nros-main-ignores-component-node-name.md)**.

## Status (2026-06-27)

**W1–W4 (the Rust unification): LANDED on main** (`…a314b02eb`). Node naming now resolves
through one path on **all 10 boards** (hosted, OwnedSpin embedded, NuttX + new
`run_with_deploy`, RTIC, Embassy); the three languages still funnel into the one shared
`RmwConfig` → `CffiRmw::open` sink. Verified by `just check` (green pre- and post-rebase) +
a whole-branch review (fixed: NuttX `from_env`-arm dropped `boot_config`; RTIC-mps2
`option_env` fallback; edition-2024 `unsafe(link_section)` on bare-metal). This closes the
Rust scope of **#98** (was honored on 2/10 boards, now all) and the Rust core of **#101**.

Build-verification gaps (pre-existing, environmental, not code defects): NuttX-riscv /
mps2-an385-freertos / threadx-qemu-riscv64 full builds need `just nuttx setup` / FreeRTOS
sources; the ARM/host siblings build clean and the per-board edit is uniform.

**Remaining: W5 (C), W6 (C++), W7 (cleanup)** — the C/C++ session-naming source decision
(`nros_support_init_named` / `nros_cpp_init` / codegen-entry) is a parallel-stack effort,
deliberately deferred from the Rust land.

## Why

`node_name` + `locator` + `domain_id` are assembled in three places (Rust/C/C++) from four
sources (runtime env / baked `Config` / `option_env!` / nothing on NuttX). Result: the
launch-declared node name appears on only 2 of ~10 boards; `[deploy.nuttx*]` is inert; the C
session uses a PID name; C++ defaults `"nros_cpp"`. The *sink* is already unified
(`RmwConfig → CffiRmw::open`); only the *source assembly* is fragmented. This phase introduces
ONE resolver with precedence model A feeding that existing sink, plus ONE embedded bake site.

## Design decisions (locked, see RFC-0045)

- **Precedence A**, per field: `env (hosted only, if set) > baked overlay > compiled default`.
- **Resolver in `nros-node`** on `ExecutorConfig`, taking a plain-field `BootConfig` (no
  `DeployOverlay` type → no `nros-node`↔`nros-platform` cycle). Three call-sites map their
  source into `BootConfig`.
- **One embedded bake site**: a `repr(C)` `.nros_boot_config` blob (a baked const this phase;
  patchable static is the follow-on). The macro (Rust) and cmake (C/C++) populate it.
- **Out of scope** (RFC-0045 non-goals): the config-patch tool, the build-time plan image,
  storage-backed override (#80), and multi-node per-node identity (the multi-node half of #98).

## Waves

### W1 — resolver core (`BootConfig` + `ExecutorConfig::resolve`)
**Files:** `packages/core/nros-node/src/executor/types.rs` (add `BootConfig`, `resolve`);
tests in `packages/core/nros-node/src/executor/types.rs` (`#[cfg(test)]`) or
`packages/testing/nros-tests/tests/boot_config_resolution.rs`.

- Add `pub struct BootConfig<'a> { node_name, locator: Option<&'a str>, domain_id: Option<u32>,
  namespace: Option<&'a str> }`.
- Add `ExecutorConfig::resolve(baked: BootConfig<'a>, hosted_env: bool) -> ExecutorConfig<'a>`,
  implemented in terms of the existing `new`/`from_env`/`node_name`/`domain_id` builders:
  per field take env (only if `hosted_env` and the var is set) else baked else the existing
  compiled default.
- `from_env` stays; `resolve(.., hosted_env=true)` with an all-`None` baked must reproduce
  today's `from_env` result (regression guard).

**Acceptance:** unit tests cover the precedence truth table per field (env-set vs unset ×
baked-set vs unset × hosted true/false); `resolve(BootConfig::default(), true)` ≡ `from_env()`;
`resolve(.., false)` ignores env. `cargo test -p nros-node` green.

### W2 — hosted boards adopt the resolver (no behavior change)
**Files:** `packages/boards/nros-board-posix/src/lib.rs` (the `run_with_deploy` at ~:199),
`packages/boards/nros-board-native/src/lib.rs`.

- Replace the inline `from_env()` + `if let Some(name) = deploy.node_name` block with:
  map `DeployOverlay {node_name,locator,domain_id,namespace}` → `BootConfig` →
  `ExecutorConfig::resolve(bc, hosted_env = true)`.

**Acceptance:** existing native tests stay green (incl. the #98 `ws-params-rust` interop test
asserting `/param_talker`, and any test that sets `NROS_LOCATOR` per run). No diff in observable
behavior; this wave is a refactor to the shared path.

### W3 — single embedded bake site (`.nros_boot_config`)
**Files:** define `BakedBootConfig` (`repr(C)`) in `packages/core/nros-node/src/executor/types.rs`
(co-located with the resolver that reads it); emit it from `packages/core/nros-macros/src/main_macro.rs`
(bake from the parsed launch/overlay — extends the existing `deploy_overlay_tokens` site); a
helper `BootConfig::from_baked(&BakedBootConfig) -> BootConfig`.

- `BakedBootConfig { magic: u32, version: u16, set_flags: u16, domain_id: u32,
  node_name: [u8;64], locator: [u8;96], namespace: [u8;64] }`; `#[no_mangle]
  #[link_section=".nros_boot_config"] #[used] static NROS_BOOT_CONFIG`.
- `BootConfig::from_baked` reads `set_flags` → `Option` fields (NUL-trim the byte arrays).
- Macro emits the static for `target_os = "none"` / embedded board paths, populated from the
  same single-node launch identity it already bakes for `DeployOverlay.node_name` (Phase 264/265).

**Acceptance:** an embedded fixture's binary contains a `.nros_boot_config` section with the
baked name/locator/domain (assert via `nm`/`objdump` in a build-step check or a host unit test
of `from_baked` round-trip). `from_baked(baked_for("robot1", "tcp/…", 7)) == BootConfig{Some…}`.

### W4 — embedded Rust boards adopt the resolver (the all-boards #98 fix)
**Files:** `packages/boards/nros-board-stm32f4/src/entry.rs:115`,
`packages/boards/nros-board-mps2-an385/src/entry.rs:138`,
`packages/boards/nros-board-esp32-qemu/src/board_entry.rs:177`,
`packages/boards/nros-board-threadx/src/entry.rs`,
`packages/boards/nros-board-freertos/src/entry.rs`,
`packages/boards/nros-board-nuttx-qemu-arm/src/entry_212n.rs`,
`packages/boards/nros-board-nuttx-qemu-riscv/src/entry_212n.rs`,
`packages/boards/nros-board-rtic-stm32f4/src/lib.rs:476`,
`packages/boards/nros-board-embassy-stm32f4/src/lib.rs` (or equivalent).

- Each board replaces hardcoded `.node_name("nros_app")` / `option_env!` with
  `ExecutorConfig::resolve(BootConfig::from_baked(&NROS_BOOT_CONFIG), hosted_env = false)`.
- **NuttX (arm + riscv): add the missing `run_with_deploy` override** so the overlay reaches
  the boot path at all (today it inherits the dropping default).
- RTIC/Embassy: thread the baked config into `init_hardware_with_deploy` (RTIC already takes
  `&DeployOverlay`; add the Embassy parity) so node name is launch-driven, not `option_env!`.

**Acceptance:** for a representative embedded board per family with a QEMU/host-sim e2e
(threadx-linux is the cheapest), `ros2 node list` (or the in-binary `.nros_boot_config` →
session name) shows the launch-declared name, not `/nros_app`. The NuttX deploy overlay
(locator/domain) is no longer inert (assert the baked values reach the session). Closes the
all-boards remainder of #98.

### W5/W6 design — confirmed bug + chosen mechanism (Option A: blob reuse, 2026-06-27)

**The bug is real and #98-shaped — verified empirically.** A native C++ workspace entry
(`examples/workspaces/cpp`, launch nodes `talker`+`listener`) shows **`/nros_cpp`** in
`ros2 node list`, NOT `/talker`/`/listener`. Cause: the entry's `create_node("talker")` calls
reuse the primary session (same primary-session-reuse as the Rust #98 root), so the graph shows
the **session/init default** name. C++ defaults that to `"nros_cpp"` (`node.hpp:602`); C defaults
to `nros_{pid}` / `"nros"` (`nros-c/src/support.rs:233-253`). The launch name reaches
`create_node` but never the session, so it never reaches the graph.

**Chosen mechanism: Option A — reuse the `.nros_boot_config` blob across languages.** RFC-0045
made `BakedBootConfig` `repr(C)` + magic-tagged precisely so C/C++ can share it. C/C++ read the
struct's `node_name` field **directly** (plain C struct — verify `magic`/`version`, then the
NUL-terminated `node_name[64]`); no Rust FFI needed for the read. One bake site, all three
languages; the future config-patch tool then covers C/C++ too.

**Shared prerequisite (W5):** add a C header mirror of the struct —
`packages/core/nros-c/include/nano_ros/boot_config.h` — declaring the `repr(C)` layout
(`magic`/`version`/`set_flags`/`domain_id`/`node_name[64]`/`locator[96]`/`namespace[64]`), the
`NRBC` magic + `BOOT_SET_*` bits, and a small inline reader
`const char* nros_boot_config_node_name(const struct nros_baked_boot_config*)` returning the
name or `NULL`. Layout MUST match `nros-platform-api/src/boot_config.rs` exactly (a build-time
or test assertion on `sizeof`/offsets guards drift).

### W5 — C entry emits + reads the `.nros_boot_config` blob; fix `nros_<pid>` session name
**Files:** `packages/core/nros-c/include/nano_ros/boot_config.h` (new, the mirror above);
`packages/cli/nros-cli-core/src/codegen/entry/emit_c.rs` (the C entry emitter);
`packages/core/nros-c/src/support.rs` (the null-`session_name` fallback).

- `emit_c.rs`: for a single-node entry, emit a `static const struct nros_baked_boot_config
  NROS_BOOT_CONFIG __attribute__((section(".nros_boot_config"), used)) = { … };` populated from
  the launch node name (the codegen already has it as `n.name`), and have the generated entry
  pass `nros_boot_config_node_name(&NROS_BOOT_CONFIG)` as the `session_name` to
  `nros_support_init_named(...)` (instead of the `NULL` that triggers the PID default).
- `support.rs`: when `session_name` is `NULL`, fall back to the unified default `"node"` (not
  `nros_{pid}`), for consistency with the Rust resolver's compiled default.

**Acceptance:** a native C workspace entry shows its configured node name in `ros2 node list`
(extend a C interop test to assert it), not `/nros_<pid>`. `boot_config.h` layout matches the
Rust struct (asserted).

### W6 — C++ entry emits + reads the blob; fix `"nros_cpp"` default
**Files:** `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` (the C++ entry emitter);
`packages/core/nros-cpp/include/nros/main.hpp` (board adapters `run_components`);
`packages/core/nros-cpp/include/nros/node.hpp` (`init` default).

- `emit_cpp.rs`: emit the same `NROS_BOOT_CONFIG` static (reuse the C header `boot_config.h`).
- `main.hpp` board adapters (`NativeBoard`/`ZephyrBoard`/… `run_components`): read
  `nros_boot_config_node_name(&NROS_BOOT_CONFIG)` and pass it to
  `nros::init(locator, domain, name)` instead of calling the 2-arg `init()` that defaults
  `"nros_cpp"`. When the blob has no name (multi-node / unset), keep the `"node"` default.

**Acceptance:** the native C++ workspace entry that today shows `/nros_cpp` shows its configured
node name (`/talker` for a single-node launch). Multi-node still collapses to one session node
(the deferred per-node-session piece, same as Rust). Build-verify native C + C++ + mixed entries.

### W7 — cleanup: collapse duplicate board-key maps; resolve `setup_transport`
**Files:** `packages/core/nros-macros/src/main_macro.rs` (`board_path_for`),
`packages/cli/nros-cli-core/src/codegen/entry/emit_rust.rs` (`board_path_for`);
`packages/core/nros-platform/src/board/entry.rs` (`setup_transport`).

- Merge the two `board_path_for` maps into one shared table (the `emit_rust` one is missing
  `freertos` → falls back to `NativeBoard`; unify so all keys resolve identically).
- Decide `setup_transport`: keep + document it as the custom-transport seam (mps2-an385 XRCE is
  the one user), or remove the trait method + the macro's unconditional call. Record the call.

**Acceptance:** one board-key table; `freertos` resolves to the FreeRTOS board on both paths.
`setup_transport` either documented or removed (no dead unconditional emit). `just check` green.

## Sequencing

W1 (resolver) → W2 (hosted, proves the path with zero behavior change) → W3 (bake site) →
W4 (embedded boards, needs W1+W3 — the bulk of the #98 fix) → W5 (C) → W6 (C++) → W7 (cleanup).
W5/W6 are independent of W4 and may interleave once W1 lands. Each wave ships independently and
keeps `just ci` green.

## Acceptance (phase)

- `ros2 node list` shows the launch/`system.toml`-declared node name on a representative board
  per language **and** per platform family (native Rust/C/C++ + at least one embedded family,
  threadx-linux preferred for cheap e2e), not `/node`/`/nros_app`/PID/`nros_cpp`.
- One resolver (`ExecutorConfig::resolve`) with a tested precedence table is the only place
  boot-config precedence is decided; no board hardcodes `node_name`.
- NuttX `[deploy.*]` (locator/domain/name) is honored (no longer inert).
- One `.nros_boot_config` bake site exists, populated by both the Rust macro and cmake, read by
  the resolver on embedded.
- One `board_path_for` table. `setup_transport` documented-or-removed.
- #98 closed (all boards); #101 resolved.

## Risks / decisions

- **Behavior change on embedded is intended** (name now appears; NuttX overlay now applies) —
  call it out in the changelog; existing embedded e2e tests that asserted `/nros_app` (if any)
  must be updated to the configured name.
- **`.nros_boot_config` survival**: the section must be `KEEP`-ed in each board's linker script
  and the static `#[used]`, or linker GC drops it. Verify per board in W3/W4.
- **Fixed-size fields** bound name(64)/locator(96)/namespace(64); longer values are a build-time
  error in the macro/cmake emit (don't silently truncate).
- **C/C++ env on hosted**: confirm whether hosted C/C++ should honor `NROS_LOCATOR` etc. like
  Rust (set `hosted_env=true` there) — RFC-0045 model A says yes on hosted; verify no test
  depends on the old fixed behavior.
