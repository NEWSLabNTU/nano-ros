---
id: 1340
title: "The arena budgets a full receive region for every subscription, and a backend that dispatches IN PLACE claims none — 9,768 bytes per subscription on zenoh and XRCE"
status: resolved
resolved_in: 2026-10-01
type: tech-debt
area: executor, build
severity: medium
found: 2026-09-12
related: [issue-1319, issue-1255, issue-1190, issue-1577, issue-1522, phase-454, phase-457, rfc-0100]
---

## What the model says and what the runtime does

`Executor::register_subscription_buffered_on` asks the backend whether it can
hand a sample to the callback out of its own ring:

```rust
if handle.supports_process_in_place() {
    let entry_offset = self.arena_alloc::<SubInplaceEntry<M, F>>()?;
    …
    return Ok(HandleId(slot));
}
```

That test runs **before** the slot size is computed, so the buffered region —
`buffered_region_size(depth, slot)` — is never allocated. Both schemaless
backends answer an unconditional `true`:

* `nros-rmw-zenoh`'s `Subscription::supports_process_in_place` is
  `fn(&self) -> bool { true }`;
* XRCE's `xrce_subscription_supports_in_place` writes `true` and its
  `process_raw_in_place` slot is non-NULL (the capability is the conjunction of
  the two, per `rmw_vtable.h`);
* Cyclone leaves both slots NULL, so its subscriptions do take the buffered
  path.

`nros-node/build.rs` budgets a full region for every subscription regardless.

## Measured

`packages/testing/nros-tests/bins/contract-monitor`, `contract-monitor-sub`: a
Rust node on zenoh subscribing `std_msgs/Header` at the default `KEEP_LAST(10)`.
A probe over `Executor::arena_used()` across the one registration:

```
arena used: open=0 node=0 sub=672
SUB CLAIM   = 672
```

Against the model for the same image (`NROS_SUBSCRIBER_BUFFER_SIZE=880`,
`NROS_SUBSCRIPTION_BUFFER_SIZE=1496`, one declared subscription):

```
ARENA_SIZE=12840  REQUIRED=12840  PUBSUB_REGION=9768
```

So the subscription is budgeted **9,768 + 1,024 = 10,792 bytes** and claims
**672**. `mem-report` prices the linked image's executor backing at 27,152 bytes
of `.bss`; an image whose subscriptions all dispatch in place could give most of
the per-subscription half back.

This is the OVER direction, so nothing fails — which is why it has sat.
`report_arena_headroom` is the advisory that would say so at run time.

## Why phase-454 W5 did not take the saving

W5 recorded the fact (`RegistrationPath::RustTypedInPlace`, RFC-0100 D1) and
left the PRICE where it was. Lowering it needs one thing the descriptor does not
carry:

**The Rust GENERIC registration path does not consult the capability.**
`supports_process_in_place` has exactly one call site in `nros-node`, in
`register_subscription_buffered_on`. `register_subscription_buffered_generic`
— what `node_mut(id).subscription(t).generic(ty, hash)` lowers to, and what a
bridge writes — allocates `buffered_region_size(depth, RX_BUF)` unconditionally
on the same backend. An `[[endpoint]]` row carries `(kind, type, topic)` and
says nothing about which of the two spellings the image's code uses, so pricing
the in-place row at zero would under-size every generic subscription on zenoh
and XRCE, which is the direction that ships `NodeError::BufferTooSmall`.

## What a fix has to decide

* whether a generic registration should ALSO take the in-place path when the
  backend offers it — one call site moves, and the saving then needs no new
  fact; or
* whether the descriptor should distinguish a typed endpoint from a generic one,
  which is a property of the CALL SITE rather than of the contract — the same
  limit phase-454 W10 addresses for the C half's `rx_size_bound<M>`.

The first is the smaller change and removes the reason for the second. Either
way acceptance is a measured `mem-report --baseline` on an image with a GENERIC
subscription, not only on a typed one.

## Progress, 2026-09-29 — the runtime half landed; acceptance is NOT met

