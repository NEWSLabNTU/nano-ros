# Phase 472 — gate reach sweep

**Status (2026-09-28). AUDIT LANDED; W1–W9 open.** An audit of every tracked
`scripts/check-*` gate against one question, the codebase-audit checklist's **I6
second-order** rule: *a gate must be able to fail on the case it names.*

**155 of 320 gates failed it** — each by a demonstrated mutation, not by
argument. That is roughly one hole per two gates. The holes are not scattered:
they fall into nine classes, and every class has a fix that is ONE shared helper
rather than 155 local edits. This phase is that sweep.

**Prior:** checklist I6 (a 2026-07-28 audit found four gates narrower than their
rule — #321, #328, #332, #334); issue 0196 (a gate's reach must equal the rule it
enforces); CLAUDE.md "Fix the CLASS, not the reported site". Issues 1362, 1379,
1388, 1401 and 1431 were five instances of this class found one at a time in the
fortnight before the audit; this phase exists because meeting it one gate at a
time was not keeping up.

**Source:** six parallel auditors on `origin/main` at `5d9fc529ec`, one bucket of
51–55 gates each, each in its own worktree so their mutations could not corrupt
one another. Fourteen findings were independently re-verified by the
coordinator; all fourteen held.

---

## Method, and the standard that made it worth trusting

For each gate: read the **rule** it states (docstring, header comment, and the
failure message it prints), then read the code that decides its **population**
(what it actually examines), and compare.

A finding is **CONFIRMED** only if the auditor constructed the case the gate
claims to catch, ran the gate, and it **passed (rc=0) when it should have
failed** — then restored the tree. Almost every confirmed finding also carries a
**positive control**: the same defect placed where the gate DOES read makes it
fail, which proves the gate was invoked correctly and the mutation was not
vacuous.

Anything believed but not demonstrated is recorded as SUSPECTED and is not
counted. Vacuous controls were the main hazard: a mutation whose anchor silently
did not match prints the same "OK" as a real hole, and this happened to the
coordinator twice and to two earlier agents during the fortnight that prompted
the audit. Every mutation was checked with `git diff` before the gate ran.

## Results

| bucket | gates | confirmed | suspected | could not run |
| --- | ---: | ---: | ---: | ---: |
| 00 `action-client-arena-budget` .. `cmake-verb-reachable` | 55 | 18 | 4 | 7 |
| 01 `codegen-tool-reconfigure` .. `feature-set-ssot` | 52 | 30 | 7 | 2 |
| 02 `ffi-struct-mirrors` .. `literal-domain-id` | 54 | 34 | 11 | 5 |
| 03 `manifests-parse` .. `qos-mask-derivation` | 53 | 28 | 5 | 7 |
| 04 `qos-profile-ssot` .. `template-copy-out` | 55 | 23 | 3 | 7 |
| 05 `test-domain-assignment` .. `zephyr-workspace-resolvers` | 51 | 18 | 6 | 7 |
| **total** | **320** | **155** | **36** | **35** |

By severity: **2 P1, 94 P2, 59 P3**, plus one dead gate
(`check-board-manifest-drift.sh`, which can only ever print "nothing to audit").

Of the 80 gates with no selftest, the ones in each bucket with a confirmed hole
are named per class below; in nearly every case a one-line negative control would
have caught it.

---

## The nine classes

Each work item names its member gates and ONE fix. Fixing members one at a time
is the failure mode this phase exists to end.

### W1 — composite actions are unread

Gates that read `.github/workflows/*.yml` and never `.github/actions/*/action.yml`.
Composite actions are real CI steps: `nightly.yml` calls `setup-nros-cli` "the SOLE"
CLI acquisition path for its jobs.

Members: `check-ci-cli-from-source` (P2), `check-workflow-repo-env` (P2),
`check-workflow-indexed-apt` (P2), `check-ci-no-fixture-tolerance`,
`check-ci-no-verb-fallback`, `check-workflow-setup-spelling`,
`check-git-dir-layout-assumptions`, `check-grep-q-error-conflation`,
`check-pipefail-sigpipe-assertions`.

