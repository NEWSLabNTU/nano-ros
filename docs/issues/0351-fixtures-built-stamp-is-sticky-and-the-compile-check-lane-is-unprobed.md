---
id: 351
title: "Build-stage fixture gates answer PRESENCE, not truth — the compile-check lane has neither the input-signature nor the toolchain predicate the workspace lane already uses"
status: resolved
type: limitation
severity: medium
area: build, testing
related: [issue-0030, issue-0196, issue-0309, issue-0350]
---

## Finding (2026-07-29, from the issue-0350 post-mortem)

Issue 0350 — `compile-check-fixtures.sh` failing wholesale on `main` — sat
unnoticed for three days. The obvious explanation ("nothing asserts the script's
exit status") is **wrong**, and worth correcting because it points at the wrong
fix. The propagation chain is sound at every link:

| link | behaviour |
| --- | --- |
| `compile-check-fixtures.sh` | `set -euo pipefail` → exits 1 on the first failing fixture |
| `build-test-fixtures` recipe | `set -e` → aborts; the stamp write never runs |
| `_require-fixtures` (test-all prereq) | no stamp → fails loudly with a build hint |
| `cmake_node_register_metadata.rs` | asserts these very fixtures' `nros-metadata.json` |

Every piece works. The red still hid, because each gate answers **"is the
artifact present?"** when the question is **"is the artifact TRUE?"** — i.e. was
it produced by a successful, current build. Presence is a weaker question, and
the gap between the two is where this lived.

## The three ways presence diverges from truth

### 1. The suite stamp is STICKY — present ≠ recent

`target/nextest/.fixtures-built` is written by `build-test-fixtures` (and
`build-all`) on success and **removed by nothing** — `git grep fixtures-built`
returns two writes, one read, zero deletions. It answers "did this build stage
EVER succeed?".

1. A run succeeds → stamp written.
2. A change breaks a fixture (here: the 305-W2 verb sweep, `bb0b08419`).
3. The next `build-test-fixtures` aborts under `set -e` → **the old stamp
   survives untouched**.
4. `_require-fixtures` sees a stamp and passes; `test-all` proceeds.

A first-ever run on a clean checkout is safe (no stamp → hard fail). It is
precisely the *regression* — the case that matters — that the gate cannot see.

Note the same script already gets this right one level down:
`compile-check-fixtures.sh:118` does `rm -f "$staged/.compile-ok"` **before** the
build and writes it after. The per-fixture stamps are cleared before the attempt
they certify; only the suite-level stamp is sticky.

### 2. A per-fixture stamp is presence-only — present ≠ current

`.compile-ok` records *that* a build succeeded, not *what it was built from*. A
source edit after the stamp leaves it valid-looking forever. Nothing in the
compile-check lane compares inputs to outputs.

### 3. Absent is indistinguishable from broken

`require_cmake_fixture` → `require_prebuilt_binary_fresh` is tier-aware: a
missing fixture is a hard failure in the full tier but a **`[SKIPPED]`** under
`NROS_FIXTURES_OPTIONAL=1`. Correct for "the toolchain isn't installed", wrong
for "the fixture is broken" — and the resolver cannot tell them apart, because
both present as a missing file. So the lane people run locally reports
green-with-skips for something that cannot be built at all.

## The lane next door already solved all three

This does not need inventing. `workspace-fixtures-build.sh` +
`check-fixtures-stale.sh` answer truth, with two mechanisms:

- **Input signature.** `workspace-fixture-signature.sh` hashes the manifest
  record plus the source tree; the builder writes
  `.nros-workspace-fixture.<id>.inputsig` after a successful build, and
  `workspace-fixture-stale.sh` recomputes and compares. The question is "does
  this stamp match current inputs?", not "does a stamp exist?" — that is (1) and
  (2) closed together, since a failed build never writes a signature and a source
  edit invalidates it.
- **An explicit toolchain predicate.** `workspace_toolchain_present` →
  `nros_toolchain_present` (`scripts/test/toolchain-gate.sh`, a SHARED predicate
  since phase-300 W4) *asks whether the cross toolchain exists* and drops the
  fixture from the required set with an informational message (issue 0030). It
  never infers absence from a missing artifact — that is (3) closed.

The compile-check lane has neither. It is the same shape as this week's other
findings: a rule implemented properly in one place and not in its sibling.

## And the lane is drift from a documented rule

`AGENTS.md:79` already prescribes where compile-intent checks belong:

> If a test's *intent* is to verify that something compiles (a macro form, a
> codegen output, an API shape), make it a **fixture in the build step** — add a
> row to `examples/fixtures.toml` … and have the test assert the fixture exists /
> inspect the built artifact.

