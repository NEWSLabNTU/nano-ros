# Zephyr Native EDF Slice — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the RTOS realizer's `deadline_real = Native` claim TRUE end-to-end on Zephyr — the runtime actually calls `k_thread_deadline_set` for a real-time tier that carries a deadline — or degrade honestly, driven by a single `edf` capability knob.

**Architecture:** Three seams. (1) Host — the realizer's per-board `SchedCaps.edf` is sourced from a per-deploy `edf` knob (`Deploy.extra`), so its degradation record is accurate against the image instead of a hardcoded per-platform guess. (2) Runtime — `nros-board-zephyr`'s `run_tiers` calls a new `cfg`-gated C shim `nros_zephyr_set_current_deadline` (→ `k_thread_deadline_set`) for boot + spawned tiers whose class is real-time and that carry a deadline, when the `zephyr-edf` cargo feature is on; the executor's cooperative `SchedContext` deadline monitor stays the miss-handler in both the Native and Backfill cases. (3) A Zephyr QEMU fixture with two equal-priority deadline tiers proves the shim fires (trace marker), built against an authored `prj.conf` that sets `CONFIG_SCHED_DEADLINE=y`.

**Tech Stack:** Rust (edition 2024, `no_std` board crate), C (Zephyr kernel shim), Zephyr `CONFIG_SCHED_DEADLINE`, QEMU (`qemu_cortex_m3`/existing zephyr harness), `cargo nextest`, the vendored `ros-launch-manifest` `model` + `sched` crates.

## Global Constraints

- **Runtime EDF trigger** is `class == Some("real_time") && deadline_us.is_some()`. `TierSpec` carries NO `sched_class` field — do not invent one; the board decides EDF from `class` + `deadline_us`.
- **Zephyr priority direction:** numerically LOWER = higher urgency; negatives = cooperative. `CONFIG_SCHED_DEADLINE` orders **equal-priority** threads by earliest deadline (a tiebreak, not a global EDF) — matches one-`k_thread`-per-tier.
- **`k_thread_deadline_set` takes CYCLES**, not µs — convert with `k_us_to_cyc_near32(us)`.
- **`prj.conf` is authored per-example**, not codegen-emitted (the CLI only knows a `prj_conf` PATH). The fixture's `prj.conf` carries `CONFIG_SCHED_DEADLINE=y` directly. Auto-emitting `CONFIG_SCHED_DEADLINE` from the deploy knob is an explicit NON-GOAL (follow-up).
- **No compilation inside tests** — the fixture builds the artifact in the build stage; the test consumes the prebuilt ELF. "Does it compile with EDF?" is the fixture build, not a runtime test.
- **Test greps use `nros_tests::output::*` constants, never literal strings.** The board (no_std) emits a literal log line; the matching constant lives in `nros-tests` and its doc-comment MUST state it mirrors that literal.
- **Edition 2024 FFI:** `unsafe extern "C" { … }` blocks; call sites in `unsafe { … }`.
- **`cargo +nightly fmt` before committing** (nightly-only rustfmt options). `just check` (or at least `check-cli-tests` for host tasks) green locally before push.
- **The Zephyr board crate has NO host cargo path** (`cargo check -p nros-board-zephyr` errors "outside of workspace") — its Rust compiles only under the Zephyr build. Task 2's compile is verified by Task 3's fixture build, not a host `cargo` command.
- **Commit trailers (every commit):**
  ```
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_01SwjKNujon1qUVmwC7yhRJQ
  ```
- **Tracked by** phase-296 W5.5 + RFC-0052 §"CAPS provenance". This plan implements that wave.

---

### Task 1: Host — `SchedCaps.edf` sourced from the deploy `edf` knob (CAPS honesty)

Makes the realizer's degradation record accurate: `edf` comes from `Deploy.extra["edf"]` (overriding the per-platform default), so a Zephyr deploy with `edf = false` records an accurate `Degrade`, and the CLI never claims Native for an image that won't have `CONFIG_SCHED_DEADLINE`.

**Files:**
- Modify: `packages/cli/nros-cli-core/src/orchestration/rtos_realizer.rs` (add `sched_caps_from_deploy`, next to `sched_caps_for` ~line 113; tests appended to the existing `#[cfg(test)] mod tests` ~line 399)

