---
id: 783
title: "`RclReturnCode` exists and is unreachable, and RFC-0036 documents a Rust
  error type the user API never returns"
status: open
type: bug
area: api, docs
related: [rfc-0036, rfc-0037, phase-379]
---

## Problem

Three separate facts, found together while classifying phase-379's `types`
stage:

**1. The error vocabulary is exported, but not where a reader looks.**
`packages/api/nros/src/lib.rs:855` re-exports `NodeError`, which is what every
fallible call in the Rust user API returns:

```rust
// packages/core/nros-node/src/executor/node.rs:211
pub fn create_publisher<M: MessageForRmw>(
    &mut self, topic_name: &str,
) -> Result<EmbeddedPublisher<M>, NodeError>
```

`NodeError::Transport(TransportError)` is its most common variant, so handling
one means naming `TransportError`.

**CORRECTION (2026-08-25).** This issue originally claimed `TransportError` was
not exported and that a caller therefore could not match on the error. **That was
wrong.** It is exported unconditionally at `lib.rs:687`, in the "Re-export
transport types" block. Phase 379's `other` stage caught the error and it was
verified before this correction.

What is actually true is much narrower and not a capability gap: the "Re-export
core types" block at `lib.rs:206` — which lists `CdrReader, CdrWriter, Clock,
ClockType, DeserError, Deserialize, Duration, Logger, MessageInfo,
PUBLISHER_GID_SIZE, RawMessageInfo, RosMessage, RosService, SerError, Serialize,
Time` — contains neither error type, so a reader looking for the error vocabulary
beside the other core types does not find it there. That is a discoverability
nit. The two findings below are the substance of this issue.

**2. `RclReturnCode` exists and is unreachable.** (Verified again 2026-08-25:
`grep -c RclReturnCode packages/api/nros/src/lib.rs` → 0.) `nros-core/src/error.rs:36`
defines it as the `rcl_ret_t` mirror RFC-0036 describes. Nothing in
`packages/api/nros` re-exports it, so a Rust user cannot name it. rclrs exports
its equivalent; phase-379's correlator reports `rust:RclReturnCode` as
`theirs-only` for exactly this reason.

**3. RFC-0036 documents a type the user API does not return.** Its Errors table
says:

> rclrs `RclrsError` → Rust `NanoRosError { code: RclReturnCode, context, nested }`

`NanoRosError` is defined at `nros-core/src/error.rs:235` and appears **nowhere**
in `packages/core/nros-node` or `packages/api` (re-verified 2026-08-25) — a user meets `NodeError`, which
has none of that shape. The RFC already carries a "naming note" correcting an
earlier `RclrsError` mislabel; the corrected name is also not the one users see.

Related, and worth separating before anyone acts: there are **two distinct
`NodeError` enums** — `nros-node/src/node.rs:45` (`MaxPublishersReached`,
`InvalidPublisherHandle`, …) and `nros-node/src/executor/types.rs:534`
(`Transport(TransportError)`, `NameTooLong`, …). The facade exports the second.
Whether the first should exist under that name is its own question.

## Why it matters

A `no_std` user has no `Box<dyn Error>` and no `anyhow`; matching on the error
enum is the only way to react to a failure. An error type whose variants name
types the caller cannot import is only usable as "something went wrong", which
is the same information a `bool` carries.

And the documentation half is the more expensive one: RFC-0036 is the authority
a porting user reads to learn how nano-ros differs from rclrs, and its Errors
row describes a type that does not participate in the API. This is the second
time that row has been wrong.

## Evidence

* `packages/api/nros/src/lib.rs:206-209` — the core re-export block, without
  `NodeError`'s vocabulary.
* `packages/api/nros/src/lib.rs:855` — `NodeError` exported.
* `grep -rl NanoRosError packages/core/nros-node/src packages/api` — no matches.
* `scripts/api-parity.py --topic types --lang rust` — `RclReturnCode`,
  `RclrsError`, `RclrsErrorFilter`, `RclErrorMsg` all `theirs-only`.

## Direction

Not decided here; phase-379 W2 records the classification and W5 owns the
facade's export policy. The three parts are separable:

* Export `RclReturnCode`, if it is meant to be user vocabulary, and consider
  moving `TransportError` next to `NodeError` in the core block so the error
  vocabulary reads as one group.
* Decide whether `NanoRosError` is dead code or the intended user error. If it
  is dead, delete it — a second error type that documents the API but is not in
  it is worse than none.
* Correct RFC-0036's Errors row to whatever survives, and check it against
  `scripts/api-parity.py` rather than by reading.
