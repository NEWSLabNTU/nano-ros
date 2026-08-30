# Audit findings — 2026-07-28 (quick)

- Depth: **quick** (grep-led, 5 parallel reader lanes, lead-verified P1s)
- Categories: all A–J
- Target: `f29b480e0` (origin/main, worktree clean)
- Baseline: `audit-findings-2026-07-17.md` (deep, target `0d4a9b139`)
- Delta audited: **535 commits / 1491 files**, incl. 10 new RFCs (0051–0060),
  31 new test files, 93 new issue docs
- Lanes: cmake+self-containment · core-code · API-shape · testing · examples+docs
- Spend: 5 reader agents, ~543k subagent tokens, 288 tool calls
- Verification: every **P1** was re-checked by the lead against the source (one
  was refuted and demoted); P2s are finder-confirmed by read, not adversarially
  refuted (quick-mode contract).

## Filed as

#321–#335 plus **#336** (the bootstrap-drift P1 — filed as #320, renumbered on
push because origin already had one). The SystemModel absolute-path finding is
**not** ours: another session filed it concurrently as **#320** with a fuller
diagnosis (vanished `meta.record` targets, write-only `sha256`es), so this run's
#334 was narrowed to the remaining leak (`zephyr.rs:417`) and cross-references it.
Two sessions independently finding the same defect within hours is itself a data
point for the "gates narrower than their rule" note below — nothing in CI looks
for tracked absolute paths.

## Resolved since last run

**All nine P2s from the 07-17 deep audit are fixed and archived** — #219
(cmake CLI-resolver fragmentation), #220 (book/CLI drift), #221, #222 (RTOS
fixture freshness — both parts, verified: all four resolvers now use
`require_prebuilt_binary_fresh*` and `zpico_drift_gate.rs:22` carries the
sanctioned-exception rationale), #223/#224 (action CDR silent fallback,
`PROBE_TIMEOUT_MS` duplication), #225 (backend-named module in agnostic core),
#226 (C++ param sequence storage), #227 (domain-0 sentinel). Clean baseline.

Trend improvements: inline output-marker literals **22 → 6**; phase/wave-named
test FILES **2 → 0**.

---

## P1 (8)

- **bootstrap** · `scripts/bootstrap.sh:189` · P1 · **new** · `ensure_cli_submodules()` inits the retired `packages/cli/third-party/ros-launch-manifest` and guards on `${sub}/types/Cargo.toml`, but `.gitmodules` now declares only `packages/cli/third-party/ros-launch-resolve` and the CLI's path deps run *through* it (`nros-cli-core/Cargo.toml:46-49`) — the guard file never appears, the real submodule is never initialized, and a fresh clone cannot build the CLI. It only works on this host because two retired submodule worktrees still sit on disk.
  · fix: point `sub` at `ros-launch-resolve`, guard on `third-party/ros-launch-resolve/resolve/Cargo.toml`, keep `--init --recursive` (the nested rlm + parser are now required). **Lead-verified.**
- **bootstrap-doc-drift** · `activate.sh:75` (+8 more) · P1 · **new** · Nine documented copies of the same dead `git submodule update --init packages/cli/third-party/ros-launch-manifest` command: `activate.fish:50`, `book/src/getting-started/{installation.md:148, first-node-rust.md:23, first-node-cpp.md:23, first-node-c.md:22, workspace-bringup.md:27, workspace-node-pkgs.md:28, workspace-entry-pkg.md:27, workspace-from-app-node.md:89}`, `book/src/reference/cli.md:24`.
  · fix: one SSoT (a book include + a shell helper) plus a CI grep for the retired path, which would have caught all nine.
- **doc-drift** · `AGENTS.md:285` · P1 · **new** · The canonical install instruction says init CLI submodules "(NOT `--recursive`)" — post-RFC-0060 that leaves the CLI unbuildable, because the pinned submodule nests rlm + parser two levels deep. Same line still advertises PATH-wiring `play_launch_parser`, and :286 claims `just doctor` fails on it (now a test-only prereq).
  · fix: rewrite as the scoped `--init --recursive` form and reconcile with the "never `--recursive` from a worktree" landmine at :294 (that landmine is about the *unscoped* form).
