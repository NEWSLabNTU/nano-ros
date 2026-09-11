---
id: 1236
title: "The 62 shipped crates that CANNOT `forbid(unsafe_code)` carry all 5 047
  of the tree's unsafe occurrences, and nothing records their direction of
  travel"
status: resolved
type: tech-debt
area: [core, ci]
related: [1221, 0196, phase-450]
---

## What

Issue 1221 put `#![forbid(unsafe_code)]` on the ten shipped crates measured at
literal zero `unsafe`. `forbid` is the right instrument there and a very poor
one everywhere else: it is a property, not a budget, so it says nothing about
the crates that legitimately carry unsafe — which is where all of it is.

Measured 2026-09-08 over the 72 crates in
`packages/{core,api,platform,boards,rmw,drivers,tooling}`, counting occurrences
of the token in each crate's `src/`. Total: **5 047**, over the 62 crates that
are not at zero.

| crate | src LOC | `unsafe` |
| --- | ---: | ---: |
| `api/nros-c` | 22 463 | 1 057 |
| `api/nros-cpp` | 11 150 | 984 |
| `core/nros-node` | 46 544 | 600 |
| `rmw/cffi` | 8 824 | 540 |
| `platform/nros-platform-cffi` | 2 230 | 318 |
| `rmw/zenoh/nros-rmw-zenoh` | 10 120 | 148 |
| `drivers/net/nros-smoltcp` | 2 758 | 134 |
| `rmw/zenoh/zpico-sys` | 2 521 | 110 |
| `drivers/ipc/nvidia-ivc` | 1 040 | 107 |
| `api/nros` | 12 547 | 103 |
| … 52 more crates | | 1–62 each |

Five crates hold **3 499 of 5 047**, 69 %. Four of the five are FFI surfaces
where that is the job; `nros-node` is not, and its share is ~69 % a hand-rolled
type-erased bump arena in `executor/{arena,spin}.rs`.

## Why it matters, and why it is NOT a proposal to reduce the count

The arena exists to avoid `alloc`, which is the point of `nros-node`; the C and
C++ API crates exist to be an ABI. Neither number is a defect and issue 1221 was
explicit that reducing them is a design question, not a cleanup.

What is missing is *direction*. A `String::from_utf8_unchecked` added to
`nros-rmw-zenoh` next month is a normal-looking diff that nothing objects to,
and there is no record of whether the tree's unsafe is growing, shrinking, or
being moved between crates. `forbid` cannot answer that for a crate at 148.

## Shape that fits this repo

A census-and-ratchet, modelled on `scripts/check-std-census.py`: per-crate
counts, frozen in a baseline that may only shrink, failing on an increase. Two
things it must get right, both of which this repo has been bitten by:

* **The count must be about code, not spelling.** A `grep` for the token is
  satisfied by a rename and counts comments and doc examples. A census over
  syntax (`unsafe` blocks, `unsafe fn`, `unsafe impl`, `unsafe extern`)
  measured separately is the useful shape — 1221's own table had to state
  "occurrences of the token" precisely because the two differ.
* **Its reach must equal the rule it enforces** (issue 0196's shape, which four
  gates in this tree have failed). A census whose crate list is authored drifts
  the moment a crate is added; it must enumerate from `cargo metadata`, and a
  crate it cannot read is a reported failure, not an absence.

It must also carry its own selftest on the normal path
(`check-gate-selftests`): a planted `unsafe` block that the census does not
report is the failure mode that makes the whole thing decoration.

## Decide, or say that it was decided against

The second half of 1221's fix sketch was *"decide and record a position for the
rest of the core — even 'these crates carry unsafe and here is the ceiling' is
better than silence, because it makes the bimodality a stated design fact
instead of something a reader has to measure."* That is still true and still
unwritten. Closing this as `wontfix` with that sentence written down is a
legitimate outcome; leaving it unmeasured is not.

## Evidence

* `docs/issues/archived/1221-no-crate-in-the-safety-core-forbids-unsafe-code.md`
  — the ten crates that took `forbid`, the three that deliberately did not, and
  the argument for why `forbid` is the gate for that population and cannot be
  for this one.
* `scripts/check-std-census.py` — the ratchet shape this would follow.

## Resolved (phase-450 W5, 2026-09-11)

`just check unsafe-census` — per-crate, shrink-only, counting SYNTAX
(`unsafe {}` / `fn` / `impl` / `extern` / `trait`) after stripping comments and
string literals. **4,411 syntactic sites over 73 crates**, against this issue's
5,047 token occurrences over 62; the issue said the two differ and this measures
which. 13 self-test cases on the normal path, and a planted `unsafe { }` in
`nros-node` reds it.

**The obvious enumeration was the wrong one, and that is the finding.**
`cargo metadata --no-deps` reports workspace MEMBERS — 37 crates, 3,775 sites.
The root workspace excludes ~130 paths (issue 1217), several of them real crates
no lane compiles (issue 1309), so a members-only census would have inherited that
blind spot and reported a smaller, cleaner number than the truth. Enumerated from
`git ls-files` instead: 73 crates, 4,411 sites — 36 crates and 636 sites that
would otherwise have been invisible.

Still open as a DECISION, not work: this issue's second half asks for a recorded
position on the remaining core ("these crates carry unsafe and here is the
ceiling"). The census makes that statable with real numbers; it does not make the
statement. Filed forward rather than left implicit.
