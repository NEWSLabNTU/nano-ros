# nano-ros Issues

This directory tracks nano-ros bugs, limitations, and tech-debt — one file
per issue, mirroring the repo's numbered-RFC convention
(`docs/design/NNNN-*.md`) and the roadmap `archived/` pattern. Each file
carries YAML frontmatter plus the issue body (problem, evidence, current
state, fix/direction). Open issues live directly in `docs/issues/`; resolved
ones move to `docs/issues/archived/`.

## Conventions

**Frontmatter schema** (every issue file):

```yaml
---
id: 7                    # the issue number (matches the 4-digit filename id)
title: Unbounded message sequences capped at 64 elements
status: open             # open | resolved | wontfix
type: enhancement        # bug | enhancement | tech-debt
area: codegen            # codegen | rmw | memory | cmake | zephyr | nuttx | freertos | threadx | build | testing
related: []              # e.g. [rfc-0023, phase-218] — cross-links to RFCs / phases
resolved_in:             # (resolved only) commit short-hash or phase, e.g. "Phase 140"
---
```

**Lifecycle**:

1. Open an issue as `docs/issues/NNNN-slug.md` with `status: open`.
2. When resolved, set `status: resolved` + `resolved_in:` and **move** the
   file to `docs/issues/archived/NNNN-slug.md` (trimmed to a terse
   resolution summary).
3. **Numbering** = the next integer after the highest existing id.
   **Slug** = a kebab-case form of the title; the filename id is the
   zero-padded 4-digit issue number.

## Issue vs RFC vs phase doc

- **Issue** (`docs/issues/`) = a bug, limitation, or tech-debt item.
- **RFC** (`docs/design/NNNN-*.md`) = a design decision.
- **Roadmap phase** (`docs/roadmap/`) = an implementation plan.

Issues cross-link to the RFCs and phases that inform or resolve them via the
`related:` frontmatter field.

## Open issues

- **#110** — [No per-entry way to size the executor callback table
  (`NROS_EXECUTOR_MAX_CBS`) to a declared topology](0110-executor-max-cbs-per-entry-sizing-knob.md):
  `MAX_CBS`/`ARENA_SIZE` is a build-time const baked into `nros-node`; workspace-global cargo
  `[env]` is the only lever to raise it, so raising it for a fat native entry also bloats lean
  RAM-bound embedded entries in the same workspace. Wants a topology-derived const-generic
  `Executor` or a per-entry build knob. Split from #95 (diagnostic half resolved).
- **#96** — [In-process (same-executor) node-to-node delivery does not happen — pub/sub
  AND service](0096-in-process-same-executor-service-roundtrip-broken.md): two nodes on
  one `Executor`/session do not talk — a same-process subscriber/queryable never receives
  the same-process publisher/client (zenoh does not loop a session's own publications back
  to itself). External processes receive normally. The phase-263 A1 service and B1 safety
  demos are therefore cross-process (separate entries).
- **#102** — [~60 examples ship untested; advanced capabilities
  native-only](0102-example-fixture-coverage-holes.md): zephyr (22), freertos/nuttx C/C++ (24)
  single-node examples exist + are claimed in the RFC-0026 matrix but have zero fixtures; native
  C/C++ variants + Rust async untested; lifecycle/params/safety/QoS/tiers/multihost exercised on
  native only. Add fixtures or honestly de-scope the matrix. (C/C++ embedded *workspace-entry*
  e2e is landing under phase-263 C2x — narrows the workspace axis, not the single-node holes.)
- **#103** — [cross-language capability surface
  uneven](0103-cross-language-capability-surface-gaps.md): core entity APIs (pub/sub/service/
  action/QoS/bridge) are present in Rust+C+C++, but multi-type parameters are string-only in
  C/C++, C++ has no lifecycle wrapper (must call C), and RT tiers are Rust-only (C none, C++
  affinity-only). Param/lifecycle services are declarative in Rust but manual in C/C++. Parity
  enhancement; sequence after #98/#101/#102.
