# Phase 478 -- the island's developer-experience findings: borrowed toolchains, silent knobs, derivations without arithmetic

**Status (2026-10-05). PROPOSED -- nothing implemented.** Records the
nano-ros half of the developer-experience gaps the Autoware safety island
(simple-autoware-safety-island) met while building, flashing and running its
RTSS@Work 2026 demo on an NXP S32K344. The runtime findings of the same demo
(the violation channel, monitor arming, the route-level check, the heap) are
[phase-474](phase-474-safety-island-board-findings.md)'s and are cross-referenced
here, not repeated. Nothing below was measured for this document except where a
line says "checked 2026-10-05"; every other figure is quoted from the documents
under **Source**.

**Why a new phase and not a section of 474.** 474 holds what the RUNNING image
gets wrong: monitors, rings, heap after FirstSpin, link defaults. The items
here are about the person building the image: which checkout's tools ran, which
number won, why a derived number is what it is, and what a refusal tells you to
do. Different owners in the tree (the CLI, the doctor, the cmake ladder, the
docs) and different tests, so they get their own acceptance list.

**Prior:** [phase-474](phase-474-safety-island-board-findings.md) (the runtime
half), [phase-463](phase-463-host-census-reconciles-contract-with-code.md)
(contract vs code, the island's E3 experiments), phase-412 (derived entity
knobs, liveliness), RFC-0095 and design 0014 (the shared store and
`nros-sdk.lock`), phase-431 W1 (the foreign-binary guard), commit `3d52070ec`
(RMW snippet sizing became `configdefault`).

