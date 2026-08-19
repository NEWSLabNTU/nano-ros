---
id: 690
title: "`case_08_c_qos` fails in-sweep and passes solo: the QoS profile assertion reads the FIRST endpoint block, which need not be the one under test"
status: resolved
type: bug
area: testing
related: [issue-0309, issue-0312, issue-0445, issue-0670, phase-263]
---

## Symptom

`workspace_features_e2e::workspace_features::case_08_c_qos` fails with the
publisher advertising the DEFAULT profile:

```
[c qos] the PUBLISHER does not advertise its code-declared QoS
(`Durability: TRANSIENT_LOCAL` missing) — the per-entity profile was dropped
somewhere between the node and the wire
  Reliability: RELIABLE
  History (Depth): KEEP_LAST (10)
  Durability: VOLATILE
```

`RELIABLE + VOLATILE + KEEP_LAST(10)` is exactly `nros_c_qos_default()`.

## It is intermittent, and only in-sweep

Measured in the ROS distrobox (the only environment where this assertion runs
at all — see "Why nobody saw it" below), all on the SAME freshly built tree at
`ded3c0a96`:

| run | scope | result |
| --- | --- | --- |
| tier-1 sweep | 1453 tests | FAIL |
| solo, with `case_13` + `case_17` | 5 tests | PASS |
| tier-1 sweep | 1453 tests | PASS |

So it is not a product regression: the same binaries pass and fail. It is the
in-sweep-only class CLAUDE.md already names ("retest a QEMU red SOLO before
filing"), reaching a cell whose assertion is about a VALUE rather than about
timing — which is why it does not read like a flake.

## Why the sibling cells do not fail

`case_13_cpp_qos` and `case_17_mixed_qos` pass in every run. `case_17` reuses
the **byte-identical** C component (`diff` of
`c_qos_talker_pkg/src/QosTalker.c` against `mixed_qos_talker_pkg/src/QosTalker.c`
differs in one comment word), an identical launch file, and a model with no QoS
in it. Ruled out by reading, in this order:

- the `nros_cpp_qos_t` hand-mirror in `nros-c/include/nros/component.h` — it is
  field-for-field identical to the `nros_cpp_ffi.h` SSoT, `tx_express` included,
  so this is not the issue-0160 by-value drift class;
- `nros_cpp_node_t` — opaque in the mirror, so a C TU cannot get its size wrong;
- QoS overrides — neither generated entry calls
  `nros_cpp_node_set_qos_overrides`, and both models declare no QoS.

What is left is the entry carrier (`NROS_MAIN_C` vs `NROS_MAIN`) — and an
intermittent failure is not what a compile-time difference between two carriers
produces.

## The likely mechanism, and why it is still "likely"

`nros_tests::ros2::topic_endpoint_block` returns the **first** block of the
requested kind:

```rust
let start = report.find(&marker)?;
```

If more than one PUBLISHER is on `/chatter` when `ros2 topic info -v` runs, the
assertion reads whichever ROS 2 listed first. A foreign publisher using the
default profile would fail this assertion exactly as observed, and would only
be present under a sweep — which matches the table above. Each cell does start
its own `ZenohRouter::start_unique()`, so this requires endpoints reaching each
other despite that; unproven, and the reason this issue is open rather than
fixed.

**The evidence to settle it did not survive the failure.** The panic printed the
selected block and not the report it came from, so "the profile was dropped" and
"this is someone else's endpoint" are indistinguishable in every failure
recorded so far — issue 0445's class, one layer in. Fixed here: the assertion
now carries the full report and the endpoint count, so the next occurrence says
which of the two it is. Both sweeps run after that change passed, so the
evidence is armed and unspent.

## Why nobody saw it

The profile assertion is gated on `require_ros2()` and SKIPS without ROS 2
(deliberately — issue 0309 kept the delivery coverage everywhere and added the
profile check where a peer exists). No CI host here has ROS 2, so this assertion
has only ever run inside the distrobox. Issue 0309 recorded all three cells
passing, mutation-checked, on 2026-07-28 — that was a ROS 2 host, and it is the
last time before now that the check ran at all.

## Fix direction

Select the endpoint block by the node under test rather than by position — the
report already delimits blocks with `Node name:`, which is what
`topic_endpoint_block` scans for as a terminator. Note that all three qos cells
name their node `qos_talker`, so node-name selection distinguishes a foreign
`/chatter` talker but not the sibling cells from each other; if the evidence
comes back showing a sibling, the discriminator has to be the GID or the
namespace.

Do not "fix" it by asserting that ANY publisher block matches: that weakens the
assertion in the exact direction issue 0309 was filed to close.

## Fixed 2026-08-19 — select by the node under test, and keep the sibling case visible

Took the stated direction. `nros_tests::ros2` gains a parser over the whole
report rather than a scan for one marker:

* `topic_endpoints(report) -> Vec<TopicEndpoint>` — every record, split on the
  `Node name:` delimiter, each carrying its own `node`, `kind` and block. Keys
  only on `Node name:` and `Endpoint type:`, because the other fields
  (`GID:`, `Node namespace:`) appear on some distros and not others.
* `topic_endpoints_for_node(report, kind, node) -> Vec<TopicEndpoint>`.

`workspace_features_e2e` asks for `qos_talker` / `qos_listener`, the names the
launch files give these nodes (`demo_bringup/launch/*qos*.xml`).

**It returns a Vec on purpose.** This issue already recorded that all three qos
cells name their nodes identically, so node selection rules out a FOREIGN
process and cannot separate the siblings. Returning `Option` would have picked
the first of two — the same defect one level up, in the code written to fix it.
More than one match now fails with its own message naming the count and saying
the discriminator has to be the GID or the namespace.

`topic_endpoint_block` is kept: `qos_override_e2e` has its own inline copy of the
positional logic, and both are correct where exactly one endpoint of a kind
exists. Converging those is a separate change; what mattered here was that the
CELL WITH TWO stopped guessing.

### Verified

* **Deterministic, and the part that actually proves it** — three unit tests on
  a two-publisher report built to the shape issue 0312 captured:
  `endpoints_are_split_by_node_and_kind` (each record carries its own QoS, not
  the next one's), `selecting_by_node_ignores_a_foreign_publisher` (with the
  foreign endpoint listed FIRST and advertising `nros_c_qos_default()` — the
  positional helper reads it, node selection does not), and
  `two_endpoints_sharing_a_node_name_are_both_returned`.
* **The assertion still bites** — mutation-checked by expecting a durability
  value nothing advertises: `case_08_c_qos` fails. This issue warned against
  "fixing" it by accepting any block, and that check is what rules it out.
* **In-sweep** — `just ci`, 1471 tests: all three qos cells PASS. The only real
  failure is `case_06_c_lifecycle`, the known ros2cli daemon flake, which passes
  solo.

**A green sweep is not proof and is not claimed as such.** This issue's own
table records a sweep that passed before any change; that is what "intermittent"
means. The deterministic evidence is the unit tests — they show the exact
selection difference on a fixed input, which no sweep can.

### Still not established: whether a foreign endpoint was ever really there

The mechanism remains what this issue called it — likely. Each cell starts its
own `ZenohRouter::start_unique()`, so a foreign publisher requires endpoints
reaching each other across routers, which is unproven. What changed is that the
question no longer has to be answered to make the assertion correct: it now
reads the node under test whatever else is on the topic, and says so explicitly
when two endpoints share a name. If the sibling message ever fires, that settles
it the other way.