**Interfaces:**
- Consumes: `ros_launch_manifest_model::{Deploy, ExtraValue}` (already a dep — `ros-launch-manifest-model`), existing `SchedCaps` + `sched_caps_for(target: &str) -> SchedCaps`.
- Produces: `pub fn sched_caps_from_deploy(target: &str, deploy: Option<&Deploy>) -> SchedCaps` — starts from `sched_caps_for(target)`, then if `deploy`'s `extra["edf"]` is an `ExtraValue::Bool(b)`, sets `.edf = b`. Later bake-wiring (out of this slice) calls this instead of `sched_caps_for`.

- [ ] **Step 1: Write the failing test**

Append to `mod tests` in `rtos_realizer.rs`:

```rust
use ros_launch_manifest_model::{Deploy, ExtraValue, Target};

fn zephyr_deploy_with_edf(edf: bool) -> Deploy {
    let mut extra = std::collections::BTreeMap::new();
    extra.insert("edf".to_string(), ExtraValue::Bool(edf));
    Deploy {
        target: Target::default(),
        extra,
        ..Default::default()
    }
}

#[test]
fn deploy_knob_overrides_platform_edf_default() {
    // Platform default for zephyr is edf = true.
    assert!(sched_caps_for("zephyr").edf);
    // A deploy that turns edf OFF must be honored.
    let caps = sched_caps_from_deploy("zephyr", Some(&zephyr_deploy_with_edf(false)));
    assert!(!caps.edf, "deploy edf=false must override the platform default");
    // A deploy with no edf key falls back to the platform default.
    let caps_default = sched_caps_from_deploy("zephyr", None);
    assert!(caps_default.edf, "no knob → platform default (true for zephyr)");
}

#[test]
fn deploy_edf_false_produces_accurate_degrade() {
    // The honesty property: edf=false → realize_rtos records a deadline Degrade.
    let input = input_two();
    let ranked = chain_aware_rank(&input);
    let caps = sched_caps_from_deploy("zephyr", Some(&zephyr_deploy_with_edf(false)));
    let plan = realize_rtos(&ranked, &input, &caps);
    let hi = plan.nodes.iter().find(|n| n.name == "/hi").unwrap();
    assert!(matches!(hi.deadline_real, DimRealization::Degrade { .. }));
    assert!(plan.degradations.iter().any(|d| d.node == "/hi" && d.dim == "deadline"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path packages/cli/Cargo.toml -p nros-cli-core sched_caps_from_deploy -- --nocapture`
Expected: FAIL — `cannot find function sched_caps_from_deploy` (and the `ExtraValue`/`Target` import may not resolve until Step 3).

- [ ] **Step 3: Write minimal implementation**

Add just below `sched_caps_for` (~line 174) in `rtos_realizer.rs`:

```rust
/// [`SchedCaps`] for a target, with the per-deploy `edf` capability knob
/// applied. The knob is the bake-authoritative SSoT (RFC-0052 §"CAPS
/// provenance"): a `[deploy.<board>] edf = <bool>` in the deploy config
/// (carried on `Deploy.extra`) OVERRIDES the platform default, so the
/// realizer's `Native`-vs-`Degrade` decision is accurate against the image
/// that will actually be built. Absent knob → the platform default stands.
pub fn sched_caps_from_deploy(
    target: &str,
    deploy: Option<&ros_launch_manifest_model::Deploy>,
) -> SchedCaps {
    let mut caps = sched_caps_for(target);
    if let Some(d) = deploy {
        if let Some(ros_launch_manifest_model::ExtraValue::Bool(b)) = d.extra.get("edf") {
            caps.edf = *b;
        }
    }
    caps
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path packages/cli/Cargo.toml -p nros-cli-core sched_caps_from_deploy deploy_edf_false_produces_accurate_degrade -- --nocapture`
Expected: PASS (2 tests). If `Target`/`ExtraValue` import path differs, fix the `use` to the real re-export (grep `pub use` / `pub enum ExtraValue` in `ros-launch-manifest/model/src/lib.rs`).

- [ ] **Step 5: Format + commit**

```bash
cd /home/aeon/repos/nano-ros
cargo +nightly fmt --manifest-path packages/cli/Cargo.toml -p nros-cli-core
git add packages/cli/nros-cli-core/src/orchestration/rtos_realizer.rs
git commit -m "feat(296-W5.5): SchedCaps.edf from the deploy edf knob (CAPS honesty)

The realizer's edf capability now comes from a per-deploy edf knob
(Deploy.extra['edf']) overriding the platform default, so a Zephyr deploy
with edf=false records an accurate deadline Degrade instead of the CLI
claiming Native for an image that will lack CONFIG_SCHED_DEADLINE.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SwjKNujon1qUVmwC7yhRJQ"
```

