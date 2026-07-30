---
id: 351
title: "The `.fixtures-built` stamp is never invalidated, so a build stage that STOPS working keeps its green marker — and the compile-check lane is outside the staleness probe entirely"
status: open
type: limitation
severity: medium
area: build, testing
related: [issue-0196, issue-0309, issue-0350]
---

## Finding (2026-07-29, from the issue-0350 post-mortem)

Issue 0350 — `compile-check-fixtures.sh` failing wholesale on `main` — sat
unnoticed for three days. The obvious explanation ("nothing asserts the script's
exit status") is **wrong**, and worth correcting because it points at the wrong
fix. The propagation chain is actually sound at every link:

| link | behaviour |
| --- | --- |
| `compile-check-fixtures.sh` | `set -euo pipefail` → exits 1 on the first failing fixture |
| `build-test-fixtures` recipe | `set -e` → aborts; the stamp write never runs |
| `_require-fixtures` (test-all prereq) | no stamp → fails loudly with a build hint |
| `cmake_node_register_metadata.rs` | asserts these very fixtures' `nros-metadata.json` |

Every piece works. The red still hid, for two reasons that are each worth
fixing on their own.

## Defect 1 — the stamp is STICKY

`target/nextest/.fixtures-built` is written by `build-test-fixtures` (and
`build-all`) on success, and **removed by nothing** — `git grep fixtures-built`
returns two writes, one read, zero deletions.

So the stamp answers "did this build stage EVER succeed?", not "did it succeed?".
The regression case is the whole problem:

1. A run succeeds → stamp written.
2. A change breaks a fixture (here: the 305-W2 verb sweep, `bb0b08419`).
3. The next `build-test-fixtures` aborts under `set -e` → **the old stamp
   survives untouched**.
4. `_require-fixtures` sees a stamp and passes. `test-all` proceeds as if the
   fixtures were built.

A first-ever run on a clean checkout is safe (no stamp, hard fail). It is
precisely the *regression* — the case that matters — that the gate cannot see.
This is a success marker that is never cleared before the attempt it certifies.

## Defect 2 — the compile-check lane is outside the staleness probe

`scripts/check-fixtures-stale.sh` (the `_check-fixtures-stale` gate) enumerates
its subjects from `examples/fixtures.toml` via `fixtures-manifest.py list` /
`list-workspaces`. The compile-check fixtures are **not in that manifest** —
they live in a hardcoded array inside `compile-check-fixtures.sh` (the
`l9_register_cpp:…` entries).

So the staleness gate has *zero* coverage of that lane: it cannot notice a
compile-check fixture whose sources moved past its `.compile-ok` stamp, and it
did not notice these never being produced at all.

This is issue 0196's rule ("build-side stale probes must watch the same inputs
as test-side gates") with the probe's subject list narrower than the class it
enforces — the same shape as the four gates the 2026-07-28 audit found.

## Why the test that covers it did not go red

`cmake_node_register_metadata.rs` resolves through `require_cmake_fixture` →
`require_prebuilt_binary_fresh`, which is tier-aware: a missing fixture is a hard
failure in the full tier but a **`[SKIPPED]`** under `NROS_FIXTURES_OPTIONAL=1`
(the light host-integration lane). A lane running optional therefore reports
green-with-skips for a fixture that cannot be built at all.

That is correct behaviour for "the toolchain is absent" and wrong for "the
fixture is BROKEN" — the resolver cannot distinguish the two, because both
present as an absent artifact.

## Ways to fix, cheapest first

**A. Clear the stamp before the attempt** (fixes defect 1; ~1 line)

`rm -f target/nextest/.fixtures-built` at the TOP of `build-test-fixtures` (and
`build-all`). A failed or interrupted run then leaves no stamp and
`_require-fixtures` fails with its existing message.

Fail-closed and precisely targeted. The cost is that an interrupted (Ctrl-C)
build now also demands a re-run before `test-all` — arguably correct, since an
interrupted build stage IS unverified, and `NROS_SKIP_FIXTURE_CHECK=1` already
exists for the deliberate bypass.

**B. Put the compile-check fixtures in `fixtures.toml`** (fixes defect 2)

Make the manifest the SSoT for that lane too, so `check-fixtures-stale.sh` and
anything else reading the manifest cover it for free. Needs a `kind` for
"configure-only / cross-build" rows, since these produce a stamp or a JSON rather
than a runnable binary — real work, but it retires a hardcoded list that is
already drifting from the manifest world around it.

**C. Distinguish "absent" from "broken" in the resolver**

Have a failed build-stage lane drop a `.build-failed` marker that
`require_prebuilt_binary_fresh` treats as a hard error even under
`NROS_FIXTURES_OPTIONAL`. Then a broken fixture is red in every tier while a
genuinely absent toolchain still skips. Closes the gap in "Why the test did not
go red" without making the optional tier useless on machines missing SDKs.

**D. A `just check` step asserting the script's exit code** — REJECTED

It would re-run the build stage inside the check tier (minutes of cmake/cargo),
and `check-fast` is explicitly buildless. The exit code is already propagated
correctly; the defect is the stamp's stickiness, not a missing assertion.
Recording the rejection so it is not re-proposed.

**Recommended: A now** (one line, removes the sticky-green class outright), then
**C** (small, and it is the piece that would have turned this red in the lane
people actually run), with **B** when someone is next in the manifest.

## The general shape

A cached success marker that outlives the thing it certifies is the same pattern
as issue 0309's count-based proofs and issue 0268's museum binaries: the signal
is real, but it answers a weaker question than the one being asked. Worth
checking any other stamp in the tree against "is this cleared before the attempt
it certifies?"
