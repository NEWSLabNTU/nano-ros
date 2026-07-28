---
id: 309
title: "Count-based e2e proofs cannot detect a wrong configuration — a whole feature stayed dark behind a green suite"
status: resolved
type: limitation
area: testing
related: [issue-0306, issue-0307, issue-0308]
---

## Finding (2026-07-28)

`workspace_features_e2e.rs` proves most workload cells with `Proof::*Count`
variants: spawn the fixture, count received messages, pass if the count is
reached. That answers "did anything arrive?" — it cannot answer "did what I
CONFIGURED take effect?", because the default configuration usually delivers
just as well as the intended one.

Concretely, `Proof::QosMatchedCount` guarded the three QoS workspace cells while
issue 0306 made every declarative Rust entity run `QosSettings::default()`. The
publisher's declared `reliable + transient_local + depth(10)` was being
discarded, and the test stayed green for three phases, because a default
publisher to a default subscriber delivers exactly as many messages as a
QoS-matched pair does. The workspace's own doc comment described "the visible
behaviour" of a profile that was not being applied.

The same shape produced issues 0307 and 0308: parameters resolved to the wrong
values and QoS overrides never applied on two of three languages, with nothing
red anywhere.

## The pattern

A proof that observes a SIDE EFFECT common to both the correct and the broken
configuration is not a proof of that configuration. Delivery counts, exit codes
and "no panic" all have this property.

The two e2es added this week avoid it deliberately, and are the shape to copy:

- `param_live_read_e2e` — the node publishes the parameter VALUE it resolved,
  so one number on the wire distinguishes correct (120) from each specific
  failure mode (250 = source ordering lost, 999 = section specificity lost).
- `qos_override_e2e` — a stock `rmw_zenoh_cpp` peer reports the ADVERTISED
  profile per endpoint, so the assertion names the policy, the role and the
  endpoint rather than a message count.

Both were mutation-checked. That step is what makes the difference real: the
first draft of `qos_override_e2e` passed with the 0306 fix reverted, because a
whole-report `contains("TRANSIENT_LOCAL")` matched the SUBSCRIPTION's profile
while the publisher's had been dropped. A test nobody has watched fail is a
test whose discriminating power is unknown.

## Fixed for the QoS cells (2026-07-28)

`Proof::QosMatchedCount` became `Proof::QosMatchedProfile { topic }`. It keeps
the delivery count and adds, when ROS 2 is present, a per-ENDPOINT assertion on
what the endpoint ADVERTISES — `Reliability: RELIABLE`,
`Durability: TRANSIENT_LOCAL`, `History (Depth): KEEP_LAST (10)`, the profile
the workspaces declare in code. Two of the three differ from
`QosSettings::default()`, so a defaulted entity cannot satisfy them. The check
skips (never fails) without ROS 2, so the cells keep their delivery coverage
everywhere.

The endpoint-block parser and the `ros2 topic info --verbose` runner moved into
`nros_tests::ros2` (`topic_endpoint_block`, `ros2_topic_info_verbose`) rather
than being copied out of `qos_override_e2e` — copying a rule is what issues
0303/0307 were about.

**Mutation-checked, and the result is this issue's own thesis demonstrated.**
Flipping the C talker's declared durability to `NROS_C_QOS_VOLATILE` and
rebuilding both fixtures:

```
[c qos] the PUBLISHER does not advertise its code-declared QoS
(`Durability: TRANSIENT_LOCAL` missing) — the per-entity profile was dropped
somewhere between the node and the wire
```

The test reached that assertion **after** the delivery count had already
passed. The old proof would have stayed green with the QoS wrong, which is
precisely the failure this issue describes. Restored, all three cells (c / cpp /
mixed) pass.

Answering the question that motivated the audit: **the C, C++ and mixed paths
were fine.** Their publishers advertise exactly the declared profile. Issue 0306
was Rust-only — but nothing had established that, and the count proof could
never have.

One new bug fell out of the strengthening: a C-family listener receives fine yet
`ros2 topic info` reports `Subscription count: 0` — it is invisible to ROS 2
discovery (issue 0311). The QoS cells therefore assert the PUBLISHER block only,
with a comment pointing there; turning the subscription assertion on is a
one-line change once 0311 is fixed.

## Remaining (the rest of the matrix)

The other cells were NOT audited. `CustomMsgFields`, `SafetyCrcCount` and
`RemapWireName` already observe something specific to their feature (decoded
field values, CRC-valid climbing counts, the remapped wire name plus silence on
the unremapped one), so they are not obviously in this class. `LoggingLines` and
`LifecycleActive` are worth the same question — "would this still pass with the
feature removed?" — before this issue is considered closed for the whole matrix.

## Direction (as filed)

Not a mass rewrite — most cells are proving delivery, which is what they should
prove. The work is to identify the cells whose NAMED feature is invisible to a
count, and give each an observable specific to it:

1. **Audit `workspace_features_e2e`'s cells**: for each, ask "would this still
   pass if the feature were removed?" Anything that would is mislabelled — it
   proves plumbing, not the feature. Rename it or strengthen it.
2. **The QoS cells (c/cpp/mixed)** are the known-bad ones: they should assert
   the advertised profile the way `qos_override_e2e` does, not a count.
3. **Adopt mutation-checking for feature e2es.** A one-line revert plus a rerun
   is cheap, and it is the only thing that distinguishes a guard from a
   decoration. Record the observed failure text in the commit, so a later reader
   knows the test has been seen failing.

Prerequisite for (2): the C/C++ QoS workspaces have native fixture rows already;
the Rust one gained its row with issue 0306's fix.
