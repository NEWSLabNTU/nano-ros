---
id: 144
title: "`run_tiers` with ≥3 tiers: spawned tiers' setups still race the zenoh-pico interest write filter — a losing tier's publishers go silent"
status: open
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
