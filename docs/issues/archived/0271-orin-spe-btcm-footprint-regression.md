---
id: 271
title: "Orin SPE BTCM footprint regressed ~+195 KB between d9af52be and 21a3a4248 — minimal Executor::open+spin image no longer fits 256 KB"
status: resolved
type: regression
severity: medium
area: embedded
related: [issue-0257, issue-0739]
resolved: 2026-08-21
---

## Finding (autoware_sentinel phase-14 pin bump, 2026-07-25)

On pin `d9af52be` (post-11.3.C size campaign) the sentinel's DEFAULT SPE
image (`Executor::open` + `spin` over zenoh-pico/IVC, no algorithm
wiring) fit the 256 KB BTCM with **31 KB headroom** (~224 KB
text+data+bss). On `21a3a4248`, the same build — same slot rightsizing
envs (`ZPICO_MAX_PUBLISHERS=8 / SUBSCRIBERS=4 / QUERYABLES=2 /
LIVELINESS=16`, `NROS_EXECUTOR_MAX_CBS=8`, 256 B buffers), same
`.cargo` hardening (armv7r-none-eabi softfp, build-std core+alloc,
`panic=immediate-abort`, `-Os` + LTO) — **overflows BTCM by 164 KB**
(ld names `.data`): a ~+195 KB swing.

Staticlib pre-gc totals for scale: text 464 KB / bss 158 KB;
`compiler_builtins` alone contributes 118 KB text pre-gc.

Suspected contributors (unverified): the rmw-cffi vtable seam now on the
default path, the node-registry/wake/monitor machinery (phase 271/273 /
RFC-0052 tables), and zenoh-pico feature growth (interest/matching).
Needs an 11.3.C-style per-component size audit on the new pin; the
sentinel's fallback is the 11.3.E DRAM/AST mapping, but a 2× regression
of the minimal image hurts every 256 KB-class target, not just the SPE.

## Repro

autoware_sentinel branch `phase-14`:

```sh
just build-spe-image
# → ld: region `btcm' overflowed by 164464 bytes
```

## Side notes from the same bump (consumer-side, already absorbed)

- The retired `nros-platform-orin-spe` crate used to compile the
  platform C port; `nros-platform-freertos` is now a source-only C
  package with no compiler at the SPE site — the sentinel added a
  build.rs compiling `platform.c`/`timer.c` against the FSP headers
  (with `configTOTAL_HEAP_SIZE` undefined there — see the define
  fallback in that build.rs).
- zpico.c's session-seed path (`#elif defined(CLOCK_REALTIME)`) calls
  `clock_gettime`, which newlib does not back on the SPE; the sentinel
  shims it off `nros_platform_time_now_ms`. Consider gating that seed
  branch off for `ZENOH_ORIN_SPE` builds.

## Audit (2026-07-28) — 129 KB of the ~195 KB found and fixed

Reproduced first, on current pins: `just build-spe-image` overflows BTCM by
**168,760 bytes** (the issue recorded 164,464, so it had drifted slightly
worse).

### Cause 1 — a new static pool the consumer's rightsizing never covered

`nm --size-sort` on the firmware staticlib put two symbols far ahead of
everything else:

```
154,432  __NROS_SIZE_EXECUTOR_SIZE
131,072  nros_rmw_zenoh::shim::subscriber::LARGE_PAYLOADS
```

`LARGE_PAYLOADS` is `MAX_LARGE_SUBSCRIBERS(2) × RING_DEPTH(4) ×
SUBSCRIBER_LARGE_SIZE(16384)` = exactly 131,072 — every knob at its default.

`6f32fb7e4` ("zenoh-pico size-class receive buffers", RFC-0038) introduced this
pool, and `git merge-base --is-ancestor` confirms it landed **inside** the
regression window. The sentinel's SPE build sets nine tuning envs, including
`ZPICO_SUBSCRIBER_BUFFER_SIZE=256` for the SMALL pool — whose 4 KB matches
exactly — but the size-class feature added three knobs
(`ZPICO_MAX_LARGE_SUBSCRIBERS`, `ZPICO_SUBSCRIBER_LARGE_SIZE`,
`ZPICO_SUBSCRIBER_RING_DEPTH`) that no consumer knew to set. So a
correctly-rightsized image silently grew a 128 KB pool.

Fix, in the sentinel's SPE env: `ZPICO_MAX_LARGE_SUBSCRIBERS=1` +
`ZPICO_SUBSCRIBER_LARGE_SIZE=512`.

**Receipt: overflow 168,760 → 39,736 bytes.** 129,024 recovered, matching the
pool exactly.

