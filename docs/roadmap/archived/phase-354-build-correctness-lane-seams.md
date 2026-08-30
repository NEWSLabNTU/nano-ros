# Phase 354 — Contract seams: the places two sides disagree about one fact

**Status (2026-08-18). COMPLETE — W1–W4 all DONE; archived.** This header said
"PLANNING — nothing implemented" after work had landed — corrected here.

* **W1 (#493, two workspace roots / one corrosion dir)** — **DONE.** Both halves
  of the acceptance are met; see the W1 section. #493 is resolved and archived.
* **W2 (#466, tier-1's unstated setup contract)** — DONE. `d38a409ff` took
  finding (a): one gate joins the precondition batch, one is declined WITH
  evidence. #466 resolved.
* **W3 (#454, `*_send_goal_raw` does not strip the CDR header)** — **DONE.** The
  strip landed, and the wire demonstration the acceptance asked for now exists
  and is falsifiable. Two further defects fell out of writing the caller — see
  the W3 section. #454 resolved.
* **W4 (#532, clock ABI resolution)** — **DONE.** The check ran: phase-352
  covers items 1-3 and answers the open question; item 4 is a recorded
  deferral; item 5 (the wall clock) was untouched, so #532 was restated to that
  remainder — and the remainder has SINCE landed (2026-08-18): the wall clock is
  one `nros_platform_time_now_ns` and #532 is resolved. See the W4 section.

**Owns:** [issue 0493](../issues/0493-two-workspace-roots-share-one-corrosion-target-dir-duplicate-no-mangle-symbols.md),
[issue 0466](../issues/0466-tier1-setup-contract-unstated.md),
[issue 0454](../issues/0454-raw-send-goal-ffi-does-not-strip-cdr-header.md),
[issue 0532](../issues/0532-platform-clock-abi-unit-and-resolution.md).

**Related:** [phase-345](archived/phase-345-one-door-build-parity.md) (COMPLETE; named
#466 and #374), [phase-344](archived/phase-344-cmake-cache-relocation.md) and
[phase-347](phase-347-rmw-as-a-declared-provider.md) (both name #493),
[RFC-0061](../design/0061-fixture-freshness-and-test-tiers.md) / [phase-318](archived/) (the tier
contract #466 is about).

## Why a new phase rather than an existing one

Both issues are referenced by phases that are COMPLETE or whose remaining waves
are blocked on unrelated work. A referenced issue in a finished phase has no
owner — it is a footnote. These two are live defects that break a lane today.

---

## W1 — Two cargo workspace ROOTS, one corrosion target dir (#493)

`just build-test-fixtures lane=native` dies on `examples/workspaces/mixed`
(`ws-group-10`) with

```
ld.lld: error: duplicate symbol: nros_rmw_cffi_register
```

because a mixed workspace's umbrella staticlib bundles the nros stack twice.

This is adjacent to a hazard already documented in CLAUDE.md — Corrosion
`< 0.6.0` shares one `cargo/build` across workspace roots, which is why the SDK
store's newest-first prefix ordering (issue 0500) exists. **Establish first
whether #493 is that same Corrosion-version defect or a second, independent
one**, because if it is the former the fix is a pin and a gate, not a build
change.

**Acceptance.** `lane=native` reaches the mixed workspace and links. The
diagnosis names which of the two causes it was, with the Corrosion version the
failing tree resolved (`nano-ros: Corrosion <ver> via <origin>` from the
configure output — CLAUDE.md is explicit that this must be READ, never inferred
from having run the installer).

### DONE 2026-08-16

**The diagnosis: NEITHER branch as posed.** #493's resolution answers the
either/or from the other side. The Zephyr mixed entry hit the identical
duplication — surfacing as `the #[global_allocator] in nros_platform conflicts
with global allocator in: nros_platform`, a lang item rather than a
`#[no_mangle]` symbol — with **no Corrosion in it at all**: no
`corrosion_import_crate`, nothing in its `CMakeCache.txt` or `build.ninja`.
Measured there before the fix: 4 cargo invocations sharing one `--target-dir`,
5 `nros_platform` and 4 `nros_core` metadata identities in one `deps/`, three
built minutes apart (so not stale accumulation).

So the class is **one cargo artifact directory serving two workspace ROOTS**, and
Corrosion `< 0.6.0` is one way to arrange that, not the cause. Which is why the
answer was not "a pin and a gate": it needed enforcement in both lanes, and
issue 0616 supplied the Zephyr half (a configure-time `FATAL_ERROR` when two
roots claim one `--target-dir`) beside this lane's Corrosion version floor.

**The build: `linux mixed` links.** Re-measured from this tree:

```
-- nano-ros: Corrosion v0.6.1 via FetchContent [hashed per-workspace cargo dirs]
[8/8] Linking CXX executable src/native_entry_robot2/native_entry_robot2
     built: examples/workspaces/mixed/build-workspace-fixtures/src/native_entry_robot2/native_entry_robot2
```

rc=0, zero `duplicate symbol`, hashed per-workspace cargo dirs.

**Reading that line as CLAUDE.md requires found a separate live defect.** The
origin is `FetchContent`, not `SDK store` — this host has a provisioned v0.6.1
that the configure ignored and re-fetched from GitHub, because
`nros sdk-path corrosion` constructs only the VERSIONED store layout and this
host has the FLAT one. Filed as [issue 0628](../issues/0628-sdk-path-constructs-only-the-versioned-corrosion-layout.md);
it does not affect W1's acceptance (the version and topology are right either
way) and belongs to phase-365, so it is not folded in here.

## W2 — Tier 1's unstated, ORDERED setup contract (#466)

`just ci` is the instruction every task ends with, and over one session it
stopped **eight** consecutive times on a correctly-cloned tree, only one of them
a test failure. The issue has been added to repeatedly through 2026-08-12.

Partially mitigated already, and that mitigation should be credited rather than
redone:

* `just check tier-preconditions` batches the unmet preconditions and reports
  them all at once, at the head of `just ci`.
* `52e6bda8e` (2026-08-14) landed "one zephyr staleness spelling, so every entry
  is covered" — the issue's own zephyr `skip_probe = true` finding.

* 2026-08-15 landed finding (a): `check-workspace-build-output` joined the
  batch, and the launch-resolve skew below is re-stated there as a WARNING.
  `check-artifact-identity-budget` was checked and DECLINED — its `started_at`
  filter (0499/0513) already answers the tree the finding cites, and the batch
  runs before fixtures exist, so it would only ever SKIP.

The entry-resolver source-dir bridging via `ZEPHYR_WORKSPACE_ENTRY_SRC_KEY` is
DONE, not remaining — it is what `52e6bda8e` above is; all 16 resolvers now name
a leaf. (This paragraph previously credited that commit and listed its content as
outstanding two lines later, which is how the same work gets done twice.)

What remains is one item: the compile-check lane's gate being NARROWER than the
tests it gates (the issue-0196 shape, and the fourth gate this year whose
coverage was narrower than its rule).

Observed twice on 2026-08-14/15 and worth folding in: after any pull, the
ordered remedy is `just setup-cli` → `just setup-launch-resolve` → fixtures, and
`setup-cli` warns that `nros-launch-resolve` is older but does **not** fail, so
a run can proceed with the two disagreeing on an argument list (issue 0363 C).
That is the same "unstated, ordered" shape #466 is about.

**Acceptance.** The compile-check gate watches the same inputs as the tests it
gates, demonstrated by a case that the gate previously passed and the test
failed. Every precondition reachable by `check-tier-preconditions` reports as
part of ONE batch, including the launch-resolve skew.

## W3 — `*_send_goal_raw` never strips the encapsulation header (#454)

The C/C++ FFIs take a parameter named `goal_cdr` and never strip its header, so
`PollingActionClient` would ship the #448 double-encapsulation bug onto the
wire. The parameter NAME says one thing and the code does another — the seam is
between the FFI's stated contract and its behaviour.

This is a wire-format defect, not a build one, and it is here because a bug does
not warrant a phase of its own. It is small and self-contained; the risk is that
it stays unowned because it fits nowhere.

Note phase-303 (XCDR2/extensibility) is PARKED with its premise refuted and no
active driver, so it is not the home for this despite the topic overlap.

**Acceptance.** A `PollingActionClient` goal round-trips against a real peer
with exactly one encapsulation header, demonstrated on the wire rather than by
reading the encoder.

### DONE 2026-08-17 — the caller, and the two defects it found

The strip itself was one line per arm (`core.send_goal_raw(strip_cdr_header(
slice))`, C and C++) plus `scripts/check-goal-cdr-stripped.py`. The acceptance
was the hard half: it asks for a WIRE demonstration, and there was no caller to
build one from. Nothing in the tree invoked `nros_action_client_send_goal_raw` —
no example, no fixture, no test — which is the whole reason the defect shipped.

**The caller.** `packages/testing/nros-tests/bins/action-raw-goal-probe` — a
CMake C leaf with its own `[[fixture]]` row (`linux/c/zenoh`), built by the same
`fixtures-build.sh linux c zenoh` group as every native C example. It builds a
Fibonacci goal WITH its encapsulation header, ships it through the raw FFI, and
round-trips accept + get_result against the C `action-server` example.
`tests/action_raw_goal_e2e.rs` asserts on the SERVER's log, not the probe's: the
probe can only report what it believes it sent, and only the peer reports what
arrived.

It is under `packages/testing/` rather than `examples/` deliberately — a
regression probe is not a copy-out user project. That needed a dir-relative
resolver, so `build_example_cmake_rmw` is now a thin wrapper over
`build_cmake_leaf_rmw`: one locator, not a second spelling.

**Falsifiable, and falsified.** With the strip removed, rebuilt, the test fails
with the server reporting order **256** against the 7 sent. Restored, it passes
in ~4.6 s. The value is worth recording because the obvious prediction was
wrong: "the header bytes land in `order`, so the peer reads 0x00010000 = 65536"
ignores that the parsed request is `[encap][GoalId(16)][order]` — the extra four
bytes shift the tail and `order` reads a straddle. `RAW_GOAL_DOUBLE_HEADER_ORDER`
is the measured 256, and its doc says so.

**Defect 1 — the polling arms never got phase-338 W3's channel-type fix.** The
first run failed with the goal never reaching the server. Cause: `init_polling`
in `nros-c` and `nros-cpp` advertised the BARE action type
(`…::dds_::Fibonacci_`) on send_goal / get_result / feedback, where ROS 2
expects `…Fibonacci_SendGoal_`. The type name is baked into the keyexpr, so
query and queryable are different keys and every goal times out — the exact
failure `action_channel_type`'s own doc records from the raw REGISTER path.
phase-338 W3 fixed that path and left these; nothing called them, so nothing
caught it. Fixed at all six remaining sites (`nros-c` polling client + server,
`nros-cpp` polling client + server, and `Node::create_action_{server,client}_raw_sized`),
with `action_channel_type` promoted from `pub(crate)` to a `nros_node` /`nros`
export so there is one implementation rather than a fifth transcription.

**Defect 2 — actions ignore `ROS_DOMAIN_ID` ([issue 0656](../issues/0656-action-raw-register-ignores-ros-domain-id.md)).**
With the type names agreeing, the goal still did not arrive: the C action server
prints `Domain ID: 42` and then declares its queryables under `0/fibonacci/…`,
because `register_action_server_raw*` never passes a domain. Every existing
action test agreed on `0/` on both sides, so it passed for everyone. Filed, not
fixed here — it is a distinct defect with its own blast radius (actions are not
domain-isolated at all), and the test runs both peers on the default domain with
a comment naming 0656.

**Why the gate stays.** `check-goal-cdr-stripped.py` still covers what the test
cannot: the C++ arm, and any arm added later with no peer to point at.

**Defect 3, found while establishing a tier-2 baseline for this wave and fixed
([issue 0658](../issues/archived/0658-lane-skip-nested-in-aggregator-reads-as-real-failure.md)).**
Five tier-2 reds were lane SKIPS: each of five matrix aggregators tested
`msg.contains("[SKIPPED]")`, the BARE marker, so every classed `[SKIPPED:lane]`
was filed as a FAILED cell — and by the time it reached junit the marker was
nested inside an aggregate panic, where the rewriter's start-anchored match
cannot (and should not) look. `nros_tests::skip_marker` is now the one Rust
spelling, `check-skip-marker-matching` guards it, and a nested marker is NAMED
by `name-real-failures.py` rather than read as real. Tier-2 reds 9 → 3.

## W4 — Verify #532 against phase-352 before planning anything (#532)

#532 says the platform clock ABI fixes a unit but cannot express resolution, so
every port either lies or truncates.

[Phase 352](archived/phase-352-platform-clock-ns.md) is COMPLETE and its title is "one
nanosecond symbol, **plus the resolution nobody could ask for**" — which is, on
its face, exactly #532's subject. So the first task is not design: it is to
determine whether #532 is already resolved and simply never closed.

**Acceptance.** #532 is either closed against phase-352 with the specific
mechanism named, or restated to say what phase-352 did NOT cover. Do not plan
work on it before that check.

### DONE 2026-08-16 — restated, not closed

Checked against `platform.h` rather than against the phase doc, which is the
point of the wave:

| #532's proposed direction | verdict |
| --- | --- |
| 1. `clock_ns` as the one monotonic symbol | DONE — `platform.h:164` |
| 2. `clock_resolution_ns` | DONE — `platform.h:178` |
| 3. `clock_ms`/`clock_us` stop being per-port symbols | DONE **and further** — W6 retired them outright instead of keeping wrappers, gated by `check-retired-platform-clock-symbols` |
| 4. coarse path only if measured | DECIDED — RFC-0073 defers it "not refused" and names the trigger |
| 5. wall clock collapses to one `time_now_ns` | **NOT COVERED by phase-352** — done separately 2026-08-18, see below |
| open q: may `resolution_ns` change after init? | ANSWERED in the header (constant after init; a scaling port reports its coarsest) |

So #532 was neither "already resolved and never closed" nor open as written. It
is restated to item 5 alone: `time_now_ms` + `time_since_epoch_secs` +
`time_since_epoch_nanos` are still three symbols for one fact, and RFC-0073
mentions the wall clock only as EVIDENCE of the inconsistency, never as scope.
The `secs`/`nanos` split also caps seconds at `uint32_t` — a 2106 problem in a
tree that just moved monotonic to `u64` ns for range.

Per this wave's own instruction, the remaining work was NOT planned here.

**Postscript 2026-08-18 — the remainder landed.** Item 5 was done as its own
change rather than as a wave of this phase, which is what the restatement asked
for. The wall clock is now one `nros_platform_time_now_ns`; the three retired
symbols took the `uint32_t` seconds field (a 2106 overflow) with them, and the
bounded re-read loops in `nros-core` and `nros-node` — each of which named #532
in a comment as what would delete it — are single reads. #532 is resolved and
archived. Recorded here because this wave's output was the restatement, and a
restatement that outlives its subject reads as open work.

**One defect found by the check.** `condvar_wait_until`'s deadline was
documented in "`clock_ms` units" — a symbol W6 retired, so the C header
specified a parameter in terms of a function that no longer exists. The unit
never changed (every port names it `abstime_ms`; the Rust trait says
milliseconds), so this is wording catching up, not an ABI change. Fixed.

---

## Deliberately not doing

* **Not re-litigating the tier split.** RFC-0061's three tiers are settled;
  #466 is about the contract being unstated, not about the tiers being wrong.
* **Not opening a phase per bug.** W3 and W4 are singletons; they are here
  because an unowned bug is one nobody re-reads, not because they share a
  mechanism with W1/W2.
* **No new gate before W1's diagnosis.** If #493 is the known Corrosion-version
  hazard, adding a second mechanism alongside `check-cmake-corrosion-prefix`
  would be the "second spelling" antipattern CLAUDE.md names.