---

### Task 2: Runtime — Zephyr `set_current_deadline` shim + `zephyr-edf` feature + `run_tiers` wiring

Adds the kernel-native deadline application. Boot tier and spawned tiers each self-set their deadline (like they self-adopt priority) when the tier is real-time-with-a-deadline and the `zephyr-edf` feature is on. Emits a trace line the QEMU e2e (Task 4) greps.

**Files:**
- Modify: `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c` (add the C shim near the existing priority shim)
- Modify: `packages/boards/nros-board-zephyr/Cargo.toml:20` (`[features]` — add `zephyr-edf`)
- Modify: `packages/boards/nros-board-zephyr/src/entry_tiers.rs` (extern decl ~line 45; spawned-tier call in `tier_task_entry` after `apply_tier_sched_policy` ~line 133; boot-tier call after `nros_zephyr_set_current_priority` ~line 246)
- Modify: `packages/testing/nros-tests/src/output.rs` (add the marker constant)

**Interfaces:**
- Consumes: existing `nros_zephyr_set_current_priority` pattern, `TierSpec { class: Option<&str>, deadline_us: Option<u64>, name, priority, … }`.
- Produces: C symbol `void nros_zephyr_set_current_deadline(unsigned int deadline_us)`; Rust helper `fn apply_tier_deadline(tier: &TierSpec)` (private, in `entry_tiers.rs`) that emits the marker + calls the shim under `#[cfg(feature = "zephyr-edf")]`; `nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER`.

- [ ] **Step 1: Add the C shim (gated on `CONFIG_SCHED_DEADLINE`)**

In `packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c`, next to the priority shim:

```c
/* phase-296 W5.5 — apply a per-thread earliest-deadline on the CALLING
 * thread. `k_thread_deadline_set` takes CYCLES; convert from us. Compiled
 * to a no-op when the kernel lacks EDF so the image still links; the Rust
 * side additionally gates the CALL behind the `zephyr-edf` feature, so a
 * no-op here means an honest fall-through to the executor's cooperative
 * deadline monitor. */
void nros_zephyr_set_current_deadline(unsigned int deadline_us) {
#ifdef CONFIG_SCHED_DEADLINE
    k_thread_deadline_set(k_current_get(), (int)k_us_to_cyc_near32(deadline_us));
#else
    (void)deadline_us;
#endif
}
```

- [ ] **Step 2: Add the `zephyr-edf` cargo feature**

In `packages/boards/nros-board-zephyr/Cargo.toml`, under `[features]`:

```toml
# phase-296 W5.5 — gate the k_thread_deadline_set call path. On ⇒ real-time
# tiers with a deadline get kernel EDF ordering; off ⇒ executor-only
# (cooperative) deadline monitor. Must match the image's CONFIG_SCHED_DEADLINE.
zephyr-edf = []
```

- [ ] **Step 3: Add the trace marker constant**

In `packages/testing/nros-tests/src/output.rs`:

```rust
/// Emitted by `nros-board-zephyr`'s `run_tiers` when a real-time tier's
/// kernel EDF deadline is applied (phase-296 W5.5). MIRRORS the literal
/// `::log::info!` prefix in `nros-board-zephyr/src/entry_tiers.rs`
/// (`apply_tier_deadline`) — keep the two in lockstep (the no_std board
/// crate cannot depend on this crate).
pub const ZEPHYR_EDF_DEADLINE_MARKER: &str = "nros: EDF deadline set tier=";
```

- [ ] **Step 4: Wire the extern + helper + both call sites in `entry_tiers.rs`**

Add to the `unsafe extern "C"` block (after `nros_zephyr_set_current_priority`, ~line 45):

```rust
    /// phase-296 W5.5 — apply an earliest-deadline (µs) on the CALLING
    /// thread via `k_thread_deadline_set`. No-op when the image lacks
    /// `CONFIG_SCHED_DEADLINE`.
    fn nros_zephyr_set_current_deadline(deadline_us: u32);
```

Add a private helper (near the other free fns in the module):