Of the 26 entries in `compile-check-fixtures.sh`, **10 are exactly that** —
compile-time feature checks with no runtime artifact (`n9_form1/2` macro forms,
`one_dep_component_pkg` dep resolution, `o4_pkg_index`, the three
`CXX_SYNTAX_FIXTURES` API shapes, the two embassy `CARGO_CHECK_EXAMPLES`). They
live in a hardcoded shell array instead of `fixtures.toml`, which is why
`check-fixtures-stale.sh` — which enumerates the manifest — cannot see them.
That is not a design choice; it is drift from the rule.

The other **16 produce artifacts tests read or execute** (`BUILD_FIXTURES`
binaries are run; several `CMAKE_FIXTURES` yield `robot_entry` / `consumer` /
`smoke` binaries as well as `nros-metadata.json`; `CROSS_BUILD_FIXTURES` ELFs are
booted in QEMU). Those are ordinary build-stage fixtures that happen to be built
by a different recipe — so the manifest axis they need is the **builder**
(`cargo-check` / `cargo-build` / `cmake-configure` / `cross-build`) and the
output path, not a separate "compile fixture" species.

## Ways to fix

**A. Clear the suite stamp before the attempt** (defect 1; ~1 line)

`rm -f target/nextest/.fixtures-built` at the TOP of `build-test-fixtures` and
`build-all`. Makes the suite level consistent with the per-fixture discipline
already at `compile-check-fixtures.sh:118`. Fail-closed; an interrupted build now
also demands a re-run, which is correct (an interrupted build stage IS
unverified) and `NROS_SKIP_FIXTURE_CHECK=1` remains the deliberate bypass.

**B. Move the lane into `fixtures.toml`** (defect 2; compliance with AGENTS.md:79)

Add a `builder` field to the existing `[[fixture]]` table — defaulting to today's
behaviour so the 251 existing rows are untouched — plus the output path each row
produces. Retires six ad-hoc colon-delimited array formats, and gives
`check-fixtures-stale.sh` a uniform `(sources → output)` pair per row, which is
what signature comparison needs.

**C. Adopt the signature + toolchain-predicate pair** (defects 2 and 3; the real fix)

Reuse the workspace lane's mechanisms rather than new ones: an `.inputsig`
equivalent so a stamp is checkable against its inputs, and `nros_toolchain_present`
at resolve time so "toolchain absent" is *decided* rather than inferred. With the
predicate in place, a missing artifact whose toolchain IS present becomes a hard
error in every tier — which is what would have turned #350 red in the lane people
actually run.

C depends on B for the per-row data (which toolchain, which output), so they land
in that order.

**D. A `just check` step asserting the script's exit code** — REJECTED

It would re-run the build stage inside the check tier (minutes of cmake/cargo),
and `check-fast` is explicitly buildless. The exit code already propagates
correctly; the defect is that the *stamps* answer presence, not that an assertion
is missing.

**Recommended: A now** (one line, independent, removes the sticky class), then
**B → C** as one pass, since C is where presence finally becomes truth.

## Resolved by phase-319 (2026-07-30)

All three landed; the acceptance test is the scenario #350 failed — breaking a
compile-check fixture now turns the LIGHT tier red, naming the fixture and
builder, instead of reporting a skip.

- **A** — the suite stamp is cleared before the attempt it certifies, in both
  `build-test-fixtures` and `build-all`.
- **B** — all 26 rows moved to a `[[compile_check_fixture]]` table in
  `examples/fixtures.toml` (AGENTS.md:79 compliance), retiring six hardcoded
  colon-delimited arrays. Their own table rather than `[[fixture]]` fields:
  `list`'s record format is per-language and consumed positionally by
  `fixtures-build.sh`, so overloading it would have changed that contract for 251
  rows.
- **C** — per-row `.inputsig` on the `workspace-fixture-signature.sh` model
  (written only on success, recomputed and compared), plus a `.build-failed`
  marker the resolver treats as a hard error in every tier. The marker turned out
  to beat the planned toolchain predicate: the build stage already knows whether
  it could run a builder, so recording the outcome it OBSERVED is better than
  re-deriving it test-side.

Two pre-existing defects surfaced on the way, both instances of this issue's own
thesis — a marker certifying something it no longer affects:

- `orch_tiers_single` stripped `[tiers.*]` from `system.toml` while the entry
  reads the MODEL (phase-296 made it authoritative), so the overlay had stopped
  doing anything and its test was RED on `main`.
- `compile-check-fixtures.sh` swallowed builder failures wherever a builder was
  called in a condition context — bash suppresses errexit there, so a failing
  `cmake -S` fell through and the function returned its trailing `echo`'s status.

## The general shape

A cached success marker that outlives the thing it certifies is the same pattern
as issue 0309's count-based proofs and issue 0268's museum binaries: the signal is
real but answers a weaker question than the one being asked. Worth auditing every
stamp in the tree against two questions — *is it cleared before the attempt it
certifies?* and *can it distinguish "not built" from "failed to build"?*