- **test-gate-red** · `packages/testing/nros-tests/tests/ros_editions_e2e_pubsub.rs:61` · P1 · **new** · Six inline output-marker literals landed in the new ros-editions family, which is exactly what `output_marker_gate.rs` rejects — **the gate is RED on main**. Offenders: `ros_editions_e2e_pubsub.rs:61`, `ros_editions_e2e_service.rs:32`, `ros_editions_xrce.rs:82,:104`, `ros_editions_zenoh.rs:87,:111`. (`just check fast` stays green because the gate is a test, not a check.)
  · **How it landed unnoticed:** `justfile:1174` deselects the ros-editions binaries from the default sweep (they need docker + a slow per-edition image), but `output_marker_gate` is a *separate* binary that is **not** excluded and reads `tests/*.rs` from **disk** — so it polices files whose own test binaries never compile or run in `just ci`, while the authors' lane (`just ros_editions ci jazzy`) never runs the gate. Two lanes, neither covering the other.
  · fix: use `nros_tests::output::{LISTENER_LOG_PREFIX, SERVICE_RESULT_PREFIX}` / `output::listener_line(42)`; also reconsider whether the gate belongs in a lane that co-runs with the family it polices. **Lead-verified by running the gate.**
- **silent-wrong-behavior** · `packages/core/nros-node/src/executor/action_core.rs:293` · P1 · **new** · `accept_goal` sends the `accepted=true` reply and only then does `let _ = self.active_goals.push(...)`; `MAX_GOALS` defaults to **4** and no caller pre-checks capacity, so a 5th concurrent goal is acknowledged as accepted and then never recorded — it never executes, no result is ever produced, and the client waits forever.
  · fix: push into `active_goals` *before* `send_reply`, `reject_goal` when the table is full, and propagate `publish_status_array()` failure. **Lead-verified (no caller pre-check exists).**
- **silent-wrong-value** · `packages/core/nros-node/src/parameter_services.rs:122` · P1 · **new** · `from_rcl_value` discards every capacity error (`let _ = push_str` / `let _ = push`), so a wire-received parameter is silently truncated (string) or loses its tail (arrays) and is then stored as the parameter's real value; an unrecognised `type_` falls to `_ => NotSet`. `to_rcl_value` (:82) mirrors it on the reply side, and `nros-params/src/types.rs:471,487,503,519` turns oversize hosted values into `NotSet` / empty arrays via `unwrap_or_default()`.
  · fix: return `Result` from both converters, reject over-capacity input at the declare/set boundary, and map unknown `type_` to an explicit error so `SetParameters` replies `successful=false` with a reason. **Lead-verified.**
- **swallowed-error** · `packages/core/nros-node/src/executor/spin.rs:4893` · P1 · **new** · `let _ = self.session.drive_io(...)` discards the transport I/O error inside `spin_once`, and there is **no session-health surface anywhere in the crate** (grepped), so a dead/disconnected session keeps returning `Ok(())` from `spin()` forever. `nros-c/src/executor.rs:1911,1949` does the same for the C blocking spins.
  · fix: propagate the primary session's error out of `spin_once` (or a sticky health flag + consecutive-failure counter that `spin()` reports), keeping "extra sessions are best-effort" explicit. **Lead-verified.**
- **resolver-precedence** · `integrations/nano-ros/CMakeLists.txt:82` · P1 · **new** · `find_program(NROS_EXECUTABLE nros HINTS "$ENV{HOME}/.nros/bin")` — HINTS are searched *before* PATH, so a stale provisioned CLI shadows the `activate.sh`-wired in-tree one, violating the pitfall `CLAUDE.md:210` states verbatim. The ESP-IDF `codegen-system` bake at :84 then **fails soft** via `message(STATUS)`, so a museum CLI yields a stale/absent baked system tree with no error. A NEW site, outside the four #219 retired.
  · fix: `HINTS` → `PATHS`, or include `NanoRosCodegenCore.cmake` and call the shared `nros_resolve_cli()`.

### P1 refuted during verification

