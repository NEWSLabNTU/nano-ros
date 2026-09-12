---
id: 1351
title: "The Rust parity extractor builds neither `NodeCtx` nor the `sim-time`
  surface, so four ledger rows can never be matched and `install_ros_time_source*`
  has none at all — a ledger row that is unmatchable by construction reads the
  same as one that is merely unwritten"
status: resolved
type: tech-debt
area: api, tooling, rust
severity: low
related: [1066, 1020, 1323, phase-430, phase-428, RFC-0089]
---

## What happens

`scripts/api_parity/extract_rust.py` builds the Rust surface reachable from the
`nros` umbrella crate under a fixed feature list, `NROS_FEATURES`
(`scripts/api_parity/extract_rust.py:41`). Two things a phase has already
ledgered are outside that surface, for two different reasons:

* **`NodeCtx` is not re-exported from the umbrella.** `packages/api/nros/src/lib.rs`
  mentions the name only inside a doc comment (`:1443`); no `pub use` reaches
  the type. So no `NodeCtx` method has ever produced an extracted row —
  `create_timer_in_group` has none either, and that is not a gap in the API.
* **`sim-time` is not in `NROS_FEATURES`.** The feature exists on the umbrella
  (`packages/api/nros/Cargo.toml:206`, `sim-time = ["nros-node/sim-time"]`) and
  it is what gates the entry points: `NodeCtx::install_ros_time_source` and
  `install_ros_time_source_on` are both `#[cfg(all(feature = "sim-time", any(has_rmw, test)))]`
  (`packages/core/nros-node/src/executor/node.rs:1520`, `:1528`). The list was
  grown once before for exactly this class — phase-428's Q1 follow-through added
  `env` because `rust:init_with_args`, the one Rust REFUSE-LOUD row, was on no
  measured surface — and the comment recording that is still in the list.

## Why it is worth a row of its own

Four `rust:NodeCtx::*` rows exist in `docs/reference/api-parity-ledger/timer.json`
and say so at the row: "RECORDED AHEAD OF THE TOOL, deliberately … it will be
matched the day the extractor reaches it." Rows for
`install_ros_time_source*` do NOT exist — phase-430 W7 calls them "still owed,
deliberately", waiting on this same tool.

The two states are indistinguishable to a reader and to every gate:

| what is true | what the ledger shows |
| --- | --- |
| a row nobody has written yet | absent |
| a row that cannot be matched until the extractor moves | absent |
| a row whose subject was deleted | present, unmatched (issue 1323) |

So "the extractor does not reach it" is load-bearing bookkeeping, and it
currently lives in one `why` string and one roadmap document. When phase-430 is
archived, the roadmap half goes with it.

## What would close this

Either half is a real answer and they are not equivalent:

1. **Move the tool.** Add `sim-time` to `NROS_FEATURES`, and decide whether
   `NodeCtx` belongs in the umbrella's public surface at all — it is reached
   today only through `nros::main!`-generated code, which is an argument for
   leaving it out and an argument for measuring it separately, not for silence.
   Then write the `install_ros_time_source*` rows and let the four existing
   `rust:NodeCtx::*` rows correlate.
2. **Say it in the ledger's own vocabulary.** A row (or a shard-level note)
   that marks "unmatchable: outside the extracted surface, because X" so the
   absence of a correlation is an answer rather than a hole, and so issue
   1323's stale-row detection can tell the two apart when it lands.

Not urgent: nothing ships differently either way, and `check-api-parity` runs in
no workflow at all (issue 1066), so no lane is red on it. Filed so that the fact
outlives the phase document that measured it.

## Resolution (2026-09-12)

Both halves of "1. Move the tool" landed, and the measurement corrected three
things nobody could have written down without running it.

### `sim-time` joined `NROS_FEATURES`

One line, the same shape as phase-428 Q1's `env`, and the comment records the
same reason. It costs one message crate (`nros-rosgraph-msgs`) and no backend.

### `NodeCtx` is `pub use`d from the umbrella — and that is not a new surface

The decision the issue left open. Exporting it does **not** enlarge what a
porting user can reach, because every method on it was already reachable:
`Executor::node_mut` is `pub` and RETURNS a `NodeCtx`, the `nros` crate's own
first doc example is `executor.node_mut(node).create_publisher::<Int32>(…)`
(`packages/api/nros/src/lib.rs:25`), and 15 in-tree binaries call its methods.
`nros_node::executor`'s `mod node` is private, so the type had no PATH while its
whole API had callers — which cost a user the ability to write
`fn setup(ctx: &mut NodeCtx)` at all, and cost the parity tool every row.