**Live:** `.github/actions/setup-qemu-patched` runs `just` without sourcing
`activate.sh` and apt-installs three packages the index already declares (issue
1548). `check-workflow-just-provisioning` already reads composite actions — it is
the model.

**Fix:** make the shared loader `scripts/lib/workflow_commands.load_workflows`
include composite actions, and move every member onto it.

### W2 — just-file populations miss `just/check/` and the root `justfile`

Gates that glob `just/*.just` flat, or omit the extensionless root `justfile`.
`just/check/*.just` holds the 13 files of `mod check`.

Members: `check-just-recipe-refs` (P2 — also passes any `just <module> <bogus>`),
`check-just-recipe-paths` (P2), `check-named-lane-fails` (P2),
`check-skippable-tests-tolerant` (P2, **live**: `just/zephyr-setup.just:401` runs a
bare `cargo nextest` over 5 `skip!` calls), `check-nros-c-feature-agreement`,
`check-zephyr-module-binding` (misses the root `justfile`, workflows and Python —
and `colcon_nano_ros/task/nros/build.py` runs a configuring `west build` without the
module flag), `check-sysdep-remedies`, `check-sdk-guard-can-fire`,
`check-prose-issue-refs`, plus the siblings `check-lane-skip-protocol`,
`check-fixture-artifact-dir-inputs`, `check-build-profile-literals`,
`check-example-leaf-target-dirs` that share the same glob.

**Fix:** one population function — `scripts/lib` already has `check_just_sources()`
— and every member uses it.

### W3 — comments counted as evidence

Gates that match raw text, so a COMMENT satisfies a requirement.

Members: `check-entry-rmw-vocabulary` (P2, **live mask**: a doc comment in
`nros-rmw-xrce-cffi/src/lib.rs:27` counts as the xrce registration, so deleting
both real calls passes), `check-cyclone-backend-sources` (P2 — a commented-out
source name is issue 0984 exactly), `check-rmw-doc-slot-names` (P2 — its
motivating `try_recv_raw` resolves through comments elsewhere),
`check-required-features-reachable` (P2 — `--all-features` in a comment makes
every feature reachable), `check-platform-provider-features` (P2 — the issue-0617
row), `check-book-identifiers` (P2 — "occurs" is not "defined"),
`check-named-lane-fails`, `check-declared-fact-carriers`,
`check-workflow-repo-env` (`activate.sh` in a comment exempts).

**Fix:** one comment-stripper per language family in `scripts/lib`, applied before
every match that stands for "the code does X".

### W4 — an empty population reads as OK

Gates that print success over zero examined items. This is the most dangerous
class, because the gate is not merely narrow — it is off.

Members:
- **`check-cargo-custom-command-depfile` (P1).** Issue 1304 respelled the cargo
  program as `"${_ffi_cargo}"`; the lookbehind rejects `_`, so the gate examines
  zero commands, prints no count, and says "every cargo custom command has a
  DEPFILE". Deleting `DEPFILE` at all three sites it names still passes. Verified
  by the coordinator.
- `check-profile-board-mirror` (P2) — points at the retired `packages/codegen` and
  skips with rc=0 on every run, blaming a submodule that no longer exists. Verified.
- `check-nested-workspace-excludes` (P2) — the nested roots it reads are generated
  and untracked now, so it examines zero.
- `check-issue-index` (P2, **live**) — its duplicate-resolved-row arm reads the
  generated `open.md`, which has no such rows, so it can never fire. Verified.
- `check-no-std-entry-emission`, `check-host-triple-literals`,
  `check-interop-verdicts` (a missing tracked ledger reads as empty, erasing 25
  verdicts), `check-workflow-runner-isolation` and `check-required-contexts-reportable`
  (a missing PyYAML exits 0), `check-board-manifest-drift` (dead).