- `parameter_services.rs:337` "`handle_set_parameters_atomically` always reports success" — **refuted**: a pre-check loop 12 lines above validates read-only, type, range and fullness, mirroring `set()`'s failure modes. Residual (discarded `set()`/`declare()` Results, no rollback) is defence-in-depth only → recorded as P3 below.

---

## P2 (18)

### Build / CMake

- **A2/A1** · `cmake/NanoRosNodeRegister.cmake:252,418,436,855` + `NanoRosEntry.cmake:226` · P2 · **new** · The Zephyr guards are still keyed on the possibly-unset `NANO_ROS_PLATFORM` — the exact defect `5a8db2413` fixed at `NanoRosVerbs.cmake:290`, i.e. **one of six sites was fixed**. `nano_rosConfig.cmake:41` sets it as a plain directory-scoped var, so any `add_subdirectory`'d pkg that doesn't itself `find_package` compares the literal NAME and silently takes the wrong branch (OBJECT_DEPENDS ordering, interface-lib skip inverted, Zephyr carrier). Archived 0282 records this residual itself. P2 not P1 only because no in-tree example exercises the fused path.
  · fix: one `_nros_is_zephyr()` helper in `NanoRosCodegenCore.cmake` used by all six — note the fix commit introduced a *second* idiom instead, which is now itself A1 drift.
