---
id: 501
title: "`native_main_macro_misuse` shares ONE cargo target dir across five cases that all build `demo_entry`, so a sibling's successful artifact satisfies the check a misuse case expects to FAIL"
status: open
type: bug
severity: high
area: testing
related: [issue-0495, issue-0041, phase-342, phase-346]
---

## Symptom

Which cases fail varies run to run, on an unchanged tree:

| run | failed |
| --- | --- |
| full `just ci` sweep | `custom_tasks_on_owned_spin_emits_error` |
| suite, default threads | `unknown_board_emits_compile_error`, `rebuilds_on_model_touch` |
| suite, `--test-threads=1` | `rebuilds_on_model_touch` |
| suite, repeat 1 | `unknown_board…`, `custom_tasks_on_owned_spin…`, `rebuilds_on_model_touch` |
| suite, repeat 2 | `custom_tasks_on_owned_spin…`, `unknown_board…`, `rebuilds_on_model_touch` |
| suite, repeat 3 | `custom_tasks_empty_on_owned_spin_still_errors`, `rebuilds_on_model_touch` |
| any ONE case alone | PASSES |

**Four of the five cases have failed at least once**, in different
combinations, with nothing changed between runs. Every case passes in
isolation, warm or cold.

The message is misleading in a specific way:

```
expected `cargo check` to fail when `custom_tasks` is used outside RTIC.
stderr:
    Finished `dev` profile [optimized + debuginfo] target(s) in 50.07s
```

`Finished`, no error. So the assertion is not "the compile error had the wrong
text" — it is that **no compile happened at all**.

## Cause

`shared_check_target_dir()` (phase-342 W2) points every case at ONE
`CARGO_TARGET_DIR` under the build root. Each case stages its own copy of
`fixtures/n9_workspace` into a fresh tempdir and edits in the misuse under test
— but every copy builds the same package name:

```
packages/testing/nros-tests/fixtures/n9_workspace/src/demo_entry/Cargo.toml:
    name = "demo_entry"
```

Five different source trees, one package name, one target dir. A sibling case
that compiled `demo_entry` SUCCESSFULLY leaves an artifact whose fingerprint the
next case's check is satisfied by, so cargo reports `Finished` without expanding
the macro — and a test whose whole point is "this misuse must FAIL to compile"
observes success.

That is why isolation passes and why the failing set moves: it depends on which
sibling won the dir first, which nextest schedules differently every run.

**The phase-342 comment anticipated the wrong hazard.** It reasoned carefully
about the LOCK:

> Concurrent cargos DO serialize on this dir's lock (phase-340 F3), and that is
> fine here and only here: the alternative is not parallel warm builds, it is
> five COLD ones.

Serialization is indeed fine. Artifact ALIASING is the hazard, and it is not
mentioned — the same dir that makes the cases fast makes them share state. The
measured win was real (108.5 s → 10.3 s); the correctness cost was not seen.

## This likely explains #495

Issue 0495 (`rebuilds_on_model_touch` fails, "cargo short-circuits in 0.04 s
after the resolved model is touched") lists two candidate causes and marks the
trigger UNPROVEN. This is a third, and it predicts exactly that symptom: a
short-circuit in hundredths of a second is what an already-satisfied fingerprint
looks like. `rebuilds_on_model_touch` is the one case that fails in EVERY
configuration above, including serial — consistent with it being the case most
sensitive to a pre-populated dir rather than a separate defect.

Worth testing 0495 against a per-case target dir before pursuing its own
candidates.

## Fix

The shape that keeps both properties — warm and isolated — is a target dir keyed
by the CASE rather than by the binary, since it is the package-identity
collision, not the sharing, that breaks it. Cheaper variants worth measuring
first:

1. **Rename the package per case** (`demo_entry_<case>`), so fingerprints cannot
   collide and one warm dir still serves the shared dependency graph — the deps
   are what the 10x came from, not `demo_entry` itself.
2. **Per-case dir under one parent**, accepting a small rebuild of the leaf
   crate only.

And the standing rule this suite violates: **CLAUDE.md's "No compilation inside
tests"** — compile in the build stage, assert the artifact. Archived issue 0041
was this class suite-wide. A misuse case is a natural
`compile-check`-style build fixture whose EXPECTED result is failure; that
framing removes the shared-state question entirely rather than tuning it.

## Reproduce

```sh
source ./activate.sh
cargo nextest run -p nros-tests --test native_main_macro_misuse   # repeat 3x
```

Some subset fails each time, and the subset moves. Then:

```sh
cargo nextest run -p nros-tests --test native_main_macro_misuse \
    -E 'test(=unknown_board_emits_compile_error)'
```

passes every time, cold or warm.
