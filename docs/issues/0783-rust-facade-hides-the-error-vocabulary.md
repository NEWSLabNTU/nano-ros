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

## Resolution (2026-08-25)

**`NanoRosError` was dead, and so was `RclReturnCode` with it. Both deleted**,
along with `ErrorContext`, `NestedError`, `NanoRosErrorFilter`,
`TakeFailedAsNone` and `ServiceResult` — the whole `nros-core/src/error.rs`
module. Evidence, in the order it settled the question:

* `grep -rn NanoRosError --include='*.rs' packages/ examples/` outside the
  defining file returned **three** lines, all inside `nros-core` itself: the
  crate's own re-export, and `ServiceResult<T> = Result<T, NanoRosError>` in
  `service.rs`. `ServiceResult` in turn had no consumers at all.
* `RclReturnCode` had **one** appearance outside `error.rs`: the same re-export
  line. It was the `code` field of `NanoRosError` and nothing else.
* Phase 84.D1 (`docs/roadmap/archived/phase-84-api-ergonomics-and-consistency.md:85`)
  already recorded the decision: "`NodeError` is confirmed as the single
  user-facing error in every `nros-node` return signature", with "folding
  `NanoRosError` … into `NodeError`" listed as *deferred*. The fold never
  happened; the loser stayed in the tree and in the RFC.
* The decisive one: `nros-core`'s own `RosAction::register_protocol_types`
  returns `Result<(), ()>` with a comment saying the crate "cannot name
  `nros-node::NodeError`" — a unit error chosen over an error type sitting in
  the same crate, in the same module tree. Nobody who wrote that considered
  `NanoRosError` to be an error type.

Exporting `RclReturnCode` was rejected rather than deferred. Its numeric space
is `rcl_ret_t`'s (1, 2, 100, 200…); **ours is `NROS_RET_*`, 0 and −1…−16**
(`packages/api/nros-c/src/error.rs`), an independent space that exists only at
the C/C++ ABI. Nothing converted between them, and the Rust API carries no
numeric code at any layer. Exporting it would have replaced an unreachable type
with a nameable type a user can never receive — the same defect one step
further along.

**RFC-0036's Errors row now names `nros::NodeError` + `nros::TransportError`**,
with the `no_std` constraint spelled as the two things rclrs's error has that
ours cannot (a per-thread formatted-message buffer, an allocated source chain)
and the note that `core::error::Error` *is* implemented since phase-359. That is
the third name this row has carried: `RclrsError` → `NanoRosError` → `NodeError`.
The first two were prose; this one is checked by `scripts/api-parity.py`.

**The two `NodeError` enums do not collide, and never did.** `mod node` is
PRIVATE in `nros-node` (`lib.rs:102`), and `lib.rs:155` re-exports its enum as
`StandaloneNodeError`:

```rust
pub use node::{Node as StandaloneNode, NodeConfig, NodeError as StandaloneNodeError};
```

So `nros_node::NodeError` unambiguously means the executor enum, which is the
one the facade exports and the one every fallible user call returns. The
duplicate name exists only inside two private modules. Left as is.

What that investigation *did* surface, and what is deliberately NOT fixed here:
`StandaloneNodeError` has **zero** consumers in the entire tree — including the
facade, which exports `StandaloneNode` (`lib.rs:674`) without the error its
constructors return. That is a real hole of exactly this issue's shape, but
adding a fifth `Node`-ish name to `nros::` belongs with **issue 0784**, which
already catalogs `StandaloneNode` among four node-shaped exports needing
disambiguation, and with phase-379 W5, which owns the facade's export policy.
Fixing it here would have pre-empted both.

**Discoverability nit (part 1, the corrected half): done.** `NodeError` and
`TransportError` now sit in one commented block in `packages/api/nros/src/lib.rs`
instead of the boot-types block and the transport-traits block respectively.
The exported set is byte-for-byte unchanged — `scripts/api-parity.py --check`
reports the same rows before and after — and the prelude already listed the two
together, which is why this was a nit and not a gap.

Ledger (`docs/reference/api-parity-ledger/types.json`): `rust:RclReturnCode`
`gap` → `declined`; `rust:RclrsError` rewritten to drop the retracted
`TransportError` claim; `rust:TakeFailedAsNone` (+ its method) `divergence` →
`declined`, and their `why` corrected — they had been filed as part of rclrs's
action/goal model when the trait lives in `rclrs/src/error.rs` and is the
take-failed adapter we had copied and have now deleted.

**Still wrong elsewhere, not fixed here:** `docs/reference/api-parity-ledger/node.json`'s
`rust:NodeError` row repeats the retracted claim ("it is exported but
`TransportError`, its most common payload, is not"). That shard belongs to the
`node` stage.
