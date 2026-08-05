---
rfc: 0069
title: "The action payload envelope: one CDR header, ROS 2's"
status: Draft
since: 2026-08
last-reviewed: 2026-08-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0069 — The action payload envelope

> Filed from [issue 0418](../issues/0418-action-payload-envelope-not-ros-compatible.md),
> found during phase-338 W3. The defect is not in dispute; **which envelope is
> canonical** is the decision, because the interoperable answer breaks a
> convention nano-ros already ships.

## Summary

**nano-ros should put ONE CDR encapsulation header on an action feedback or
result payload — the one ROS 2 puts there — and delete the second one its raw
path currently adds.**

This is a wire-format change. It is proposed anyway because the alternative is
that nano-ros actions never interoperate with ROS 2, which is the project's
reason to exist.

## The two envelopes

ROS 2 `<Action>_FeedbackMessage` on the feedback topic:

```
[CDR header][goal_id (16 B UUID)][feedback fields]
```

nano-ros raw path today:

```
[CDR header][goal_id (4 + 16 B)][CDR header][feedback fields]
              ^^^^^^^^^^^^^^^^^              ^^^^^^^^^^^^^^^
              framing                        the extra one
```

Result payloads have the same extra header. The typed path
(`register_action_server`, used by `examples/native/rust/action-server` before
phase-338) already emits the single-header form, which is why the two nano-ros
paths cannot talk to each other.

## Why the second header exists

It is not an accident, and the code says so. `nros/src/node.rs::complete_goal`:

> "Without the header the reader eats the first data word (e.g. a sequence
> length) → empty/garbage payload (issue #35 M-F.23 follow-up: action result
> `sequence` deserialized to len 0)."

The raw consumer reads the body with `CdrReader::new_with_header`, so the body
had to carry a header for that reader to be correct. The producer was made to
match the consumer. **The bug is not the header; it is that the consumer was
written to expect one and nobody checked that against ROS 2.**

That matters for the fix: removing the producer's header alone reproduces
exactly the corruption issue #35 documents. Producer and consumer change
together or not at all.

## Options

### A. Adopt ROS 2's envelope (recommended)

Producer drops the inner header; the raw consumer stops consuming one.

- **For:** actions interoperate with `rcl_action` peers and with nano-ros's own
  typed path. One envelope in the system instead of two. Removes a divergence
  that will otherwise keep producing bugs of this shape.
- **Against:** breaks nano-ros↔nano-ros action pairs across the version
  boundary — a v0.5 embedded image and a v0.6 host cannot exchange feedback.
  Every action runtime cell must be re-run on real targets.

### B. Keep the extra header, document it as nano-ros-native

- **For:** no wire break; no re-verification of embedded lanes.
- **Against:** nano-ros actions remain permanently invisible-in-practice to
  ROS 2, and the typed and raw paths stay mutually incompatible *inside*
  nano-ros. Issue 0418's symptom becomes a permanent carve-out, and every future
  action feature has to pick a side. This is the option that keeps a defect and
  calls it a design.

### C. Translate at the boundary

Keep the internal envelope; strip/insert the extra header in the RMW shim when
talking to a non-nano-ros peer.

- **For:** no break for existing pairs.
- **Against:** requires knowing whether the peer is nano-ros, which the wire
  does not tell us. Discovery-time peer sniffing is exactly the kind of
  bidirectional guessing RFC-0064 rejects elsewhere. Rejected unless someone can
  show a reliable discriminator.

**Recommendation: A.** The compatibility this buys is the product; the
compatibility it breaks is between two of our own versions, and we control the
migration.

## What A requires

1. **Producer:** `nros/src/node.rs` — `publish_feedback`, `complete_goal`.
   Serialize the body with a header-less writer.
2. **Consumer, same commit:** `action_core.rs::try_recv_feedback_raw` and the
   `CallbackCtx::message` read path for feedback/result. Split the reader that
   consumes the *envelope* from the one that consumes the *body*, so the body
   reader no longer expects an encapsulation header.
3. **Audit** any C / C++ / ffi action client reading feedback or result. (Not
   codegen: `packs/cpp/message_exports.rs.jinja`'s `new_with_header` is ordinary
   per-message CDR and is correct.)
4. **Regression evidence, not inspection.** The bug that motivated the extra
   header (issue #35: a result `sequence` decoding to length 0) must have a test
   that fails without the fix. Its absence is why this survived.
5. **Interop proof:** a `ros2 action send_goal` against a nano-ros raw server,
   asserting feedback and result decode — the case no current cell covers,
   because every action cell is raw↔raw.

## Acceptance

- [ ] One envelope: a captured feedback payload from the raw path is
      byte-identical in shape to the typed path's.
- [ ] `examples/native/rust/action-server` runs Node-class against the existing
      typed `action-client`, delivering feedback and result (phase-338 W3's
      blocked case).
- [ ] A real ROS 2 client drives a nano-ros raw action server end to end.
- [ ] Every action Runtime cell green on real targets, embedded included.
- [ ] A test covers the issue-#35 corruption directly.

## Risks

- **Silent version skew.** Old and new images link and discover each other, then
  fail only on payload decode. Worth considering whether the type hash or a
  version marker should make the skew loud rather than mysterious.
- **The embedded lanes are the gate and they are slow.** This cannot be verified
  on a host alone; every action cell is raw↔raw today and those are exactly the
  pairs the change breaks and must re-prove.
- **Numbering.** RFC ids have no reservation tool (unlike issues), and 0062→0064
  already collided once between parallel sessions. If 0069 is taken, renumber
  and fix the two inbound links from issue 0418 and phase-338.
