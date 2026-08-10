---
id: 498
title: "The source-metadata sidecar is written non-atomically, so two fixture rows of one leaf race and the build dies on a half-written JSON"
status: resolved
resolved_in: phase-340
type: bug
severity: medium
area: build
related: [issue-0494, phase-333, phase-340]
---

## Resolution (2026-08-10)

Every writer of a sync-owned, concurrently-read file now goes through
`nros_cli_core::atomic_file::atomic_write` — temp sibling + `rename(2)`.

**The helper already existed and that is the actual lesson.** `cmd/ws.rs` had a
private `atomic_write` whose own doc comment read "the write discipline every
other sync-owned file here uses (a parallel fixture build runs many syncs at
once)". It was not: the sidecar one directory over had three plain `fs::write`
writers. A discipline living in one file's private helper is a habit, and the
sibling site is precisely what a habit does not reach. So the fix is not "call
rename here" — it is one PUBLIC helper (`src/atomic_file.rs`), the private
duplicate deleted, and a gate.

Converted:

| site | file |
| --- | --- |
| `stamp_provenance` | `orchestration/metadata_refresh.rs` |
| `mark_unprobeable` | `orchestration/metadata_refresh.rs` |
| `relativise_source_artifacts` | `orchestration/metadata_build.rs` |
| the generated harness | `orchestration/metadata_build.rs` (inlined — see below) |
| bridge runtime config | `cmd/ws.rs` |
| `ws clean` config rewrite | `cmd/ws.rs` |

`mark_unprobeable` was NOT in the original report and is the same defect:
`is_known_unprobeable` compares the whole file against a digest, so a reader
catching a truncated marker reads "not unprobeable" and pays the full failing
probe the marker exists to skip — a silent cost rather than a crash, which is
why nothing had noticed.

The generated metadata harness is a standalone crate that cannot depend on the
CLI, so it inlines temp+rename rather than calling the helper. That code is a
STRING TEMPLATE — nothing type-checks it until a fixture build compiles the
generated crate, minutes away and on someone else's machine — so it was
verified by rendering `render_harness_main` and compiling the emitted text with
`rustc --edition 2024`, which built and ran and left no temp behind.

### Gate — `check-atomic-sync-writes`

In `check-fast` (buildless). Three arms, each verified to FAIL before being
trusted: a guarded writer reverting to `fs::write`, the generated harness
losing its `rename`, and a second `fn atomic_write` appearing anywhere outside
`atomic_file.rs` (which is how the first helper failed to reach the sidecar).

Deliberately NOT "no `fs::write` in the CLI": most writes go to a private temp
or a scratch dir, and a gate that flags those is noise — and noise gets
suppressed. It names the writers of the contended paths.

### Not done, deliberately

The report asked whether the sidecar should be coordinate-keyed at all, since
three rows recompute one identical file. It should probably be reconsidered,
but that changes the freshness contract (`source_digest` keying, the negative
cache, `--target-dir` layout) and belongs with phase-340's build-artifact work,
not with a correctness fix. Atomicity is right regardless of whether the
duplicate work is removed.

### Verification

`cargo test -p nros-cli-core --lib` 517/517; `cargo clippy --all-targets
-- -D warnings` clean; three new `atomic_file` unit tests (whole-content
replacement, no temp left behind, creates a missing destination) and one
asserting the rendered harness renames.

## Symptom

`just build-test-fixtures lane=native` died mid-sweep:

```
Error: metadata harness emitted invalid JSON at
<repo>/examples/native/rust/service-client/metadata/add_two_ints_client.json

Caused by:
    EOF while parsing a value at line 1 column 0

Location:
    nros-cli-core/src/orchestration/metadata_refresh.rs:323:65
make[1]: *** [build/fixtures-build-make/linux-rust-all-489153-29211.mk:169: fixture-0053] Error 1
```

`EOF at line 1 column 0` is an EMPTY file, not malformed JSON. Inspected
immediately after the failure the same path was **1345 bytes and valid**, and a
straight re-run of the identical command completed. So the file was read during
the window in which it did not yet have contents.

## Cause

The sidecar has ONE path per component and THREE non-atomic writers, each a
plain `std::fs::write` — which truncates to zero and then fills:

| writer | site |
| --- | --- |
| the generated metadata harness | `metadata_build.rs:295` (`std::fs::write({out:?}, json)`) |
| the path-redaction rewrite | `metadata_build.rs:469` |
| `stamp_provenance` (reads it back) | `metadata_refresh.rs:323` |

`examples/native/rust/service-client` has SEVERAL fixture rows — `rmw-zenoh`,
`rmw-xrce` and `rmw-cyclonedds`, each with its own `--target-dir` — and the
fixture make graph runs them concurrently. Every one of those rows runs
`nros sync` over the same leaf, so they contend for the same
`metadata/add_two_ints_client.json`. One truncates while another is reading:

```
row A: fs::write(sidecar)  → truncate ......... write 1345 bytes
row B:                        read_to_string ↑  → ""
```

The per-RMW `--target-dir` split (phase-340) makes the rows independent
everywhere EXCEPT here: the sidecar is keyed by component, not by coordinate,
so target-dir isolation does not reach it.

## Why it is worth fixing rather than retrying

It is nondeterministic and it fails a BUILD, not a test — the whole sweep stops
and the operator has no signal distinguishing it from a real compile error. It
cost one full re-run of `lane=native` here.

**It is also a known class in this repo, fixed one file over.** Upstream
`f6290fbdb` — "fix(#494): write lane-coords atomically — ci-matrix was
non-deterministic" — is the same defect with the same shape and the same
remedy. #494 was found because the output was wrong; this one is found because
the read failed loudly, which is the luckier half of the same bug.

## Fix

Write via a temp file in the same directory + `rename()`, which is atomic on
POSIX: a reader then sees either the old contents or the new ones, never a
truncated file. All three sites, plus a sweep for siblings — `git grep -n
'fs::write' packages/cli` over anything a concurrent process reads.

Worth checking in the same pass whether the sidecar should be coordinate-keyed
at all. Three rows recomputing one identical file is wasted work even when it
does not race; if the content is genuinely coordinate-independent, one row
should produce it and the others should depend on it.

## Reproduce

Not reliably — it is a timing window. It appeared once in ~4 `lane=native`
sweeps on a 24-core host. The evidence above (empty read, valid file
immediately after, clean re-run, three truncating writers on one path) is what
identifies it; a reproduction would need the writes instrumented.