### A false lead worth recording

`__NROS_SIZE_EXECUTOR_SIZE` (154,432) looks like the biggest item and is
tempting. Its arena IS oversized in principle — the formula provisions EVERY
callback slot as a worst-case action client (`3 × 4480 + 3 × rx_buf + 1536`
≈ 15.7 KB per slot, so 8 slots ≈ 128 KB) on an image with no action clients.

But setting `NROS_EXECUTOR_ARENA_SIZE=32768` shrank the symbol
(154,432 → 59,200) and moved the overflow **not at all** — still 39,736. It is
a size-EXPORT blob (`export_size!`, for the C header) that `--gc-sections`
drops, not live footprint. Do not chase it; measure the link, not the archive.

That said, the arena formula's action-client worst-casing is worth revisiting
on its own merits for RAM, just not for this overflow.

## Still open: the remaining 39,736 bytes

Largest live `.bss` contributors in the staticlib after the fix:

```
8,688  g_pending_gets                     (zenoh-pico, ZPICO_MAX_PENDING_GETS=2 already set)
8,192  nros_rmw_cffi …static_subscriber_storage::SLOTS
4,096  SMALL_PAYLOADS                     (already rightsized)
3,584  nros_rmw_cffi::MESSAGE_INFO_TABLE
2,736  SERVICE_BUFFERS
```

`nros_rmw_cffi`'s `SLOTS` (8 KB) and `MESSAGE_INFO_TABLE` (3.5 KB) are on the
rmw-cffi vtable seam the original report suspected, and neither appears to have
a rightsizing knob. `g_pending_gets` at 8.7 KB despite
`ZPICO_MAX_PENDING_GETS=2` deserves a check that the env actually reaches
zenoh-pico's C build.

Next step is a link-map audit (`-Wl,-Map`) rather than more archive `nm`, since
this issue has now produced one false lead from exactly that confusion.

## Link-map audit (2026-07-28) — 142 KB of the ~195 KB recovered

`nm` on the archive was the wrong instrument (it produced the false lead
above). The map the BSP already emits (`out/t23x/spe.map`, via
`-Xlinker -Map`) is the right one, filtered to ALLOCATED sections
(`.text`/`.rodata`/`.data`/`.bss`) — debug sections dominate it otherwise
(`.debug_str` alone is 40 MB and occupies no BTCM).

Top allocated contributors, pre-gc:

```
137,584  sentinel_spe_firmware …rcgu.o
109,076  compiler_builtins
 21,250  zpico.o
 15,409  tasks.o          (FreeRTOS)
 14,137  libc_a-jp2uc.o        <- newlib Japanese/Unicode tables
 13,884  libc_a-categories.o   <- newlib locale categories
  6,094  libc_a-svfiscanf.o    <- newlib scanf
```

### Cause 2 — full newlib instead of newlib-nano

`--specs=nano.specs` appears nowhere in the BSP Makefile, so the image links
FULL newlib. The sentinel's own LDFLAGS force `-u printf -u vprintf
-u vsnprintf` (to override newlib's float-aware `vsnprintf` with the local
shim), and that forcing pulls newlib's formatted-output machinery, which brings
its locale/Unicode tables along: ~34 KB of jp2uc + categories + scanf in a
256 KB image that never formats a locale.

Adding `--specs=nano.specs` swaps in the reduced implementations.

**Receipt: 39,736 → 26,492 bytes.** The failing section also moves from `.data`
to `.bss`, which is the honest signal that the remaining problem is now
statically-allocated state rather than code/rodata.

### Cumulative

```
168,760  as found
 39,736  after rightsizing LARGE_PAYLOADS   (-129,024)
 26,492  after newlib-nano                  (-13,244)
