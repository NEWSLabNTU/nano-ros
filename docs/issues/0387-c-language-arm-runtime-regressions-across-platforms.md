---
id: 0387
title: C-arm (and partly C++) runtime lanes fail across platforms while the
  Rust arms pass — confirmed on fresh fixtures at low load
status: open
severity: high
created: 2026-08-02
tags: [c-api, cpp-api, threadx, zephyr, realtime, params]
related: [0380, 0356]
phases: [phase-326, phase-327]
---

## Evidence quality

Every failure below reproduced 3/3 on FRESHLY REBUILT fixtures (post
issue-0380 model restoration, CLI current, full family rebuild) at
`--test-threads 2` on an otherwise idle host — the stale-fixture and
QEMU-load explanations are eliminated. The same run's Rust siblings pass,
which rules out router/agent/env problems.

## The confirmed-red set (2026-08-02)

**Language pattern — C fails where Rust passes on the SAME platform:**

- `rtos_e2e` ThreadxLinux **C** pubsub (0 messages), **C** action
  (accepted=false), ThreadxRiscv64 **C** pubsub + service (0 responses) —
  Rust and Cpp arms of the identical cells PASS.
- `zephyr_edf_deadline_applied_c` — 0 `nros: EDF deadline set tier=` lines;
  the `_cpp` and `_rust` arms of the same fixture family PASS.
- `realtime_tiers_e2e` case_02_native_c, case_03_native_cpp,
  case_04_native_cpp_rclcpp — low tier never scheduled (`/telem` 0
  deliveries); case_01_native_rust PASSES.
- `cpp_c_param_live_read_e2e` (both arms) — component never publishes the
  baked param; `nros_cpp_get_param_integer` never reaches the callback.

**Others in the same confirmed set (not obviously the language class):**

- `entry_e2e` case_12_zephyr_rust_params — 60 s timeout, consistent.
- `realtime_tiers_e2e` case_08_nuttx_arm_cpp (`[TX] TIMEOUT: no
  completion`), case_09_nuttx_arm_c (timeout) — rust case_10 passes
  (flaky-pass under load).
- `emulator::test_qemu_rtic_action_e2e` — goal not accepted, 3/3; the rtic
  pubsub/service siblings pass (flaky under load only).

## Why this smells like ONE root (or few)

The C/C++ arms share the entry codegen (`emit_c.rs` / `emit_cpp.rs`), the
cpp-ffi layer, and the model→sched-context ingestion that Rust reaches via
a different path (`nros::main!`). Two recent events touched exactly that
surface:

