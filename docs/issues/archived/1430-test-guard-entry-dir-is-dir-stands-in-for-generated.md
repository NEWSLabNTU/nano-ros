---
id: 1430
title: "Nothing gates the class issue 1411 fixed: a test guard that uses
  `is_dir()` on an entry directory as a stand-in for \"a package was generated\",
  after that directory grew a second producer"
status: resolved
type: tech-debt
area: [testing, build, ci]
severity: low
related: [issue-1411, phase-445, rfc-0098]
found: 2026-09-21
resolved: 2026-09-22
---

## The class

Issue 1411 was one guard:

```rust
if entry.is_dir() { /* the entry was generated */ }
```

`build/<coord>/<entry>/` used to have exactly one producer, so its existence did
mean a package had been generated. Phase-445 W4/W5 (RFC-0098 D1) gave it a
second: `nros build` writes `nros-cargo.toml` into that same directory
**unconditionally**, including on the path where model resolution has already
failed and warned. From that commit on, the directory exists either way and the
guard tests nothing — the test took its degraded branch, asserted nothing it had
not already asserted, and reported `ok`.

The defect is not the `is_dir()` call. It is that **a guard standing in for "the
artifact was produced" keeps reading as correct after the thing it names grows a
producer**, and nothing in the tree notices. 1411's own corroboration makes the
point: three tests further down the same file,
`a_hand_written_entry_suppresses_generation` already asserted on
`…/native_entry/Cargo.toml` and carried a comment about the settings file
landing there either way. It was updated when the file moved. The guard above it
was not.

## What 1411 already did

* fixed the one guard (it now names `Cargo.toml`, the artifact the test
  `unwrap()`s);
* swept `is_dir()` / `exists()` across the CLI test targets and read every
  control-flow guard individually — one defect, and the four other entry-dir
  sites in the same file already named a file;
* found and fixed a worse sibling in `entry_typed_plan.rs` (a bare
  `eprintln!` + `return` that reported PASS with zero assertions);
* gave the six resolver preconditions one spelling.

So the instances are closed. What is open is that the next one is invisible.

## Why the general rule is not checkable

"A guard must test the artifact the test is about" needs to know which artifact
a test is about, which is not statically available. Do NOT try to gate that — a
gate that guesses is the reporting-OK-over-a-violation shape (issue 1396 wrote
two such rules and measured both wrong).

## The narrow rule that is checkable

A path under `build/<coord>/<entry>/` used as an `is_dir()` **condition** — as
opposed to an assertion about existence, which is a legitimate property under
test — in a test target. That is the exact shape that broke, it is decidable
from the source, and the fix is always the same: name the file.

Open questions for whoever takes it:

* Should the rule cover `.exists()` on a directory path too? 1411's sweep found
  those were all assertions or directory-walk filters, so the rule may be
  narrower than the grep.
