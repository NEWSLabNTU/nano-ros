# nano-ros gates: gates
#
# The gates about gates -- lane membership, self-tests, skip
# protocol, and whether a gate can fail at all.
#
# One of the topic files `just/check.just` imports. The names stay FLAT -- a
# gate is still `just check <name>` -- because `import` merges definitions
# rather than namespacing them, and ~350 references across docs, scripts and
# workflows spell them that way.
#
# Adding a gate here does not touch any other file, which is the point: for
# most of this repo's life every new gate appended to ONE file, and
# `just/check.just` became the worst conflict surface in the tree at 48 of
# 300 commits.

# issue 1040 — `check-api-parity` sat in `just check`'s DEFAULT list and ran in
# NO workflow at all: not pull_request, not merge_group, not nightly, not
# dispatch. Its reds were therefore invisible between local full-tier runs, and
# three landed on main in a single day alongside four others.
#
# The bar is a DAILY signal, not a blocking one — `check-build` is
# schedule/dispatch-only for good reason and must stay that way.
[private]
default-gates-run-somewhere:
    @python3 scripts/check-default-gates-run-somewhere.py

# gate.yml's warm job must cache the SAME key+path as the job it warms.
# GitHub scopes a cache to the ref that created it, so only `main` writes a
# scope pull_request/merge_group can read — a drifted key means the warm job
# succeeds, uploads, and warms nothing, with no error anywhere.
[group("checks")]
gate-cache-keys-agree:
    @python3 scripts/check-gate-cache-keys-agree.py

# The gate REGISTRIES hold one name per line, sorted — phase-400 follow-up to
# issues 0883/0884.
#
# `fast-serial` and `build` are the two lists every gate-adding PR must append
# to, and they used to pack up to fifteen names per line with new gates going
# on the tail. 22 of the last 31 commits touching this file edited such a line,
# so unrelated gates collided and the merge queue ejected them. 0884's union
# merge is not available for an authored file; one name per line, sorted, is
# the fix that needs no merge driver.
[group("hygiene")]
gate-lists:
    @python3 scripts/check/check-gate-lists.py

[private]
gate-selftests:
    @python3 scripts/check-gate-selftests.py

# Issue 1241's class, for EVERY ratchet baseline. A baseline's header is the
# part no gate reads, so a `sort` that scrambles it leaves every lane green —
# it did, twice (prose-issue-ref, then gate-selftest). Sorted comment blocks
# fail everywhere; a baseline whose writer declares `BASELINE_HEADER` must
# carry exactly that header. Buildless, milliseconds, self-testing.
[group("hygiene")]
baseline-shape:
    @python3 scripts/check-baseline-shape.py

# A gate must exercise its own failure path EVERY time it runs, not once behind
# a flag on the day it was written. A baseline ratchet: the debt may only
# shrink, and a script that gains a selftest must leave the baseline.
# Issue 0993 — a gate no pull_request or merge_group job runs is a gate that
# rots. `just check build` is invoked once, in `gate.yml`, behind a
# `schedule`/`workflow_dispatch` guard, so nothing in it gates a merge and a red
# there is invisible until somebody reads a nightly (issue 0981: a red sat on
# `main` for a day behind a green required `CI`, and two changes reached a green
# pull request while failing `just ci gate` locally).
#
# The ungated set is written down in `.config/ungated-gates.txt` and may only
# SHRINK — same ratchet as the selftest baseline. Membership is TESTED, not
# argued: a pristine worktree, then `just check <gate>`. Twenty gates passed
# that with no build artifacts and moved to this line; the nineteen that remain
# each fail it for a stated reason.
[private]
gate-visibility:
    @python3 scripts/check-gate-visibility.py

# Forbid `find` scans for git-tracked files (7m36s -> 0.8s, measured).
# Ratchet: no NEW `grep -q` conditional that reads a tool error as a
# non-match (issue 0726). Existing sites are baselined per file.
[private]
grep-q-error-conflation:
    @python3 scripts/check-grep-q-error-conflation.py

# Every CI job gated on a `vars.*` interlock must have a job that reports
# whether it ran. A skipped job does not colour its run, so post-submit reported
# SUCCESS on runs where the tier-2 matrix — every fixture build and every E2E
# cell in that lane — had not executed at all.
[private]
interlock-visibility:
    @python3 scripts/check-interlock-visibility.py

# A tier's affordability claim must be TRUE. `ci-l1` says "NO FIXTURES"; it
# reached a fixture-resolving test for as long as anyone had written the claim
# down, and nothing noticed until that lane became a REQUIRED status check and
# CI went permanently red.
[private]
lane-contracts:
    @python3 scripts/check-lane-contracts.py