```rust
/// Apply this tier's kernel EDF deadline on the CALLING thread, when the
/// tier is real-time and carries a deadline. Gated by the `zephyr-edf`
/// feature; off ⇒ the executor's cooperative `SchedContext` deadline
/// monitor is the sole enforcement (an honest Backfill). The `::log::info!`
/// literal MUST match `nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER`.
#[cfg(feature = "zephyr-edf")]
fn apply_tier_deadline(tier: &::nros::node_runtime::TierSpec<'_>) {
    if tier.class == Some("real_time") {
        if let Some(us) = tier.deadline_us {
            let us = us.min(u32::MAX as u64) as u32;
            unsafe { nros_zephyr_set_current_deadline(us) };
            ::log::info!("nros: EDF deadline set tier=`{}` {}us", tier.name, us);
        }
    }
}

#[cfg(not(feature = "zephyr-edf"))]
#[inline]
fn apply_tier_deadline(_tier: &::nros::node_runtime::TierSpec<'_>) {}
```

(If `TierSpec`'s import path in this file differs, use the same path the existing `apply_tier_sched_policy` call reaches it by — grep `TierSpec` in `entry_tiers.rs`.)

Call it on the SPAWNED thread, in `tier_task_entry`, immediately after the existing `crt.apply_tier_sched_policy(...)` (~line 133):

```rust
    apply_tier_deadline(&ctx.tier);
```

Call it on the BOOT thread, immediately after the `nros_zephyr_set_current_priority(...)` unsafe block (~line 246):

```rust
        apply_tier_deadline(boot_tier);
```

- [ ] **Step 5: Format**

```bash
cd /home/aeon/repos/nano-ros
cargo +nightly fmt --manifest-path packages/cli/Cargo.toml 2>/dev/null || true
cargo +nightly fmt -p nros-tests -p nros-board-zephyr
```

- [ ] **Step 6: Verify (deferred to Task 3) + commit**

The Zephyr board crate has no host cargo path, so its compile is proven by the Task 3 fixture build. Verify only formatting + the marker mirror here:

Run: `grep -n "EDF deadline set tier=" packages/boards/nros-board-zephyr/src/entry_tiers.rs packages/testing/nros-tests/src/output.rs`
Expected: the literal appears in BOTH files (the mirror invariant).

```bash
git add packages/boards/nros-board-zephyr/c/zephyr_run_tiers.c \
        packages/boards/nros-board-zephyr/Cargo.toml \
        packages/boards/nros-board-zephyr/src/entry_tiers.rs \
        packages/testing/nros-tests/src/output.rs
git commit -m "feat(296-W5.5): Zephyr k_thread_deadline_set shim + zephyr-edf feature + run_tiers wiring

Boot + spawned tiers self-apply a kernel EDF deadline (k_thread_deadline_set,
us→cycles) when the tier is real_time with a deadline and the zephyr-edf
feature is on; emits a trace marker the QEMU e2e greps. Feature off / no
CONFIG_SCHED_DEADLINE ⇒ the executor's cooperative deadline monitor is the
sole enforcement (honest Backfill). Compile verified via the W5.5 fixture.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SwjKNujon1qUVmwC7yhRJQ"
```

---

### Task 3: Build fixture — Zephyr QEMU image with two equal-priority deadline tiers + EDF on

An authored fixture example that exercises the Task 2 path: two real-time tiers at the SAME Zephyr priority, each with a distinct `deadline_us`, `CONFIG_SCHED_DEADLINE=y` in `prj.conf`, and the `zephyr-edf` feature enabled. Its build IS the compile proof for Task 2.

**Files:**
- Create: `packages/testing/nros-tests/bins/zephyr-edf/` (a Zephyr QEMU app — mirror the nearest existing zephyr tier fixture; find it via `find examples/zephyr packages/testing -name prj.conf` and copy the closest multi-tier one)
- Create: `packages/testing/nros-tests/bins/zephyr-edf/prj.conf` (with `CONFIG_SCHED_DEADLINE=y`)
- Modify: the fixture's `Cargo.toml` / `CMakeLists.txt` to enable `nros-board-zephyr/zephyr-edf`
- Modify: `examples/fixtures.toml` (register the fixture so `just build-test-fixtures` stages it)

**Interfaces:**
- Consumes: Task 2's `zephyr-edf` feature + `nros_zephyr_set_current_deadline`.
- Produces: a prebuilt ELF the Task 4 e2e boots; a `system.toml` (or `main!` tier table) declaring two `real_time` tiers at equal priority with `deadline_us` set.

- [ ] **Step 1: Scaffold the fixture from the nearest zephyr multi-tier example**

Run: `find examples/zephyr packages/testing -name "*.rs" -path "*zephyr*" | xargs grep -l "run_tiers\|tiers" 2>/dev/null | head`
Copy the closest multi-tier zephyr fixture into `packages/testing/nros-tests/bins/zephyr-edf/`. Set two tiers in its tier table (system.toml or `main!`), both `[tiers.<n>.zephyr] priority = 5`, `class = "real_time"`, distinct `deadline_us` (e.g. 10000 and 20000).

- [ ] **Step 2: Author `prj.conf` with EDF on**

In `packages/testing/nros-tests/bins/zephyr-edf/prj.conf`, ensure (append if copying):

```
CONFIG_SCHED_DEADLINE=y
```

- [ ] **Step 3: Enable the `zephyr-edf` feature in the fixture build**

In the fixture's `Cargo.toml`, on the `nros-board-zephyr` dependency:

```toml
nros-board-zephyr = { path = "../../../../boards/nros-board-zephyr", default-features = false, features = ["tiers", "zephyr-edf"] }
```

(Match the real relative path + the existing dependency form in a sibling zephyr fixture.)

- [ ] **Step 4: Register the fixture**

In `examples/fixtures.toml`, add an entry for `zephyr-edf` mirroring an existing zephyr QEMU fixture entry (same build-step keys: target board `qemu_cortex_m3` or the family's standard, artifact path).

- [ ] **Step 5: Build the fixture (this is Task 2's compile proof)**

Run: `just build-test-fixtures zephyr-edf` (or the family's build recipe; if the fixture builder targets by name, use it; else `just zephyr` scoped build after `source ./activate.sh`).
Expected: the fixture ELF is produced. A build error here that names `nros_zephyr_set_current_deadline` / `k_thread_deadline_set` / `CONFIG_SCHED_DEADLINE` is a Task 2 defect — fix there, rebuild. Confirm `CONFIG_SCHED_DEADLINE` compiled in:

Run: `grep -r "CONFIG_SCHED_DEADLINE" packages/testing/nros-tests/bins/zephyr-edf/build*/zephyr/.config 2>/dev/null`
Expected: `CONFIG_SCHED_DEADLINE=y`.

- [ ] **Step 6: Commit**

```bash
cd /home/aeon/repos/nano-ros
cargo +nightly fmt -p nros-tests 2>/dev/null || true
git add packages/testing/nros-tests/bins/zephyr-edf examples/fixtures.toml
git commit -m "test(296-W5.5): zephyr-edf QEMU fixture — two equal-priority deadline tiers, CONFIG_SCHED_DEADLINE

Two real_time tiers at the same Zephyr priority with distinct deadline_us +
CONFIG_SCHED_DEADLINE=y + the zephyr-edf feature. Its build is the compile
proof for the W5.5 board wiring; Task 4 boots it to confirm the deadline is
applied.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SwjKNujon1qUVmwC7yhRJQ"
```

---

### Task 4: QEMU e2e — confirm the EDF deadline is applied at boot

Boots the Task 3 fixture in QEMU and asserts, via the trace marker, that `k_thread_deadline_set` fired for BOTH real-time deadline tiers — proving the `Native` claim is honored end-to-end (not just recorded on the host).

**Files:**
- Create: `packages/testing/nros-tests/tests/zephyr_edf_deadline_applied.rs`

**Interfaces:**
- Consumes: `nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER`, the existing zephyr QEMU harness (mirror `realtime_tiers_e2e.rs`), the Task 3 fixture ELF.
- Produces: a passing e2e that fails-loud on unmet preconditions.

- [ ] **Step 1: Write the failing test**

Create `packages/testing/nros-tests/tests/zephyr_edf_deadline_applied.rs`, mirroring the harness shape of `realtime_tiers_e2e.rs`:

```rust
//! phase-296 W5.5 — the Zephyr Native EDF claim is honored end-to-end:
//! a real-time tier carrying a deadline gets `k_thread_deadline_set` at
//! boot (trace-confirmed), not merely recorded Native on the host.

use nros_tests::output::ZEPHYR_EDF_DEADLINE_MARKER;

#[test]
fn zephyr_edf_deadline_applied_for_both_tiers() {
    // Boot the prebuilt zephyr-edf fixture in QEMU and capture its log.
    // (Use the same fixture-resolve + qemu-run helper realtime_tiers_e2e.rs
    // uses; nros_tests::skip! if the fixture/emulator is absent — never a
    // bare eprintln+return.)
    let log = nros_tests::fixtures::run_zephyr_qemu("zephyr-edf")
        .unwrap_or_else(|e| nros_tests::skip!("zephyr-edf fixture unavailable: {e}"));

    let hits = log.lines().filter(|l| l.contains(ZEPHYR_EDF_DEADLINE_MARKER)).count();
    assert!(
        hits >= 2,
        "expected k_thread_deadline_set applied for both EDF tiers; \
         saw {hits} `{ZEPHYR_EDF_DEADLINE_MARKER}` line(s) in:\n{log}"
    );
}
```

(Replace `run_zephyr_qemu` with the ACTUAL helper name in `nros_tests::fixtures` used by `realtime_tiers_e2e.rs` — grep it first; keep the `skip!`-on-missing-precondition discipline.)

- [ ] **Step 2: Run to verify it fails (before the fixture is wired / with feature off)**

Run: `cd packages/testing && just zephyr test zephyr_edf_deadline_applied` (or the repo's zephyr e2e recipe; `source ./activate.sh` first).
Expected: FAIL — 0 marker lines (deadline not applied) OR a `skip!` if the fixture didn't build. A real FAIL (0 hits with the fixture present) is the pre-implementation state.

- [ ] **Step 3: (implementation already done in Tasks 2–3) — build + run green**

Run:
```bash
source ./activate.sh
just build-test-fixtures zephyr-edf
just zephyr test zephyr_edf_deadline_applied
```
Expected: PASS — ≥2 `nros: EDF deadline set tier=` lines. If the fixture flakes under a full sweep, retest SOLO (QEMU lanes flake under load — retest a red solo before filing).

- [ ] **Step 4: Cross-check the honest-degrade direction (optional, cheap)**

Temporarily build the same fixture WITHOUT the `zephyr-edf` feature (or with `CONFIG_SCHED_DEADLINE=n`); the marker should NOT appear (executor-only monitor). Revert. This confirms the gate actually gates. Do not commit the reverted state.

- [ ] **Step 5: Commit**

```bash
cd /home/aeon/repos/nano-ros
cargo +nightly fmt -p nros-tests
git add packages/testing/nros-tests/tests/zephyr_edf_deadline_applied.rs
git commit -m "test(296-W5.5): QEMU e2e — Zephyr EDF deadline applied at boot (Native honored)

Boots the zephyr-edf fixture and asserts via the trace marker that
k_thread_deadline_set fired for both real-time deadline tiers — the Native
claim honored end-to-end, closing the plan/runtime gap for the deadline dim.

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_01SwjKNujon1qUVmwC7yhRJQ"
```

---

## Final verification (after all tasks)

- [ ] `source ./activate.sh && just check-cli-tests` — host realizer tests (Task 1) green.
- [ ] `just build-test-fixtures zephyr-edf` — fixture builds with `CONFIG_SCHED_DEADLINE`.
- [ ] `just zephyr test zephyr_edf_deadline_applied` — e2e green (retest solo if it flakes in a sweep).
- [ ] `cargo +nightly fmt --check` clean across touched crates.
- [ ] Update phase-296 W5.5 "Done when" checkboxes; note the CLI bake-wiring of `sched_caps_from_deploy` (calling it from `codegen-system`) + `prj.conf` auto-emit remain the documented follow-ups.

## Non-goals (recorded — do NOT implement here)

- Auto-emitting `CONFIG_SCHED_DEADLINE` from the deploy knob (prj.conf is authored).
- Wiring `realize_rtos`/`sched_caps_from_deploy` into `codegen-system` as the default bake path (the legacy `tier_resolver` still bakes; the runtime honoring works off either path's `TierSpec`).
- Behavioral earliest-deadline-ORDERING proof (two equal-priority tiers, assert the tighter one runs first) — Zephyr's equal-priority tiebreak makes a deterministic QEMU ordering test fiddly.
- The other five dims (`budget` native reservation, `non_preempt_scope`, `placement`, `activation`, `urgency` refinement) and a formal `PlatformSched` Rust trait.
- RTOS-side priority band-scarcity collapse (`rank_to_priority` stays the simple clamp).
