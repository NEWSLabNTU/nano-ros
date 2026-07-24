# Phase 302 — tier-knob honesty: caps truthing + fail-loud drops

**Status (2026-07-25): Draft.** Fixes the implementation-completeness
audit findings (issues 0261–0265; 0266 recorded, not scheduled): every
declared tier knob either reaches the kernel, backfills loudly, or is
rejected at bake — and every L1 "Native" record is true. Companion to the
296-filed 0259/0260 (realizer derivation + accept-path fixtures), which
stay with that line of work.

**Rule (from RFC-0052, now enforced):** an unconsumed `Some(..)` tier
knob is a bug. The valid states are: kernel-applied (marker-gated),
executor-backfill (recorded as such), bake-time reject (with the reason),
or boot-time loud fallback (config-dependent knobs).

## Waves

### W1 — posix caps truthing (0261)

`sched_caps_for("posix")` → `edf/reservation/affinity: false` until real
consumers exist; realizer records become Backfill/Degrade (accurate).
Adjust any test pinning the old Native records. NOTE: touches
`rtos_realizer.rs` — coordinate with the 296 session (0259 targets the
same file); this wave lands LAST if 296 is still in flight.

### W2 — fail-loud the silent drops (0262)

- threadx `core`: bake-time reject ("no SMP core consumer on threadx")
  until `tx_thread_smp_core_exclude` lands; drop the caps `affinity:
  true` claim.
- zephyr `stack_bytes`: implement (size the shim pool slots from the
  tier table or add a stack param to `nros_zephyr_tier_task_create`) OR
  bake-time reject; implementing is preferred — the pool is already
  static, only the slot size is fixed.
- posix `priority`/`stack_bytes`/`core`: bake-time NOTE (advisory
  platform) + consume stack_bytes via `Builder::stack_size` (trivial),
  reject `core`.
- freertos uniproc pin: loud boot-time fallback print (the phase-296
  acknowledged follow-up).

### W3 — nuttx Rust arm priority (0263)

`pthread_setschedparam` at tier-thread entry (existing extern shims),
marker print, extend the W5.x-style e2e to assert it on the rust cell.

### W4 — diagnostics + dead knob (0264, 0265)

- Tierless targets: early bake/macro error "target <deploy> does not
  support multi-tier execution" instead of MissingRtosSpec("posix") /
  missing-method fallout.
- `sched_class`: bake-time reject until a consumer exists (kill the
  silent dead end); revisit as implement when phase-162 lands.

### W5 — verification + housekeeping

Per-wave marker e2e where kernel-visible; bake-reject unit tests in
orchestration-ir; `just check` + affected fixture families + the
realtime lanes. Resolve + archive 0261–0265.

Housekeeping folded in (2026-07-25): the stale-CLI mtime loop —
`setup-cli` now touches the binary after a successful build, because a
pull/rebase bumps cli source mtimes without changing content, cargo
skips the relink, and both mtime-based guards (setup-cli's scan +
cargo.sh's #197 gate) then flag the CLI stale forever until a manual
`touch`. Bit four fixture-lane runs in one session before the fix.

## Non-goals

- 0266 time-slicing knob (demand-driven enhancement).
- 0259/0260 (realizer derivation + SMP accept-path fixtures — 296 line).
- posix native SCHED_DEADLINE consumers (phase-162).
