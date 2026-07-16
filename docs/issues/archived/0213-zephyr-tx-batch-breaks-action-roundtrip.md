---
id: 213
title: "zephyr↔zephyr action roundtrip never completes with TX batching on — blocks the phase-282/290 zephyr default flip"
status: resolved
resolved_in: "2026-07-16 — fork 01db9155: declarations always bypass the tx batch; zephyr defaults-on suite 46/46"

type: bug
area: zpico
related: [issue-0145, phase-282, phase-290, rfc-0049]
---

## Problem

With `CONFIG_NROS_ZENOH_TX_BATCH=y` (pristine-configured images), both
`test_zephyr_c_action_server_to_client_e2e` and
`test_zephyr_cpp_action_server_to_client_e2e` fail **deterministically**
(3/3 retries): the client prints `Sending goal` and never completes; the
suite's other 44 lanes — pubsub, service, workspace entries, realtime tiers,
interop — pass on the same batched images.

Bisect: **batch alone reproduces** (`TX_SPLIT_LOCK=n` on the action pair,
same failure) — the split-lock steal path is not required.

## Why this was never seen

phase-282 validated batching with benches (streaming/paced) and the pubsub
lanes; the action lanes were never exercised with the knob on (it has been
default-off since phase-279). First-exercise latent break, the same shape as
half this month's findings.

## Suspects

- The phase-279 batching claim "service/query requests and replies bypass
  the batch (express)" — verify the action protocol's messages actually all
  take the express path. Actions mix `z_get` queries (goal/result) with
  publications (feedback, status): if the goal query's *declaration* or the
  server's reply publication rides the batch while the counterpart expects
  it within a handshake window, the exchange can deadlock rather than just
  lag.
- Client hangs at the FIRST step (`Sending goal`, no result within 60 s
  incl. #153-style retries) — so the goal request or its reply is lost, not
  the feedback stream. Server-side output was not captured in the failing
  runs; capture it first (the test only dumps client output on panic).
- zephyr↔zephyr both-sides-batched is the unique topology here (native↔
  zephyr pubsub lanes pass; the service lane passes with a zephyr server —
  compare what the SERVICE roundtrip does differently from the ACTION one
  under batching: service = single query/reply; action = query + sub +
  status pub + result query).

## Repro

```sh
# flip the defaults back on (or set in the pair's prj.conf):
#   CONFIG_NROS_ZENOH_TX_BATCH=y
rm -rf zephyr-workspace/build-c-action-{server,client}-zenoh
just zephyr build-fixtures     # pristine configure — .config is STICKY,
                               # a reconfigure without the wipe keeps the
                               # old value and silently tests nothing
cargo nextest run -p nros-tests --test zephyr -E 'test(c_action_server_to_client)'
```

## Impact

Blocks phase-290 W5 (the phase-282 promotion, option C): the zephyr
platform-toml `[knobs.zenoh.tx]` flip and the mirrored `zephyr/Kconfig`
defaults are REVERTED to off until this is fixed. All phase-290 machinery
(ladder, tri-state Kconfig forward, drift test) is in place — once this
issue closes, the flip is: re-add the three knob lines in
`packages/platforms/zephyr/nros-platform.toml`, set the two Kconfig
defaults to `y`, pristine-rebuild, re-run the zephyr suite + phase-282
benches.

## Ops note (cost of finding this)

Zephyr `.config` is sticky: Kconfig **default** changes only apply on
pristine configure — `just zephyr build-fixtures` over existing build dirs
keeps the old values and the mtime/content staleness gate then correctly
reports the binaries stale while the driver considers the leaves current.
Wipe `zephyr-workspace/build-*` when changing Kconfig defaults.

## RESOLVED (2026-07-16) — batched declarations outran the readiness banner

Router-debug timeline (`ZENOHD_LOG=zenoh=debug` through the harness run)
nailed it:

```
46.877  Face{3} Route query .../send_goal/...  → "no matching queryables"
47.029  Face{2} Declare queryable .../send_goal/...   (152 ms LATER)
```

Under batching, the action server's `z_declare_queryable` returns after
QUEUEING the declaration; the app then prints its readiness banner, the
harness launches the client on that banner, and the client's goal query —
express, so it hits the wire immediately — arrives at the router BEFORE
the server's still-batched declaration flushes. The C action client has
no #153-style retry, so the miss is terminal. Manual repros with sleeps
always passed (declares flushed long before), which is why the failure
looked harness-specific.

Fix (fork `01db9155`, gitlink bumped): `_z_transport_tx_get_express_status`
returns `true` for `_Z_N_DECLARE` — declarations (subscriber / queryable /
liveliness tokens) ALWAYS bypass the batch, same treatment as requests and
replies. Control-plane messages are rare and tiny; batching them buys no
throughput and breaks readiness/discovery ordering for every consumer.

Verified: both action lanes pass in ~6 s on batched images; full zephyr
suite 46/46 on PRISTINE batch+split defaults-on images (the phase-290 W5
flip, now live); drift test green.

Red herrings recorded: an unseeded manual pair reproduced a DIFFERENT
failure (identical ZIDs from native_sim's test entropy → the router
refuses to route the query; the harness always seeds, so this never
affects tests — but remember `--seed` in manual native_sim repros, the
#157 lesson again).