# A fixture build must ask whose checkout its provisioned roots are BEFORE it
# compiles against them. The ownership guard (issue 1253) was correct and
# arrived one step late: its three callers were all downstream of the build,
# because CI runs `just build tier2` and `just ci matrix` as separate steps.
# Issue 1395 widened the reach through one shared recipe; this re-derives it, so
# the reach cannot narrow again silently — which is the only failure mode a gate
# of this shape has.
[private]
provisioned-root-guard-reach:
    @python3 scripts/check-provisioned-root-guard-reach.py

# The lane-skip protocol is only a protocol where the CALLER reads it.
# `nros_lane_skip` exits 78 and one driver interprets that; a producer reached
# from anywhere else emits a skip nobody can hear, and 78 lands as a plain
# failure. `provision-zenohd` did exactly that from `native.just`'s `setup` the
# day the self-hosted runner stopped being root: an honest skip failed L3 and
# stalled the merge queue for every PR in the repo.
[private]
lane-skip-interpreters:
    @python3 scripts/check-lane-skip-interpreters.py

# Issue 0584 — an out-of-lane skip must SAY `lane`; a plain `skip!` is read as
# `capability`, so a fixture the lane deliberately did not build gets counted as
# a missing capability and the sweep summary lies about which gap a run has.
[private]
lane-skip-class:
    @python3 scripts/check-lane-skip-class.py

# issue 0650 — a fixture lane that cannot run must report SKIPPED (rc 78), never
# build nothing and print "<platform> test fixtures built." with exit 0. That
# false OK is how six diverged riscv64 examples reached main: every host without
# the toolchain reported the lane green. Self-tests its own classifier.
[private]
lane-skip-protocol:
    @python3 scripts/check-lane-skip-protocol.py

# phase-411 W2 — 0599/0650 gave a lane the SKIPPED verdict; this says WHO MAY
# HAVE IT. A platform reached by a preset or the broad default may skip and is
# reported; a platform the operator NAMED (`just zephyr build-fixtures`) must
# work, because naming it is the whole specification. Runs the protocol both
# directions on every invocation — a named skip regressing to a pass is
# invisible to a grep, so the gate makes the call instead of reading it.
[private]
named-lane-fails:
    @python3 scripts/check-named-lane-fails.py

# A test whose body only PRINTS cannot fail, so it reports PASS on exactly the
# host it was supposed to warn about. The 2026-08-21 cleanup removed 17 of these
# (10 files, 2 of them literal cross-file duplicates); this keeps the shape from
# growing back. Self-testing: `--self-test` covers the delegating-helper and
# multi-line-print cases that a naive "has no assert!" rule gets wrong.
[group("ci")]
no-vacuous-tests:
    @python3 scripts/check-no-vacuous-tests.py --self-test
    @python3 scripts/check-no-vacuous-tests.py

# Issue 1135 — the sibling of `no-vacuous-tests`, one level up. That gate reads a
# test BODY ("it only prints"); this one reads a test-local GUARD's SIGNATURE. A
# `require_*`/`ensure_*`/`need_*`/`maybe_*` helper that hands back a `bool` (or an
# `Option<()>`, a bool wearing a hat) has given the verdict to callers nobody
# checks, and four callers of one such helper wrote `if !guard() { return; }` — a
# bare `return` from a `#[test]` is a PASS, so four `ros2 param *` tests reported
# green on a host where our node never joined the graph, with the only evidence in
# an `eprintln!` that `--failure-output never` discards.
#
# It keys on the signature because the CALL SITE is not statically separable from
# its legitimate twin: `if !probe() { return; }` and `if check().is_ok() { return; }`
# are the same shape and only the meaning of the condition differs. A blanket
# "no bare `return;` in a test body" rule is decidable but flags 40 sites across
# the tree, mostly the legitimate form — an authored 40-entry allowlist, which is
# the map shape CLAUDE.md already records drifting. Self-tests its classifier on
# every run, not only behind `--self-test`.
[private]
test-precondition-guards:
    @python3 scripts/check-test-precondition-guards.py