- **#105** — [multi-node entry collapses to one graph
  node](0105-multi-node-per-node-graph-naming.md): N components on one `Executor` share the primary
  session, so `create_node` calls reuse NodeId 0 (`node_record.rs:228`) and `ros2 node list` shows
  one node, not one per component (same for Rust + C/C++). The deferred multi-node half of #98/#101;
  needs a per-node session or per-node liveliness token (decide with per-node param scoping).
Resolved issues live in [`archived/`](archived/). Recently resolved: **#115** —
[non-deterministic rustc ICE / SIGSEGV under heavy fixture-build
load](archived/0115-rustc-nondeterministic-ice-sigsegv-under-fixture-load.md): rustc crashed
intermittently mid-`build-test-fixtures` (a different crate each run — `paste`, `toml`,
`nros-macros`, …); ruled out OOM (94 GiB free), sccache (absent), and the parallel front-end;
not reproducible in isolation (24 concurrent fresh builds → 0 crashes), so it's an
environmental rustc-1.96.0 crash under the host's mixed load. Fixed with a `RUSTC_WRAPPER`
retry shim (`scripts/build/rustc-retry.sh`) that re-runs only on crash signatures (never on
real compile errors) — re-runs always advance, so a bounded retry recovers. The same host
flakiness can also crash the C/C++ linker (`ld`) at Zephyr's final link, which has no
cargo-level hook — documented residual. Also: **#113** —
[config-driven bridge endpoints not
env-overridable](archived/0113-bridge-config-endpoints-not-env-overridable.md):
`run_from_config` baked each `[[node]]`'s locator + domain with no runtime override.
Fixed (phase-267): `apply_node_env_overrides` applies `NROS_BRIDGE_<NODE>_LOCATOR` /
`NROS_BRIDGE_<NODE>_DOMAIN` over the baked config, so a deployed bridge re-points
without a rebuild and the gated test uses an ephemeral router + `unique_ros_domain_id()`.
Verified forwarding on non-baked endpoints (:7600 / domain 9). Also: **#114** —
[native C/C++ cmake fixtures race the per-build config-header
mirror](archived/0114-cpp-cyclone-fixture-build-sizes-undefined.md): the
native/posix C/C++ fixtures compiled before Corrosion's `nros_{c,cpp}_config_header`
mirror ran, reading the in-tree `#error` stub (`*_OPAQUE_U64S` undefined → cascade
`Subscription has no member storage_`) — the same 0088/0090 race on the path those
fixes excluded. Fixed (phase-267) by wiring the hard `OBJECT_DEPENDS` edge for posix
in `NanoRosEntry.cmake` (entry sources) + `NanoRosGenerateInterfaces.cmake` (the
`<pkg>__nano_ros_c` message lib); `native-cmake-rmw` now builds all four cells clean.
Also: **#112** —
[`nros-cpp` `component_node.hpp` included `<string>` unconditionally → broke Zephyr minimal
libcpp](archived/0112-zephyr-cpp-component-node-requires-string-minimal-libcpp.md): `<string>`
was gated on `__STDC_HOSTED__` (true for host `g++` even under `-nostdinc++` minimal libcpp),
but its only consumer — the `std::string`-keyed parameter overloads — is gated on `NROS_CPP_STD`.
Moved the include onto its actual consumer's gate; `<cstdio>` stays hosted. Verified: all six
Zephyr C++ XRCE entries now build to `zephyr.exe`. Surfaced after #111 unblocked the zephyr leg.
Also: **#111** —
[`nros-sizes-build` filesystem fallback searched the wrong profile
dir](archived/0111-sizes-probe-filesystem-fallback-custom-profile-path.md): the fallback built
rlib search paths from `PROFILE` (only ever `debug`/`release`), so for the custom
`nros-fast-release` profile it looked in `release/deps` while the rlib was in
`nros-fast-release/deps` → `EXECUTOR_SIZE` probe timed out → `nros-cpp` failed. Fixed with a
`profile_dir_name()` helper deriving the real profile dir from `OUT_DIR` (the component before
`build`). Verified end-to-end: the affected dev box's zephyr Rust + C fixtures now build; the
remaining zephyr C++ `<string>` failures split to #112. Also: **#95** —
[executor `MAX_CBS` overflow → opaque
`NodeRegister`](archived/0095-executor-max-cbs-overflow-opaque-noderegister.md): a topology
declaring more callbacks than `NROS_EXECUTOR_MAX_CBS` (default 4) failed as an opaque
`NodeRegister("<pkg>")` with the underlying capacity error discarded at every collapse seam.
Fixed the diagnostic half (gap A): a distinct `NodeError::ExecutorFull` threads source
(`next_entry_slot`) → `NodeDeclError::ExecutorFull` → install `-2` → the `nros::node!` register
wrapper → `RuntimeError::ExecutorFull(<pkg>)`, whose `Display` names the actionable
`NROS_EXECUTOR_MAX_CBS` knob (arena overflow keeps `BufferTooSmall`; modes now distinguishable,
regression-locked in `executor/tests.rs`). Per-entry sizing ergonomics (gap B) split to #110.
Also: **#99** —
[declarative `[[bridge]]` does not
forward](archived/0099-declarative-bridge-planner-population.md): the cross-RMW bridge
orchestration is complete + verified end-to-end — the planner emits `build.transports` +
`plan.bridges`; `nros sync` resolves topic→type via synthetic node metadata
(`[[package.metadata.nros.node.publishes]]`) → `nros-bridge.toml`; plain `nros::main!` emits
`run_from_config_str` + the backend `register()` (#106); `cargo build` links. Done in phase-267
(W0/C1–C5) + `14b7a4cc3` (synthetic type `pkg/msg/Name` namespace fix); full runtime forwarding
verified (phase-267 W-B, #107). Also: **#106** —
[RMW backend self-register ctor
dead-stripped](archived/0106-backend-self-register-ctor-dead-stripped.md): a bridge Entry
referenced no backend symbol, so the linker dead-stripped the `nros-rmw-*` crates' `.init_array`
self-register ctors → `open_multi` null vtable → `Transport(InvalidArgument)`. Fixed (`0d205c1f7`):
`nros::main!` reads the bridge's RMWs from `system.toml` and emits `nros_rmw_<x>::register()` in
the generated `main` (no per-Entry `extern crate` boilerplate). Verified via macro expansion + 4
unit tests; full runtime `open_multi` chains on #107. Also: **#107** —
[Cyclone descriptor not staged in a schema-free
bridge](archived/0107-cyclone-baked-descriptor-not-auto-staged.md): `run_from_config`'s Cyclone
egress failed `PublisherCreationFailed` (no descriptor, and `std_msgs/Int32` is NOT baked);
resolved at phase-267 W-B (fix B) — `nros sync` carries the flat field schema in `nros-bridge.toml`
and the runtime stages the descriptor via `register_type_descriptor` (self-consistent offsets,
no user build.rs). Also **#109** — [config bridge extra session ignores
`domain_id`](archived/0109-config-bridge-extra-session-ignores-domain.md): `create_node_on`
dropped the configured domain so every extra RMW participant opened on domain 0; fixed with
`create_node_on_with_domain`. Also: **#108** —
[FreeRTOS MPS2-AN385 linker omits
`.nros_boot_config`](archived/0108-freertos-linker-missing-nros-boot-config-section.md): the
phase-266 baked `.nros_boot_config` section (`8088e77c0`) overlapped `.data` because the FreeRTOS
board's hand linker `mps2_an385.ld` never placed it → `build-test-fixtures` failed linking
`qemu_freertos_entry`. Fixed (`5a6407bd2`) by adding a `.nros_boot_config > FLASH` section before
`.data` (mirroring the script's `.eh_frame_hdr` fix); `just freertos::build-examples` now builds
the entry green. **#98** + **#101** —
boot-config unification ([archived/0098](archived/0098-nros-main-ignores-component-node-name.md),
[archived/0101](archived/0101-board-boot-config-not-unified.md)): node_name/locator/domain resolved
four ways across boards → one `ExecutorConfig::resolve` path + a single `.nros_boot_config` bake
site read by Rust, C, and C++; node naming now works on all 10 boards + 3 languages (verified
`/param_talker`, `/talker`). Fixed in phase-266 (`a314b02eb` Rust, `b2c3e63f1` C/C++); residuals
split to #105. **#97** — [`nros codegen entry` embedded
runners](archived/0097-codegen-entry-cpp-native-only-no-embedded-runners.md): C/C++ LAUNCH entry
was native-only; resolved by phase-263 C2a embedded runners. **#104** —
[C entries invisible in `ros2 node list`](archived/0104-c-nodes-no-graph-liveliness.md): the ROS 2
node liveliness token was never declared on any path (`declare_node_liveliness` had zero callers),
so nodes appeared only via entity-liveliness inference — and C entries were invisible entirely.
Fixed (`194babcf1`) by threading `node_name`/`namespace`/`domain_id` `RmwConfig`→`TransportConfig`→
session and declaring + holding the node token in `ZenohSession::new`; a native C entry went from
empty `ros2 node list` to `/node` (verified). Residuals split to #105 (per-node tokens). Also: **#100** —
[baremetal standalone examples split into a sibling node
pkg](archived/0100-baremetal-standalone-examples-split-into-sibling-node-pkg.md): the
`qemu-arm-baremetal`/`stm32f4` rust examples were an Entry binary path-dep'ing + `[patch]`ing
up into a sibling `*_pkg`, breaking copy-out. Collapsed all 25 packages (23 user examples + 2
e2e fixtures) into single self-contained crates over W1–W7 (declarative, RTIC `node_pkgs`
self-reference, Embassy, shared-pkg duplication, cross-pkg placeholder inlining), and merged
the now-redundant two-pass baremetal build loop. Also: **#94** —
[`nros ws sync` line-based TOML editor](archived/0094-ws-sync-toml-line-scanner-fragility.md):
the `[patch.crates-io]` rewriter was a line scanner, not a TOML parser (duplicate table on
the quoted `[patch."crates-io"]` form; dropped patches for explicit `[dependencies.<name>]`).
Resolved at [phase-265](../roadmap/phase-265-ws-sync-config-patch-toml-edit.md) W4 — `nros sync`
writes `[patch.crates-io]` to `.cargo/config.toml` via a `toml_edit` DOM, never editing a
consumer `Cargo.toml`, so the entire A–F class is structurally impossible. Also: **#72** —
safety-e2e CRC dead over zenoh (`nros/safety-e2e` didn't reach the backend's
`safety-e2e`): fixed via the RFC-0031 capability-axis generalization (Phase 252) —
`[safety]` lowers to the entry umbrella, the board-less native backend dep, AND the
board crate's `safety-e2e` forwarding feature (gated on the board's `nros-board.toml`
`capability_features`). This pass added the forwarding feature to the last 3 zenoh
boards lacking it (embassy-stm32f4, rtic-mps2-an385, rtic-stm32f4) so the family is
uniform; 7/7 capability tests + native/declarative `crc=ok` e2e green. Residual:
optional embedded runtime e2e. See `archived/0072-*`. **#75** —
`qos_overrides` best_effort test failed on CI only (looked like a subscriber hang):
actually a test-harness output-consume race — `wait_for_output_pattern` returns its
whole read buffer on match, so the first of two sequential waits ate the later
`Waiting for` line when the listener's logs coalesced into one `read()` (deterministic
on CI's buffering, split locally). Fixed by one wait for `Waiting for` + asserting the
earlier `qos effective` line in the same buffer. host-integration 11→4→1→0. See
`archived/0075-*`. **#71** —
native cpp/mixed workspace Entry link failed on CI only: `libnros_cpp.a` + the
per-package FFI staticlib are two Rust staticlibs each bundling `std` →
duplicate `rust_begin_unwind`. Root cause = `host-integration-tests.yml`'s
`CARGO_PROFILE_RELEASE_LTO=off` overriding the FFI crate's `lto=true` (the
`panic=abort` crate relies on fat LTO to DCE-strip the redundant unwinding std;
`off`/`thin` retain it). Fixed by dropping the override on the workspace-fixtures
step (rust-core keeps it — binaries, no dup); CI-confirmed real failures 4→1. See
`archived/0071-*`. **#70** —
staticlib link-determinism gate red: the test expected the pre-D3 2-archive pair,
but #62/phase-249 landed the single-runtime model (one `libnros_c.a`, zenoh
bundled). Rewrote `staticlib_duplicate_symbols.rs` for the single archive — links
with `-u nros_rmw_zenoh_register`, NO `--allow-multiple-definition`, one `REGISTRY`;
dropped the obsolete 2-archive dup-diff. See `archived/0070-*`. **#69** —
dep-chain gate red: `dep-chain-check.sh` (1) feature-detected via a loose
substring grep that matched a dependency's requested `rmw-zenoh` feature, and
(2) ran `nros generate-rust` on package.xml-less board-driven talkers. Fixed →
own-feature detect (python) + package.xml-gated codegen; 9/9 cells pass. See
`archived/0069-*`. **#68** —
CycloneDDS ROS 2 action interop "Goal was rejected": an incomplete Phase-233.6
migration left `service.cpp::split_wire_header` re-inserting a `uint32(16)`
goal_id length prefix on the SendGoal/GetResult request receive path, which a real
`rcl_action` client never sends and the post-233.6 action core no longer reads →
`order` read 4 bytes early → out-of-range reject. Fixed by dropping the
`insert_goal_id_len_at` call (+ deleting the dead helper); `cyclonedds_ros2_interop`
5/5 PASS. See `archived/0068-*`. **#67** —
rust typed CycloneDDS publisher `PublisherCreationFailed`: phase-248 C5c removed
the `nros/rmw-cyclonedds` feature that was the sole activator of
`nros-node/__cyclonedds-link` → `cfg(rmw_cyclonedds_present)`, so `register_type::<M>`
no-op'd and the descriptor was never built. Fixed by re-exposing a marker-only
`nros/rmw-cyclonedds` (no concrete dep) + pointing 12 examples + 2 boards at it
(`custom-msg` excepted — hand-written msg, no `Message` impl). Validated: rust
cyclone talker publishes, 4 `native_api` cyclone tests pass. The action-interop
"Goal rejected" was mis-bundled → split to **#68**. See `archived/0067-*`. **#57** —
host-integration chronically red: Cause-1 fixture-build OOM (capped
`NROS_BUILD_JOBS=2×CARGO_BUILD_JOBS=2`) + post-cap residue triage (`fa2ecb60a`) +
QEMU/Zephyr exclude-leak fix. Validated locally (CI can't complete under the
multi-agent main-push cadence): builds green, 0 real failures in the
CI-equivalent set; the 5 cyclone-extras failures are CI-skipped and split out as
**#67** (rust typed cyclone publisher regression). See `archived/0057-*`. **#50** —
weak-symbol audit + checkers: SSoT allowlist + source gate
(`weak_symbol_audit.rs`) + final-image gate (`check-weak-symbols-image.sh`);
W3.1 weak-default deletion (phase-249 P4a); 155.A const-weak → weak getters.
Final close re-audited `smoltcp_init/cleanup` to optional-hook (legacy no-op
stubs; real bring-up is `nros_smoltcp` + `define_network_state!` — no strong def
exists) and fixed the #62 stub-rename allowlist drift. Gates green: source 11
files OK, image checked=20 fail=0. See `archived/0050-*`. **#62** —
D3 completion: R1 (dispatch → generated `NanoRosRmwDispatch.cmake` from
`resolve_rmw`, drift-guarded, consumed by the synth-runtime crate + top-level
link), R2 (weak `nros_app_register_backends` default deleted → missing
registration is a link error; closes #50 W3.1), R3 (triggers consolidated to
hosted `.init_array` ctor + embedded board call; linkme deleted) — all via
phase-249 + a cleanup tail (renamed the misnamed `weak_register_backends.c` →
`weak_platform_log_stubs.c`, scrubbed stale weak-no-op comments). Validated:
nros-c/nros-cpp build, cyclone `cpp_listener` links+runs, drift guard green. See
`archived/0062-*`. **#42** —
platform/std-header fragility (libc/std clashes #27/#36/#38): the class is fixed +
merge-gated (host `platform_header_matrix` + the new cross `cross_libc_precedence`
gate + the zephyr prj.conf gate; one canonical `<nros/platform.h>`; capability
SSoT). Decoupled from the linking class (#20/#62/phase-249). Fully closed — the
"centralise the libc-precedence helper" direction (C) was dropped as a non-goal
(the two-set clash is NuttX-only, one gated site). See `archived/0042-*`. **#53** —
mixed-RMW bridge stock-cyclonedds variant + cross-RMW gateway book recipe (211.I):
shipped `examples/bridges/tt-zenoh-to-cyclonedds` + an Int32 e2e
(`bridge_zenoh_to_cyclonedds`, forwards 8/8 live samples) + the
`cross-backend-bridges.md` recipe; raw publish stages the Cyclone descriptor via
`register_type_descriptor`. See `archived/0053-*`.

Recently resolved (CI infra,
2026-06-15): **#66** (renumbered from 64 — collided with the open esp32 #64) —
stale example Cargo.locks (`nros-core 0.1.0`) tripped the ABI guard + a clippy
empty-line in `nros/lib.rs`; fixed by regenerating 10 locks → 0.5.0 and reordering
the doc comment (validated via nuttx/stm32f4 builds + `check-workspace-all`).
**#65** — `check` cell red from a stale `nros/platform-posix` feature combo
(`justfile`, 248-C5c fallout) + nros-cpp clang-format drift; fixed by dropping the
removed feature and reformatting 5 headers with the CI-pinned clang-format 17.0.5.
See `archived/0066-*`, `archived/0065-*`. **#64** — esp32-c3 QEMU session-init
crashes (Load-access-fault → OOM-wipe → first-timer-fire instruction-fault): one
root class — the ~18 KB stack, starved by an oversized `.bss` esp-alloc heap,
overflowing into `.bss` along the deep zenoh-pico connect/spin path. Fixed by
OpenEth `new_in_place` (no 11 KB stack temp) + locator `.bss`-static + no_std
`CONFIG_PROPERTY_SIZE` 256→64 + esp-println `log::Log` logger + heap 96→48→16 KB
(stack ≈98 KB). Two-node `esp32_emulator` e2e GREEN. See `archived/0064-*`.

Recently resolved (phase-244):
**#49** — example source platform/RMW leakage: re-audit (all example/template
source, 2026-06 rescopes) → 0 blocking major; native/rust cleaned to Shape B (D7),
the zephyr cyclonedds FVP straggler migrated to the typed carrier (C2.1), residual
`minor` = node-lib `#![no_std]` (E4 accepted). qemu-riscv64-threadx → phase-245.
See `archived/0049-*`. **#60** —
platform/RMW-agnosticism audit closed by phase-248 (all four fix-path tiers
converged: cyclone vtable seam, platform cfg → vtable, boards' concrete RMW
optional, `platform-*`/`rmw-*` features retired from `nros`/`nros-c`/`nros-cpp` +
every example/fixture/codegen; embedded runtime-green on freertos/threadx-rv64/
nuttx/baremetal). The SOURCE-layer sibling **#49** + the registration-trigger
**#62**/phase-249 remain. **#61** — zephyr cmake feature remediation closed
`wontfix` (premise void: C3.2 was superseded by 241.D3, so the features remain on
`main`). See `archived/0060-*`, `archived/0061-*`. **#63** —
native Rust cyclonedds binaries dropped the posix platform C port (undefined
`nros_platform_wake_*`): `nros-rmw-cyclonedds-sys` had no `nros-platform` dep, so
nothing re-anchored the cffi rlib's `#[used]` force-link static (zenoh anchors it,
cyclone didn't) → the posix C port was DCE'd. Fixed by mirroring zenoh's
`platform-posix` feature + `__FORCE_LINK_PLATFORM_CFFI` static on the sys crate
(`de85cadc2`). Verified 2026-06-15: native cyclone Rust talker links clean. See
`archived/0063-*`. **#35** —
the 13 zephyr native_sim e2e failures were four distinct root causes, not load
flakes: 9 XRCE (`xrce_session_drive_io` looped on the wall-clock stub
`nros_platform_time_now_ms` returning 0 → switched to monotonic
`nros_platform_clock_ms`), 1 zenoh pubsub (test/example readiness markers), 2
rust service/action (the single-node `ExecutorNodeRuntime` had no service/action
dispatch → Phase 212.M-F.23), 1 cyclonedds (`__register_linked_rmw()` had no
`rmw-cyclonedds` branch → `Executor::open` returned `NoBackend` on linkme-blind
targets). 13/13 green. See `archived/0035-*`.

Recently resolved (Phase 239):
**#39** — C++ `init_with_launch_auto` null-locator env-fallback (fixed in the
3-arg `init` overload); **#40** — C++ action callback truncated result (a symptom
of #39 + a latent result offset 8→5); **#43** — C++ action server empty result
for a C-framed goal (a stale pre-233.6 C fixture writing a removed GoalId
sequence prefix; resolved by a fresh build); **#45** — FreeRTOS Entry-pkg
build/panic-handler (Component → rlib-only + board-owned `panic_semihosting` +
`mps2_an385.ld`); **#46** — FreeRTOS Entry-pkg stack-overflow at Executor
(app-task stack 256→384 KiB + zenoh heap 512 KiB→2 MiB; runtime gate un-ignored +
green); **#48** — FreeRTOS Entry firmware never connected over zenoh: the zenoh
RMW backend was never linked/registered (→ `NoBackend`) and the deploy
locator/ip/gateway was inert (`Config::default()` `192.0.3.x`). Fixed by linking
+ registering the backend (`nros/rmw-zenoh` + `__register_linked_rmw()` on
`target_os = "none"`) and threading the deploy block into the boot `Config` via
`BoardEntry::run_with_deploy` + `DeployOverlay`; `freertos_run_plan_runtime` now
asserts the connected run. See `archived/0039-*`, `archived/0040-*`,
`archived/0043-*`, `archived/0045-*`, `archived/0046-*`, `archived/0048-*`.