**Fix:** every gate prints the count it examined and FAILS on zero unless the
empty population is declared and stated. `check-vendor-fetch-pinned`'s "NOTHING TO
CHECK … not a pass" is the model; `nros_check_skip` is the ledger.

### W5 — scan roots that stop short of the tree

Gates rooted at `cmake/` or `packages/<some>/` that the rule's subject has since
spread beyond — usually into `zephyr/`, `packages/**/cmake`, `examples/`,
`integrations/`, or the root `justfile`.

Members (selection): `check-board-facts-delivery` (P2, possibly a **live bug** —
the NuttX lane delivers no board facts, issue 1541), `check-cc-build-policy` (P2,
**live**, issue 1542), `check-weak-symbols` (P2, **live**, issue 1543),
`check-feature-set-ssot` (P2, **live**, issue 1547), `check-emitter-just-spelling`
(P2, **live**), `check-cpp-no-std-stdio` (P2 — 212 of 285 C/C++ files outside,
including the 62 public nros-cpp headers), `check-zenohd-router-skips` (P2,
**live**), `check-no-vacuous-tests` (P2, **live**: 11 print-only tests in `src/`),
`check-doc-recipe-refs` (P2, **live**: 24 dead `just` references),
`check-markdown-links` (P2, **live**: RFC-0034's 8 links to a moved issue),
`check-no-unbounded-condvar-wait`, `check-build-profile-literals`,
`check-entry-locator-ssot`, `check-third-party-is-submodules`,
`check-nuttx-shared-tree-headers`, `check-host-platform-vocabulary` (depth 1 only),
`check-rust-targets-covered` (its own config file says its reach is narrow).

**Fix:** derive populations from `git ls-files` by FILE KIND, never from a
directory list. A gate that legitimately scopes narrower states the scope and why.

### W6 — first match, or any match, where the rule is per item

Gates that read one occurrence per file when the rule is about every occurrence.

Members:
- `check-config-header-producers` + `check-config-fallback-macros` (P2, **live
  precondition**) — `nros_config_generated_nuttx.h` defines `NROS_CODEGEN_VERSION`
  at lines 28 AND 113. The gates read the first; GCC uses the last. A version bump
  that edits line 28 alone passes both gates while every NuttX image compiles
  against line 113 — issue 1115's shape, armed. Verified. Issue 1540.
- `check-board-cargo-config-shape` (first blob only; the NuttX riscv blob is
  unchecked), `check-executor-stack-floor`, `check-dds-isolation-symmetry`,
  `check-tier-spin-gap` (a file-level `extern` prototype satisfies the rule — removing
  the gap from BOTH Zephyr tier loops passes), `check-generated-schema-coverage`,
  `check-image-paths-apply-policy`, `check-literal-domain-id` (truncates at the
  first `mod tests` substring, so `mod tests;` at `executor/mod.rs:127` hides 57
  lines of shipped code).

**Fix:** per-item matching, and refuse a duplicate definition outright where the
language makes the last one win.

### W7 — authored lists where the population should be harvested

Gates whose population is a hand-maintained list that has gone stale.

Members: `check-ffi-struct-mirrors` (2 of 3 mirrored structs; the unchecked one
carries `callback_group`, the field that drifted twice), `check-fixture-binary-names`,
`check-lane-contracts` (5 `require_*` names; the ~320 `build_*` resolvers emit the
exact failure it exists to prevent), `check-decoupling` (lists `posix-c`, which does
not exist; misses four real platform crates), `check-atomic-sync-writes`,
`check-msg-dep-is-path` (**live**: `px4_msgs` and `custom_msgs`),
`check-rmw-ret-sign` (43 of 66 status slots; and its headline rule has no failing
path at all), `check-codegen-tool-reconfigure`, `check-cxx-compat-shim-coverage`,
`check-sdk-store-not-enumerated` (**live**, issue 1546).

**Fix:** harvest from the source of truth; turn the authored list into an
EXEMPTION list whose entries carry reasons.

### W8 — exemptions wider than their rationale

Members: `check-test-precondition-guards` (**P1, live** — exempts a helper that
returns a real value "because the caller will `skip!`", which nothing checks; issue
1539), `check-one-producer-per-tool` (forwarding ANY tool excuses producing any
other), `check-posix-platform-purity` (any `__linux__` within 12 lines counts as a
guard, even after `#endif`), `check-lane-scope-consumers` (exempts `native_*`
files, which the host lane runs), `check-tier-has-ci-owner` (a step `name:` line
counts as an owner; `just ci matrix build` counts as owning tier 2),
`check-fixture-artifact-dir-inputs`, `check-rmw-agnostic`, `check-ros2-daemon-queries`,
`check-retired-submodule-refs`.

**Fix:** key each exemption on the exact thing its rationale names, and give every
exemption a selftest row showing it does NOT cover the neighbouring case.

### W9 — selftest discipline, including the meta-gate

- **`check-gate-selftests` misclassifies (P2, live).** It does not recognise
  `if args.self_test:` as a flag guard, so 17 gates that run their selftest ONLY
  behind a flag are counted compliant — among them `check-no-vacuous-tests`, which
  the meta-gate reports as running its selftest on the normal path. Verified. It
  also mis-baselines `check-image-paths-apply-policy`, which runs its controls
  inline.
- **`check-sdk-store-not-enumerated`'s docstring says "the self-test below is the
  point, not a formality" — and there is no self-test.** Verified.
- **Ratchets never forced down** — `check-wait-evidence-discarded` baselines
  counts no file is held to (a file can regrow to its old count); suspected in
  `check-unsafe-census` and `check-kconfig-overridden-values`.
- **Selftests that exercise a copy** — `check-fixture-require` and
  `check-self-pkg-package-xml` test a different matcher than the one the gate runs.

**Fix:** the meta-gate recognises flag guards; every gate with a confirmed hole
above gains the negative control that would have caught it; a ratchet fails when
a count rises above ANY recorded value, not only above its own row.

---

## Live defects filed separately

The classes above are gates that fail to guard. These are things **broken in the
tree today**, which the audit found because the gates meant to catch them could
not:

| issue | defect |
| --- | --- |
| 1539 | **P1.** Five zenoh integration tests `let Some(_) = router() else { return };` — PASS having run nothing on a host without zenohd |
| 1540 | `NROS_CODEGEN_VERSION` defined twice in the NuttX config header; gates read the first, GCC the last |
| 1541 | the NuttX cargo lane delivers no board facts — needs a ruling on whether it must |
| 1542 | an ungoverned `cc::Build` in `examples/mps2-an385-baremetal/c/talker/build.rs:94`, on issue 0478's compiler |
| 1543 | three weak-symbol sites no audit list covers |
| 1544 | test-harness violations the gates missed: 18 fixture `match` bypasses, 11 print-only `src/` tests, a zombie-blind `ps` scan, 6 `ZenohRouter::start*().expect()` |
| 1545 | dangling references outside every gate's scope: 24 dead `just` recipes, RFC-0034's 8 broken links, 6 dangling issue ids, ledger citations, and `book/src/reference/c-api.md:72` (red on `main` in a non-gating lane) |
| 1546 | the SDK store enumerated newest-version-first in two places — pending a ruling on whether issue 0500's ordering survives phase-365 |
| 1547 | `ros-humble` hardcoded 11 times in `zephyr/CMakeLists.txt` |
| 1548 | the `setup-qemu-patched` composite action bypasses the repo-env and indexed-apt rules |

## Acceptance

Per work item: the named fix lands as ONE shared helper, every member gate moves
onto it, each member gains the negative control that would have caught its
recorded hole, and the mutation recorded here is re-run and now FAILS.

For the phase: re-running this audit's method over the same gates finds no
confirmed hole in any class W1–W9. The audit is repeatable by design — six
buckets, one question, mutation-confirmed — and should be re-run whenever a class
fix lands, because a class fix that reached only its reported members is how this
phase came to be necessary.
