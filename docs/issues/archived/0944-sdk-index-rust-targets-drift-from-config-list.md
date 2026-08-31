---
id: 944
title: "`[rust.target.*]` in the SDK index is a third copy of the cross-target list, ungated and already stale"
status: resolved
type: bug
area: cli, tooling
related: [issue-0833, issue-0943, rfc-0014, rfc-0062, rfc-0013]
---

## The drift

Two hand-authored lists of the cross Rust targets this tree builds for:

| Where | Read by | Has `armv8r-none-eabihf`? |
| --- | --- | --- |
| `config/rust-targets.txt` | `just workspace rust-targets` (install), `just doctor` (verify), `scripts/ci/runner-doctor.sh`, and now `check-tier-preconditions` ([[issue-0943]]) | yes |
| `[rust.target.*]` in `nros-sdk-index.toml:984-1001` | `nros setup --check` | **no** |

Nothing reconciles them. `scripts/check-rust-targets-covered.py` asserts that
every triple appearing in a board TOML, a cmake toolchain file or a
`.cargo/config.toml` has a row in `config/rust-targets.txt` — it does not look
at the index at all.

So `nros setup --check`, the CLI's own doctor surface, cannot report
`armv8r-none-eabihf` missing on a host that needs it: the index does not know
the target exists. That is precisely the shape issue 0833 fixed one layer down,
recurring one layer up — and it is the reason to fix it structurally rather than
by adding the row.

Also absent from the index: `armv7a-nuttx-eabihf` and
`riscv32imac-unknown-nuttx-elf`. Those two are arguably CORRECT to omit (no
prebuilt std; they are `build-std` rows), which is itself the point — the two
lists have different membership rules and neither states the other's.

## Why it is easy to miss

`just doctor` runs `nros setup --check` but filters it to `[BROKEN]` lines only
(`just/workspace.just:373-383`, issue 0929). Index-declared `[MISSING]` rust
targets therefore never reach the doctor's output through that path, and the
`rust-targets` step a few lines later — reading the OTHER list — is what
actually reports them. Two checks, two lists, one of them silently doing the
work while the other looks like it is.

## Fixed

**The gate**, in `check-rust-targets-covered.py` — the script that already owns
this list, rather than a fifth file. The new direction is EXACT, not superset,
and in both senses:

* a `rustup` row MUST have a `[rust.target.*]` entry — otherwise
  `nros setup --check` is blind to it, which is exactly how armv8r was missed;
* a `build-std` row MUST NOT — those have no prebuilt rust-std, `rustup target
  list` never reports them, and `rustup target add` on one fails. An entry
  there would make the CLI print a remedy that cannot work.

Several aliases may share a triple (`thumbv7m` and `thumbv7m-nightly` differ
only by `toolchain`), so the check is on the set of triples, not the aliases.
The index is parsed by regex rather than a TOML library so the gate keeps
working on a host with no `tomllib` — it runs before any provisioning — and so
a syntax error in the index is reported by `just check sdk-index`, which owns
it, instead of surfacing here as a stack trace.

**The missing row**, `[rust.target.armv8r-hf]`, with the reason it was absent
recorded beside it.

**A self-test**, run ahead of the gate itself. The tree exercises at most one of
the three verdicts at a time, so the other two would have shipped unproven —
issue 0942's lesson that a gate never shown to fail is not known to work. Five
cases: in-sync, missing-from-index, build-std-listed, stray-entry, and
build-std-absent-is-fine.

## Verified end to end, by removing the target

    $ rustup target remove armv8r-none-eabihf

    $ nros setup --check
      [MISSING] rust  target armv8r-hf (armv8r-none-eabihf)
                (run: rustup target add armv8r-none-eabihf)

    $ just check tier-preconditions
      [x] cross Rust target(s) declared by this tree are not installed
          remedy: just workspace rust-targets

Both surfaces now name it, with a remedy that works. Before this, the first
reported nothing and the second did not exist ([[issue-0943]]).

## What is still NOT fixed, deliberately

`[rust.target.*]` remains a flat global list that `[board.*]` cannot reference:
`resolve_packages` only resolves into `[tool.*]`/`[source.*]`/`[gated.*]`/
`[prereq.*]`, so `nros setup <board>` still cannot provision a board's target.
That is the deeper defect — and the reason this table drifted, since nothing
board-scoped consumed it. RFC-0013's deferred custom-board work (phase-201)
names it directly: "cargo already fetches these; nros provisions nothing, just
declares the rustup target + runner tool". Fixing it means giving `[board.*]` a
`rust_targets` key and teaching resolution to honour it, which is phase-201's
scope, not a gate's.

Also unchanged: `just doctor` still filters `nros setup --check` to `[BROKEN]`
lines (`just/workspace.just:373-383`, issue 0929), so the index's `[MISSING]`
rust-target lines do not reach the doctor's output through that path — the
`rust-targets` step a few lines later, reading the other list, is what reports
them. Two checks, two lists, one silently doing the work. Worth revisiting when
the two lists become one.

## Acceptance

* ~~The two lists cannot disagree without a gate failing, and the gate names
  which direction is wrong.~~ Met — three named verdicts, each self-tested.
* ~~`nros setup --check` reports a missing `armv8r-none-eabihf` on a host that
  needs it.~~ Met, verified by removing the target and reading both surfaces.
