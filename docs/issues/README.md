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

| id | title                                                                 | type        | area   | file |
|----|-----------------------------------------------------------------------|-------------|--------|------|
| 68 | CycloneDDS ROS2 action interop — nano C action server rejects the ROS2 client goal ("Goal was rejected") | bug | rmw | [0068-cyclonedds-ros2-action-goal-rejected.md](0068-cyclonedds-ros2-action-goal-rejected.md) |

Resolved issues live in [`archived/`](archived/). Recently resolved: **#67** —
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