# Issue 1430 — the third member of that family, keyed on the PATH a guard probes.
# `no-vacuous-tests` reads a test BODY, `test-precondition-guards` reads a
# helper's SIGNATURE; this reads what the condition is ABOUT.
#
# Issue 1411: `if !entry.is_dir()` stood in for "the entry package was
# generated", and was a fair stand-in until RFC-0098 D1 (phase-445 W4/W5) gave
# `build/<coord>/<entry>/` a SECOND producer — `cmd::build` writes the image's
# `nros-cargo.toml` there unconditionally, on the model-resolution FAILURE path
# too. From that commit the directory existed either way, the guard could not be
# false, its documented fallback became unreachable, and the `unwrap()` five
# lines down panicked on a file that was never generated. A directory is a
# CONTAINER: its existence is a claim about everyone who creates containers
# there, and that set grows.
#
# Narrow on purpose, and each narrowing MEASURED over the 336 test targets
# rather than chosen: only a condition whose false branch REPLACES the test (the
# probe is negated, or the `if` has an `else`) — a positive `if d.is_dir() {
# asserts }` is defensible and the one live site of it would have been flagged
# for nothing; `exists()`/`try_exists()` are in (zero of them are conditions on
# such a path anywhere, so including them flags nothing and closes a one-token
# bypass); and the path must be EVIDENCED as a directory by an `is_dir()` probe
# or a `join`/`read_dir` receiver, which keeps six extensionless BINARY paths
# under a build root out. NOT a case inside `no-vacuous-tests`: that gate's unit
# is a `#[test]` body, and 20 of the 79 generated-output path bindings in this
# corpus live in plain helper fns. Buildless, self-testing on the normal path,
# every test target. `--sweep` re-runs the measurement.
[private]
test-generated-dir-guards:
    @python3 scripts/check-test-generated-dir-guards.py

# Issue 1491 — `nros_scoped_target_dir <suffix>` puts a gate's cargo scratch at
# `target-<suffix>/`, a SIBLING of `target/` that the root `.gitignore`'s
# `/target/` line does not cover, so each one needs its own entry. That list is
# AUTHORED and it drifted: `target-excluded-tests` reached 329 MB, untracked, at
# the repo root — left by a gate on the merge-gating PR lane, which is the
# `git add -A` hazard CLAUDE.md names. The required entries are DERIVED from the
# call sites here rather than trusted to the list.
[private]
scoped-target-dirs-ignored:
    @python3 scripts/check-scoped-target-dirs-ignored.py

# Issue 0975 — a REQUIRED check that cannot report on a pull request freezes the
# repo, and it freezes it SILENTLY: no red check, no message, just PRs that never
# merge. Requiring `L3` (a merge_group-only job) did exactly that to seven PRs.
# Also rejects a required job gated on `vars.`/`secrets.`, which can be switched
# off in Settings with nothing in any diff to show it.
[private]
required-contexts-reportable:
    @python3 scripts/check-required-contexts-reportable.py

# issue 0658 — `[SKIPPED:<class>]` does not contain `[SKIPPED]`, so a hand-rolled
# literal match reclassifies every CLASSED skip as a FAILURE. Five matrix
# aggregators had written that literal independently, turning five tier-2 lane
# skips into tier-2 reds. One helper per language now owns the match
# (`nros_tests::skip_marker`, `scripts/test/skip_marker.py`). Self-tests its own
# matcher against the actual pre-fix line on every run.
[private]
skip-marker-matching:
    @python3 scripts/check-skip-marker-matching.py

# issue 0702 — a test that cannot fail is worse than no test. Rejects an `Err`
# arm that prints a diagnosis and decides nothing, which is the shape eight
# separate tests had been wearing for months (see the script's docstring for the
# roll-call). Buildless: it reads tracked test sources, so it belongs on the
# fast line beside the other source gates.
[private]
tests-can-fail:
    @python3 scripts/check-tests-can-fail.py

# Issue 0670 — a timed-out wait's evidence must not be thrown away.
#
# `wait_for_output*` returns `Err` carrying what the process PRINTED;
# `.unwrap_or_default()` replaces it with `""`, so the assertion reports `got:`
# with nothing after it. `contract_monitor_parity` failed exactly that way, and
# the empty `got:` is why its real cause (issue 0671 — an unguarded
# `epoch_us_fn` clobber leaving the age monitor with no clock) took a separate
# investigation instead of being readable off the failure.
#
# A gate rather than a sweep, because the obvious mechanical fix is WRONG:
# `unwrap_or_else(|e| e.to_string())` folds in text that NAMES the pattern being
# waited for, so `seen.contains(<pattern>)` matches the complaint about the
# missing pattern and the test passes exactly when it should fail. Each site
# needs its assertion read. `collect_until_count` returns the two on separate
# channels for that reason.
#
# 87 sites are baselined as a SHRINKING backlog; what this buys today is that an
# eighty-eighth cannot arrive silently.
wait-evidence-discarded:
    @python3 scripts/check-wait-evidence-discarded.py
