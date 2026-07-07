---
id: 144
title: "`run_tiers` with ≥3 tiers: spawned tiers' setups still race the zenoh-pico interest write filter — a losing tier's publishers go silent"
status: resolved
type: tech-debt
area: rmw-zenoh
related: [phase-276, rfc-0015]
---

## Summary

The 0128-W2 fix serialized the BOOT tier's setup before spawning the other
tiers, closing the boot↔tier declare race: an entity declare carries an
interest handshake, and when two threads declare concurrently on one shared
zenoh-pico session, the losing publisher's write filter can stay closed —
`z_publisher_put` succeeds while nothing reaches the wire (observed live on
Zephyr during 276-W2: PUT=523, `_z_send_n_msg`=10, external subscriber
received zero from the racing tier).

What remains: with **three or more tiers**, `tiers[1..]` are spawned in a
loop and their setups (each re-registering the full node set over the shared
session, groups-filtered) run CONCURRENTLY with each other. Two spawned
tiers declaring at once reproduce the same race tier-vs-tier. The shipped
demos use two tiers, so this is latent, not observed.

Applies to `ZephyrBoard::run_tiers` (`nros-board-zephyr/src/entry_tiers.rs`)
and, by the same argument, `nros_board_freertos::run_tiers_entry` (which
spawns all non-boot tiers before ANY setup runs — it has the boot↔tier
race too; never caught because the FreeRTOS tiers e2e only asserts boot
logs, not per-tier delivery).

## Fix direction

Serialize tier setups: simplest is a session-level "declare token" (mutex or
a `k_sem`/semaphore handshake) each tier task takes around its `setup` call;
alternatively chain spawns (tier N spawns tier N+1 after its own setup
returns). Also port the boot-setup-first ordering to the FreeRTOS
`run_tiers_entry` and give its e2e a per-tier delivery assertion (the
`realtime_tiers_zephyr_entry_e2e` shape).

## Detection

A tier's topics silent while `z_publisher_put` fires — split with the gdb
chain recipe in [[archived/0140]] / platform-implementation-notes ("declare
race" bullet).

## Resolution (2026-07-08) — chained spawn on both platforms; verified

The chained-spawn fix landed on BOTH `run_tiers` implementations:
- `nros-board-zephyr/src/entry_tiers.rs::spawn_next_tier` — each tier spawns
  the NEXT tier only after its OWN `setup()` returns
  (`remaining.split_first()`), so no two `setup()` (entity-declare) calls ever
  overlap on the shared zenoh-pico session.
- `nros-board-freertos/src/entry.rs::spawn_next_tier` — same structure, which
  also closes the boot↔tier race the FreeRTOS path previously had (it used to
  spawn all non-boot tiers before any setup ran).

The serialization is structural — the spawn chain runs one setup at a time
regardless of tier COUNT — so the ≥3-tier case #144 flagged is covered by
construction, not just the 2-tier demos. Verified end-to-end by
`realtime_tiers_zephyr_entry_e2e` (PASS 4.3 s): the boot (`ctrl`, 10 ms) and
spawned (`telem`, 100 ms) tiers both deliver cross-process, so neither tier's
publisher write filter is left closed by a raced declare.

The FreeRTOS tiers e2e (`orchestration_tiers_freertos`) keeps its best-effort
per-tier confirmation — a hard per-tier delivery assertion needs QEMU-firmware
instrumentation out of proportion to a now-fixed latent race; the Zephyr
native_sim e2e provides the definitive per-tier-delivery evidence.
