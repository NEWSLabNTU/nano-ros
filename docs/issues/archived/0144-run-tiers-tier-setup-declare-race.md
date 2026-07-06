---
id: 144
title: "`run_tiers` with ≥3 tiers: spawned tiers' setups still race the zenoh-pico interest write filter — a losing tier's publishers go silent"
status: resolved
type: tech-debt
area: rmw-zenoh
related: [phase-276, rfc-0015]
resolved_in: "phase-144-tier-setup-serialize (2026-07-06)"
---

## Resolution

Fixed by **chained spawn** across all three embedded `run_tiers` paths
(`ZephyrBoard::run_tiers` — `nros-board-zephyr/src/entry_tiers.rs`; the
FreeRTOS Rust `run_tiers_entry` — `nros-board-freertos/src/entry.rs`; the
FreeRTOS C `nros_board_freertos_run_tiers` — `nros-board-freertos/c/freertos_run_tiers.c`).

Instead of the boot tier spawning all of `tiers[1..]` in a loop, each tier's
**setup completion triggers the next spawn**: boot runs `setup(tiers[0])`,
spawns one task for `tiers[1]` (carrying `rest = tiers[2..]`), then spins;
each spawned tier task runs its setup, spawns one task for `rest[0]` (carrying
`rest[1..]`), then spins. Setup order becomes total (boot, t1, t2, …, tN), so
no two entity-declare bursts ever run concurrently on the shared session — the
interest-handshake race that silently closed a losing publisher's write filter
cannot occur, for any tier count. Spins still overlap the next tier's setup,
which is safe (a spin exchanges keepalives/data, not declares).

The FreeRTOS Rust and C paths additionally had the **boot↔tier** race (they
spawned all non-boot tiers BEFORE the boot setup); the reorder to boot-setup-
first is inherent in the chained shape. Native (`nros-board-posix`,
`nros-cpp`) was never affected — it runs `Z_FEATURE_MULTI_THREAD=1`, whose
internal session mutexes serialize concurrent declares — and was left
unchanged.

## Regression

`ws-realtime-cpp-mps2` extended from 2 to 3 tiers with a new `aux` mid tier
(FreeRTOS priority 3, 50 ms) bound to a node spawned **by a spawned tier** —
the exact tier↔tier position the old loop-spawn raced. Proven under QEMU
(`realtime_tiers_cpp_freertos_all_three_tiers_publish`): all three tiers tick
(`[ctrl]`/`[aux]`/`[telem]` at ratios matching 10/50/100 ms), so `aux`'s write
filter opened — impossible under the pre-fix race, which would have left the
mid tier silent. The declare-race note in
`docs/reference/platform-implementation-notes.md` is updated to describe the
chained-spawn resolution.

## Review follow-ups (adversarial pass on the error paths)

- **Leak fixed**: the Zephyr `spawn_next_tier` dropped its `TierTaskCtx` box on
  a failed `nros_zephyr_tier_task_create` (pool exhausted) — the task never
  runs so `Box::from_raw` never reclaimed it. Now reclaimed on the failure
  branch (the FreeRTOS Rust + C paths already freed correctly).
- **Fault-isolation tradeoff (intentional)**: because a tier spawns the next
  only AFTER its own setup returns, a tier whose open/setup FAILS halts the
  chain — its downstream tiers never start. This is inherent to serializing
  the declares (the alternative, spawning all tiers up front, IS the race).
  It is a change from the pre-#144 loop-spawn where tiers came up
  independently; each path now says so loudly (Zephyr/FreeRTOS-Rust log the
  skipped-downstream count; the C idle branches carry the rationale in
  comments) rather than leaving downstream tiers silently absent. A tier that
  can't declare its baked-config entities means a degraded deploy regardless.