```

142,268 recovered — about 73% of the regression, and both causes are
consumer-side configuration of nano-ros features, not nano-ros defects.

## Remaining: 26,492 bytes, all `.bss`

Five symbols account for essentially all of it:

```
8,688  g_pending_gets                (zenoh-pico)
8,192  nros_rmw_cffi …static_subscriber_storage::SLOTS
4,096  SMALL_PAYLOADS                (already rightsized to 256 B buffers)
3,584  nros_rmw_cffi::MESSAGE_INFO_TABLE
2,736  SERVICE_BUFFERS
```

`g_pending_gets` is NOT an env-plumbing failure — I checked, the knob reaches
the C define (`nros-zpico-build/src/lib.rs`), and `ZPICO_MAX_PENDING_GETS=2` is
already set. The slot STRUCT is simply ~4.3 KB each. Shrinking it needs either
one slot or a smaller per-slot reply buffer.

`nros_rmw_cffi`'s `SLOTS` (8 KB) and `MESSAGE_INFO_TABLE` (3.5 KB) are the
rmw-cffi vtable seam the original report suspected, and neither has a
rightsizing knob today. That is the clearest nano-ros-side action: give them
the same `env_usize` treatment every other pool has.

## Sentinel-side fixes landed (2026-07-28)

Pushed to `NEWSLabNTU/autoware_sentinel` main as `8d099b4`, `1715492`,
`698e44f`:

1. Rightsize the zenoh-pico large-payload pool (-129,024 bytes).
2. Link against newlib-nano (-13,244).
3. **A guard bug in `scripts/spe/apply-patches.sh`** — every Makefile edit sat
   behind one `grep -q ENABLE_NROS_APP` check. That marker is present as soon
   as ANY injection has run, so an edit added later never reaches a checkout
   patched before it existed: the script reports "already present — skipping"
   and the flag never lands. It cost a debugging cycle here — the rebuild came
   back at an identical byte count while the script claimed success.

   Now one marker per edit, checking what THAT edit injects. Verified on an
   already-patched tree: the old edit still skips, the new pass injects, a
   second run is a no-op.

A patch script that reports success without patching is worse than one that
fails, and this class is easy to reintroduce — the convention is recorded in
the script.

## Round 2 (2026-07-28) — 91% recovered; the pattern named

Every figure below is a measured LINK, not an archive estimate.

| change | bytes | side | status |
| --- | --- | --- | --- |
| `LARGE_PAYLOADS` rightsize | 129,024 | consumer | landed `8d099b4` |
| newlib-nano (`--specs=nano.specs`) | 13,244 | consumer | landed `1715492` |
| `NROS_RMW_SUBSCRIBER_SLOTS=4` | 4,056 | consumer | landed `3b37942` |
| `ZPICO_MAX_PENDING_GETS=1` + `RING_DEPTH=2` | 8,032 | consumer | landed `3b37942` |
| `NROS_RMW_MESSAGE_INFO_SLOTS` knob | 3,136 | **nano-ros** | landed `1570c17e7`, not yet applied |

```
168,760  as found
 14,404  now