Recently resolved (Phase 243): **#48 (nuttx)** — the NuttX link dropped the whole
`nros_platform_*` ABI (undefined refs from `libnros_rmw_zenoh` / `libzpico_sys`).
Root cause was NOT the typed carrier (original diagnosis corrected): the board
crate's `cc` platform-port build emitted the default `static=` (`+bundle`), folding
the port into `libnros_board_nuttx_qemu_arm.rlib`, which precedes the referencers on
the link line ⇒ single-pass `ld` drops it. Fixed in `nuttx_platform_build.rs` with
`cargo_metadata(false)` + a hand-emitted
`static:-bundle,+whole-archive=nros_platform_nuttx` (trailing, order-independent).
See `archived/0048-nuttx-typed-carrier-link-drops-platform-port.md`. (Note: id 48
is shared with the earlier resolved FreeRTOS-slirp issue — a pre-existing numbering
collision.)

Recently resolved (Phase 240.5): **#47** — C/C++ action client now callback-based
(`nros::bind_action_client` = `set_callbacks` + a poll-timer pump per RFC-0041);
NuttX cpp+C action E2E green in QEMU. See `archived/0047-*`.

**#44** — esp-idf `platform.c` compile failed: esp-idf riscv `FreeRTOSConfig_arch.h`
uses linker symbols `_heap_start`/`_heap_end` (`&_heap_end - &_heap_start`) this TU
never declared. Fixed by declaring them `extern int` (matching esp-idf), gated to
`ESP_PLATFORM`, before `<FreeRTOS.h>`. Verified: esp32c3 `platform.c.obj` compiles.
See `archived/0044-*`.
