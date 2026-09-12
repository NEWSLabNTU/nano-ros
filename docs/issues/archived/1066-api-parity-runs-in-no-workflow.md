---
id: 1066
title: "`check-api-parity` is run by NO workflow on any event, and cannot run in
  the CI container because the image ships no `clang`"
status: resolved
type: bug
area: ci
related: [issue-1040, issue-1059, issue-1188, issue-1201, issue-1358, phase-379,
  phase-417, rfc-0089, phase-413]
---

## Problem

`grep -rn "api-parity" .github/workflows/` returned **nothing**. It was step 4
of `just ci gate` and a documented part of `ci-l1`, but no `pull_request`, no
`merge_group`, no `schedule` and no `workflow_dispatch` job invoked it.

So the parity ledger — 2158 rows whose whole purpose is to fail when our user
API drifts from rclc/rclcpp/rclrs — was checked only by whoever ran the full
local tier. That is how phase-421's `nros_node_get_serialization_format` (C) and
`Node::serialization_format` (C++) landed UNLEDGERED and stayed that way
(issue 1059).

## Two causes, both measured

**1. The image had no `clang`.** `scripts/api_parity/extract_cxx.py` extracts our
C and C++ surface by running `clang -Xclang -ast-dump=json …`, and
`grep -n clang ci/docker/ci-base/Dockerfile` matched nothing. The image installs
`clang-format` separately and pinned (`just setup-clang-format`), which is a
different binary — an easy thing to mistake for coverage.

**2. `cargo +nightly` named a toolchain the image does not have.** The container
installs `nightly-2026-04-11` and creates no `nightly` alias, so the bare form
either failed or silently fetched a *different*, unpinned nightly. rustdoc's
JSON is an UNSTABLE format, so "some other nightly" is not a harmless
substitution: its schema is exactly what the extractor parses.

Both were invisible locally because a developer box has clang and usually has a
`nightly` toolchain. Same shape as the rest of issue 1059's family — the local
environment answering a question the CI environment would answer differently.

## How it was fixed, in four pieces landed by three changes

**Cause 2** — `extract_rust.py` reads the pin from `tools/rust-toolchain.toml`
instead of spelling `+nightly`.

**Cause 1** — `0c73d061b` added `clang` to `ci/docker/ci-base/Dockerfile`. That
alone did not deliver it: issue 1201 found `images.yml` had been handing buildx
`context: ci/docker/ci-base` while the Dockerfile `COPY`ed repo-relative paths,
so every ci-base build since 2026-09-04 failed in under a minute and the
floating `humble` tag kept serving 2026-08-29 content. A floating tag that stays
put on a failed build is indistinguishable from one that was never asked to
move. Fixed; the image has published successfully on 2026-09-06, -07, -10 and
-12.

**The lane** — `565f8c74f` deleted `api-parity` from
`.config/gate-lane-exempt.txt`. That file's polarity is inverted (a recipe in
`just/check.just` is a fast-lane gate BY DEFAULT), so one deleted line is the
whole placement: `just check fast` in `gate.yml`'s `check` job now runs it on
`pull_request`, `merge_group`, `push` and the nightly.

**The input hole** — this change. The gate's only authored inputs are
`docs/reference/api-parity-ledger/` and `docs/reference/api-surface/`, both
under `docs/`, and `gate.yml`'s `code` probe classified anything under `docs/`
as documentation and skipped the whole `check` job. So a pull request whose
entire subject was the ledger — including one deleting a row for a divergence
that still exists — got no parity verdict until the merge queue. The probe now
excepts those two prefixes, ahead of its `docs/*` arm.

## Measured, so the placement is not a guess

**The gate is GREEN on main**, 2026-09-12, `6f40985a5`: `just check api-parity`
exits 0 and ends `every divergence carries a ledger entry`. Nothing here landed
a merge-gating step over a red lane.

**It runs in the container**, and that is also the image evidence — a 217 s
clang + rustdoc extraction that exits 0 says more about what the image holds
than any `which clang` step. From run 34687504281's `check` job, a
`pull_request` on 2026-09-12:

```
check-fast: slowest gates (ms)
    217543   14.1%  api-parity
  busy 1542s over wall 386s at -P4 => 4.0x effective parallelism
check-fast (parallel): 319 gate(s) ran at -P4 …; slowest api-parity 217543ms
```

| | |
| --- | --- |
| local wall, 24-core, warm | **39 s** (measured twice: 38.7 s, 39.2 s) |
| in-container, inside the lane at -P4 | **217.5 s**, 14.1 % of the lane's busy time |
| the lane's own wall with it | **386 s** — so it is not the critical path |
| fixture / SDK / QEMU / ROS install | **none** |
| leaves a `target/` dir | **no** — rustdoc writes into a temp `CARGO_TARGET_DIR` |

`check-lane-contracts` accepts it because it RESOLVES no artifact its lane did
not produce: the C side is `-I` over source headers, the Rust side builds its
own rustdoc JSON into a fresh temp dir.

**A dedicated `gate.yml` step was considered and rejected on that last row.** It
would buy a verdict named `api-parity` instead of `check fast`, and cost a
second full 217 s extraction — SERIAL, against a job whose whole fast-lane wall
is 386 s. `just check fast` already prints `[FAIL] api-parity` with the row key.

**The `post-submit.yml` job is gone.** Issue 1040 put one there when the gate
ran nowhere and was red on main; both halves have since moved, and the merge
queue tests the MERGED state with `main-rules` admitting no other path to
`main`, so a post-submit run re-asked a question already answered about the same
tree — at 217 s per merge plus an `apt-get install clang` the image now ships.
`dep-chain` stays: it needs codegen output, so no earlier lane can run it.

## Negative control

Deleting one row from `docs/reference/api-parity-ledger/serde.json` —
`c:cdr_begin_dheader`, a real exported symbol — makes the gate reject it:

```
  + cdr_begin_dheader          UNLEDGERED   ours-only
1 item(s) differ with no ledger entry. Add a row to docs/reference/api-parity-ledger/<lang>.json
  c:cdr_begin_dheader  (ours-only)
error: recipe `api-parity` failed on line 48 with exit code 1
```

That same edit is what demonstrates the input hole: run through the `code`
probe's classifier body as it stood, a changed path of
`docs/reference/api-parity-ledger/serde.json` yielded `code=false`, so the job
holding the gate would have skipped. After the fix it yields `code=true`, while
`docs/issues/*.md`, `book/**` and `docs/reference/*.md` still yield `false`.

## Found in passing, filed separately

Running the gate leaves the tracked root `Cargo.lock` MODIFIED — issue 1358.
Not a blocker for the placement (a CI checkout is ephemeral) and not fixed here,
because `--locked` over a lock cargo wants to rewrite is a hard error, which is
the one thing this change must not introduce into a merge-gating lane.