* Is the right home a new gate, or a case inside `check-no-vacuous-tests`? That
  gate already owns "a test whose only effects are prints"; this is the
  neighbouring "a test whose guard cannot fail". One spelling is preferable to a
  second gate if the shapes fit (issue 0196 / the repo's one-helper rule).
* The vacuity guard matters: zero findings on a tree that has the fixed guard is
  correct, so the self-test must plant the pre-1411 shape rather than assert a
  nonzero count.

## Acceptance

A gate (or gate case) that FAILS against the pre-1411 `build_verb_pipeline.rs`
guard and PASSES against the current tree, with the planted shape in its
self-test, and a reach covering every test target rather than the one file that
had the defect.

## Resolution

Gate: **`check-test-generated-dir-guards`** (`scripts/check-test-generated-dir-guards.py`,
recipe `just check test-generated-dir-guards` in `just/check/gates.just`). Buildless,
self-testing on the normal path, 22 negative controls plus a corpus floor. Green on the
current tree (which is the correct answer) and red against the pre-1411 shape.

### The three open questions, answered by measurement

**1. Does `.exists()` on a directory path belong in the rule?**

YES, and it is free. Measured over the 336 tracked test targets: **zero** `exists()` /
`try_exists()` probes on a generated-output directory path are used as a CONDITION. All
nine such uses are assertions (`assert!(!bake.join("system_main.c").exists())`,
`test_metadata_ros.rs`'s five `install/` sites, `self_bringup.rs:188`, `abi_guard.rs:216`).
So including them flags nothing today, and excluding them would have left a one-token
bypass of an `is_dir()` rule. Re-run the measurement with
`python3 scripts/check-test-generated-dir-guards.py --sweep`.

The narrowing that `exists()` DID force is a different one. A path is in scope only when it
is EVIDENCED as a directory — the probe is `is_dir()` (nobody asks that of a binary), or the
binding is a `join()` / `read_dir()` / `create_dir_all()` receiver. Without that,
"extensionless last segment ⇒ directory" flags six BINARY paths under a build root
(`build/xrce-agent/MicroXRCEAgent`, `build/cyclonedds/bin/idlc`,
`packages/cli/target/release/nros`, `build/borrowed-e2e`, …) for doing the correct thing,
which is naming the file.

**2. New gate, or a case inside `check-no-vacuous-tests`?**

NEW GATE, and the reason is structural rather than a preference. That gate's unit of
analysis is a `#[test]`-attributed fn BODY — `test_bodies()` yields nothing else. Measured:
of the 79 generated-output directory path bindings in the corpus, **20 (a quarter) sit in
plain helper fns**, not in test bodies — `boot_and_connect`
(`freertos_run_plan_runtime.rs:169`, which carries exactly this kind of guard),
`run_cell`, `spawn_probe`, `spawn_cyclone_binary`, `stage_project`, `prebuilt_sitl_dir`,
`builder_includes`, … Hosting the rule there would put a quarter of the shape's living
space out of reach by construction, which is the issue-0196 shape this repo keeps
re-filing. The new gate reads FILES, so it is a sibling of `check-test-precondition-guards`
(a helper's SIGNATURE) rather than a case inside either — three gates, three units:
body / signature / the path a condition is about.

**3. The vacuity guard.**

The self-test plants the pre-1411 `build_verb_pipeline.rs` guard verbatim in form and
requires exactly one finding; the fixed spelling beside it requires zero. Nothing asserts a
nonzero count against the real tree. The floor the gate DOES assert is on its CORPUS
(≥ 100 test targets, at least one outside `packages/testing/`), because "OK (0 test files)"
is what a gate that has stopped covering anything prints —
`check-test-precondition-guards`' argument, reused.

### The rule, and the narrowing that flagging correct code would have cost

A path naming a directory under a generated-output root (`build`, `target`, `install`,
`out`) may not be probed with `is_dir()` / `exists()` / `try_exists()` in a condition
**whose false branch replaces the test** — the probe is negated, or the `if` carries an
`else`. Inside an `assert*!` it is fine: "this directory exists" is a legitimate property,
and 14 sites assert one.

The "false branch replaces the test" clause is the narrowing the issue asked for rather
than a shipped false positive. One live site has the positive-with-no-`else` shape:

    packages/cli/nros-cli-core/tests/orchestration_self_bringup_cargo_metadata.rs:195
        let out_root = root.join("build/cargo_self_bringup/nros");
        let preserved = out_root.join("metadata");
        if preserved.is_dir() {
            for entry in fs::read_dir(&preserved).unwrap() { assert!(...) }
        }

Its property is an ABSENCE — "no synthetic `Cargo.toml` was preserved" — which a missing
`metadata/` satisfies, so the guard is defensible and the block it wraps is the test rather
than a substitute for it. That is the decidable difference from 1411, whose guard chose NOT
to run the test (it returned). It is left alone, deliberately, and recorded here: the shape
is a VACUITY risk (if nothing ever produces `metadata/`, the assertion never runs), not the
grew-a-second-producer risk this gate is about. `check-no-vacuous-tests` documents its own
blind spot the same way.

Widening the output-root set beyond `build/` was also measured rather than assumed: adding
`target`, `install` and `out` contributes **zero** further conditions and six further
assertions, so it changes no verdict today while keeping the rule's claim ("this is
generated output") as wide as the rule's reason.

### Reach, proved with a planted violation

`git ls-files '*/tests/*.rs' 'tests/*.rs'` — **336 files**; git matches `*` across `/`, so
nested targets (`nros-cli-core/tests/common/mod.rs`) are included. Same corpus as
`check-no-vacuous-tests` and `check-test-precondition-guards`.

The pre-1411 guard planted in two files that never had the bug, one per workspace:

    packages/testing/nros-tests/tests/nav2_compat.rs:129
    packages/cli/rosidl-codegen/tests/parity_helpers.rs:465

Both caught, each naming the spelling, the literal and why (`negated`). Reverted after
measuring.

### Negative controls, both directions, at corpus scale

Planted into the same two real files and the gate stayed GREEN: an existence assertion, an
absence assertion (`assert!(!entry.exists())`), a `read_dir` walk filter whose loop variable
is a build-rooted path's child, the tolerant positive-no-`else` tolerance, and an
extensionless binary probed with `exists()`. Plus, in the self-test: 1411's own explanatory
COMMENT quoting the code it replaced (line and block form — offsets are preserved when
comments are blanked, because the first exploratory sweep reported that comment as a
finding), a source-tree fixture precondition, a `build/.../*.json` FILE path, and an
`if let` that must not be mis-parsed into a condition.

### What else the sweep found

Nothing needing a fix. Full classification over 336 targets: **1 condition** (the
positive-no-`else` site above, out of the rule with a stated reason), **14 assertions**, 0
other. The tree is clean of the 1411 shape, which is what 1411 already established; this
records it and keeps it that way.

### Verified

* `python3 scripts/check-test-generated-dir-guards.py --selftest` — 23/23
* the gate on the tree — OK (336 test targets)
* planted pre-1411 shape ×2 — FAILED, both named
* `just check gate-lists` — OK, 341 fast gates derived (the new one among them)
* `just check default-gates-run-somewhere` — OK, 363 gates all reached by a workflow event
* `just check gate-selftests` — OK, 237/341 run their own selftest
* `just check fast` — 3 of 341 red, all three pre-existing and environmental in a fresh
  worktree, each reproduced on this tree with the change REMOVED: `capability-conditionals`
  and `xrce-vendored-versions` need unprovisioned submodules, `codegen-version-refusal` case
  E shells `packages/cli/target/release/nros` and so read the PARENT checkout's binary
  (stamped 3 against this tree's 7).