The alternative — teaching the extractor to reach crate-internal types — was
refused on two grounds. `--document-private-items` is deliberately not passed
(the extractor says so in its docstring: "the surface under comparison is the
one a user can reach"), and turning it on would pull thousands of genuine
internals into a bucket count that is supposed to mean "what a ported program
gets". A per-type allowlist would be an AUTHORED map measuring a surface a user
cannot name, which is the RMW-parity lesson and the wrong half of it.

`NodeCtx` joins `CallbackGroup` and `NodeHandle` on the line they already
shared, then `nros_node`'s root, then `nros`'s `rmw-cffi` block.

### What the widening cost: 25 rows, not the two this issue named

| | rust lane |
| --- | --- |
| before | same 97, arity-only 7, systematic 2, differs 6, ours-only 1280, theirs-only 493 |
| after | same 97, arity-only **9**, systematic 2, differs **5**, ours-only **1305**, theirs-only **492** |

23 rows authored (the two `install_ros_time_source*` this issue is named for,
three `Executor::*` sim-time accessors, and 18 `NodeCtx` methods: the callback-
group family, the generic/view subscription family, the builders, the
callback-delivery service and action clients). Same shape as phase-428's `env`:
the estimate was two rows and the bill was 25.

### Three corrections the measurement forced, which reading could not

1. **The two rows "recorded ahead of the tool" were keyed WRONG, not merely
   unmatched.** `TYPE_SYNONYMS` folds `NodeCtx` onto `Node`, so the report emits
   `rust:Node::create_timer_on_clock` and `rust:NodeCtx::create_timer_on_clock`
   is a key it can never print. The schema already said "run the report to get
   the spelling; do not guess it", and the guess could not have been right. This
   is the issue's own thesis, demonstrated: an unmatchable row and a row for a
   name that does not exist are the same object.
2. **Two EXISTING rows argued from an absence the widening disproved, with no
   signature moving.** `rust:Node::create_service` went `theirs-only` →
   `arity-only` and `rust:Node::create_subscription` went `differs` →
   `arity-only`, because `NodeCtx` contributes a second overload of ours under
   each key. Both said, in prose, that ours was only the declarative spelling.
   Both amended. RFC-0089's "the ledger is a join over two moving surfaces" —
   with the surface that moved being ours, and the movement being a MEASUREMENT
   change rather than a code change.
3. **Two C++ rows had been inert since they were written**, found by the new
   gate rather than by looking: `cpp:GenericTimer::GenericTimer<FunctorT,
   std::shared_ptr<Clock>>` and its `WallTimer` twin. The extractor renders that
   defaulted template argument as empty, so the printed key is
   `…<FunctorT, >`; the report served those lines by INHERITANCE from the type
   row, which is what the `divergence*` asterisk says. Deleted rather than
   re-keyed — their `why` is a verbatim copy of the type row's, so re-keying
   would carry no verdict at a spelling that moves with the next clang.

### The class, gated

`check-ledger-key-spelling` (`scripts/check-ledger-key-spelling.py`, fast line,
buildless): **a ledger key must be a fixed point of `correlate.normalize`** —
the function that computes every key the report prints. A key that is not one
can never be emitted, so it matches nothing in any bucket whatever the tree
does, which is exactly the state indistinguishable from "unwritten". Derived
from the correlator's own tables rather than an authored list of bad spellings,
so a new fold tightens it with no edit.

Measured on landing: **4 of 2770 rows, all four real** — the two above and the
two C++ ones.

A hard failure rather than the counted-exemption design the issue's option 2
proposed, for two reasons. A prose declaration is not checkable, which is the
failure being fixed: the two rows DID say they were recorded ahead of the tool,
and a reader had no way to tell that from a typo. And an exemption list is a
place to park a defect whose fix is one line — re-key, or delete.

Negative control on the normal path, as `check-gate-selftests` requires: nine
live keys that must NOT be flagged (a glob key, a `_BASE_KEEP` type, the
`<FunctorT, >` constructor spelling, `LifecyclePollingNodeCtx` — which is not
`NodeCtx`) are checked FIRST, then five mutated keys that must be caught, each
against the exact spelling the gate should suggest.

### What is NOT closed

Issue 1323, the reverse walk: a well-spelled key whose SUBJECT no longer exists
still passes silently. That question needs the extractor; this one does not, and
they are complementary halves of the same asymmetry.