1. `2ce930e39` (#356 Part2, 2026-07-30) — "author c/cpp/mixed embedded
   deploys + regenerate multi-board models".
2. The issue-0380 regenerations (models re-resolved; the deploy tables the
   C/C++ entry consumes changed shape) — the dims are restored, but the
   C-arm consumption may key on something the regeneration also changed.

Note this failure class sits exactly where RFC-0061 says tier 2 is blind:
platform×language pairing (1-wise sees each platform and each language,
never their product). This host's first pairwise-grade sweep is what
surfaced it.

## Triage guidance

Start with the ThreadX C pubsub cell (smallest: 0 messages, no QEMU for
threadx-linux, C entry TU inspectable in the fixture build dir): diff the
generated C entry TU + baked config against the Rust arm's, then check
whether the C entry ever opens its session (the issue-0380 threadx symptom
was "only the boot tier comes up" — if the C entry's tier table is empty,
the publisher task never spawns, which also explains action/service).
Verify against `git show 2ce930e39` for the deploy-table consumption
change, and against a pre-2ce930e39 fixture build if needed.

The param_live_read pair and the native realtime C/C++ arms likely fall
out of the same fix; re-run the confirmed set after.

## Investigation (2026-08-02) — the entry-codegen hypothesis is REFUTED

Reproduced the threadx-linux C pubsub codegen directly
(`nros codegen entry --lang c --typed --board threadx-linux --model
.../c/src/demo_bringup/config/system_model.yaml --metadata <ws>/nros-metadata.json`)
and compared:

- **The generated entry is well-formed** — it creates BOTH nodes (listener +
  talker) and calls each C component's `__nros_c_component_<pkg>_configure`. The
  "empty tier table / publisher never spawns" theory does NOT hold; the entry
  places the publisher. (The earlier `grep nros_cpp_node_create` "0 nodes" was a
  false alarm — the C++ emitter uses `::nros::create_node`.)
- **The entry is byte-identical from the OLD (pre-2ce930e39) vs NEW (regenerated)
  model** — the only near-here model delta the regen made is dropping a
  top-level `target: linux` on nodes, which the entry codegen never consumes. So
  the model regeneration did NOT change the C entry.
- Routing an embedded C board (`--lang c` + `board_is_embedded`) to the **C++**
  emitter with the C component seam (`__nros_c_component_*` +
  `ThreadxBoard::run_components`) is BY DESIGN (`cmd/codegen.rs:287-291`).

**Reframing: not a regression — newly-EXERCISED code.** `2ce930e39` *added*
`[deploy.{threadx-linux,freertos,nuttx}]` to the C/C++ workspaces, enabling
C-embedded cells that had no fixtures before; the pairwise sweep is the first
time they RAN (matches the RFC-0061 tier-2 blindness note). And the NATIVE C
failures (`realtime_tiers case_02_native_c`, `cpp_c_param_live_read_e2e`) use the
*pure-C native* runner (`nros_board_native_run_components`), NOT the embedded C++
runner — so the shared root is NOT the entry emitter but the **C-component
runtime path / the baked config the C path consumes at runtime**, common to
native + embedded C.

**Next step is RUNTIME, not codegen.** Build one failing C cell (start native,
no QEMU — `realtime_tiers case_02_native_c` or the param C arm), run under
`NROS_RMW_TRACE_OPEN=1` + component prints, and find where the C path diverges
from the passing Rust/Cpp arm (session open? publisher create? sample send?
sched-context apply?). Likely a small number of latent C-runtime bugs in the
newly-run cells, not one fix. The entry-codegen path is CLEARED.

## ROOT CAUSE + FIX (2026-08-02) — the tier class

Instrumented `nros_board_native_run_tiers`'s per-tier thread on the native C
realtime fixture (`ws-realtime-c`): the low `/telem` tier's `open_over_session`
rc=0, `set_active_groups` ok, **`setup` rc=0** (node + publisher + timer all
created), then the FIRST `nros_cpp_spin_once` returns **-15 =
`NROS_CPP_RET_REENTRANT`** → the tier loop's `if rc != OK break` kills the tier
before it ever spins. Telem: 0 ticks; ctrl (boot tier, primary executor): fine.

`spin_once` returns REENTRANT when the `in_dispatch` guard is already set.
`nros_cpp_executor_open_over_session` builds a borrowed executor into a
`MaybeUninit<CppContext>` but wrote only `.executor` + `.domain_id` — it left
**`.in_dispatch` UNINITIALIZED**, so the guard read garbage-true and every first
spin on a borrowed-tier executor spuriously returned REENTRANT. `in_dispatch`
was added by **#0290** (the C++ reentrancy guard); that change updated
`nros_cpp_init` but NOT the borrowed-executor open path — which is why this is
NEW (the guard is recent) and why C/C++ fail (both go through the C-ABI
`CppContext`) while Rust passes (its tier path never touches this field).

**Fix:** `nros_cpp_executor_open_over_session` now writes `.in_dispatch = false`,
mirroring `nros_cpp_init`. Verified on `ws-realtime-c` native C: telem went 0 →
54 ticks in 6 s. This covers the whole tier class — native C/C++/rclcpp realtime
and the EDF-deadline cells (all use `nros_board_native_run_tiers` →
`open_over_session`).

**Still open (likely SEPARATE roots, not this fix):** the embedded C
pubsub/action/service cells (ThreadX/NuttX single-tier, no borrowed executor →
this bug does not apply) and `cpp_c_param_live_read_e2e` (single-node param, no
tiers). Re-run the confirmed set; the tier + EDF rows should flip green, and
whatever remains is a distinct root to chase next.
