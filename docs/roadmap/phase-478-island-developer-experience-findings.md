# Phase 478 -- the island's developer-experience findings: borrowed toolchains, silent knobs, derivations without arithmetic

**Status (2026-11-23). IN PROGRESS. Owner: the ASI port session ("ASI
port").** Since 2026-11-23 this is THE island phase: every nano-ros issue and
missing feature the safety island (autoware-safety-island fork) needs for its
migration to nano-ros is a row of
[The island port's needs](#the-island-ports-needs-2026-11-23) below, either a
work item here (I36-I52, T4) or a "tracked by" line naming where it lives.
I45 landed on the island (2026-11-23). The open items, in order: I50, I51,
I52, I36, I37, I38, I41, I49, I43, I44, I40, I39, I46, I47, I48; I42 and
then I26's island switch; T4 after each. T2, T3 and D4's on-board check below are older and stay open. Done
means the island's branch `nano-ros-next` drops the workaround each row
names. Items claim with `just claim phase-478-<item>`.

Before that, **Status (2026-10-24). IN PROGRESS -- D1-D4 decided, I1-I8 landed; T2 and
T3 wait on the island's board.** Refreshed 2026-10-24: D2/I3 and the first half
of I8 had already landed under phase-474 I5 and issue 1534's configure warning,
and most issues the 2026-10-05 text cited as open are resolved (T1). What is
left needs the S32K344 and the island tree: the fresh-worktree and island
`image-facts` re-run (T2), the stated-below-derived build of the island image
(T3), and on-board confirmation of D4's epoch. I8 is complete (2026-11-20):
phase-474 T3 closed issue 1534 with a configure refusal instead of a new
default, so the help text is where the pair stays. I9-I11 were added 2026-11-15, from the island's GitHub reports:
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
| 5.4 | no board time source over serial | here D4 (Zephyr landed); issue 0758; the generic board clock seam is [phase-582](phase-582-wall-clock-source.md) |
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
T3 owns closing [issue 1534](../issues/archived/1534-zephyr-tx-flush-task-outranks-the-read-task.md) (resolved 2026-10-30; residue [issue 2164](../issues/archived/2164-zephyr-serial-join-main-priority-and-rx-ring-default.md))
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

**Second half (2026-11-20).** Phase-474 T3 closed issue 1534 without moving
either default: the main priority is Zephyr's symbol and Kconfig cannot derive
`read_k + 1`, so the configure refuses a serial image whose main thread is not
below the read task and prints the number (issue 2164), and the RX ring stays
1024 because no lane measures a serial join. The `configdefault` this item
planned is therefore not built, and the help text carries the pair for good:
- `ZENOH_READ_PRIORITY`'s help no longer says the read task sits below the
  main thread (true only at Zephyr's main 0, the case a serial image refuses),
  and names `CONFIG_MAIN_THREAD_PRIORITY=5`, the refusal and the RX ring.
- `NROS_ZENOH_SERIAL_RX_RING_BYTES`'s help states the main-priority line and
  why, with no issue ids or project names in its text.
- The serial guide's Zephyr step already names both lines and the refusal.

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

### I25 -- a PATH change does not rebuild the launch resolver

**Status: DONE (2026-11-17).** [Issue 2660](../issues/archived/2660-pyo3-rebuilds-on-path-change.md).
With `PYO3_PYTHON` unset, pyo3-build-config registers PATH as a rebuild
trigger, so `just setup-launch-resolve` rebuilt pyo3-ffi and the resolver
(about 30 s on CI) whenever PATH differed, as it does for a consumer that puts
its `nros` on PATH. Every recipe that builds a pyo3 crate now pins
`PYO3_PYTHON` to the interpreter pyo3 would find (`scripts/build/pyo3-python.sh`),
and `just check pyo3-path-stable` holds a prepended PATH entry to a Fresh
pyo3-ffi.

### I26 -- a toolchain pin can name a commit or a checkout

**Status: DONE in nano-ros (2026-11-20); the island's switch is its own.**
[Issue 2645](../issues/archived/2645-toolchain-pin-cannot-name-a-commit.md)
(GitHub #4006). `nros-toolchain.toml` pins a release only, so the island,
which tracks main, vendors nano-ros, builds the CLI itself and sets
`NROS_SKIP_STALE_CHECK=1` because `nros configure` stales the checkout it
runs from. Built: `git = "<url>"` with `rev = "<sha>"` (cargo's spelling)
and `path` beside `version`, exactly one form set; a git pin is fetched and
built into `toolchains/git-<rev>/` on first use. The
reported stale-guard misfire did not reproduce (2645 part B): the island's
own script turned the guard off over a submodule its nested gitlinks made
read as dirty, so there is no nano-ros guard defect to fix, and with a commit
pin the island drops both its CLI build and the skip.

### I27 -- an SDK Eigen 3.4 that builds on arm-none-eabi

**Status: WONTFIX (user ruling 2026-11-17).** [Issue 2646](../issues/archived/2646-sdk-eigen-for-arm-none-eabi.md)
(GitHub #4013). Eigen 3.4's `psincos` does not compile where newlib's
`int32_t` is `long`; the island patches a copy of the host's Eigen. nano-ros
does not ship patched general-purpose C++ libraries: a user brings and
patches their own, which a nano-ros package already supports. The
documentation gap the issue recorded is closed (2026-11-17): the book's
[Bring your own C++ library](../../book/src/guide/build/bring-your-own-cpp-library.md)
page adds a user's Eigen as a workspace package (the shape a ported project
ships), or finds it in a prefix they own, and the public
`nano_ros_hosted_cxx()` makes a node package's C++ hosted on a freestanding
FreeRTOS toolchain, held by `just check hosted-cxx-opt-in`.

### I28 -- a string array's element count is its own bound

**Status: DONE (2026-11-17).** [Issue 2647](../issues/archived/2647-param-slot-sized-by-string-array.md).
A store slot is as large as its largest value, the string array, and that was
the product of the string length and the ARRAY length. So an image with a
`string` and a `double_array`, and no string array, paid 32 x 256 B in every
slot: 285,696 B at 32 slots against 23,552 B without the product. The string
array now has its own element bound, `MAX_STRING_ARRAY_LEN`, derived to 0
unless a `string_array` is declared. Follow-ups: issues 2649 (a shared string
arena) and 2650 (the per-slot description).

### I29 -- an application trace marker through the installed sink

**Status: DONE (2026-11-16).** [Issue 2641](../issues/archived/2641-app-trace-marker.md),
GitHub #4009. `rclc_trace_marker(id, arg)`, `rclcpp::trace_marker` and
`nros::trace_marker::emit_app_marker` emit an application event through the
installed callback trace sink, on the executor events' path. Application ids
are 1..=15, the block the executor's 16-27 were placed above; any other id is
refused. Zephyr's default sink carries it to CTF; FreeRTOS, POSIX, NuttX and
ThreadX need a sink the application installs.

### I30 -- a board build's executable that is not an image

**Status: DONE (2026-11-17).** [Issue 2678](../issues/archived/2678-plain-executable-opt-out-on-a-board.md),
GitHub #4016. On a board every `add_executable` is an image (phase-518), so
the island's SocketCAN test driver set the private
`_NROS_IN_IMAGE_ADD_EXECUTABLE` to keep the builtin.
`nano_ros_add_plain_executable(<name> <sources>...)` is the public verb: the
builtin on every platform, marked `NROS_PLAIN_EXECUTABLE`.

### I31 -- a board network file on a Zephyr image

**Status: DONE (2026-11-17).** [Issue 2679](../issues/archived/2679-zephyr-board-network-file-net-config.md),
GitHub #4011. A board network file was refused on Zephyr, so the island stated
its address in its own Kconfig symbols and waited for DHCP by hand. A Zephyr
board now declares its interface in `[board.net]` (`fvp-aemv8r-smp`,
`mps2-an385-zephyr`), and `nros build` renders the one resolved interface into
`NET_CONFIG_*` / `NET_DHCPV4` as `build/nros/net/<image>.conf`, appended last,
refusing a second interface, a stated MAC and an `os_config` fragment that
sets the same symbols (naming both sources). RFC-0103 D10d's locator refusal
now applies only to an RMW whose descriptor states a host peer, so a Cyclone
DDS image is not refused.

### I32 -- variants of one host may share a static address

**Status: DONE (2026-11-22).** [Issue 2690](../issues/archived/2690-image-alternatives.md),
[RFC-0136](../design/0136-image-alternatives.md), built in
[phase-586](phase-586-image-alternatives.md). The island builds its TAP
image four ways (`fvp-tap`, `-stats`, `-trace`, `-trace-ring`), all at
`192.168.10.2`, and boots one. `check_collisions` refuses the shared address,
so the images cannot state it in a board network file (I31). A top-level
`[image_alternatives] <group> = [ids]` declares the images mutually exclusive;
the check lets one group share an address and MAC and still refuses any two
images that can run together. Never inferred from names.

## The island port's needs (2026-11-23)

The safety island (autoware-safety-island fork) is porting to current
nano-ros in its phase 15 ("a platform-agnostic island"). Its branch
`nano-ros` builds on the released nros11 CLI; its branch `nano-ros-next`
pins main (`8d4e70a382`, 2026-11-23) and builds the CLI from source. Its
phase-15 doc (`documentation/roadmap/phase-15-platform-agnostic-island.md` on
`nano-ros-next`) lists what still waits on nano-ros, by its own GitHub tracker
entries #4005 to #4018 and by nano-ros issue ids. This section is the single list
of those needs: each is a work item below or a "tracked by" row. Inventory
read on island `nano-ros-next` `f6c8bef` and nano-ros main `a1cbbb6b04`.

Done for a row means: the island's `nano-ros-next` deletes the workaround the
row names, and its three CI lanes (Zephyr FVP, FreeRTOS POSIX, FreeRTOS
AN536) stay green.

### The island's tracker issues, mapped

| island tracker | nano-ros | state on main (2026-11-23) | what the island still does |
| --- | --- | --- | --- |
| #4005 | 0758, 0772, 2165, 2865 | 0758, 0772 open, their asks built by [phase-582](phase-582-wall-clock-source.md) (W1 sources, W1d `board`, W2 libc shims, W3 SNTP re-sync); 2165 resolved; 2865 resolved (PR #4639, archived) | P5 done on `nano-ros-next` over phase-582; I49 closes the issue files |
| #4006 | 2645 (I26), 2770 (I34), 2771 (I35) | resolved | waits on a release the prebuilt launcher can read (I42), then I26's switch |
| #4007 | 2629 | resolved | controller images take FVP params from rows; the test programs are images since 2861-2864; `build.sh`'s `test/zephyr_app` builds (tap UDP tunnel, trace, stats) are the island's to turn into images (T4) |
| #4008 | 2766 (I33) | resolved (PR #4462) | 0001 and 0002 removed; 0003 is issue 2878 (I38) |
| #4009 | 2641 (I29) | resolved | `asi_common/include/common/diag/trace_marker.hpp` still calls `sys_trace_app_marker` itself; the island's to switch to `rclcpp::trace_marker` (T4) |
| #4010 | 2636 (I21) | resolved | done on `nano-ros-next` (P4) |
| #4011 | 2679 (I31) | resolved | FVP images use board network files (P6); the S32Z half is issue 2879 (I39) |
| #4012 | 2633 | open (RTD strong symbols, `board`-stack `[canN]` hand-off) | P7 done over the driver; the S32Z2 hardware backend is I47 |
| #4013 | 2646 (I27) | wontfix | the island's `src/eigen` workspace package; nothing waits |
| #4014 | 2631 | resolved (PR #4086) | used on `nano-ros-next`; the module pins are issue 2880 (I40, resolved) |
| #4015 | 2632 | open (NETC netif, port patch, full link) | I46 |
| #4016 | 2678 (I30) | resolved | used on `nano-ros-next` |
| #4017 | 2630 | resolved (PR #4039) | used on `nano-ros-next` |
| #4018 | 2634 (I19) | resolved | used on `nano-ros-next` |

### Cross-reference: every need, and where it is tracked

| need (island evidence on `nano-ros-next`) | status | tracked by |
| --- | --- | --- |
| metadata probe adds a node's workspace `<depend>`s (controller_pkg `CMakeLists.txt:22-39` and the seven `src/autoware/*` fallbacks) | fixed on main, PR #4153, in the island's pin | issue 2680 ([phase-568](phase-568-build-args-and-profiles.md) W3); the island deletes the fallbacks (T4) |
| a workspace root adds only its bringup's closure (controller_pkg `NROS_BRINGUP_DIR` guard, `:13-17`) | fixed on main, `976970a3da` | issue 2754; T4 |
| `nros run` of a hand-written C/C++ entry (`.github/scripts/ci-helpers.sh` `nros_run_program`, `--plan` with the ELF added) | fixed on main, `68aaaaebe0` | issue 2752; T4 |
| the probe seeds launch values (C++ `declare_parameter` defaults kept, P3) | fixed on main, `a7574ca6eb` | issue 2640 ([phase-566](phase-566-probe-launch-seeds.md)); T4 |
| Zephyr root adds packages in `<depend>` order (the Autoware add-once guards) | fixed; the island dropped the guards | issue 2756 |
| owned west entry: deploy, workspace modules, nested bringup census, `os_config` base | fixed; used on `nano-ros-next` | issues 2861, 2862, 2864 |
| owned west entry: `NROS_INTERFACE_SEARCH_PATH` (`asi_tests/asi_*_zephyr/CMakeLists.txt:39-40`) | fixed in nano-ros (2876 resolved); the island drops its lines next | issue 2876, I36 |
| owned entry that never starts nano-ros on the host gets a census (`can_output_test_bringup` `[census] on_missing = "warn"`) | fixed, PR #4671; the island states `runs_nano_ros = false` on `nano-ros-next` `56b9971` | 2871 (archived), I45 (done) |
| owned entry census adds the entry's workspace `<depend>`s (FreeRTOS `asi_unit_test` / `asi_dds_loopback_test` `add_subdirectory(asi_common)` fallback) | open; issue 2680 fixed the probe only | issue 2899, I50 |
| owned census goes stale when the entry's CMakeLists changes (a stale census masked issue 2899 locally) | open | issue 2900, I51 |
| the probe seeds the parameters instances agree on when they disagree on others (controller's 148 `declare_parameter` defaults, P3) | open | issue 2901, I52 |
| `nros run` `--forbid`, `--settle`, `--zephyr-workspace` | fixed; used on `nano-ros-next` | issue 2863 |
| the Zephyr workspace ladder skips the project's west workspace (`run-zephyr-fvp-ci.sh:202-213`, `--zephyr-workspace` on every call) | fixed in nano-ros (2877 resolved); the island drops its flag next | issue 2877, I37 |
| the island's last Zephyr patch, an out-of-tree tracing backend (`patches/zephyr/0003-*`, `scripts/apply-zephyr-patches.sh`) | declined in 2766, NOT FILED as its own question | issue 2878, I38 |
| S32Z network: board network file, VLAN, second interface (`network_config.cpp`, the last `PLATFORM_*` reader; the DHCP sleep in `board_network_hook.cpp`) | NOT FILED (2679 D5 cut it) | issue 2879, I39 |
| S32Z Zephyr module pins (`actuation_module/west.yml`: `cmsis`, `hal_nxp`, `zephyr-lang-rust` by hand) | fixed in nano-ros (2880 resolved); the island imports the fragment next | issue 2880, I40 |
| compile-time log floor (`common/logger/logger.hpp`, `CONFIG_LOG_LEVEL`, P4) | NOT FILED | issue 2881, I41 |
| a release past nros11 (branch `nano-ros` on `0.6.0-nros11`; everything else on `nano-ros-next` with a source CLI) | NOT FILED | issue 2882, I42; planned by [phase-590](phase-590-nros12-release-plan.md) |
| `halt` after `platform` on a "none" port is unreachable (island writes `sntp` alone, P5) | NOT FILED | issue 2883, tracked by [phase-582](phase-582-wall-clock-source.md) |
| SNTP re-sync paced on guest time (`sntp_resync_interval_ms = 0` on every image, P5) | NOT FILED | issue 2884, tracked by [phase-582](phase-582-wall-clock-source.md) |
| SNTP re-sync thread stack fixed at 2048 B (7024 B measured on the FVP) | filed with its fix, in review (PR #4632; the issue file lands with it) | [phase-582](phase-582-wall-clock-source.md) W3; I44 needs it |
| `board` clock re-asked on an interval | open follow-up; the island does not need it today | issue 2866 ([phase-582](phase-582-wall-clock-source.md)) |
| FreeRTOS wall clock resolves after the network (`fallback_settle_ms = 6000` on an536, s32z2) | fixed, PR #4639; resolved and archived | issue 2865 ([phase-582](phase-582-wall-clock-source.md)); I49 |
| board-owned CAN frame driver (P7) | done for the island's boards; open for the S32Z2 RTD backend | issue 2633 ([phase-560](phase-560-contract-revision-follow.md) W9), I47 |
| s32z270-freertos RTD variant: NETC netif, the NXP `port.c` Thumb-resume patch (`s32z2_board_glue/vendor_patched/port.c.patch`), a full link | open | issue 2632, I46; hardware and twin halves in [phase-512](phase-512-on-hardware-measurements.md) and [phase-524](phase-524-emulator-twins.md) W3 |
| zenoh-pico CAN link over the frame driver | open; not an island need (it runs Cyclone DDS) | issue 2769, I48 |
| images that never run together share an address (the four TAP images) | done | issue 2690, [phase-586](phase-586-image-alternatives.md) |
| the Arm FVP is user-provided | done; the island provisions its own | issue 2802 |
| zenoh CAN links on hardware; a generated silence watchdog | not island needs (no zenoh CAN; the island has its own staleness gate) | issues 2488, 2572 |
| FVP board conf values below the island's (`prj-fvp.conf` heap, pthread, net buffers) | the image's own sizing, stated per image; nothing to file | -- |

### I36 -- an owned west entry gets the workspace's interface search path

- **Issue:** [2876](../issues/archived/2876-owned-west-entry-no-interface-search-path.md).
- **What:** the owned entry configures with the workspace's `src/` on
  `NROS_INTERFACE_SEARCH_PATH`, as a generated app does.
- **Acceptance:** 2876's (fixture with no line in its CMakeLists; owned and
  generated carriers held to one search path); the island deletes the three
  `set(NROS_INTERFACE_SEARCH_PATH ...)` lines.
- **Status:** the nano-ros half is done: `nros build` passes the search path
  on the owned entry's west line (issue 2876, resolved). The island's
  deletion waits on its pin.

### I37 -- the Zephyr ladder finds the project's west workspace

- **Issue:** [2877](../issues/archived/2877-zephyr-workspace-ladder-skips-project-west-workspace.md).
- **What:** a rung for the west workspace the nano-ros workspace sits in,
  above the store's; the build names the tree it took.
- **Acceptance:** 2877's; the island's FVP lane runs `nros build` / `nros run`
  with no `--zephyr-workspace`.
- **Status:** the nano-ros half is done: the ladder takes the project's west
  workspace below `$NROS_ZEPHYR_WORKSPACE` and above the store, and says so
  (issue 2877, resolved). The island's deletion waits on its pin.

### I38 -- a ruling on the out-of-tree tracing backend patch

- **Issue:** [2878](../issues/archived/2878-zephyr-tracing-custom-backend-patch.md).
- **What:** nano-ros carries the 3.7 patch beside 2766's, or the book
  documents a consumer's own Zephyr patch over a provisioned tree.
- **Acceptance:** 2878's; under the first answer the island deletes
  `patches/zephyr/` and `scripts/apply-zephyr-patches.sh`.
- **Status:** done on the nano-ros side: ruled the first answer. Both patch
  sets (3.7 and 4.4) add `CONFIG_TRACING_BACKEND_CUSTOM(_NAME)`, and
  `just zephyr test-tracing-custom-backend` shows an image's own backend
  receive the CTF stream on both lines (issue 2878, resolved). The island's
  deletion waits on its pin.

### I39 -- the S32Z network from a board network file

- **Issue:** [2879](../issues/2879-s32z270-rtu0-zephyr-no-board-network.md).
- **What:** `[board.net]` on `s32z270-rtu0-zephyr`; a VLAN tag (and, if
  agreed, a second interface) in a board network file, or a board-owned hook.
- **Acceptance:** 2879's configure check here; the tagged bring-up measured
  under [phase-512](phase-512-on-hardware-measurements.md) or on the
  [phase-524](phase-524-emulator-twins.md) twin (linked, not done here); the
  island deletes `network_config.cpp` and its `PLATFORM_*` defines.

### I40 -- a downstream west fragment for the S32Z270 Zephyr board

- **Issue:** [2880](../issues/archived/2880-s32z270-rtu0-zephyr-no-west-downstream.md).
- **What:** `west-downstream.yml` with `zephyr-lang-rust`, `hal_nxp`, `cmsis`,
  held in lockstep by a check.
- **Acceptance:** 2880's; the island imports it and keeps no hand pin but
  `zephyr` and nano-ros.
- **Status:** the nano-ros half is done: the board ships the fragment
  (`zephyr-lang-rust`, `hal_nxp`, `cmsis`), `just check
  zephyr-west-downstream` holds it to `west.yml` and the Zephyr tree's pins,
  and the module-fixtures job fetches through it (issue 2880, resolved). The
  island's import waits on its pin.

### I41 -- a compile-time log floor

- **Issue:** [2881](../issues/archived/2881-rclcpp-log-min-severity-missing.md).
- **What:** `RCLCPP_LOG_MIN_SEVERITY` with upstream's meaning.
- **Acceptance:** 2881's (`just check cpp`: arguments below the floor are not
  evaluated); the island's `log_*` shim loses its `CONFIG_LOG_LEVEL` gating.
- **Status:** the nano-ros half is done: `RCLCPP_LOG_MIN_SEVERITY` and its C
  twin `RCLC_LOG_MIN_SEVERITY`, set by a compile definition (issue 2881,
  resolved). The island's deletion waits on its pin.

### I42 -- a release past nros11

- **Issue:** [2882](../issues/2882-nros12-release-for-consumers.md).
- **What:** the next release cut once the release lanes are green. The plan
  (contents, blockers, steps, a recommended window) is
  [phase-590](phase-590-nros12-release-plan.md), owned by its release
  planner; the user sets the date. This phase supplies its island-shaped
  blocker (phase-590's B4, PR #4688).
- **Acceptance:** a published release; the island folds `nano-ros-next` into
  its default branch on it, then I26's island switch (a commit pin) is
  possible.

### I43 -- an unreachable `halt` is refused or warned

- **Issue:** [2883](../issues/archived/2883-zephyr-clock-halt-unreachable-after-platform.md) (resolved 2026-11-24, superseded by RFC-0141 D1's amendment).
- **Tracked by:** [phase-582](phase-582-wall-clock-source.md), which owns
  RFC-0141 and took the issue (2026-11-23). Linked here, not worked here.

### I44 -- SNTP re-sync usable on a simulator

- **Issue:** [2884](../issues/2884-sntp-resync-paced-on-guest-time.md); needs
  PR #4632 (the SNTP re-sync stack as a knob, with its issue file) first.
- **Tracked by:** [phase-582](phase-582-wall-clock-source.md), which took
  the issue (2026-11-23). The island's part stays in T4: a non-zero
  `sntp_resync_interval_ms` on its FVP images once 582 rules.

### I45 -- a census for an owned entry that never starts nano-ros

- **Issue:** [2871](../issues/archived/2871-census-for-entry-without-nano-ros.md),
  resolved by PR #4671 (ruled option (a): `[image.<id>] runs_nano_ros =
  false`). Left: the island's swap on its next pin.
- **Acceptance:** 2871's; the island's `can_output_test_bringup` drops
  `[census] on_missing = "warn"`.
- **Done (2026-11-23):** the island's `nano-ros-next` `56b9971` states
  `runs_nano_ros = false` on the CAN image (`can_output_test_bringup`) and
  drops `on_missing = "warn"`.

### I46 -- the s32z270-freertos RTD variant, finished

- **Issue:** [2632](../issues/2632-s32z270-freertos-rtd-variant.md), "Still
  open" 1-3: the NETC netif glue, the port patch (a licence answer first),
  a full image link.
- **Needs:** a host with the NXP trees; the hardware run belongs to
  [phase-512](phase-512-on-hardware-measurements.md), the board descriptor to
  [phase-524](phase-524-emulator-twins.md) W3 (linked, not absorbed).
- **Acceptance:** 2632's three items; the island's `[image.s32z2]` names
  `s32z270-freertos-rtd` and deletes `src/s32z2_board_glue/`, the
  `ASI_S32Z2_NXP_PORT` switch and `scripts/provision-nxp-freertos.sh`'s env.

### I47 -- the S32Z2 CAN frame driver over the vendor drivers

- **Issue:** [2633](../issues/2633-board-owned-can-frame-driver.md) (design
  home [phase-560](phase-560-contract-revision-follow.md) W9): the
  `s32z270-freertos` strong symbols over the RTD CAN driver, and the
  `board`-stack `[canN]` hand-off.
- **Needs:** I46's NXP trees.
- **Acceptance:** 2633's; the island's `s32z2` image states a CAN device and
  sends frames with no island backend.

### I48 -- the zenoh-pico CAN link over the frame driver

- **Issue:** [2769](../issues/2769-zenoh-can-link-over-frame-driver.md). Taken
  here because it is island-shaped (one device owner on a `board`-stack
  board); the island itself runs Cyclone DDS, so it is last.
- **Acceptance:** 2769's.

### I49 -- the clock issues closed against phase-582

- **What:** issues 0758 and 0772 asked for what phase-582 built (sources, the
  `board` hook, the FreeRTOS libc shims, SNTP re-sync); issue 2865's fix
  merged (PR #4639). Record the island's evidence (its tracker #4005; CI run
  45075448610 at main `9e9deb715f`: FVP "nros: wall clock from sntp", AN536
  "nros: wall clock from board", "Stamp clock: absolute ... synced") in each
  and resolve them, or name what is still missing. 2866 stays a phase-582
  follow-up.
- **Acceptance:** 0758, 0772 and 2865 resolved and archived, or each names
  its remaining gap.

### I50 -- an owned entry's census adds its workspace `<depend>`s

- **Issue:** [2899](../issues/2899-owned-census-omits-entry-workspace-depends.md).
- **What:** the owned census host build adds the entry's workspace
  `<depend>` closure, through the helper issue 2680 gave the probe.
- **Acceptance:** 2899's; the island's FreeRTOS test entries
  `asi_unit_test` and `asi_dds_loopback_test` drop their
  `add_subdirectory(asi_common)` fallback.

### I51 -- an owned census reads stale when the entry's CMakeLists changes

- **Issue:** [2900](../issues/2900-owned-census-freshness-ignores-entry-cmakelists.md).
- **What:** the entry's `CMakeLists.txt` and `package.xml` are freshness
  inputs of its owned census, by content digest.
- **Acceptance:** 2900's test. Land with or before I50, so its island
  check runs on a fresh census.

### I52 -- probe seeds per parameter when instances disagree

- **Issue:** [2901](../issues/2901-probe-seeds-all-or-nothing-on-disagreement.md).
- **What:** a ruling between (a) seed the agreed keys and leave only the
  disagreeing ones on declaration defaults, and (b) one probe per model;
  then the build.
- **Acceptance:** 2901's; the island's controller drops its declaration
  defaults except where the ruling still needs them (today the
  `wheel_base`, `max_pitch_rad`, `min_pitch_rad` split between
  `system_model.yaml` and `system_carla_model.yaml` keeps all 148).

### I53 -- the Zephyr hook takes a level name for `log.max_level`

- **Issue:** [2951](../issues/2951-zephyr-log-max-level-refuses-level-name.md).
- **What:** a level name stated for `log.max_level` reaches Kconfig as its
  index, as `knobs.toml`'s `names` column documents and the cargo road
  already accepts.
- **Acceptance:** 2951's test; the island's Zephyr images write
  `log.max_level = "info"` instead of `2`.

### I54 -- the CMake road exports the image's `log.max_level`

- **Issue:** [2952](../issues/2952-cmake-road-log-max-level-not-exported.md).
- **What:** every CMake-road configure (FreeRTOS POSIX and AN536, S32Z2,
  linux census images) defines the image's resolved ceiling as
  `NROS_LOG_MAX_LEVEL` (0..6), the twin of `CONFIG_NROS_LOG_MAX_LEVEL`.
- **Acceptance:** 2952's test; the island derives `RCLC_LOG_MIN_SEVERITY` /
  `RCLCPP_LOG_MIN_SEVERITY` from it on those images instead of a fixed INFO.

### T4 -- the island drops each workaround

- **What:** for every row above marked fixed, and for each item as it lands,
  the island's `nano-ros-next` deletes the workaround and its CI stays
  green. The rows fixed on main today: the probe fallbacks (2680), the
  bringup guard (2754), `nros_run_program`'s `--plan` (2752), the C++
  parameter defaults (2640), the test entries' `asi_common` fallback (2680,
  2861; verify), the trace marker call (2641), the `test/zephyr_app` builds
  as images (2629, 2861-2864).
- **Acceptance:** each deletion is a commit on the island's `nano-ros-next`
  with a green CI run, recorded in its phase-15 doc; a row whose deletion
  fails gets a new issue here.
- **Progress (2026-11-23):** on the island's `nano-ros-next` `eaa89c5` (CI
  run 45151249929 green at `ebb1924`): the P9 `add_subdirectory` probe
  fallbacks are gone (2680); the trace marker is `rclcpp::trace_marker`
  (2641); the `NROS_BRINGUP_DIR` guards are gone (2754); `--plan` replaces
  `nros_run_program`'s hand-added ELF (2752); I45's swap at `56b9971`.
  Kept: the FreeRTOS test entries' `asi_common` fallback (fails without it,
  issue 2899 / I50; masked locally by issue 2900 / I51); the P3 C++
  parameter defaults (issue 2901 / I52). Nothing to drop for 2629: the
  remaining `test/zephyr_app` builds still take `ARMFVP_EXTRA_FLAGS`, and
  turning them into images is the island's own work.

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

The island port's items (2026-11-23), in this order, each followed by its T4
deletion on the island: I45 (done); I51 then I50 (the island's
last test-entry fallback); I52 (the controller's defaults); I36 and I37 (the FVP lane's last
flags and CMake lines); I38 (after I37, so the ladder decides which tree a
carried patch lands in); I41; I49; I43; I44 (after PR #4632 merges); I40;
I39; I46, then I47 (both need a host with the NXP trees; hardware runs under
phase-512); I48 last. I42 whenever the release lanes are green, then I26's
island switch.

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
- [x] I8: the serial-link guide and Kconfig help name the main-priority and
      RX-ring lines; 474 T3 kept them stated, not defaults, so they stay.
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
- [x] I25: a PATH change leaves pyo3-ffi Fresh in every recipe that builds a
      pyo3 crate; `check pyo3-path-stable` with an unpinned control (issue 2660).
- [ ] I26: the island pins one nano-ros commit in `nros-toolchain.toml`, with no vendored CLI build and no `NROS_SKIP_STALE_CHECK` (issue 2645).
- [x] I28: a `string_array`'s element count is `MAX_STRING_ARRAY_LEN`, derived
      from a `string_array` declaration; `ParameterValue` is no larger than its
      largest variant with that bound (unit test), and `just mem-report` before and
      after on a string + `double_array` image is recorded in issue 2647.
- [x] I29: `rclc_trace_marker` / `rclcpp::trace_marker` /
      `nros::trace_marker::emit_app_marker` reach the installed sink with id
      and arg, and refuse every id outside 1..=15 (issue 2641; `just check c`,
      `just check cpp`, `nros-node` tests).
- [x] I30: on `freertos` a plain `add_executable` reaches the image verb and
      `nano_ros_add_plain_executable` does not, on `posix` it is the same
      builtin, and no sources is refused (issue 2678;
      `just check cmake-ament-surface` case 14).
- [x] I31: a Zephyr image's board network file renders `NET_CONFIG_*` for a
      static address (`NET_DHCPV4=n`) and a lease, last in `EXTRA_CONF_FILE`;
      a second interface, a stated `mac` and a conflicting fragment are
      refused; a Cyclone DDS image off `qemu` with no locator is accepted and
      a zenoh one refused, read from the descriptor (issue 2679;
      `nros-cli-core` `image_net` and `builder::zephyr` tests).
- [x] I32: RFC-0136 built: `[image_alternatives]` validated, `check_collisions`
      lets one group share an address and MAC, the D5 tests and D6 docs (issue 2690).
- [x] I33: nano-ros carries the Zephyr CTF `app_marker` event patch (and the
      `k_thread_priority_set` CTF fix) in `zephyr/patches/`, applied by
      `nros setup board`, so a CTF image emits callback and application
      markers with no consumer patch (issue 2766; PR #4462, the island
      removed its 0001 and 0002 at main `14be463e27`).
- [x] I34: `nros pin --rev <sha>` / `--path <dir>` write a commit or checkout
      pin, refusing what the launcher refuses (issue 2770).
- [x] I35: `nros doctor` reports a toolchain pin that disagrees with
      `west.yml`'s nano-ros revision (issue 2771).
- [ ] I36: an owned west entry configures with the workspace's `src/` on
      `NROS_INTERFACE_SEARCH_PATH`; the island's three hand-set lines go
      (issue 2876).
- [ ] I37: the Zephyr workspace ladder takes the project's own west
      workspace above the store's and says so; the island's FVP lane names
      no `--zephyr-workspace` (issue 2877).
- [x] I38: the out-of-tree tracing backend patch ruled and built or
      documented; under the first answer the island carries no Zephyr patch
      (issue 2878).
- [ ] I39: an S32Z image names a board network file, with its VLAN tag; the
      island's `network_config.cpp` and `PLATFORM_*` defines go (issue 2879).
- [ ] I40: `s32z270-rtu0-zephyr` ships a checked `west-downstream.yml`; the
      island imports it (issue 2880).
- [ ] I41: `RCLCPP_LOG_MIN_SEVERITY` compiles out the levels below it, with
      their arguments; the island's `CONFIG_LOG_LEVEL` gating goes (issue
      2881).
- [ ] I42: a release past nros11 is published; the island's default branch
      moves to it (issue 2882).
- [ ] I43: tracked by phase-582 (issue 2883).
- [ ] I44: tracked by phase-582 (issue 2884); the island's FVP images set a
      non-zero interval once it rules.
- [x] I45: an image states that its owned entry runs no nano-ros and gets an
      empty census; the island drops `on_missing = "warn"` (2871; island
      `nano-ros-next` `56b9971`).
- [ ] I46: the s32z270-freertos RTD variant has its NETC netif, the port
      patch question answered, and a full link; the island names the variant
      (issue 2632).
- [ ] I47: the S32Z2 CAN frame driver over the vendor drivers (issue 2633).
- [ ] I48: the zenoh-pico CAN link rides the frame driver (issue 2769).
- [ ] I49: issues 0758, 0772 and 2865 resolved against phase-582 with the
      island's evidence, or each names its remaining gap. 2865 done
      (archived). 0758 waits on PR #4632 and the island's SNTP-step
      measurement; 0772 waits on issue 2866 (phase-582 owner's verdicts,
      2026-11-23).
- [ ] I50: an owned entry's census adds its workspace `<depend>`s; the
      island's FreeRTOS test entries drop the `asi_common` fallback (issue
      2899).
- [ ] I51: an owned census is stale when the entry's `CMakeLists.txt` or
      `package.xml` changes (issue 2900).
- [ ] I52: the probe seeds the agreed parameters when instances disagree on
      others, or probes per model; the island drops the controller's
      declaration defaults (issue 2901).
- [ ] T4: every fixed row of "The island port's needs" has its island
      workaround deleted on `nano-ros-next` with a green CI run.
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

## Issues homed here (2026-11-24)

Filed from the island port at nano-ros pin
`55d9a6d761b64d4633d3ec6820ad37f68f2acfe7`.

- [issue 2951](../issues/2951-zephyr-log-max-level-refuses-level-name.md): the Zephyr image hook refuses a level name for `log.max_level` (I53).
- [issue 2952](../issues/2952-cmake-road-log-max-level-not-exported.md): the CMake road hands an image's `log.max_level` to cargo only, so C/C++ code cannot derive its log floor from it (I54).

## Issues homed here (2026-11-23)

Filed from the island port's inventory (its phase 15 on `nano-ros-next`
`f6c8bef`, nano-ros pin `8d4e70a382`), or taken here as island-shaped and
unowned.

- [issue 2876](../issues/archived/2876-owned-west-entry-no-interface-search-path.md): an owned Zephyr west entry gets no workspace `NROS_INTERFACE_SEARCH_PATH` (I36).
- [issue 2877](../issues/archived/2877-zephyr-workspace-ladder-skips-project-west-workspace.md): the Zephyr workspace ladder never looks at the project's own west workspace (I37).
- [issue 2878](../issues/archived/2878-zephyr-tracing-custom-backend-patch.md): Zephyr 3.7 cannot select an out-of-tree tracing backend; the island carries the patch (I38; resolved).
- [issue 2879](../issues/2879-s32z270-rtu0-zephyr-no-board-network.md): `s32z270-rtu0-zephyr` declares no `[board.net]`, and no VLAN or second interface renders (I39).
- [issue 2880](../issues/archived/2880-s32z270-rtu0-zephyr-no-west-downstream.md): `s32z270-rtu0-zephyr` has no `west-downstream.yml` (I40, resolved).
- [issue 2881](../issues/archived/2881-rclcpp-log-min-severity-missing.md): no compile-time log floor, `RCLCPP_LOG_MIN_SEVERITY` (I41).
- [issue 2882](../issues/2882-nros12-release-for-consumers.md): no release past nros11 (I42; planned by phase-590).
- [issue 2883](../issues/archived/2883-zephyr-clock-halt-unreachable-after-platform.md): `halt` after `platform` on a "none" port was unreachable and accepted (I43; homed in phase-582; resolved, superseded by RFC-0141 D1's amendment).
- [issue 2884](../issues/2884-sntp-resync-paced-on-guest-time.md): the SNTP re-sync interval runs on guest time (I44; homed in phase-582).
- [issue 2871](../issues/archived/2871-census-for-entry-without-nano-ros.md): an owned entry that never starts nano-ros gets no census (I45; resolved by PR #4671).
- [issue 2899](../issues/2899-owned-census-omits-entry-workspace-depends.md): an owned entry's census builds its package without the workspace packages it `<depend>`s on (I50).
- [issue 2900](../issues/2900-owned-census-freshness-ignores-entry-cmakelists.md): an owned census reads fresh after the entry's CMakeLists changes (I51).
- [issue 2901](../issues/2901-probe-seeds-all-or-nothing-on-disagreement.md): the probe drops every launch seed when instances disagree on one key (I52).
- [issue 2632](../issues/2632-s32z270-freertos-rtd-variant.md): the s32z270-freertos RTD variant's NETC netif, port patch and full link (I46; phase-524 W3 stays its descriptor home).
- [issue 2633](../issues/2633-board-owned-can-frame-driver.md): the S32Z2 CAN frame driver's vendor backend and the `board`-stack hand-off (I47; phase-560 W9 stays its design home).
- [issue 2769](../issues/2769-zenoh-can-link-over-frame-driver.md): the zenoh-pico CAN link over the frame driver (I48).
- [issue 2911](../issues/archived/2911-census-prepass-skips-compile-check-nros-image-rows.md) (resolved 2026-11-24 by PR #4723): the census pre-pass never took the census of a compile-check row configured by `nros build`, so the issue-2862 rows `west_owned_entry_census_flat` / `_nested` refused on the self-hosted runner.
- [issue 2924](../issues/archived/2924-seed-bound-tables-span-language-roots.md) (resolved 2026-11-24): a west image's message-bound seed read one language root of the probe's bound tables, so `west_owned_entry_workspace_module` (a C node's `std_msgs` under `nano_ros_c`, the workspace's `demo_msgs` under `nano_ros_cpp`) refused in the tier-1 census pre-pass on a fresh tree.

## Issues homed here (2026-11-02)

- [issue 2165](../issues/archived/2165-zephyr-smp-nros-log-stops-after-four-records.md) (resolved 2026-11-19: does not reproduce at main; the sink hazard is [issue 2738](../issues/archived/2738-zephyr-log-sink-buffer-on-caller-stack.md)): on the island's Zephyr SMP image (`fvp_baser_aemv8r_smp`), only the four early-ring `nros_log` records reach the console. Every later record is lost with no `report_lost` warning. The same sources log normally on FreeRTOS.

## Issues homed here (2026-11-20)

- [issue 2770](../issues/archived/2770-nros-pin-rev-and-path.md) (resolved 2026-11-22): `nros pin` wrote only a release pin, so a commit or checkout pin from issue 2645 was edited by hand (I34).
- [issue 2771](../issues/archived/2771-doctor-toolchain-pin-matches-west.md) (resolved 2026-11-21): `nros doctor` did not compare a commit pin with `west.yml`'s nano-ros revision, so a Zephyr consumer keeps its own lockstep script (I35).
- [issue 2766](../issues/archived/2766-zephyr-ctf-app-marker-patch-carried-by-consumer.md): the Zephyr trace sink calls `sys_trace_app_marker` through a weak reference, and the CTF event that defines it is a Zephyr patch only the island carries, so other users' CTF images emit no markers (I33; GitHub #4008).

## Issues homed here (2026-11-18)

- [issue 2690](../issues/archived/2690-image-alternatives.md) (resolved 2026-11-22): images of one host that never run together are refused for sharing a static address; RFC-0136 lets an `[image_alternatives]` group share it (I32, phase-586).
- [issue 2689](../issues/archived/2689-source-metadata-rejects-v6-sidecar.md): the CLI's `SourceMetadata` rejected every schema v6 probe sidecar (issue 2650's description and constraints lengths), and the C/C++ refresh dropped the error, so every sidecar read as stale and every Zephyr C++ configure failed at `NanoRosPromise.cmake`. Found on the island's CI against main `a49a8dacaf`; fixed by PR #4190 (resolved).
- [issue 2686](../issues/archived/2686-constraints-truncation-names-description-knob.md): a truncated `additional_constraints` text is logged as a description that did not fit, telling the user to raise `NROS_MAX_PARAM_DESCRIPTION_LEN` when `NROS_MAX_PARAM_CONSTRAINTS_LEN` sizes it.

## Issues homed here (2026-11-17)

- [issue 2670](../issues/archived/2670-imperative-param-declare-invisible-to-census.md) (resolved): a Rust parameter declared through `Executor::node_mut(..).declare_parameter` reached no census hook, so a contract that omitted it sized the store short with no `param-missing-in-contract` error. The four imperative routes now report through a registration observer.
- [issue 2681](../issues/archived/2681-rust-node-builder-reaches-no-census-hook.md) (resolved 2026-11-19): a Rust node built with `Executor::node_builder(..).build()` or `Executor::create_node` reaches no census hook, so the census does not count it unless it declares a parameter.
- [issue 2678](../issues/archived/2678-plain-executable-opt-out-on-a-board.md) (GitHub #4016, resolved): a board build had no public way to declare an executable that is not an image; `nano_ros_add_plain_executable` is it (I30).
- [issue 2679](../issues/archived/2679-zephyr-board-network-file-net-config.md) (GitHub #4011, resolved): a board network file was refused on Zephyr; it renders into `NET_CONFIG_*` (I31).
- [issue 2665](../issues/archived/2665-target-session-chain-start-deadline-under-load.md): `target_session::tests::teardown_waits_for_a_chain_that_is_stopping` failed under host load at its 5 s chain-start deadline and passes alone. Resolved: the chain could end before the helper first looked; it now waits for the test's go.
- [issue 2660](../issues/archived/2660-pyo3-rebuilds-on-path-change.md) (GitHub #4084, resolved): the launch resolver rebuilt pyo3-ffi whenever PATH changed (I25).

## Issues homed here (2026-11-16)

Filed from the Autoware Safety Island's phase-15 P3 on nano-ros 5b9e8844a4.

- [issue 2652](../issues/archived/2652-codegen-tool-cache-outlives-invoker.md) (GitHub #4067, resolved): a build dir kept the cached `nros` codegen tool after a different `nros build` ran into it (I24). Found on nano-ros 09219edc75.
- [issue 2662](../issues/archived/2662-parity-params-bulk-on-alpha.md) (GitHub #4091, resolved): two ros2_cli parity params cases failed once issue 2639's 70 bulk parameters were on `/alpha`, the node they compare with stock (I22).
- [issue 2647](../issues/archived/2647-param-slot-sized-by-string-array.md) (resolved): every parameter slot was sized for a 32 x 256 B string array when the image declared a string and any array, with no string array (I28).
- [issue 2649](../issues/2649-param-string-array-shared-arena.md): once a string array is declared, every slot still pays its product; a shared string arena (follow-up to I28).
- [issue 2650](../issues/archived/2650-param-descriptor-description-per-slot.md) (resolved): every slot carried a 256 B description; the room is now derived from what the code sets, on every road (follow-up to I28).
- [issue 2639](../issues/archived/2639-list-parameters-silently-caps-at-64.md) (resolved): `ros2 param list` and `ros2 param dump` showed 64 of the controller's 149 parameters and said nothing (I22).
- [issue 2638](../issues/archived/2638-launch-list-param-flattened-to-string.md) (resolved): a list parameter from a `<param from>` file reached the node as one comma-joined string (I20).
- [issue 2640](../issues/archived/2640-metadata-probe-no-launch-values.md) (resolved): the C/C++ metadata probe constructed a node with no launch values and a default-size store, so a node declaring parameters without defaults could not be probed; the probe now seeds the launch values ([phase-566](phase-566-probe-launch-seeds.md)).
- [issue 2645](../issues/archived/2645-toolchain-pin-cannot-name-a-commit.md) (GitHub #4006, resolved): `nros-toolchain.toml` could not pin a commit, so a consumer on main vendors nano-ros and sets `NROS_SKIP_STALE_CHECK` (I26).
- [issue 2646](../issues/archived/2646-sdk-eigen-for-arm-none-eabi.md) (GitHub #4013, wontfix): no SDK Eigen for cross builds; Eigen 3.4's psincos fails on newlib's `int32_t = long`; users bring their own (I27).

## Issues homed here (2026-11-15)

Filed from the GitHub reports of one session scaffolding the L4 Safety Island
workspace (0.6.0-nros8, and main `37232ecff1` for the first). A seventh report
from that session, `FREERTOS_PORT not set` on `freertos-posix`, is
[issue 2341](../issues/archived/2341-freertos-posix-port-not-reaching-zpico.md),
fixed after that release.

- [issue 2636](../issues/archived/2636-rclcpp-throttle-once.md) (GitHub #4010, resolved): `RCLCPP_*_THROTTLE` was refused and `RCLCPP_*_ONCE` / `_SKIPFIRST` were missing. The throttle now measures on its `clock` argument (I21).
- [issue 2641](../issues/archived/2641-app-trace-marker.md) (GitHub #4009, resolved): no public call emitted an application event through the installed trace sink; `rclc_trace_marker` / `rclcpp::trace_marker` do now, ids 1..=15 (I29).
- [issue 2615](../issues/archived/2615-model-gate-refuses-sibling-param-file.md) (resolved): a model whose launch loads a param file from a sibling package was refused as non-portable (I17).
- [issue 2618](../issues/archived/2618-knob-delivery-check-false-mismatch-after-link.md): the knob delivery check refuses a build after link over a list separator and an override row (I18).
- [issue 2634](../issues/archived/2634-param-store-full-declare-silently-uses-default.md) (GitHub #4018, resolved): `declare_parameter` past a full parameter store returned the code default in silence. It now stops the boot and names the knob (I19).
- [issue 2609](../issues/archived/2609-zephyr-app-workspace-module-lost-to-cache.md) (GitHub #3948, resolved): a generated Zephyr app lost its workspace Zephyr modules to the `-DZEPHYR_EXTRA_MODULES` cache entry (I15).
- [issue 2610](../issues/archived/2610-zephyr-app-misses-workspace-msg-packages.md) (GitHub #3952, resolved): a generated Zephyr app did not find the workspace's own message packages (I16).
- [issue 2601](../issues/archived/2601-resolve-ignores-image-target-platform-file.md): `nros configure` resolved every model for `posix`, so an image's `<stem>.system.<target>.yaml` was never read (I14, resolved).
- [issue 2603](../issues/archived/2603-flaky-sdk-index-stamp-input-test.md): `configure_stamp::tests::the_sdk_index_is_a_stamp_input` failed once under a full gate and passes alone. Resolved: a concurrent test removed `AMENT_PREFIX_PATH` from the shared test process.
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
- [issue 2590](../issues/archived/2590-cpp-entry-rust-arm-drops-node-identity.md): a Rust component installed by a C++ entry ignores the launch `<node name= namespace=>`. Found fixing issue 2568.
- [issue 2593](../issues/archived/2593-cmake-road-drops-image-profile.md) (GitHub #3899, resolved): `[image.<id>] profile` was silently ignored on the CMake road. It now picks the build type and the runtime cargo profile, and is refused where it cannot apply (I9).
- [issue 2594](../issues/archived/2594-zephyr-generated-app-no-workspace-kconfig.md) (GitHub #3906): a generated Zephyr west app cannot get the workspace's own Kconfig symbols. Ruled option A on 2026-11-15; resolved by I10 and I11.
