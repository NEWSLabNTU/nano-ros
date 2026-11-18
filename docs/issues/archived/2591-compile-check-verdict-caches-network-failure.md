---
id: 2591
title: "A compile-check verdict row records a transient network failure as its verdict, and reuses it while its inputs are unchanged"
status: resolved
resolved_in: "the change that archives this issue"
type: bug
area: [build, tests]
severity: medium
found: 2026-11-15
related: [issue-1926]
---

## What happens

A `cargo-check-verdict` row in `examples/fixtures.toml` records the exit code
and stderr of a `cargo check` that is expected to fail, and the test asserts
the diagnostic. On 2026-11-15 the `just ci gate` run built
`main_macro_misuse_custom_tasks_empty` while a fetch of the
`ros-launch-manifest` git tag failed:

```
error: failed to get `ros-launch-manifest-model` as a dependency of package `nros-macros v0.6.0 (…)`
…
  network failure seems to have happened
```

`scripts/build/compile-check-fixtures.sh` recorded that as the row's verdict.
`linux_main_macro_misuse::custom_tasks_empty_on_owned_spin_still_errors`
then failed, expecting `only valid for the RTIC framework`. A rebuild did not
help: `cc_row_reusable` reuses a `.verdict` whose `.inputsig` matches, so the
network failure stood as the verdict until `NROS_COMPILE_CHECK_REBUILD=1`
forced the row. The test-side probe reported the fixture FRESH ("a DEGRADED
probe — it compared NOTHING").

## The class

Issue 1926 already refuses to reuse a verdict that is the nros CLI refusing
to run (`_cc_verdict_cli_refusal`): an answer about the HOST, not about the
code under test, recorded as if it were the code's. A failure to fetch a
dependency is the same kind of answer. It is about the network, and no input
the signature covers can change it.

## Direction

Refuse to record, and refuse to reuse, a verdict whose stderr is cargo
failing to obtain a source. The signatures are `failed to load source for
dependency`, `failed to fetch into`, `network failure seems to have
happened`, and `Couldn't resolve host`. Keep one list beside
`_cc_verdict_cli_refusal`. A refused row fails its build loudly, naming the
network, instead of writing a verdict, so the next build retries it.

## Fix

`_cc_verdict_fetch_failure` in `scripts/build/compile-check-fixtures.sh`
sits beside `_cc_verdict_cli_refusal`. It holds the one list of cargo's
source-fetch failures:
- `failed to load source for dependency`;
- `failed to fetch into`;
- `network failure seems to have happened`;
- `Couldn't resolve host`.

It matches them with whitespace collapsed, as the CLI-refusal check does.
Both doors ask it:
- `_cc_write_verdict_stamp`, the only writer of a `.verdict`, refuses to
  record such output. It fails the row's build naming the network, so no
  stamp is written and the next build retries.
- `cc_row_reusable` refuses to reuse a verdict already on disk that holds
  one.

`check-verdict-cli-refusal` covers it on the fast line. It runs the real
lifted bodies against the 2026-11-15 stderr and asserts three things: the
output is not recorded, and a poisoned verdict is not reused under either
`output` shape. Against main's builder all three are red.

Sweep: `git grep -n '_cc_verdict_cli_refusal\|_cc_verdict_fetch_failure\|_cc_write_verdict_stamp' -- scripts just`
(the gate's check 2 keeps `_cc_write_verdict_stamp` the only `.verdict` writer).
