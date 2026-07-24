---
id: 247
title: "realtime_tiers_e2e threadx_linux_rust: /ctrl counter 0 on a fresh image (pre-existing; baseline-verified)"
status: resolved
type: bug
severity: medium
area: threadx
related: [issue-0246]
---

## Finding (2026-07-24, during the phase-296 W5.10 preempt-threshold work)

`realtime_tiers_e2e::case_15_threadx_linux_rust` FAILS (~8.7 s):
"high-tier /ctrl counter 0 is not ≥3× the low-tier /telem counter" — the
spawned `high` (ctrl, 10 ms) tier publishes NOTHING while the boot `low`
(telem) tier delivers.

**Baseline-verified pre-existing:** with the W5.10 changes stashed
(threshold declaration + board markers) and the fixture lane rebuilt from
clean tree, the cell fails identically. Not the preempt-threshold work.

Notes:
- The phase-297 W5 note says a boot-reprioritize fix landed for exactly
  this starvation shape (app@4 starving high@5) — either it regressed, is
  incomplete on a fresh rebuild, or the phase-297 agent's work is still in
  flight. Coordinate with phase-297 before debugging independently.
- The W5.10 marker e2e (`threadx_preempt_threshold_applied`) PASSES on the
  same image — bring-up + the `low` boot tier work; specifically the
  SPAWNED high tier's publish path is dead (#246/#245 family: check
  executor-storage sizing + the chain-spawn path before assuming a race).

## Debugging session 2 (2026-07-24) — substantially narrowed

Instrumented the ctrl component (temp `log::info` on tick) + manual boots
with router + sinks:

- **The ctrl timer FIRES at full rate** (~100 Hz, counter monotonic) on the
  spawned `high` tier, and `publish_to_topic` returns **Ok** every tick —
  yet the host sink on `/ctrl` receives ZERO. `/telem` (boot tier, same
  session) delivers at exactly its rate simultaneously.
- So the failure is WIRE-SIDE, silent: puts accepted, nothing leaves (or
  nothing routable leaves) for that publisher.
- `Z_FEATURE_MULTI_THREAD` is **1 and effective in the library** (the
  platform manifest `defines_kv` reaches the unified builder; `_z_mutex_*`
  symbols linked in the image) — the earlier "single-threaded zenoh raced
  by two tiers" theory is DEAD. (The generated header's `#ifndef` fallback
  0 is cosmetic; the -D wins.)
- Prime suspect: the **per-publisher interest-based write filter never
  opens** for publishers declared on the SPAWNED tier — zenoh-pico
  short-circuits puts (returns OK) when its filter says no matching
  subscriber. The boot tier's publisher (telem) opens fine; the spawned
  tier declares later and its filter state may never see the router's
  subscriber-interest reply (reply consumed/mis-associated when BOTH tier
  threads drive `zp_read` via the ThreadX select arm?). The #144 comment
  documents exactly this failure shape ("the losing publisher's write
  filter stays closed and every put is silently dropped").
- ZENOH_DEBUG=3 via the platform manifest produced no extra output —
  tracing needs a different hook (zenoh-pico log sink on threadx).

**Next:** trace `_z_write_filter` ctx state for the ctrl publisher
(gdb-from-start with a breakpoint on `_z_write_filter_callback` /
`_z_trigger_interest`, or a temp printf in filtering.c), and compare the
interest IDs in the router's debug log against the two publishers. If
confirmed, the fix likely belongs in the zpico threadx spin arm (interest
replies must be processed before the spawned tier's declare completes —
extend the #144 serialization to cover filter-open) or in zenoh-pico's
filter/interest association under concurrent readers.

## Debugging session 3 (2026-07-24) — router traces + a partial fix

gdb (with `handle SIGUSR1 nostop pass` — the ThreadX-Linux port schedules
via SIGUSR1, gdb must pass it) + `RUST_LOG=zenoh=trace` on the router:

- The guest registers TWO interests (one per publisher write-filter);
  `_z_add_interest` REGISTERS BEFORE SENDING (verified in primitives.c:579)
  so a reply cannot beat the table insert.
- The ROUTER receives both interests and correctly schedules the replies:
  `DeclareFinal(3)` for telem, then `DeclareSubscriber + DeclareFinal(7)`
  for ctrl (the matching remote subscriber!), 2.6 ms apart, on the one TCP
  link.
- The guest processes ONLY `Final(3)`. The interest-7 batch is consumed at
  the transport but never reaches `_z_interest_process_declares` — the
  ctrl filter stays unopened, puts short-circuit Ok.
- Transport rx is otherwise alive (keepalives maintain the lease across
  15 s runs; tx flows continuously).

**Partial fix landed:** `zpico.c`'s ThreadX arm now serializes `zp_read`
across tier threads (`g_threadx_read_mutex`, TRY-lock so a losing spinner
skips the round) — two concurrent readers on one TCP stream split message
reassembly and is wrong regardless. Verified compiled+linked; the cell
STILL fails identically, so a second defect remains in the concurrent
rx/tx handling on this arm.

**Next:** instrument `_z_handle_network_message` / the unicast rx path to
see where the interest-7 batch dies (count messages handed up vs bytes
read; suspect partial-read state or the rx zbuf being reset between the
Final(3) batch and the next), and compare against a single-tier threadx
image (talker) where filters open fine.

## Repro

```
bash scripts/build/workspace-fixtures-build.sh threadx-linux rust
cargo nextest run -p nros-tests -E 'test(threadx_linux_rust)'
```

## Resolution (2026-07-24, phase-297 session) — root cause was in flight, now landed

The wire-silent /ctrl was root-caused independently by the phase-297 W5
session BEFORE this issue was filed; the fix chain was local at filing time
and pushed 2026-07-24:

1. **Frame loss in zenoh-pico's polled read (the actual killer):**
   `_zp_unicast_read(single_read=false)` resets the rx zbuf each poll and
   processed only the FIRST stream frame a recv pulled in — the interest-7
   reply (DeclareSubscriber + DeclareFinal for the ctrl publisher) rode the
   same TCP burst as interest-3's and was discarded on the next poll's
   reset. Exactly matches this issue's session-3 trace ("the guest consumes
   but never processes the interest-7 batch"). Fixed in the vendored fork:
   zenoh-pico `87f7a84d` (drain every buffered frame per poll).
2. **Reader serialization:** this issue's `g_threadx_read_mutex` TRY-lock
   (e24fa4f1d) and the phase-297 session's atomic-flag guard landed as a
   redundant DOUBLE guard in the same spin arm; deduplicated to the mutex
   version (the atomic flag removed).
3. Same-family fixes already in: ULONG pointer truncation, boot-tier
   priority adoption, z_sleep-not-select yield (see archived phase-297 doc).

Verified: `realtime_tiers_e2e::case_15_threadx_linux_rust` PASSES on the
merged tree (fresh CLI + fixture rebuild after syncing the ros-launch-
manifest submodule). If it reds again on another host, FIRST check the
zenoh-pico submodule is at ≥ `87f7a84d` and the fixture was rebuilt after.
