---
id: 1351
title: "The Rust parity extractor builds neither `NodeCtx` nor the `sim-time`
  surface, so four ledger rows can never be matched and `install_ros_time_source*`
  has none at all — a ledger row that is unmatchable by construction reads the
  same as one that is merely unwritten"
status: open
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