- **A5** · `cmake/NanoRosBootstrapCodegen.cmake:43` · P2 · **new** · A fifth surviving bespoke `nros` resolver (#219 claimed unification). Precedence is correct, but it ignores `$ENV{NROS_CLI}` and caches into `_path_codegen`, which the stale-path re-detect never clears — once the CLI moves, the module re-blesses a dead path.
- **A5/A1** · `zephyr/cmake/nros_rmw_cyclonedds.cmake:264` + `packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake:61,116` · P2 · **new** · Three `idlc` resolvers for one host tool, and all put the retired in-tree `build/cyclonedds/bin` in **HINTS**, inverting the precedence their own comments document — a museum `idlc` shadows a fresh ROS 2/SDK one, yielding descriptors from a stale compiler (`find_descriptor → nullptr` class).
- **G2** · `packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsTypeSupport.cmake:149` · P2 · **new** · The msg→IDL converter is resolved by a four-level source-tree walk-up to the repo root, 15 lines below a comment forbidding exactly that; consumed from any other layout the hint dangles and codegen "silently degrades … every `find_descriptor()` fails at runtime".
- **A1** · `packages/api/nros-c/cmake/NanoRosLink.cmake` · P2 · **new** · A dead 261-line duplicate of `cmake/NanoRosLink.cmake` defining the same public verbs from a retired design era, sitting beside four live modules so it reads as authoritative.
- **A1/A4** · `cmake/toolchain/riscv64-threadx.cmake:135,152` vs `packages/api/nros-c/cmake/nros-threadx.cmake:323` · P2 · **new** · Duplicated rust-lld/llvm-ar probe, both hardcoding the build-host triple in the rustc sysroot path; with `NO_DEFAULT_PATH` a non-x86_64 host silently falls back to GNU ld — straight into the picolibc TLS `errno` link failure lld exists to avoid.
- **A1** · `cmake/templates/*_entry_main{,_c}_typed.cpp.in` · P2 · **new** · Ten near-duplicate entry templates; 60 of 74–94 lines byte-identical. The RFC-0043/0044 shape-branch block is copied ten times, so any shape change is a ten-file edit (the phase-246 generator class).

### Testing

- **E6** · `packages/testing/nros-tests/tests/ros_editions_zenoh.rs:40` · P2 · **new** · The phase-311 edition×RMW×workload lanes are five hand-written per-cell files with 16 near-identical `#[test]` bodies, none consuming `matrix::CELLS` — the RFC-0051 pattern phase-295 already landed.
- **E5** · `packages/testing/nros-tests/src/matrix.rs:223` · P2 · **new** · `Cell` has no ROS-edition field, so ARCHITECTURE §2's third declared axis is structurally outside the matrix table (edition comes from an env var) — which is *why* the family above can't be matrix-derived.
- **E5** · `docs/design/ARCHITECTURE.md` §2 · P2 · **new** · §2 still says supported editions are humble/iron with "jazzy/rolling planned", but jazzy is the delivered default (`just/ros-editions.just:13,111`), has a real `ros-jazzy` feature, and is the only edition the zenoh interop lane can run. The real carve-out (humble/iron ship no `rmw_zenoh_cpp` apt package) exists only as a code comment at `ros_env.rs:957`.
- **E1** · `packages/testing/nros-tests/src/fixtures/binaries/mod.rs:2380` · P2 · **new** · ~30 resolvers still use existence-only `require_prebuilt_binary` on cargo-built artifacts that *have* a sibling `.d` — the museum-binary trap #222 closed for the four RTOS resolvers was never propagated here. Highest-value sites: `:2380` (all qemu-arm-baremetal shared fixtures), `:3040`, `:4195`, `:4434`, `zephyr.rs:899,:961`.
- **E4** · `packages/cli/rosidl-codegen/tests/heap_compile_check.rs:14` · P2 · **new** · **24 `#[ignore]` tests are permanently unreachable** — nothing in `just/`, `justfile`, `.github/workflows/` or `.config/nextest.toml` passes `--ignored`/`run-ignored`. That includes the 8 rosidl-codegen compile-checks that are the only gate on heap/borrowed storage-mode codegen.
- **E7** · `packages/testing/nros-tests/tests/output_marker_gate.rs:16` · P2 · **new** · The gate hardcodes its own 9-entry `MARKERS` table instead of consuming `src/output.rs` (~30 markers) — it enforces an SSoT using a second copy of the data it guards. `xrce_ros2_interop.rs:390` uses marker-ish strings absent from the table, so the gap is already live.
- **E8** · `packages/testing/nros-tests/src/ros_env.rs:59,638,946` and `tests/orchestration_tiers_freertos.rs:116` · P2 · **new** · The **zenoh** ros-editions cell (specifically — the cyclone block legitimately isolates via `unique_ros_domain_id()`) pins a fixed `tcp/127.0.0.1:7447` + hardcoded `domain = 0` for six tests, with isolation resting entirely on `--test-threads=1` buried in `just/ros-editions.just:147`; the code comment asserts "the lane is serial anyway" with nothing enforcing it. Separately, `start_slirp(7447)` is the only bare port literal among 14 call sites, landing inside the allocator's own 7400–7799 window.

### API / layering

- **C1** · `packages/api/nros-cpp/include/nros/main.hpp:103` (+ `src/lib.rs:754,:2215`, `executor.hpp:139`) · P2 · **new** · The bounded spin policy — `NROS_ENTRY_SPIN_MS` env read via a hand-rolled parser, wall-clock budgeting, cooperative yield — is implemented in the C++ header and exists **four times** with diverged behavior (only the header copy checks `nros::ok()`).
- **C1/C4** · `packages/api/nros-cpp/include/nros/node.hpp:710` · P2 · **new** · `init()` implements the baked-macro/hosted-default rungs of the RFC-0045 ladder in the header, duplicated verbatim across both overloads, splitting the ladder between shim and Rust resolver.
- **C1** · `packages/api/nros-cpp/include/nros/action_client.hpp:82` · P2 · **new** · `GoalAccept::ffi_deserialize` hand-decodes the goal-acceptance wire payload (goal_id at byte 0, accepted at byte 16) in a public C++ header with a magic `SERIALIZED_SIZE_MAX = 32` for a 17-byte payload — the #226 class again (wire decoding belongs to codegen/serdes).
- **C5** · `packages/api/nros-cpp/include/nros/node.hpp:717` + `nros-c/include/nros/app_main.h:61` + `nros-node/src/executor/types.rs:160` + `nros/src/init.rs:128` · P2 · **new** · The zenoh-router default `tcp/127.0.0.1:7447` is hardcoded independently in **four** layers, two of them RMW-blind. Related: `nros-platform/src/board/config.rs:36` puts a backend name in a core public API (`fn zenoh_locator()`), and `nros/src/lib.rs:385` hardcodes `::nros_rmw_zenoh::register()` / `::nros_rmw_xrce_cffi::register()` in the façade macro — both the #225 class.
- **C7/C3** · `packages/rmw/cffi/src/lib.rs:1008` · P2 · **new** · `nros_rmw_cffi_set_custom_transport` exports the hand-written `nros_rmw::NrosTransportOps` while `rmw_transport.h:153` declares `const nros_transport_ops_t*` — RFC-0054's header-as-ABI-SSoT is bypassed at this export (the header doc even inverts the SSoT: "mirror of the Rust-side NrosTransportOps"), and `abi_layout_check.c` has no assert for this struct. *Correction to the lane's report: the generated mirror is not dead repo-wide — `nros-c/transport.rs` and `nros-rmw/custom_transport.rs` do use it.*
- **C7** · `packages/core/nros-rmw-abi/include/nros/rmw_vtable.h:51` · P2 · **new** · `create_session`'s `uint8_t mode` carries zenoh's `whatami` (Client/Peer) — a backend-specific parameter with no `rmw.h` counterpart (`rmw_init_options_t` has domain_id/enclave/security/discovery, no session mode) and, unlike every neighbouring slot, no doc comment or legal-value list.
- **C6** · `packages/api/nros-cpp/include/nros/executor.hpp:139` · P2 · **new** · `Result spin(uint32_t duration_ms)` reuses rclcpp's `spin()` name for bounded spinning; rclcpp's time-budget counterpart is `spin_some(max_duration)` and `spin()` blocks until shutdown, while Rust's `spin(Duration) -> !` means "forever, per-iteration timeout" and needed a separate `spin_default()`. Three semantics under one standard name.
- **B5** · `packages/api/nros-cpp/include/nros/bridge.hpp:17` · P2 · **new** · A fresh issue-0112 instance: `<string>`/`<vector>` included **ungated** (and used as `SessionSpec` members). Worse, the `check-cpp` freestanding probe (`justfile:1806`) compiles this header but does not pass `-nostdinc++`, so the gate structurally cannot catch the 0112 class.
- **B5/B7** · `packages/api/nros-c/include/nros/check.h:32` · P2 · **new** · The default `NROS_CHECK_LOG` unconditionally `#include <stdio.h>` and calls `printf` in a public C header consumed by embedded no_std C nodes — no hosted gate, no log-level gate.
- **B3** · `packages/rmw/cffi/src/lib.rs` (21 sites) · P2 · **new** · 21 `.expect("rmw vtable: …")` unwrap `Option<extern fn>` vtable slots on the embedded path (including hot-path `drive_io`, `has_data`, `publish_raw`), while `nros_rmw_cffi_register_named:756` validates only the name/NULL pointer — a C backend with an incomplete vtable panics mid-spin on a no_std target.
- **B6/I4** · spin quantum, ~12 sites · P2 · **new** · The 10 ms spin quantum is a bare literal across 6 board crates, the entry macro and 3 C++ headers; docs already drift to `spin_once(100)`.

### Examples / UX

- **F2** · `just/workspace.just:294` · P2 · **new** · `just doctor` gates on the retired `play_launch_parser` prereq and never checks `nros-launch-resolve`, which `nros ws sync` hard-requires (`ws.rs:471` bails without it) — doctor reports a green tree that cannot sync a workspace, and MISSING for a binary the CLI no longer invokes.
- **H3** · `book/src/getting-started/workspace-entry-pkg.md:128` · P2 · **new** · The book teaches the exact PATH invocation issue 0285 abolished ("`nros sync` … using `play_launch` from PATH" + a copy-pasteable `play_launch resolve` command); also `workspace-cpp.md:242,:266`, `workspace-bringup.md:295`, `user-guide/component-and-entry-pkg.md:120`.
- **H1** · `packages/cli/CLAUDE.md:43` · P2 · **new** · Still describes the retired vendoring shape (`third-party/{play_launch_parser, ros-launch-manifest}`) and has no row for `nros-launch-resolve`, the binary every workspace build depends on; `nros-cli-core/Cargo.toml:36` still claims resolution shells out to `play_launch_parser` (no such call site exists).
- **G2/J1** · 68 × `examples/**/config/*_model.yaml` · P2 · **new** · Tracked generated models bake `/home/aeon/repos/nano-ros/...` into `meta.inputs[].path` *and* claim provenance from the retired resolver (`meta.resolver.tool: play_launch 0.8.2`), while the current emitter deliberately writes its own name so consumers can tell what built the artifact — and no consumer validates either field (`ws sync` staleness is mtime-only). Build impact today is nil; the costs are the RFC-0026 copy-out contract, non-reproducibility (any other dev's re-run rewrites all 68 → recurring conflicts), and leaking `$HOME` into a public repo.
  · fix: rewrite `meta.inputs[].path` repo-relative in `ws.rs`, regenerate, and gate with `git grep -nE '/home/|/Users/' -- examples/` — the sweep confirms these 68 files plus `zephyr.rs:417` are the *only* tracked absolute-path leaks, so the gate goes green immediately.
- **G2** · `packages/testing/nros-tests/src/zephyr.rs:417` · P2 · **new** · Hardcodes `/home/aeon/repos/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8` as the `QEMU_BIN` fallback while the adjacent dtb lookup resolves via `zephyr_workspace_path()`; on any other host `Command::new` fails with a bare ENOENT. The pinned `0.16.8` is also a second SSoT (owned by `scripts/zephyr/setup.sh:78`).
- **F1** · `packages/cli/cargo-nano-ros/src/scaffold.rs:684` · P2 · **new** · For four of the eight platforms the book advertises, `nros new` emits `# TODO: add board crate for this platform = { version = "*", … }` — interpolated as the dependency *name*, so the whole line is a TOML **comment** and the board dep vanishes with no diagnostic. `scaffold_c`/`scaffold_cpp` go further (`let _ = platform;`) while the book promises a platform-tuned skeleton.
- **J1** · `packages/cli/cargo-nano-ros/src/scaffold.rs:744` · P2 · **new** · The scaffolder — the first code a new user sees — emits the retired hand-rolled entry shape (`#![no_main]` + `#[unsafe(no_mangle)] extern "C" fn main() -> !` + `loop {}` + two TODOs), i.e. a project that does not run, while every tracked Rust entry example is the one-liner `nros::main!(model = …)`.
- **J1** · `examples/px4/cpp/uorb/nros-register-check/sitl_register_stub.c:1` · P2 · **new** · A weak-symbol link stub inside a copy-out example, self-labelled "SITL build scaffold — NOT application logic". Framework gap: the weak fallback belongs in `nros-platform-px4`/the PX4 cmake module (the phase-247 weak-symbol machinery already exists).
- **J1** · `examples/native/rust/lifecycle-node/src/main.rs:44` · P2 · **new** · The Rust lifecycle example makes the user write five `unsafe extern "C" fn(*mut c_void) -> u8` callbacks returning a raw discriminant; the module doc admits it exists "so this path exercises exactly the same FFI surface the C API uses" — an FFI regression test wearing an example's clothes. rclcpp/rclrs users override safe methods.

---

## P3 (6, not filed)

- `parameter_services.rs:337` — discarded `set()`/`declare()` Results + no rollback after the pre-check (the refuted P1's residue).
- `nros-cpp/src/lib.rs:2131` unconditional `eprintln!` multi-tier banner; `nros-node/src/executor/types.rs:358,369` one-shot deprecation warnings.
- `rtos_e2e.rs:303,:489-503` — `skip_reason` now returns `None` unconditionally, so the `eprintln!("[SKIP]")` + `return` branch is dead, but its 15-line doc-comment still documents a live silent-PASS path and the marker (`[SKIP]`) doesn't match the harness's `[SKIPPED]`.
- `packages/cli/rosidl-codegen/src/config.rs:455` — `fn mode_phase1_gate()` (E3), exercising `StorageMode::is_phase1_supported` — the phase number is baked into a *public* signature.
- `cmake_minimum_required` drift — 252 files at 3.22, 61 at 3.20, 9 at 3.16, one at **3.8** (rejected outright by CMake 4). The 3.20 cluster is Zephyr-facing and defensible but undocumented.
- `cmake/NanoRosVerbs.cmake:328` — `nros_components_register_node` is the only public verb not prefixed `nano_ros_`; the ament counterpart makes `nano_ros_components_register_node` the derivable name.
- `just/ros-editions.just:145` — the zenoh recipe's comment claims "jazzy runs 5/6 (ROS→nano action server is #0292, ignored)", but `ros_editions_zenoh.rs:170` is a plain `#[test]` with a hard `assert!` (no `#[ignore]`, no skip) and #0292 is archived as fixed. The recipe prints "PASS (cyclone + xrce + zenoh)" on a path that either passes 6/6 or hard-fails — the comment is stale either way.
- **~5.1 GB of orphaned build output** in husks of moved/deleted examples (`examples/native/rust/entry-poc` 3.4 G, `examples/qemu-arm-baremetal/rust/phase216-rtic-e2e` 1.7 G, plus `examples/zephyr/rust/{xrce,dds}`, `examples/qemu-esp32-baremetal/rust/dds`, …), and the two retired `packages/cli/third-party/{play_launch_parser,ros-launch-manifest}` worktrees — the latter are what mask P1-1.

## Cross-cutting pattern (the most actionable thing in this report)

**Class fixes are landing at the reported site.** Three independent instances,
all confirmed this run:

| class | fixed | left armed | now |
| --- | --- | --- | --- |
| sizes-header mirror | 0088, 0114, 0122, 0123, 0245 | each next build path | 0268 (structural fix + gate) |
| Zephyr unset-variable guard | #282 — **1 of 6** sites, *and* it added a second idiom instead of a shared helper | 5 sites | **#326** |
| fixture freshness probe | #222 — **4 of ~34** resolvers | ~30 in `binaries/mod.rs` | **#328** |

The rule that follows: grep for every sibling before fixing, land ONE shared
helper rather than a second spelling, and record the sweep command in the commit
message so it can be re-run. Added as a `CLAUDE.md` practice ("Fix the CLASS,
not the reported site") and cross-linked from both #326 and #328.

**Second-order version of the same problem — gates narrower than their rule.**
Four found this run: #321 (the marker gate lints sources whose binaries the sweep
excludes), #328 (24 `#[ignore]` tests no lane runs), #332 (`check-cpp` cannot
detect the 0112 class because it never passes `-nostdinc++`), #334 (provenance
fields nothing validates). A gate that cannot fail on the case it names is worse
than no gate — it reads as coverage. This is the issue-0196 rule ("build-side
probes must watch the same inputs as test-side gates") generalised.

## Process finding

**Issue-status hygiene has drifted again.** Six files sit in `docs/issues/`
with `status: resolved` (0309, 0312, 0313, 0314, 0318, 0319); 11 of the 17
files there are genuinely open. The same drift was cleaned on 2026-07-26
(`9fd17ec4b`), so this is a recurring process gap, not a one-off — worth a
`just` gate (`status: resolved` in `docs/issues/*.md` ⇒ fail) rather than
another manual sweep.

## Coverage

- **Swept exhaustively:** A2 (all 39 hits read and each killed with a reason),
  A5, B5 (all 17 hits), E1/E3/E4/E7/E9, G2 absolute paths (complete set
  established: 68 yaml + `zephyr.rs`), J1 (all 67 hits triaged).
- **Sampled by risk:** B3 (1172 hits → 204 genuine runtime lines; the list is
  ~83 % test-module false positives), B7, I3 (parameter/action/lifecycle/spin
  clusters).
- **Not covered:** Rust-side B5 (`use std::` in no_std-intended crates) — the
  pre-scoped list was C/C++-only; B6 systematically (only the spin quantum and
  two spot checks); `nros-node/src/{lifecycle,lifecycle_services,executor/handles,
  executor/handoff}.rs` `let _` sites; `nros-cpp/src/lib.rs` + `nros-c/src/*.rs`
  in depth (~15k lines of CFFI); RFC-0049's board/platform TOML resolver
  implementation; per-cell `realtime_tiers_*_e2e.rs` / `*_entry_e2e.rs` (~40
  files, pre-existing debt phase-295 W3 is still migrating — deliberately not
  filed); each of the ~28 `examples/README.md` matrix cells against a live lane;
  no clean-system bootstrap run (issue #204, out of scope).
- **Recommended targeted deep run:** `/audit deep C,E` — the C7 rmw.h parity
  surface and the E5/E6 matrix-vs-edition axis both need more than a quick pass,
  and the ros_edition axis is the newest, least-audited dimension.