```

### A correction

An earlier note here said `nros_rmw_cffi`'s `SLOTS` had "no rightsizing knob".
Wrong — `NROS_RMW_SUBSCRIBER_SLOTS` has existed since issue 0269 (default
8 × 1 KiB = exactly the 8,192 measured). The consumer simply did not set it.

### The actual finding

Four of the five wins are the SAME shape: **the knob already existed and the
consumer did not know.** This build tunes nine environment knobs with a comment
explaining each, and still inherited ~145 KB of defaults across four separate
features — because each feature that added a static pool added its knob
silently.

Only ONE item needed nano-ros code (`MESSAGE_INFO_SLOTS`, hardcoded at 64 while
every neighbour was env-tunable). The rest was a discoverability failure, not a
defaults failure.

So the durable fix is not more knobs — it is making the existing ones
enumerable: a generated table of every static pool with its env var, default,
per-unit cost and bytes-at-default. That would have surfaced all four at once,
and would keep surfacing them as features land. Worth more than any individual
knob, and it is the thing this issue most argues for.

## Remaining: 14,404 bytes

Applying `NROS_RMW_MESSAGE_INFO_SLOTS=8` at the sentinel's next nano-ros pin
bump takes it to ~11,268. The original good pin had 31 KB headroom, so the
last ~11 KB is one more map round, not an architectural problem.

The sentinel's firmware builds against a PINNED nano-ros checkout
(`~/repos/nano-ros-sentinel`, at `54175c040`) that predates the knob; bumping
it pulls in the phase-312 launch-toolchain restructure, which is too broad to
fold into a footprint fix.


## Phase-358 W1 attempt, 2026-08-15 — the re-measurement is BLOCKED, and that is the finding

Phase 358 opens by warning that this number is probably stale in the project's
favour (`58d271471` carved the remap table, Executor 11632 -> 4992) and asks for
a current BTCM figure before anything is designed. **That figure cannot be
produced today**, for a reason worth writing down rather than rediscovering.

The repro lives in `autoware_sentinel` (`just build-spe-image`), which consumes
nano-ros by git pin. That pin is still `d9af52be` — the GOOD one. So the
sentinel as checked out builds the image that FIT; the regression was seen on a
bump it never landed. Re-measuring means bumping the pin to current main, and
three separate things the consumer names have since been removed from nano-ros:

| the sentinel asks for | state on current main |
| --- | --- |
| `nros-board-orin-spe` (crate) | **gone** — no such crate; `git ls-files` finds only archived docs |
| `platform-orin-spe` (feature) | **gone** — "a back-compat alias for `platform-freertos` since 121.10 … gone with their boards" (phase-337 W7.b, in `nros-platform/Cargo.toml`) |
| `rmw-zenoh` (feature) | **retired** by RFC-0054 when the backends moved behind the CFFI seam; `nros` carries `rmw-cffi` / `rmw-cyclonedds` / `rmw-lending` (this is also what broke `just book`, issue 0581) |

So this is a CONSUMER PORT, not a measurement: the board crate must become the
consumer's own (the out-of-tree board seam, phase-346), the platform feature
becomes `platform-freertos` directly — which is what the alias forwarded to
anyway — and the RMW selection moves to `rmw-cffi` plus a backend.

Until that port happens, **this issue's number can never be refreshed**, and any
plan that depends on knowing the current footprint is blocked behind it. That is
a more useful statement than "needs a size audit".

### What was measured, and why it does not substitute

`EXECUTOR_OPAQUE_U64S` under this issue's own knob set
(`NROS_EXECUTOR_MAX_CBS=8`, `NROS_SUBSCRIPTION_BUFFER_SIZE=256`) is **18031
u64s ≈ 144 KB** — but on `x86_64`, and this issue's budget is a 256 KB BTCM on
`armv7r`, where pointers are half the width. A host figure is not comparable to
it and is not offered as one. (It is also larger than the same crate's
default-knob host figure of 11191 u64s, which I cannot account for without
resolving the arena term — another reason not to lean on it.)

The honest state of phase 358's suspicion: `58d271471` plausibly recovered a
large part of this, and **that remains untested**. Testing it needs an armv7r
build of the minimal image, which needs the port above.


## Closed 2026-08-21 — 91 % recovered, the durable half split to #0739

Re-checked against current main rather than trusting this doc, which was last
touched 2026-08-15.

**The regression is substantially resolved.** 168,760 bytes of overflow -> 14,404,
and ~11,268 once `NROS_RMW_MESSAGE_INFO_SLOTS=8` is applied at the consumer's
next pin bump. Four of the five wins were consumer-side configuration; the one
nano-ros-side item landed and is still live (`packages/rmw/cffi/build.rs`,
default 64). Nothing here is a nano-ros defect any more.

**The durable finding is now #0739 and is DONE.** This issue argued its most
valuable outcome was making the existing knobs enumerable. Verified 2026-08-21
that none of the five knobs this audit needed appeared in the book's
environment-variables reference. `scripts/gen-pool-inventory.py` now generates
`book/src/reference/static-pool-inventory.md` (34 knobs, gated on the fast
lane), and it independently reproduces this issue's link-map figures from
source: `LARGE_PAYLOADS` 131,072 and `SLOTS` 8,192.

### Two corrections to what was written here

**The consumer port is probably no longer blocked.** The 2026-08-15 entry says
the port needs "the out-of-tree board seam, phase-346". That phase is COMPLETE
as of 2026-08-12 — "The RFC-0064 seam, actually reachable from out of tree",
closing issues 0415 and 0432 — which predates the entry that names it as
pending. `nros-board-orin-spe` and `platform-orin-spe` are still gone, so the
port is still WORK, but the seam it was waiting for exists.

**Phase 358's suspicion is most likely a false lead, and the disproof is in
this file.** 358 hoped `58d271471` (Executor 11632 -> 4992) recovered much of
the overflow, and treats an armv7r image as the only way to test it. But the
"false lead worth recording" section above already ran that experiment:
shrinking `__NROS_SIZE_EXECUTOR_SIZE` from 154,432 to 59,200 moved the overflow
**not at all**, because it is a size-EXPORT blob `--gc-sections` drops. By the
same reasoning the Executor carve should not move BTCM either. Whoever picks
this up should re-read that section before spending a cycle on the arena.

(Separately, the claim that no comparable figure is obtainable has also expired:
32-bit in-tree builds report `EXECUTOR_OPAQUE_U64S` today — `armv7a-nuttx-eabihf`
11034, riscv32 11036. By the paragraph above that number is the wrong instrument
for THIS budget, but it is no longer unobtainable.)

### Residual, if someone wants the last ~11 KB

Consumer-side, and it needs the port: bump the sentinel pin, apply
`NROS_RMW_MESSAGE_INFO_SLOTS=8`, and take one more link-map round. The original
good pin had 31 KB headroom, so this is a measurement exercise, not an
architectural one. Reopen or file fresh against the consumer rather than
carrying a mostly-fixed regression open indefinitely.