**Source:** in simple-autoware-safety-island at main `dcdd921`:
`docs/dx-ux-gaps-2026-10.md` (added by the island in parallel with this phase;
sections 3 "Building the image", 4 "Flashing, booting, bring-up", 5 "Running
and observing", 8 "Top 10" and 9 "Cheap versus structural"; item numbers below
such as "gap 3.4" are that document's); `docs/takeover-trace.md` (sections 9-11);
`docs/board-bringup-triage.md` (sections 1, 3, 5);
`docs/boot-through.md` (the W8a table); the board conf
`src/zephyr_entry/boards/mr_canhubk3_s32k344.conf`; `scripts/env.sh`;
`docs/roadmap/phase-9-rtss-demo-open-items.md`. The island's nano-ros pin is
`f03d9d190`.

---

## Why

The gaps document ranks two nano-ros DX items in its top 10: the wrong-checkout
and per-worktree environment traps (its #5), and hand-set numbers that silently
beat the derivation (its #9). Both cost hours each time, both fail SOMEWHERE
ELSE than where the cause is, and one of them led to pushes with checks skipped:
on 2026-09-24 a fresh nano-ros worktree ran the island's vendored, older `nros`
from PATH, a codegen version refusal followed, and eight-plus units were told it
was a known host red and pushed with `NROS_SKIP_PREPUSH_CHECKS=1` (gap 3.1). It
was not a host red and it was never in any diff.

The pattern under every item: the tool knows the fact a person needs (which
checkout it belongs to, the derived count, the terms of a sum, the remedy that
still exists) and does not say it at the moment it matters.

## How each gap maps

Gaps from the island document whose owner includes nano-ros. "here" means an
item of this phase; anything else is where it is already tracked.

| gap | what | where it lives |
| --- | --- | --- |
| 3.1, 3.2, 3.12 | a checkout silently uses another checkout's `nros`, SDK or workspace; setup is per worktree and unchecked | here D1, I1, I2; issues 1253, 1254, 1234, 1399, 1373 |
| 3.3 | a value written in the wrong config file is ignored | mostly fixed upstream by `3d52070ec`; see "Already answered" |
| 3.4 | a stated knob beats the derivation in silence | here D2, I3 |
| 3.5 | three heap figures disagree; QEMU needed 1 MiB once Autoware joined | phase-474 I5 (cross-ref only) |
| 3.6 | the liveliness count (25) has no recorded arithmetic | here D3, I4 |
| 3.7, 4.5 | refusals that mislead, failures that announce nothing | here I5; issues 1120, 1303, 1036, 1252 |
| 3.8 | a contract edit does not invalidate the model | issue 1121 |
| 3.9 | a standing "no producer" on every `nros sync` | phase-474 I4 |
| 3.10 | scheduling derivation inert, one stderr line | issue 1371 |
| 3.11 | slow first build; `nros setup native` source-builds zenohd | issue 0374 (resolved); host hygiene is the island's |
| 3.13 | `image-facts --for-entry zephyr_entry` refused on the pristine tree | here T2 (re-verify) |
| 1.6 | a declared endpoint is never checked against the code | phase-463 (W6 flips the island); issue 1419 |
| 2.2, 2.3 | the CLI's vendored play_launch moves on its own; versions do not name the grammar | here I7 |
| 4.1 | a console-less board's log needs four unannounced steps | here I6 |
| 4.2 | RX ring default at its edge; main thread priority unstated | phase-474 T3 owns the values; here I8 owns where a board author learns them |
| 4.3 | the lease `min()` looked like a peer join | phase-474 T1 |
| 5.1, 5.2, 5.7 | violations invisible; ring keeps the first 8; start-up entries | phase-474 D1, D2, I1, I2, T4 |
| 5.3 | the runtime monitor checks a callback, the budget is a route | phase-474 D3 |
| 1.4 | a budget is also a deadline and a monitor | phase-474 D4 |
| 5.4 | no board time source over serial | here D4; issue 0758 |
| 5.6 | instruments that read zero or are absent | phase-474 I3, T2 |
| 7.2 | `find .` walks into worktrees | issue 1565 (resolved) |
| 7.5 | issue status drift | here T1; issue 1489 |

The island's own stale-checkout habit (a six-weeks-old `~/repos/nano-ros`) is
the user's, not nano-ros's, and is not an item.

## Design

### D1 -- a checkout cannot borrow another checkout's toolchain

What the tree does today:

- `scripts/bootstrap.sh:279-281`: `install_nros_source` is a no-op when ANY
  `nros` is on PATH ("nros already on PATH"), wherever it lives.
- `scripts/bootstrap.sh:407-408`: `shell-doctor` reports `[OK] nros on PATH`
  for any `nros`, also wherever it lives. The version-lockstep line below it
  catches a different VERSION, not a different CHECKOUT at the same version.
- The phase-431 W1 guard (`refuse_if_foreign_to_workspace`,
  `packages/cli/nros-cli-core/src/stale_guard.rs`) refuses a foreign `nros`
  inside a checkout, but stands down under `NROS_SKIP_STALE_CHECK` and when the
  other tree has no `packages/cli` ([issue 1253](../issues/1253-zephyr-workspace-manifest-binds-foreign-module.md),
  open). A downstream project with a vendored nano-ros (the island's
  `third-party/nano-ros`) is a checkout with a `packages/cli`, so it is a valid
  donor for a nano-ros worktree that has not run `just setup-cli`.
- The shared Zephyr workspaces bind an SDK INSIDE one checkout. Checked
  2026-10-05: `~/.nros/workspaces/zephyr/3.7/env.sh` and `.../4.4/env.sh` both
  export `ZEPHYR_SDK_INSTALL_DIR` under
  `simple-autoware-safety-island/third-party/nano-ros/scripts/zephyr/sdk/`
  (0.16.8 and 1.0.1), while the store holds both versions under
  `~/.nros/sdk/`. So every checkout on this host that builds Zephyr through the
  shared workspace compiles with the island submodule's SDK copy: the mirror
  image of [issue 1254](../issues/1254-zephyr-sdk-installs-inside-the-checkout.md)
  (open), where the island bound a sibling's.
- `nros-sdk.lock` (design 0014: "committed per workspace", index = desired,
  lock = installed) is written and read back by `nros store gc`, but no build
  step checks that the toolchain IN USE is the one the lock names; a `--prefix`
  install is not recorded at all (issue 1254, `cmd/setup.rs:871` per that
  file). The island's lock is untracked (`?? nros-sdk.lock`, gap 3.12; the
  island's to commit or ignore) and says `zephyr-sdk 0.16.8 prebuilt`, which is
  not what its workspace's `env.sh` points at.

Proposal, to decide in this document:

1. **The doctor refuses a foreign path.** `shell-doctor` and `bootstrap.sh
   doctor` report `[FAIL]` when `command -v nros` resolves outside the checkout
   (or, in a downstream project, outside the nano-ros that project pins), and
   name both paths and the one command that fixes it (`just setup-cli`).
   `install_nros_source` treats a foreign `nros` as absent, not as done.
2. **An explicit, checked lock.** At configure time the Zephyr module compares
   `ZEPHYR_SDK_INSTALL_DIR` (and the workspace's west manifest path, issue 1253's
   second consequence) with the nearest `nros-sdk.lock`; a path outside the store
   and outside this checkout refuses, naming the lock, the path in use and its
   owner. A `--prefix` install either records itself in the lock with its path,
   or the lock check treats it as foreign.
3. **The build entry point runs the cheap half of the doctor.** The three things
   a fresh worktree lacks (its own CLI, the submodules the fast gates read, the
   repo's `core.hooksPath`) are checked by the first `just` recipe that needs
   them, with one message listing all three, instead of surfacing as three
   different errors at three stages (gap 3.2, issue 1373).

Not proposed: keying build directories by checkout. That is issues 1399 and
1596's question (1596 resolved it for the Zephyr build dirs) and stays there.

### D2 -- a stated number that loses to the derivation says so

`zephyr/cmake/nros_cargo_build.cmake:432-438`, `_nros_resolve_derivable_knob`:
"Someone stated a number. It wins over the derivation, in both directions and
without comment". The C-array floor (`cmake/NanoRosPoolFloor.cmake`, issue
1015) only raises a 0. So on the island the board conf's
`CONFIG_NROS_MAX_LIVELINESS=32` was right at 29 tokens and went 26 short the day
`params:` was declared, with nothing at build time and nothing at boot that the
board could show (board conf lines 552-570; triage section 3 item 1). The island
now follows "state board facts, never contract-derivable counts" as a comment
(board conf lines 563-570), which is a convention, not a check.

Proposal: when a knob is derivable and a value is stated, compute the derived
value anyway and compare.

- stated BELOW the derived demand: a configure WARNING naming the knob, both
  numbers, the rung that stated it (Kconfig, environment, board facts) and the
  file it came from; optionally a refusal with an explicit acknowledgement knob
  for an image that knows better. Decide which.
- stated ABOVE: a STATUS line with both numbers, so the over-provision is
  visible and reviewable.
- equal: say nothing new; suggest deleting the stated line.

Related, and not the same: [issue 1490](../issues/1490-kconfig-knob-forwarding-gate-tests-mention-not-table-row.md)
(open) is about a knob not being FORWARDED, not about a forwarded one losing;
[issue 1368](../issues/1368-frag-max-size-not-checked-against-derived-bound.md)
(open) is the same compare-to-nothing class for `ZPICO_FRAG_MAX_SIZE` against the
derived receive bound and should land on the same mechanism. The heap is
[phase-474](phase-474-safety-island-board-findings.md) I5's: the configured
102,400 against a stale 133,952 ask is a derivation that does not yet cover the
read path, and 474 owns making the two agree;
[issue 1424](../issues/archived/1424-zephyr-heap-size-is-a-guess-with-a-peak-reporter-nothing-reads.md)
is resolved for native_sim only (its own "Not done": a real board, QEMU). Once
474 I5 derives a heap, the heap knob joins D2's compare like any other.

### D3 -- a derived count carries its arithmetic

The island records its image's liveliness count as a bare 25
(`docs/boot-through.md`, the W8a table); the only formula written down (board
conf lines 563-565, "1 session + 4 names + 14 pubs + 11 subs + 2 servers + 2
clients + 24 parameter services = 58") is for the earlier 4-node image, and its
terms do not match the code's (parameter services are inside the queryable
count per issue 1270, not a separate term). Nobody can check 25 from the
record.

The derivation has the terms (`entity_inventory.rs:3218-3226`: node tokens +
publishers + subscribers + queryables + service clients). The fragment it emits
prints the terms in PROSE and only the total (`entity_inventory.rs:4399-4405`,
`set(NROS_DERIVED_MAX_LIVELINESS {})`), while the next knob in the same file
prints its terms WITH their values (`entity_inventory.rs:4411-4419`, the
same-session-queries line, issue 1549), and the boot report prints peak,
capacity and floor for the heap.

Proposal: every derived pool knob carries a one-line composition with numbers,
in the emitted fragment, in `nros image-facts`, and in the configure STATUS line
of D2 -- for liveliness, "25 = 4 node tokens + P publishers + S subscribers + Q
queryables + C clients". The format is shared, so a test can hold the sum equal
to the value.

### D4 -- a time source for a board with no IP stack

[Issue 0758](../issues/0758-platform-sntp-epoch-source.md) (open) proposes
`nros_platform_epoch_us()` and an SNTP provider. The S32K344 island reaches the
host only through zenoh over a UART; it has no IP stack, so SNTP cannot reach
it. Consequences on the island (gap 5.4, its phase 9 W3): the board's hazard
command carries a boot-relative stamp and the simulator never shows ENABLE from
the board, while native_sim with SNTP shows it 38-167 ms later; the trace merge's
two anchors disagree 13.05-47.62 ms over serial, so every cross-clock row is an
upper bound.

Decide the provider for a link-only board: an epoch delivered over the zenoh
session (a timestamp from the router or the island gateway, one exchange at
join, offset kept by `epoch_us()`), or SNTP framed over the serial link by the
gateway. 0758's first work item (the ABI function with a returns-0 sentinel and
its POSIX implementation) is independent of this choice and can land first.
The island consumes it in its phase 9 W3 and W9.

## Implementation

### I1 -- the doctor names a borrowed `nros`

D1.1 and D1.3: `shell-doctor` / `bootstrap.sh doctor` FAIL on an `nros` outside
the checkout; `install_nros_source` does not accept a foreign one; the first
recipe that needs the CLI, the fast-gate submodules or the hooks checks all
three at once. Issue 1373 is the gate half of the same fresh-worktree shape.

### I2 -- the lock is checked at configure

D1.2: the Zephyr module refuses an SDK or a west manifest that belongs to
another checkout and is not in the lock; `--prefix` installs are recorded or
refused. Then re-provision the shared workspaces on this host so that their
`env.sh` names store paths, and close issue 1254 and the module half of issue
1253 on that run.

### I3 -- stated-vs-derived on every derivable knob

D2 in `_nros_resolve_derivable_knob`, with the message-bound and entity
inventories both feeding it; the `FRAG_MAX_SIZE` comparison of issue 1368 on the
same path.

### I4 -- compositions in the fragment and in `image-facts`

D3 for every `NROS_DERIVED_*` pool knob, liveliness first.

### I5 -- refusals that point at something that exists, failures that reach the record

- [Issue 1120](../issues/1120-entity-inventory-refusal-names-retired-entities.md)
  reads `status: open`, but its defect is gone on main: the refusal now names the
  contract sidecar (`entity_inventory.rs`, near the "declare no entities"
  refusal), fixed under issue 1033 by `51fdfef44`; `ENTITIES` remains only in
  comments and in the cmake retirement error. Confirm on an out-of-tree
  consumer, then resolve and archive 1120. (The island document lists it as a
  cheap open fix; it was already done.)
- [Issue 1303](../issues/1303-runtime-refusals-are-silent-on-freestanding.md)
  (open): the two runtime refusals emit through a sink that is a no-op on
  freestanding targets.
- [Issue 1036](../issues/1036-arena-exhaustion-is-half-silent-and-wholly-unreachable.md)
  (open, narrowed to silicon): since `823d8fb63` (2026-10-02, after the island's
  pin) every error-level `nros_log` record is counted in the boot record with the
  first one's file and line. That covers the Rust shim's liveliness-not-declared
  log, which is the island's "STILL SILENT" item 1 in its triage section 3.
  It does not cover the zenoh-pico C shim's own
  `printk("zpico: liveliness pool exhausted ...")` (`zpico.c:3293`), which goes
  to the unwired console. Route the C shim's pool-exhaustion lines to the record
  too, and have the island re-check its triage section 3 after its next pin bump.
- [Issue 1252](../issues/1252-message-bound-knobs-have-no-pre-configure-twin.md)
  (open): the board build refused its own image on
  `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS`.

### I6 -- a console-less board has a documented log path

The island's triage section 1 needs four steps no error mentions to read a log
over RTT (clone the SEGGER module, `-DZEPHYR_EXTRA_MODULES`, `pyocd rtt` under
`script`, the `_SEGGER_RTT` address) and a larger up-buffer. nano-ros owns the
part every console-less board shares: a Zephyr guide page saying that the boot
record (`read-boot-report.py`) is the first channel, what it carries since
issue 1036's record v9, and the RTT recipe as the second; optionally an
`nros-rtt` snippet that sets the module and buffer. The board-specific half
(which UART is wired) stays the island's.

### I7 -- the CLI names the grammar it embeds

`packages/cli/third-party/play_launch` is a submodule the CLI vendors, moved by
nano-ros PRs on its own schedule, while the island's CI pins a play_launch wheel
and its PATH holds a third copy (gap 2.3). `nros --version` (or a `--verbose`
form) prints the vendored play_launch commit and the rlm tag it parses with, so
"which grammar judged this contract" has an answer from the tool that judged it.
The play_launch-side `--version` fix is play_launch's.

### I8 -- the serial-link pair is learnt from nano-ros, not from the island's conf

Every board that runs zenoh over a UART needs, today, two lines the island found
by measurement (board conf lines 683-700): `CONFIG_MAIN_THREAD_PRIORITY=5`
(Zephyr's 0 sits above the read task's band while main registers entities) and
`CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES=4096` (default 1024, `zephyr/Kconfig:682-685`;
measured 793-1024 at joins, 1,226 in a soak). [phase-474](phase-474-safety-island-board-findings.md)
T3 owns closing [issue 1534](../issues/1534-zephyr-tx-flush-task-outranks-the-read-task.md)
with a derived main priority and a measured ring default. This item is only the
DX half, and it is cheap: until T3 lands, the serial-link guide and the Kconfig
help name both lines and why; when T3 lands, the default moves under
`NROS_ZENOH_LINK_SERIAL` as a `configdefault` (the mechanism `3d52070ec` used
for snippet sizing), so a board conf can still override it, and D2's compare
reports a stated value below it.

## Test / check

### T1 -- issue status sweep

The island document read several nano-ros issues as open that are not, and one
as open whose defect is fixed. Checked 2026-10-05 on origin/main:

| issue | file status | actual |
| --- | --- | --- |
| 1253, 1254, 1234, 1399, 1373 | open | open |
| 1596 | resolved (archived) | resolved; the island document lists it as open |
| 1424 | resolved (archived) | resolved for native_sim; board and QEMU not done (474 I5) |
| 1425 | resolved (archived) | resolved |
| 0374, 1565 | resolved (archived) | resolved |
| 1120 | open | defect fixed by `51fdfef44` (I5) |
| 1490, 1368 | open | open; 1490 is forwarding, not stated-wins (D2) |
| 0758 | open | open (D4) |
| 1533, 1534, 0852 | open | 474 T3 |
| 1303, 1036, 1252 | open | open (I5) |
| 0934, 0941, 1121, 1371, 1419, 1489 | open | open |

Resolve 1120 per I5. Issue 1489 is the general form of this row.

### T2 -- the fresh-worktree run, and `image-facts` on the island

- Create a worktree of nano-ros beside an older checkout whose `nros` is first
  on PATH. Acceptance: the doctor FAILs naming both paths; a Zephyr configure
  against a workspace whose SDK lives in another checkout refuses naming the
  lock; after `just setup-cli` and the named submodules, `just check fast` is
  green with no skip from another tree.
- Re-run `nros image-facts --for-entry zephyr_entry` on the island's pristine
  tree at the current pin (gap 3.13, recorded on an older pin as "refused, no
  launch file"); fix or record the refusal.

### T3 -- the island image, built with D2 and D3

Build the island's board image with one derivable knob stated below its derived
value (for instance `CONFIG_NROS_MAX_LIVELINESS=20`). Acceptance: configure warns
(or refuses, per D2's decision) naming the knob, 20, the derived value and the
board conf; the emitted fragment and `image-facts` print the liveliness
composition and its sum equals the resolved value.

## Already answered on main

- **Gap 3.3, a value in the wrong file.** `3d52070ec` (2026-08-24) moved the
  zenoh snippet's `MAIN_STACK_SIZE`, `HEAP_MEM_POOL_SIZE`,
  `SYSTEM_WORKQUEUE_STACK_SIZE` and `NET_PKT`/`NET_BUF` counts to
  `configdefault` (`zephyr/Kconfig`, the `if NROS_RMW_ZENOH` block), so a board
  conf now wins. The island's triage section 5 and the `-D` column in its
  `board-build` recipe describe the earlier precedence. Issues 0934 and 0941
  (open) hold the general config-surface question.
- **Gap 1.6, declaration vs code.** Not untracked: phase-463 and issue 1419
  (open) are the host census; W3's verdicts reproduce the island's E3a-E3c, and
  W6 flips the island to `refuse`.

## Cross-repo

- **play_launch** phase 85 ("what the island left open") owns the version-string
  half of gap 2.2, the grammar keys (transport on the walk, timer jitter,
  service-edge cost, on-demand topics) and the checker output gaps. I7 here only
  makes the nano-ros CLI name what it embeds.
- **simple-autoware-safety-island** phase 9 consumes D4 (its W3 time source, W9
  hazard lights) and re-checks its triage after I5; the island-side cheap fixes
  (commit or ignore `nros-sdk.lock`, refresh the liveliness comment in the
  board conf, the triage and runbook pages) are its own.

## Order

D1 before I1 and I2; D2 before I3; D3 before I4; I3 and I4 before T3. D4,
I5, I6, I7 and I8 are independent. I8's second half waits on 474 T3.

## What this phase does not do

- The runtime items of phase-474 (violations, arming, route checks, the heap
  derivation, the link defaults' values).
- Key build directories by checkout (issues 1399, 1596).
- Change the island's configuration, contract or documents.

## Acceptance

- [ ] D1, D2, D3, D4 each decided and written into this document.
- [ ] I1: the doctor FAILs on an `nros` outside the checkout, and the first
      recipe that needs them names a missing CLI, submodules and hooks at once.
- [ ] I2: a Zephyr configure refuses an SDK or west manifest from another
      checkout that the lock does not name; the shared workspaces on the
      development host name store paths; issue 1254 resolved.
- [ ] I3: a derivable knob stated below its derived value warns or refuses at
      configure, naming both numbers and the stating file.
- [ ] I4: every derived pool knob prints its composition with values, and a test
      holds the sum equal to the value.
- [ ] I5: issue 1120 resolved and archived; the C shim's pool-exhaustion lines
      reach the boot record.
- [ ] I6: a Zephyr guide page names the boot record and the RTT recipe for a
      console-less board.
- [ ] I7: `nros --version` names the vendored play_launch commit and rlm tag.
- [ ] I8: the serial-link guide and Kconfig help name the main-priority and
      RX-ring lines until 474 T3 makes them defaults.
- [ ] T1: every row of the status table matches its issue file (1120 archived).
- [ ] T2: the fresh-worktree run passes as stated; `image-facts` on the island
      re-run and its result recorded.
- [ ] T3: the island image shows the stated-below-derived message and a
      liveliness composition that sums to the resolved value.