**Landed (PR #1451): the first option above.** The generic registration now
takes the in-place path when the backend offers it. The worry that kept it
`false` — the buffered entry is shared with `add_arena_subscription_callback`
— did not hold: that caller supplies its own subscriber and never reaches
`open_subscription`, so the branch is local to one entry point.
`DeclaredSubscriptionShape::BufferedRaw.in_place_capable()` flipped in the
same commit, so the probe's row and the executor's claim move together.

Two corrections to this issue's text, from reading the tree as it is now:

- "`supports_process_in_place` has exactly one call site in `nros-node`, in
  `register_subscription_buffered_on`" — phase-456 W8 moved it into
  `open_subscription`. The asymmetry this issue describes survived the move
  because the generic path read the answer and discarded it.
- The second option ("distinguish a typed endpoint from a generic one") was
  already done by phase-457 W3/W5 before this landed: `registration_path` is
  per endpoint and observed at the call site. That is what made the first
  option safe to take — the descriptor states `in_place` only for a shape the
  executor actually dispatches in place.

**Why acceptance is not met.** The acceptance is a `mem-report --baseline` on an
image with a generic subscription, and on the image measured it does not move:

| `examples/native/rust/listener` | descriptor row | `ARENA_SIZE` | RAM in symbols |
| --- | --- | --- | --- |
| before (`BufferedRaw` → `false`) | `unbounded` | 14,424 | 157,642 |
| after | `in_place` | 14,424 | 157,642 |

The runtime claim did change — the unit test measures it — but the ARENA is
sized by `nros-node/build.rs`'s `max_cbs` fallback on every road except
Zephyr's resolver, because the per-kind model runs only when five
`NROS_ENTITY_COUNT_*` carriers arrive. So the freed bytes become headroom.
That is **issue 1577**, and it blocks this one on cargo and plain-cmake images.

Also found while measuring: the metadata probe's freshness digest omits the
nano-ros crates that decide what it reports, so a change to this very
classifier is not picked up by `nros sync` — **issue 1578**. The stale
direction observed was safe; the reverse flip would under-size.

**What closes this issue:** #1577, then the measurement above on an image whose
`required` clears `ARENA_FLOOR` (8,192 — a one-subscription image clamps to it
either way).

**Update 2026-09-29 — #1577 landed; the arena moves now.** A five-subscription
copy of the listener (untracked, deleted after), `nros sync` + `nros build`:
the zenoh build's model prices the five `in_place` rows at `REQUIRED` 7,168
(`ARENA_SIZE` 8,192, the floor) and all five register; the same image with
those rows priced as buffered is 22,528. That is this issue's saving, reaching
the arena on a cargo leaf for the first time — 14,336 bytes on five
subscriptions. What is still missing for closure is the acceptance as written:
a `just mem-report --baseline` on a TRACKED image with enough subscriptions to
clear the floor. No in-tree single-package leaf has more than one.

## Resolution (2026-10-01)

**Resolved, with the acceptance AMENDED: the `mem-report --baseline` is on an
untracked image, and an automated check stands in for "tracked".** Both halves
of the reasoning are below, because the amendment is the part a reader should
be able to argue with.

### Measured — `just mem-report --baseline`, before and after

The five-subscription copy of `examples/native/rust/listener` again (untracked,
deleted after): the tracked leaf's `Cargo.toml`, `system.toml` and `main.rs`;
`lib.rs` registering `create_subscription_for_callback_name::<String>` on
`/chatter` … `/chatter5`; a `system.contract.yaml` stating all five
`KEEP_LAST(1)`, as the tracked one states its one. `nros sync` wrote five
`registration_path = "in_place"` rows and `subscription_entities = 5`;
`nros build native` (zenoh) built it.

- **after** — the tree as it is: `arena_model::REQUIRED` 7,168,
  `ARENA_SIZE` 8,192 (the floor).
- **before** — the SAME image, the in-place rows priced as buffered: a
  temporary local edit making `in_place_dispatch_trusted()` return `false`
  (reverted, and the rebuilt "after" ELF is byte-identical to the first one).
  That is the arena this image had before this issue's saving, and what the
  issue-1577 guard gives a Cyclone build of it: `REQUIRED` = `ARENA_SIZE` =
  22,528, with the guard's warning once per row.

```
$ just mem-report --baseline before.json after.elf
RAM (.bss + .data), by section:  182,586 bytes  (-14,336)
RAM attributed to symbols:       157,602 bytes  (-14,336)
        18,064    9.9%  nros_node::executor::backing::EXECUTOR_BACKING (.llvm.4814622804028721805)  (-14,336)
        18,071    9.9%  nros_node  (-14,336)
```

**14,336 bytes of `.bss`, all of it `EXECUTOR_BACKING`** (32,400 → 18,064),
i.e. exactly 22,528 − 8,192. Nothing else in RAM moved: `.bss` is the only
RAM section that changed, `nros_node` the only crate, and the other symbol
differences are anonymous constants of equal size renamed by their
`.llvm.<hash>`. Per row that is
3 × 1,024 (a `KEEP_LAST(1)` triple buffer at the default `RX_BUF`); the floor
takes back the last 1,024 — the model's in-place `REQUIRED` is 7,168, not
8,192 — so the saving a sixth subscription adds is the whole 3,072.

**It runs on the smaller arena.** `rmw_zenohd` on `tcp/127.0.0.1:17947`, the
"after" binary with `NROS_LOCATOR` pointed at it: five `Subscriber created`
lines, no `BufferTooSmall`, and `ros2 topic pub -t 3` (humble,
`rmw_zenoh_cpp`) on each of the five topics delivered **15 of 15** samples,
three per topic.

**The tool could not show it at first.** The first `--baseline` run printed
no delta for `EXECUTOR_BACKING` at all: `nm -C` prints LLVM's per-build
`(.llvm.<hash>)` suffix as part of the name, the baseline joined on the raw
name, and the hash differs between two builds — so the one symbol a rebuild
measurement is about is the one it could never match. That is issue 1180, and
its fix landed on `main` while this PR was in review (the join strips the
per-build decorations and reports per-owner deltas); this PR's own patch to
`nros-mem-report.py` was dropped on rebase in favour of it.

### Why not a tracked image

Searched with `git grep` over `examples/` and `packages/testing/`: no tracked
image has two or more subscriptions that are in-place-capable AND a
`system.contract.yaml` — and without a contract there is no descriptor, so no
row prices anything (CLAUDE.md, "no contract ⇒ no file"). The four tracked
contracts: `examples/native/rust/listener` (one subscription — clamps to the
floor either way, 3,072 vs 6,144 against 8,192), the `cpp` and
`derived-tiers-cpp` workspaces (C++), and `realtime-rust/derived_bringup` (no
subscription). The multi-subscription images that exist have no contract
(`nros-bench/executor-fairness`, `wake-latency-cortex-m3`, test bins) or take a
path that cannot dispatch in place (`workspaces/safety`'s `_with_safety`
listener).

Making one would mean a new image whose only purpose is to host this number —
a themed example dir RFC-0066 rules out (a feature is a node package, a
configuration a fixture axis, and neither changes how many subscriptions an
image has), or giving the ROS-demo `listener` four subscriptions it does not
have. Neither is worth the maintenance for a number that is now recorded and
reproducible from the recipe above.

### Addendum (2026-10-02) — the acceptance as originally written, on a TRACKED image

The amendment above is no longer needed: the measurement now has a tracked
image to stand on. `packages/testing/nros-tests/bins/in-place-subscriptions`
is a cargo leaf whose one component registers EIGHT subscriptions through
`create_subscription_for_callback_name` (`/chatter1` … `/chatter8`), with a
`system.contract.yaml` stating each `KEEP_LAST(1)`. It lives under
`nros-tests/bins/` — a test image, not a themed example — so RFC-0066's
objection does not reach it, and it has no fixture row: it is measured with
`nros build native` + `mem-report`, which need no matrix cell. Eight, not five,
so BOTH sides clear the 8,192-byte floor and the delta is the whole per-row
saving rather than "saving minus floor".

`nros sync` writes eight `registration_path = "in_place"` rows and
`subscription_entities = 8`. Before = the same tree with `in_place_dispatch_trusted()`
in `nros-node/build.rs` forced false (a temporary local edit, reverted; the
rebuilt "after" ELF is byte-identical — `cmp` — to the first one):

```
ARENA_SIZE   before 34,816   after 10,240   (= 2,048 + 8 x 4,096  vs  2,048 + 8 x 1,024)

$ python3 scripts/nros-mem-report.py --baseline before.json after.elf
RAM (writable allocated sections): 199,514 bytes
vs baseline: section RAM -24,576, symbol RAM -24,576
        20,832   10.4%  nros_node::executor::backing::EXECUTOR_BACKING (...)  (-24,576)
        20,832   10.4%  [executor storage]  (-24,576)
```

**−24,576 bytes = 8 × 3,072**, all of it `EXECUTOR_BACKING`, nothing else in
RAM moved. **It runs on the smaller arena:** against `rmw_zenohd`, eight
`Subscriber created` lines, then `ros2 topic pub -t 3` (humble,
`rmw_zenoh_cpp`) on each of the eight topics delivered **24 of 24** samples,
three per topic, with no `BufferTooSmall` and no arena-exhaustion line.

Found on the way: renaming the fixture's component left its
`metadata/<old>.json` behind and the leaf road composed it in — 13 rows and a
162,936-byte arena for an image that registers 8. Filed as issue 1639.

### What guards it instead

`packages/core/nros-node/tests/arena_model_in_place.rs`, on the `test-unit`
lane. The descriptor pricing — `SubEndpoint`, `descriptor_subscriptions`,
`row_slot_bytes`, `subs_arena_from_descriptor`, `buffered_region` — moved from
`build.rs` into `build/sub_arena.rs`, which `build.rs` and the test both
include by `#[path]`: one copy of the arithmetic, not a mirror. The one
build-environment input, whether the backends claim in-place dispatch, became
a parameter. The test parses a descriptor in the shape `nros sync` wrote for
the measured image and asserts, with the sizes this build of `nros-node`
emitted:

1. five stated `in_place` rows on a trusting build price at exactly five entry
   structs (5,120 — the measured `REQUIRED` 7,168 less `BASE_OVERHEAD`);
2. the same rows on a non-trusting build (the issue-1577 guard) price higher
   by exactly the five receive regions (20,480 — the measured 22,528 less
   `BASE_OVERHEAD`);
3. a row whose path nobody observed (issue 1522) keeps its region beside
   in-place rows that lose theirs;
4. an in-place row with no depth refuses the whole sum.

Mutation-checked: pricing an in-place row as buffered turns three of the four
red.

What it does NOT cover, stated rather than implied: the composition outside
these functions — the probe stating `in_place` (phase-457 W3), the counts
reaching the model (issue 1577), `ARENA_FLOOR` — is still evidenced only by the
build measured above. The runtime half stays
`executor::tests::a_generic_subscription_on_an_in_place_backend_claims_no_receive_region`.

### Landed in

- PR #1451 — a generic subscription on an in-place backend claims no receive
  region (the runtime half).
- PR #1474 — issue 1577: the arena's per-kind model runs off Zephyr, with
  backends declaring their dispatch.
- The PR that archives this issue — the descriptor pricing moved to
  `build/sub_arena.rs` with its test.
- Issue 1180 (archived) — `mem-report --baseline` joining symbols across builds.

## Record — the objection this resolution amends (written 2026-10-01, before it landed)

Kept as a record, not as a live claim: the Resolution above answers it by
AMENDING the acceptance, which is exactly the step this note said was missing.
It is retained for the one fact the Resolution does not carry — that issue 1623
was found on the way, and would have hidden this saving on an incremental build.

Checked on branch `fix/executor-arena-exact-0810-1340-1370-1036-1496`: the
runtime half and the model half are both in (`claims_no_region` rows price the
entry struct only), so nothing in sizing is left here. What is not done is this
issue's acceptance as written — a `mem-report --baseline` on a TRACKED image
with enough in-place subscriptions to clear the 8,192-byte floor. None of the
images measured for issue 0810 qualifies: the three native realtime workspaces
have no contract and so no descriptor (fallback), and the declared talker has
no subscription. Found on the way, and fixed there: issue 1623 (the descriptor
variable was never watched), which would have hidden this saving on an
incremental build exactly as it hid 0810's.
