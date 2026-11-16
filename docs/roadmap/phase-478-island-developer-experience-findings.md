# Phase 478 -- the island's developer-experience findings: borrowed toolchains, silent knobs, derivations without arithmetic

**Status (2026-10-24). IN PROGRESS -- D1-D4 decided, I1-I8 landed; T2 and
T3 wait on the island's board.** Refreshed 2026-10-24: D2/I3 and the first half
of I8 had already landed under phase-474 I5 and issue 1534's configure warning,
and most issues the 2026-10-05 text cited as open are resolved (T1). What is
left needs the S32K344 and the island tree: the fresh-worktree and island
`image-facts` re-run (T2), the stated-below-derived build of the island image
(T3), and on-board confirmation of D4's epoch. I8's second half waits on
phase-474 T3. I9-I11 were added 2026-11-15, from the island's GitHub reports:
the CMake road's image `profile` (I9), then a generated Zephyr app's package
closure and its workspace Zephyr modules (I10 and I11, issue 2594, option A,
done).

Originally (2026-10-05, PROPOSED). Records the
nano-ros half of the developer-experience gaps the Autoware safety island
(simple-autoware-safety-island) met while building, flashing and running its
RTSS@Work 2026 demo on an NXP S32K344. The runtime findings of the same demo
(the violation channel, monitor arming, the route-level check, the heap) are
[phase-474](archived/phase-474-safety-island-board-findings.md)'s and are cross-referenced
here, not repeated. Nothing below was measured for this document except where a
line says "checked 2026-10-05"; every other figure is quoted from the documents
under **Source**.

**Why a new phase and not a section of 474.** 474 holds what the RUNNING image
gets wrong: monitors, rings, heap after FirstSpin, link defaults. The items
here are about the person building the image: which checkout's tools ran, which
number won, why a derived number is what it is, and what a refusal tells you to
do. Different owners in the tree (the CLI, the doctor, the cmake ladder, the
docs) and different tests, so they get their own acceptance list.

**Prior:** [phase-474](archived/phase-474-safety-island-board-findings.md) (the runtime
half), [phase-463](archived/phase-463-host-census-reconciles-contract-with-code.md)
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
| 3.1, 3.2, 3.12 | a checkout silently uses another checkout's `nros`, SDK or workspace; setup is per worktree and unchecked | here D1, I1, I2 (landed); issues 1253, 1254, 1234, 1399, 1373 (all resolved) |
| 3.3 | a value written in the wrong config file is ignored | mostly fixed upstream by `3d52070ec`; see "Already answered" |
| 3.4 | a stated knob beats the derivation in silence | here D2, I3 (landed, `06d1880a13`) |
| 3.5 | three heap figures disagree; QEMU needed 1 MiB once Autoware joined | phase-474 I5 (cross-ref only) |
| 3.6 | the liveliness count (25) has no recorded arithmetic | here D3, I4 (landed) |
| 3.7, 4.5 | refusals that mislead, failures that announce nothing | here I5; issues 1120, 1303, 1036, 1252 (all resolved) |
| 3.8 | a contract edit does not invalidate the model | issue 1121 (resolved) |
| 3.9 | a standing "no producer" on every `nros sync` | phase-474 I4 |
| 3.10 | scheduling derivation inert, one stderr line | issue 1371 (open) |
| 3.11 | slow first build; `nros setup native` source-builds zenohd | issue 0374 (resolved); host hygiene is the island's |
| 3.13 | `image-facts --for-entry zephyr_entry` refused on the pristine tree | here T2 (re-verify) |
| 1.6 | a declared endpoint is never checked against the code | phase-463 (archived); issue 1419 (resolved) |
| 2.2, 2.3 | the CLI's vendored play_launch moves on its own; versions do not name the grammar | here I7 (landed) |
| 4.1 | a console-less board's log needs four unannounced steps | here I6 (landed) |
| 4.2 | RX ring default at its edge; main thread priority unstated | phase-474 T3 owns the values; here I8 owns where a board author learns them |
| 4.3 | the lease `min()` looked like a peer join | phase-474 T1 |
| 5.1, 5.2, 5.7 | violations invisible; ring keeps the first 8; start-up entries | phase-474 D1, D2, I1, I2, T4 |
| 5.3 | the runtime monitor checks a callback, the budget is a route | phase-474 D3 |
| 1.4 | a budget is also a deadline and a monitor | phase-474 D4 |
| 5.4 | no board time source over serial | here D4 (Zephyr landed); issue 0758 |
| 5.6 | instruments that read zero or are absent | phase-474 I3, T2 |
| 7.2 | `find .` walks into worktrees | issue 1565 (resolved) |
| 7.5 | issue status drift | here T1 (refreshed 2026-10-24); issue 1489 (resolved) |

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
  other tree has no `packages/cli` ([issue 1253](../issues/archived/1253-zephyr-workspace-manifest-binds-foreign-module.md),
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
  image of [issue 1254](../issues/archived/1254-zephyr-sdk-installs-inside-the-checkout.md)
  (resolved 2026-10-23: the store half; I2's configure refusal stays here),
  where the island bound a sibling's.
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

**Decided (2026-10-24).** What changed underneath this section first: issue
1254 (resolved 2026-10-23) moved the SDK into the store, so `just zephyr setup`
no longer writes one inside a checkout, and the `~/.nros/workspaces/zephyr/*`
directories quoted above no longer exist on this host. Issue 1253's manifest
half is checked by `check-zephyr-workspace-checkout.sh` on every tier-2 run, and
issue 1379's `nros_assert_single_checkout` refuses a module from another
checkout at configure. Issue 1373 found the fast tier green in a pristine
worktree and added `check pristine-checkout`; issue 1513 added
`_require-worktree-provisioning`, which reports every missing submodule and
both binaries at once.

1. **Kept as proposed.** Both doctors FAIL on a borrowed `nros`. "Borrowed"
   means it resolves (through symlinks) into a DIFFERENT nano-ros checkout.
   An `nros` outside any checkout is a release or store install, and stays OK.
   This is the same three-row rule `scripts/lib/checkout-paths.sh` states for
   inherited paths, asked through one new helper, `nros_foreign_checkout_of`.
2. **Narrowed: ownership, not the lock.** The configure refuses an SDK
   inside another checkout. It does not compare the SDK against
   `nros-sdk.lock`. The lock records versions, while the defect was a path; a
   store SDK at a version the lock does not name is `drift.rs`'s `tool` kind,
   already a separate check. The west-manifest half is issue 1379's refusal
   plus 1253's tier-2 check, so I2 adds nothing for it.
3. **Mostly answered by 1513.** The one missing item was the hooks, which are
   not a precondition of the lane (it passes without them). So they appear as
   a NOTE in the same report rather than as a failure.

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

**Decided and landed (`06d1880a13`, phase-474 I5): a WARNING, never a
refusal.** `_nros_compare_stated_to_derived` runs on both stating rungs
(Kconfig, environment) in `_nros_resolve_derivable_knob`. A stated value below
the derivation warns, above it prints a STATUS with both numbers, and equal says
the line can be deleted. No acknowledgement knob: a stated value already wins,
which is what "a derived value is a default" means, and a refusal would turn
every board that knows better into an override fight. The message names the
RUNG, not the file. Zephyr merges Kconfig fragments before this code sees a
value, so the file is in the build's `Merged configuration` lines, not here.

Related, and not the same: [issue 1490](../issues/archived/1490-kconfig-knob-forwarding-gate-tests-mention-not-table-row.md)
(open) is about a knob not being FORWARDED, not about a forwarded one losing;
[issue 1368](../issues/archived/1368-frag-max-size-not-checked-against-derived-bound.md)
(resolved 2026-10-23) was the same compare-to-nothing class for the frag ceiling
against the receive class. It landed as a refusal on both roads (the Zephyr
configure, and `nros-rmw-zenoh`'s build over `zpico-sys`'s published ceiling),
through one rule in `nros_sizing_descriptor::receive_ceiling`. The heap is
[phase-474](archived/phase-474-safety-island-board-findings.md) I5's: the configured
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

**Decided (2026-10-24).** A composition exists for a knob that is a SUM of
declared counts: `MAX_SUBSCRIBERS`, `MAX_PUBLISHERS`, `MAX_QUERYABLES`,
`MAX_LIVELINESS`. A knob that is one count (`EXECUTOR_MAX_NODES`), a max
(`RUNTIME_MAX_CELL_ENTITIES`) or a flag (`RMW_LOCAL_QUERYABLE`) has no
arithmetic to show; its fragment comment already says what it counts. Each
composition is carried as `NROS_COMPOSITION_<knob>`, deliberately NOT
`NROS_DERIVED_*`, because that prefix is the set of facts
`check-knob-delivery` follows to the compiler. **Not in `image-facts`:** that
query stops before generation and carries no entity facts at all, since the
entity inventory is composed at configure by `nano_ros_entry()`. The query that
answers it is `nros ws entity-inventory`, whose JSON now has a `compositions`
table. A merge across images keeps the composition of the image that set the
max, so the merged line still sums to the merged value; a per-term max would
describe no image.

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

**Decided (2026-10-24): the timestamps zenoh already puts on every sample.
Neither of the two options above.** rmw_zenoh's router config enables
timestamping for router, peers and clients (`timestamping.enabled`, checked in
Humble's `DEFAULT_RMW_ZENOH_ROUTER_CONFIG.json5`). So every sample a ROS 2 host
sends arrives stamped with the publisher's HLC, which is that host's wall clock
in NTP64 since the UNIX epoch, or with the router's if the publisher left it
unstamped. That is the clock the stamped-message peers compare against. It
needs no gateway, no new exchange and no protocol, and it re-acquires for as
long as samples arrive, which issue 0758 measured a one-shot epoch cannot do.
`CONFIG_NROS_EPOCH_FROM_ZENOH`: the first timestamped sample installs the
offset; after that, each `CONFIG_NROS_EPOCH_FROM_ZENOH_INTERVAL_MS` window
re-installs the largest offset it saw, i.e. the least-delayed sample's.
Accepted limits:
- low by one link latency;
- no epoch for an image that receives nothing;
- Zephyr only, since FreeRTOS and ThreadX answer 0 and issue 0772 keeps them.

0758's first work item had already landed (`nros_platform_epoch_us`, every
port).

## Implementation

### I1 -- the doctor names a borrowed `nros`

D1.1 and D1.3: `shell-doctor` / `bootstrap.sh doctor` FAIL on an `nros` outside
the checkout; `install_nros_source` does not accept a foreign one; the first
recipe that needs the CLI, the fast-gate submodules or the hooks checks all
three at once. Issue 1373 is the gate half of the same fresh-worktree shape.

**Landed (2026-10-24).**
- `scripts/bootstrap.sh shell-doctor` FAILs on a borrowed `nros`, naming
  both checkouts and `just setup-cli && source ./activate.sh`.
- `install_nros_source` builds this checkout's CLI instead of returning early.
- `just doctor` gains a `borrowed` state for the case it used to pass as
  "in-tree CLI not built yet, whatever is on PATH is fine".
- `_require-worktree-provisioning` notes an unset or wrong `core.hooksPath`.
  An absolute spelling of `.githooks` counts as set.
- Found on the way: `bootstrap.sh --dry-run` BUILT the CLI. A
  `# profile-literal-ok` marker comment inside the `\` continuation ended the
  `run_cmd` call, so `run_cmd` got a label and no command, and `cargo build`
  ran on its own, outside `--dry-run`. The marker now sits above the call
  (still inside `check-build-profile-literals`' 3-line window), and a
  repository sweep found no other comment that cuts a continuation short.

Measured with a synthetic second checkout first on PATH:
`[FAIL] nros on PATH (…/bin/nros) belongs to ANOTHER checkout`, and
`--dry-run` prints both `cargo build` lines without running them.

### I2 -- the lock is checked at configure

D1.2: the Zephyr module refuses an SDK or a west manifest that belongs to
another checkout and is not in the lock; `--prefix` installs are recorded or
refused. Then re-provision the shared workspaces on this host so that their
`env.sh` names store paths, and close issue 1254 and the module half of issue
1253 on that run.

**Landed (2026-10-24), narrowed per D1.2.** `nros_assert_sdk_not_foreign` in
`zephyr/cmake/nros_checkout_guard.cmake` runs right after issue 1379's
application check. It refuses `ZEPHYR_SDK_INSTALL_DIR` inside a different
checkout, naming both and `just zephyr setup`. `-DNROS_ALLOW_FOREIGN_SDK=ON`
downgrades it to a warning. 1254 and 1253 were closed by their own work, and
the `--prefix` install no longer exists (1254: `setup.sh` installs into the
store). New gate `just check zephyr-checkout-guard` runs both functions over
synthetic checkouts under `cmake -P`, eight rows, with a downgraded-guard
negative control on every run. No lane has two checkouts, so nothing else
reaches either refusal.

### I3 -- stated-vs-derived on every derivable knob

D2 in `_nros_resolve_derivable_knob`, with the message-bound and entity
inventories both feeding it; the `FRAG_MAX_SIZE` comparison of issue 1368 on the
same path.

**Landed** in `06d1880a13` (phase-474 I5). Issue 1368 landed as a refusal on
both roads through `nros_sizing_descriptor::receive_ceiling` (`e6396d757f`).

### I4 -- compositions in the fragment and in `image-facts`

D3 for every `NROS_DERIVED_*` pool knob, liveliness first.

**Landed (2026-10-24)** for the four summed pools (D3's decision).
- `entity_inventory::Composition`, and `DerivedEntityKnobs::compositions`
  built beside the sums they mirror term for term.
- The fragment writes `set(NROS_COMPOSITION_<knob> "<sum> = <terms>")`.
  `_nros_derived_composition` appends it to the configure's `DERIVED from`
  STATUS and to D2's warning (`… derives (25 = 4 node tokens + …)`).
- The inventory JSON carries a `compositions` table.
- `compositions_sum_to_their_knobs` holds each total equal to its knob, on
  one image and across a merge.

### I5 -- refusals that point at something that exists, failures that reach the record

- [Issue 1120](../issues/archived/1120-entity-inventory-refusal-names-retired-entities.md)
  reads `status: open`, but its defect is gone on main: the refusal now names the
  contract sidecar (`entity_inventory.rs`, near the "declare no entities"
  refusal), fixed under issue 1033 by `51fdfef44`; `ENTITIES` remains only in
  comments and in the cmake retirement error. Confirm on an out-of-tree
  consumer, then resolve and archive 1120. (The island document lists it as a
  cheap open fix; it was already done.)
- [Issue 1303](../issues/archived/1303-runtime-refusals-are-silent-on-freestanding.md)
  (open): the two runtime refusals emit through a sink that is a no-op on
  freestanding targets.
- [Issue 1036](../issues/archived/1036-arena-exhaustion-is-half-silent-and-wholly-unreachable.md)
  (open, narrowed to silicon): since `823d8fb63` (2026-10-02, after the island's
  pin) every error-level `nros_log` record is counted in the boot record with the
  first one's file and line. That covers the Rust shim's liveliness-not-declared
  log, which is the island's "STILL SILENT" item 1 in its triage section 3.
  It does not cover the zenoh-pico C shim's own
  `printk("zpico: liveliness pool exhausted ...")` (`zpico.c:3293`), which goes
  to the unwired console. Route the C shim's pool-exhaustion lines to the record
  too, and have the island re-check its triage section 3 after its next pin bump.
- [Issue 1252](../issues/archived/1252-message-bound-knobs-have-no-pre-configure-twin.md)
  (open): the board build refused its own image on
  `NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS`.

**Done (checked 2026-10-24).** 1120, 1303, 1036 and 1252 are all resolved and
archived. The C shim's pool-exhaustion line needed no routing. The liveliness
`printk` in `zpico.c` is the only exhaustion `printk` in the shim, and its one
caller, `declare_entity_liveliness` in `nros-rmw-zenoh`'s `shim/session.rs`,
already logs an `nros_log` ERROR on that return. Since 1036 the boot record
counts every such ERROR, with its file and line. The `printk` is a second
copy of the same event for a console, if there is one. The island's
re-check of its triage section 3 after its next pin bump is the island's.

### I6 -- a console-less board has a documented log path

The island's triage section 1 needs four steps no error mentions to read a log
over RTT (clone the SEGGER module, `-DZEPHYR_EXTRA_MODULES`, `pyocd rtt` under
`script`, the `_SEGGER_RTT` address) and a larger up-buffer. nano-ros owns the
part every console-less board shares: a Zephyr guide page saying that the boot
record (`read-boot-report.py`) is the first channel, what it carries since
issue 1036's record v9, and the RTT recipe as the second; optionally an
`nros-rtt` snippet that sets the module and buffer. The board-specific half
(which UART is wired) stays the island's.

**Landed (2026-10-24):** book page
[Observing a Board With No Console](../../book/src/guide/deploy/log-and-observe.md).
It covers three channels, in order:
1. the boot record: what record v11 carries, and the `--addr-only` /
   `savemem` / decode commands;
2. `CONFIG_NROS_FAULT_LOG`;
3. RTT last, with the island's four-row table and the 16 KiB up-buffer.

The page says why the order matters: issue 0913 measured that attaching the
probe kills a serial zenoh session (resolved 2026-11-09 in phase-512: only a
halt past the 60 s lease drops it, and the board re-opens on its own; the page
says so). No `nros-rtt` snippet: the SEGGER module
is not in the west allowlist, and adding it is a module decision
(`[zephyr_module.*]`), not a documentation one.

### I7 -- the CLI names the grammar it embeds

`packages/cli/third-party/play_launch` is a submodule the CLI vendors, moved by
nano-ros PRs on its own schedule, while the island's CI pins a play_launch wheel
and its PATH holds a third copy (gap 2.3). `nros --version` (or a `--verbose`
form) prints the vendored play_launch commit and the rlm tag it parses with, so
"which grammar judged this contract" has an answer from the tool that judged it.
The play_launch-side `--version` fix is play_launch's.

**Landed (2026-10-24) on `nros version`, not `--version`.** `install.sh`,
`check-release-manifest.py` and the launcher's dispatch each read `--version`
as one line (`awk '{print $NF}'`), and `nros version` was already the
multi-line form (it prints the codegen version). It now also prints
`play_launch <sha>` (issue 0409's `NROS_PLAY_LAUNCH_SHA`) and
`ros-launch-manifest <tag>`. `build.rs` reads the tag from the
`ros-launch-manifest-types` row of the crate's own manifest. CLAUDE.md requires
one tag across every crate, so that row answers for all of them.

### I8 -- the serial-link pair is learnt from nano-ros, not from the island's conf

Every board that runs zenoh over a UART needs, today, two lines the island found
by measurement (board conf lines 683-700): `CONFIG_MAIN_THREAD_PRIORITY=5`
(Zephyr's 0 sits above the read task's band while main registers entities) and
`CONFIG_NROS_ZENOH_SERIAL_RX_RING_BYTES=4096` (default 1024, `zephyr/Kconfig:682-685`;
measured 793-1024 at joins, 1,226 in a soak). [phase-474](archived/phase-474-safety-island-board-findings.md)
T3 owns closing [issue 1534](../issues/archived/1534-zephyr-tx-flush-task-outranks-the-read-task.md) (resolved 2026-10-30; residue [issue 2164](../issues/2164-zephyr-serial-join-main-priority-and-rx-ring-default.md))
with a derived main priority and a measured ring default. This item is only the
DX half, and it is cheap: until T3 lands, the serial-link guide and the Kconfig
help name both lines and why; when T3 lands, the default moves under
`NROS_ZENOH_LINK_SERIAL` as a `configdefault` (the mechanism `3d52070ec` used
for snippet sizing), so a board conf can still override it, and D2's compare
reports a stated value below it.

**First half landed.**
- The `NROS_ZENOH_SERIAL_RX_RING_BYTES` help names the island's 4096 and the
  main-priority pairing.
- `zephyr/cmake/nros_rmw_zenoh.cmake` WARNS at configure when the main
  thread is not below the read task on a serial link, naming the number to
  use (issue 1534's configure half).
- The serial guide's new "Zephyr: a board that joins over a UART" section
  (2026-10-24) names both lines and why. Its stale Zephyr row (`uart_poll_*`
  only) now names the interrupt-driven ring of issue 0852.

The second half waits on phase-474 T3; issue 1534 stays open there.

### I9 -- an image's `profile` reaches the CMake road

**Status: DONE (2026-11-15).** [Issue 2593](../issues/archived/2593-cmake-road-drops-image-profile.md),
GitHub #3899. `[image.<id>] profile` is read only on the cargo road, and
`nros build` configures every non-posix CMake image as Release. The profile is
read through the CMake road's image facts. It maps to `CMAKE_BUILD_TYPE` and
the runtime crates' cargo profile (`NROS_CARGO_PROFILE`), and joins the build
coordinate. A value that maps to neither is refused, naming
`-- -DCMAKE_BUILD_TYPE=<type>`.

### I10 -- a generated Zephyr app adds the image's whole package closure

**Status: DONE (2026-11-15).** [Issue 2594](../issues/archived/2594-zephyr-generated-app-no-workspace-kconfig.md),
GitHub #3906, ruled option A on 2026-11-15, and this item is its
prerequisite. `builder/west_app.rs` `add_subdirectory`'d only the node
packages the launch file names. `launch_closure` in `cmd/build.rs` now returns
those plus every workspace package they `<depend>` on, transitively, from the
package graph `nros build` discovers: CMake packages (the native cmake root's
`routing::cmake_node` predicate, so interface packages stay out) and Zephyr
modules. A support package's own CMake therefore runs in a generated app as it
does in a hand-written one. The registered population and the message-bound
seed read the same closure.

### I11 -- a workspace package that is a Zephyr module brings its Kconfig

**Status: DONE (2026-11-15).** Issue 2594, option A. `builder/west_app.rs`
reads each closure package's `zephyr/module.yml` and appends the package to
`ZEPHYR_EXTRA_MODULES` before `find_package(Zephyr)`; a module whose CMake is
the package's own is not added twice. The ruling: a package in the image's closure (I10)
that carries `zephyr/module.yml` with a `kconfig:` entry joins
`ZEPHYR_EXTRA_MODULES` beside the nano-ros checkout (`builder/zephyr.rs`,
`module_arg`), so the symbols it defines are defined in the generated app and
an `os_config` fragment can assign them. The book pages that change:
`guide/platforms/zephyr-west-app.md`, `guide/platforms/zephyr.md`,
`guide/build/configure-an-image.md` (`os_config` assigns, a module defines)
and `guide/build/organize-a-larger-workspace.md` (a support package as a
Zephyr module).

### I12 -- configure and the model gate agree on a stale model

**Status: DONE (2026-11-15).** [Issue 2598](../issues/archived/2598-launch-param-file-not-a-model-input.md),
GitHub #3923. `nros configure` decided a model was current by its own stamp,
while every consumer asked the model gate, so a model the gate refused as
stale was never re-resolved and only `rm -rf build` cleared it. The configure
check now asks the gate (`workspace_model_missing` -> `model_gate::verify`).
The second half is play_launch's: the resolver does not record a `<param
from>` file as a model input, so a parameter-file edit was silent to both.
It is fixed in play_launch `191d949f`, pinned here in dd571ca75f.

### I13 -- a Rust package in a mixed image needs no registration of its own

**Status: DONE (2026-11-15), option A.** [Issue 2599](../issues/archived/2599-mixed-image-rust-package-needs-cmake-registration.md),
GitHub #3857. The CMake registration a Rust package needed in a mixed image
restated its `[[component]]` row. It is now derived from the row
(`builder/cargo_node.rs`, one rule for the native CMake root and the Zephyr
west app: `nano_ros_workspace(CARGO_NODES ...)` and `nros_register_cargo_node`).
A package that ships its own `CMakeLists.txt` keeps it, and `nros new` no
longer writes the shim.

### I14 -- each model is resolved for the target its images run on

**Status: DONE (2026-11-15).** [Issue 2601](../issues/archived/2601-resolve-ignores-image-target-platform-file.md).
`nros configure` resolved every model for the resolver's default `posix`, so
a Zephyr image's `<stem>.system.zephyr.yaml` was never read. rlm v0.3.22 moved
the supervision terms into that file, so this had a deadline: the island kept
a deprecated contract spelling that rlm's next release refuses. Each model now
gets `--target` from the images that resolve to it, by the board-platform rule
`[tasks.<name>.<rtos>]` uses; two targets sharing one model are named in a
warning.

### I15 -- a generated Zephyr app keeps its workspace modules

**Status: DONE (2026-11-15).** [Issue 2609](../issues/archived/2609-zephyr-app-workspace-module-lost-to-cache.md),
GitHub #3948. I11's modules were appended to a normal variable, and Zephyr
reads the `-DZEPHYR_EXTRA_MODULES` cache entry `nros build` passes before it,
so a workspace module never reached Kconfig. The app now adds its modules to
that cache entry.

### I16 -- a generated Zephyr app finds the workspace's message packages

**Status: DONE (2026-11-15).** [Issue 2610](../issues/archived/2610-zephyr-app-misses-workspace-msg-packages.md),
GitHub #3952. The CMake root puts the workspace's `src/` on
`NROS_INTERFACE_SEARCH_PATH`; the generated Zephyr app did not, so its
configure missed the workspace's message packages. Both generators now emit
one block (`cmake_root::interface_search_path_cmake`).

### I17 -- a sibling package's param file does not make the model unusable

**Status: DONE (2026-11-15).** [Issue 2615](../issues/archived/2615-model-gate-refuses-sibling-param-file.md).
I12 made a `<param from>` file a model input. A file in a sibling package can
only be recorded as an absolute path, and the model check refused every
absolute path, so the island stopped building. Only an absolute path under the
bringup is the legacy non-portable shape now; a sibling input is hash-checked.

### I18 -- the knob delivery check refuses a build after link

**Status: DONE (2026-11-16).** [Issue 2618](../issues/archived/2618-knob-delivery-check-false-mismatch-after-link.md).
The check now compares both sides in one spelling and leaves a stated value
to configure, which already reports it against the derivation. The island's
`board-build` rerun advice is the island's to fix.
`NROS_SUBSCRIBED_TYPE_BOUNDS` reads as a mismatch because the derived and
resolved lists use different separators, and a `max_publishers` override row
is flagged. The ELF is still produced.

### I19 -- a declaration past the parameter store's capacity stops the boot

**Status: DONE (2026-11-16).** [Issue 2634](../issues/archived/2634-param-store-full-declare-silently-uses-default.md),
GitHub #4018. `rclcpp::Node::declare_parameter<T>` returned the code default
when the store was full, so a launch value was lost in silence and the
island's controller (149 parameters) depended on a hand-counted
`max_parameters`. A full store now latches `set_error` (`NROS_CPP_RET_FULL`) in
C++, returns `RCLC_RET_FULL` in C and `Err` in Rust, and the runtime logs the
node, the parameter, the capacity and the knob in every spelling. The size is
already derived from the contract's `params:` when every node declares them;
deriving it from the census is not done.

### I20 -- a list parameter from a param file arrives as an array

**Status: DONE (2026-11-16).** [Issue 2638](../issues/archived/2638-launch-list-param-flattened-to-string.md).
The bake wrote a list as `a,b,c` and the image read it as one string, so the
island's MPC rate-limit lists reached the controller as strings. A list is now
baked as YAML flow text and seeded as an array of its element type; a list
ROS refuses is refused, and `[]` seeds nothing.

### I21 -- `RCLCPP_*_THROTTLE` on the clock argument, and `_ONCE` / `_SKIPFIRST`

**Status: DONE (2026-11-16).** [Issue 2636](../issues/archived/2636-rclcpp-throttle-once.md),
GitHub #4010. The island's vendored controller files log through
`RCLCPP_WARN_THROTTLE(get_logger(), *get_clock(), ms, ...)`, which was refused,
and use `_ONCE`, which did not exist. The throttle now reads the `clock`
argument and hands the reading to the C throttle rule
(`rclc_log_throttle_admit_at`); an invalid clock warns once and is throttled
on the platform's monotonic clock.
`_ONCE`, `_SKIPFIRST` and `_SKIPFIRST_THROTTLE` are added at every level, with
per-site static state. The island's own `std::map` throttle can go.

### I22 -- `ros2 param list` and `ros2 param dump` see every parameter

**Status: DONE (2026-11-16).** [Issue 2639](../issues/archived/2639-list-parameters-silently-caps-at-64.md).
`list_parameters` stopped at the generated message's 64 names and said
nothing, so the island's controller listed 64 of its 149 parameters, and
`get_parameters` refused the request `dump` sends for all of them. Both now
answer every name; without a `params:` declaration the service buffers are
sized for a full store, and the prefix and depth filters follow rclcpp.
The 70 parameters the interop case counts live on the fixture's own `/bulk`
node, not `/alpha`, whose list the ros2_cli parity cases compare with stock
([issue 2662](../issues/archived/2662-parity-params-bulk-on-alpha.md)).

### I23 -- board-facts resolves a workspace board as `nros build` does

**Status: DONE (2026-11-16).** [Issue 2637](../issues/archived/2637-board-facts-misses-workspace-boards.md).
`nros dev image board-facts` loaded the checkout's board catalog alone, so a
board package the workspace carries resolved for `nros build` and failed in
the west configure it spawns; the island exported `NROS_EXTRA_BOARD_PATH` to
get past it. Every verb now asks one lookup, `cmd::board::catalog_for_dir`.

### I24 -- a build dir follows the `nros` that builds it

**Status: DONE (2026-11-16).** [Issue 2652](../issues/archived/2652-codegen-tool-cache-outlives-invoker.md)
(GitHub #4067). A build dir configured once by the in-tree CLI kept that
binary as `_NANO_ROS_CODEGEN_TOOL` after a release `nros build` ran into it,
and the configure failed "in-tree nros CLI is STALE"; the island passed
`-D_NANO_ROS_CODEGEN_TOOL` on every configure to get past it. `nros build`
now passes its own path on every CMake configure it starts (cmake, west,
census). A bare `cmake` re-configure keeps the cache, and refuses a cached
tool whose `--codegen-version` the tree does not accept.

## Test / check

### T1 -- issue status sweep

The island document read several nano-ros issues as open that are not, and one
as open whose defect is fixed. First checked 2026-10-05; re-checked against
each issue file on 2026-10-24, when most of the table had closed under other
work:

| issue | status 2026-10-05 | status 2026-10-24 |
| --- | --- | --- |
| 1253, 1254, 1234, 1399, 1373 | open | resolved, archived |
| 1596, 1424, 1425, 0374, 1565 | resolved | resolved (1424 still native_sim only; 474 I5) |
| 1120 | open, defect fixed by `51fdfef44` | resolved, archived |
| 1368 | open | resolved, archived (a refusal on both roads) |
| 1490 | open | open -- forwarding, not stated-wins (D2); resolved 2026-11-12, re-measured on main, archived |
| 0758 | open | open; the link-only provider landed (D4), re-acquisition on the SNTP path and the FreeRTOS half (0772) remain |
| 1533, 0852 | open | resolved, archived |
| 1534 | open | open -- 474 T3 (I8's second half); resolved 2026-10-30, residue issue 2164 |
| 1303, 1036, 1252 | open | resolved, archived (I5) |
| 0934, 0941, 1121, 1419, 1489 | open | resolved, archived |
| 1371 | open | open |

**Done (2026-10-24).** Every row now matches its issue file.

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

- [x] D1, D2, D3, D4 each decided and written into this document (2026-10-24).
- [x] I1: the doctor FAILs on an `nros` outside the checkout, and the first
      recipe that needs them names a missing CLI, submodules and hooks at once.
- [x] I2: a Zephyr configure refuses an SDK from another checkout (narrowed
      from "that the lock does not name", D1.2); the west manifest half is
      issue 1379's refusal; issue 1254 resolved. No shared workspace exists
      on the development host any more, so there was none to re-provision.
- [x] I3: a derivable knob stated below its derived value warns at configure,
      naming both numbers and the stating rung (`06d1880a13`).
- [x] I4: every SUMMED pool knob prints its composition with values, and a test
      holds the sum equal to the value (D3 says which knobs are sums).
- [x] I5: issue 1120 resolved and archived; the pool-exhaustion event reaches
      the boot record through the Rust caller's ERROR log (checked, no change).
- [x] I6: a book page names the boot record, the fault record and the RTT
      recipe for a console-less board.
- [x] I7: `nros version` names the vendored play_launch commit and rlm tag
      (`--version` stays one line; I7).
- [ ] I8: the serial-link guide and Kconfig help name the main-priority and
      RX-ring lines (done) until 474 T3 makes them defaults (waits on T3).
- [x] I19: a declaration past a full parameter store stops the boot in C++
      (`set_error`), returns `RCLC_RET_FULL` in C and `Err` in Rust, and the
      logged line names the knob; a test per language (issue 2634).
- [x] I20: a list parameter from a `<param from>` file seeds as an array of
      its element type; the features `cpp_params` e2e reads each back (issue 2638).
- [x] I21: `RCLCPP_*_THROTTLE` measures on its `clock` argument (an invalid clock
      throttles on the platform's monotonic clock), and `_ONCE` / `_SKIPFIRST` /
      `_SKIPFIRST_THROTTLE` exist at every level; `log_conditional_runtime` (issue 2636).
- [x] I22: `list_parameters` answers every name and `get_parameters` takes a
      list as long as the store; stock `ros2 param list` / `dump` over 70
      parameters on the interop fixture's `/bulk` node (issues 2639, 2662).
- [x] I24: every CMake configure `nros build` starts names that `nros` as
      the codegen tool, and a bare re-configure refuses a cached tool outside
      the accepted codegen range; `build_verb_pipeline.rs` and
      `just check codegen-tool-cache` (issue 2652).
- [x] T1: every row of the status table matches its issue file (1120 archived).
- [ ] T2: the fresh-worktree run passes as stated; `image-facts` on the island
      re-run and its result recorded. Needs the island tree at a pin that
      carries this phase.
- [ ] T3: the island image shows the stated-below-derived message and a
      liveliness composition that sums to the resolved value. Needs the
      island's board build.
- [ ] D4 on the board: `nros: wall-clock epoch acquired from zenoh sample
      timestamps` on the S32K344 with `CONFIG_NROS_EPOCH_FROM_ZENOH=y`, and the
      hazard command's stamp compared with host time (the island's phase 9 W3).

## Open issues that name this phase

Open issues whose `related:` names this phase (listed 2026-10-12, so the
link runs both ways). Listing one here records the link, not a claim that
this phase will fix it.

- [issue 0772](../issues/0772-freertos-lwip-wall-clock-epoch.md) — FreeRTOS/lwIP has no wall-clock epoch — the same consumer that drove the Zephyr one runs a FreeRTOS board too
- [issue 0913](../issues/archived/0913-the-debugger-is-not-a-passive-instrument.md) -- attaching pyocd RTT kills the zenoh session. Moved to phase-512 and resolved there 2026-11-09: halts under the 60 s lease keep the session, longer ones recover without a router restart.
- [issue 1957](../issues/1957-s32ds-image-crate-unverified-in-ide.md) — s32ds builds its staticlib through the generated image crate since phase-506 W4a and has not been built in the S32 Design Studio since

## Issues homed here (2026-11-02)

- [issue 2165](../issues/2165-zephyr-smp-nros-log-stops-after-four-records.md): on the island's Zephyr SMP image (`fvp_baser_aemv8r_smp`), only the four early-ring `nros_log` records reach the console. Every later record is lost with no `report_lost` warning. The same sources log normally on FreeRTOS.

## Issues homed here (2026-11-16)

Filed from the Autoware Safety Island's phase-15 P3 on nano-ros 5b9e8844a4.

- [issue 2652](../issues/archived/2652-codegen-tool-cache-outlives-invoker.md) (GitHub #4067, resolved): a build dir kept the cached `nros` codegen tool after a different `nros build` ran into it (I24). Found on nano-ros 09219edc75.
- [issue 2662](../issues/archived/2662-parity-params-bulk-on-alpha.md) (GitHub #4091, resolved): two ros2_cli parity params cases failed once issue 2639's 70 bulk parameters were on `/alpha`, the node they compare with stock (I22).
- [issue 2639](../issues/archived/2639-list-parameters-silently-caps-at-64.md) (resolved): `ros2 param list` and `ros2 param dump` showed 64 of the controller's 149 parameters and said nothing (I22).
- [issue 2638](../issues/archived/2638-launch-list-param-flattened-to-string.md) (resolved): a list parameter from a `<param from>` file reached the node as one comma-joined string (I20).
- [issue 2640](../issues/2640-metadata-probe-no-launch-values.md): the C/C++ metadata probe constructs a node with no launch values and a default-size store, so a node declaring parameters without defaults cannot be probed.

## Issues homed here (2026-11-15)

Filed from the GitHub reports of one session scaffolding the L4 Safety Island
workspace (0.6.0-nros8, and main `37232ecff1` for the first). A seventh report
from that session, `FREERTOS_PORT not set` on `freertos-posix`, is
[issue 2341](../issues/archived/2341-freertos-posix-port-not-reaching-zpico.md),
fixed after that release.

- [issue 2636](../issues/archived/2636-rclcpp-throttle-once.md) (GitHub #4010, resolved): `RCLCPP_*_THROTTLE` was refused and `RCLCPP_*_ONCE` / `_SKIPFIRST` were missing. The throttle now measures on its `clock` argument (I21).
- [issue 2615](../issues/archived/2615-model-gate-refuses-sibling-param-file.md) (resolved): a model whose launch loads a param file from a sibling package was refused as non-portable (I17).
- [issue 2618](../issues/archived/2618-knob-delivery-check-false-mismatch-after-link.md): the knob delivery check refuses a build after link over a list separator and an override row (I18).
- [issue 2634](../issues/archived/2634-param-store-full-declare-silently-uses-default.md) (GitHub #4018, resolved): `declare_parameter` past a full parameter store returned the code default in silence. It now stops the boot and names the knob (I19).
- [issue 2609](../issues/archived/2609-zephyr-app-workspace-module-lost-to-cache.md) (GitHub #3948, resolved): a generated Zephyr app lost its workspace Zephyr modules to the `-DZEPHYR_EXTRA_MODULES` cache entry (I15).
- [issue 2610](../issues/archived/2610-zephyr-app-misses-workspace-msg-packages.md) (GitHub #3952, resolved): a generated Zephyr app did not find the workspace's own message packages (I16).
- [issue 2601](../issues/archived/2601-resolve-ignores-image-target-platform-file.md): `nros configure` resolved every model for `posix`, so an image's `<stem>.system.<target>.yaml` was never read (I14, resolved).
- [issue 2603](../issues/2603-flaky-sdk-index-stamp-input-test.md): `configure_stamp::tests::the_sdk_index_is_a_stamp_input` failed once under a full gate and passes alone.
- [issue 2598](../issues/archived/2598-launch-param-file-not-a-model-input.md) (GitHub #3923, resolved): a resolved model stayed stale after `nros configure`, and a launch parameter file was not one of its inputs (I12).
- [issue 2599](../issues/archived/2599-mixed-image-rust-package-needs-cmake-registration.md) (GitHub #3857, resolved): a Rust package in a mixed image needed a CMake registration that restated its `[[component]]` row. It is now derived from the row (I13).
- [issue 2564](../issues/archived/2564-hand-entry-bringup-image-drops-domain-id.md) (resolved): a hand-written entry naming `BRINGUP` and `IMAGE` baked domain 0, not the image's `domain_id`. It now asks the image's domain of the one reader, under the agreement rule.
- [issue 2565](../issues/archived/2565-mixed-image-rust-pkg-needs-cmake-shim.md) (resolved): a mixed image's Rust package needed an undocumented CMake shim. `nros new --to` writes it now, the book states it, and the contract row was issue 2376's fix.
- [issue 2566](../issues/archived/2566-sync-ignores-nested-packages-and-package-paths.md) (resolved): `nros sync` ignored `src/<group>/<pkg>` and `package_paths`. The scan now walks `src/` and every `package_paths` entry recursively, as `nros build` does.
- [issue 2567](../issues/archived/2567-rust-codegen-msg-constants-not-reexported.md) (resolved): Rust codegen did not re-export `.msg` constants. They are associated consts of the type now (`IslandMode::MODE_NOMINAL`).
- [issue 2568](../issues/archived/2568-rust-ctx-param-misses-launch-values.md) (GitHub #3860, resolved): a Rust `ctx.param()` got the default, not the launch value, in a C++ entry, because the Rust component install seam carried no params. It now carries them as a flat C-string array. The census `param-phantom` half is a separate cause (a `ctx.param` read is not a recorded declaration) and stays open.
- [issue 2569](../issues/archived/2569-cpp-metadata-probe-assumes-class-header.md) (resolved): the C++ metadata probe built a `configure`-shape class with the `NodeHandle` constructor (fixed on main by issue 2339 after 0.6.0-nros8), and derived a nested namespace's header differently from the entry.
- [issue 2582](../issues/archived/2582-cmake-workspace-road-drops-image-knob-rows.md) (resolved): on the hand-written `nano_ros_workspace()` road, `[image.<id>.knobs.*]` rows were silently ignored. The root now claims the image it names, so the rows reach the cargo carrier, and it refuses when it names none.
- [issue 2584](../issues/archived/2584-configure-ignores-no-marker.md) (GitHub #3871, resolved): `nros configure` resolved a package that carried an ignore marker, so a contract-only bringup kept under `src/` failed the whole workspace. The marker list now has one home, `nros_pkg_index::is_ignored_dir`, and every package walk asks it.
- [issue 2586](../issues/archived/2586-cpp-alias-header-digit-capital-name.md) (GitHub #3882, resolved): the ROS-style C++ alias header missed rosidl's name when a digit or an acronym came before a capital (`float32stamped.hpp` for `Float32Stamped`). The alias now takes rosidl's name through one function, `naming::ros_alias_header`.
- [issue 2589](../issues/archived/2589-census-ctx-param-read-unseen.md) (GitHub #3860, resolved): the census did not see a Rust node's `ctx.param()` read, so its contract row read `param-phantom`. A read is now recorded and claims the row. Split from issue 2568.
- [issue 2590](../issues/2590-cpp-entry-rust-arm-drops-node-identity.md): a Rust component installed by a C++ entry ignores the launch `<node name= namespace=>`. Found fixing issue 2568.
- [issue 2593](../issues/archived/2593-cmake-road-drops-image-profile.md) (GitHub #3899, resolved): `[image.<id>] profile` was silently ignored on the CMake road. It now picks the build type and the runtime cargo profile, and is refused where it cannot apply (I9).
- [issue 2594](../issues/archived/2594-zephyr-generated-app-no-workspace-kconfig.md) (GitHub #3906): a generated Zephyr west app cannot get the workspace's own Kconfig symbols. Ruled option A on 2026-11-15; resolved by I10 and I11.
