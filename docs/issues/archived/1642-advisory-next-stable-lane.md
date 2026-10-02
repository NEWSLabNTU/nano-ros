---
id: 1642
title: "The workspace toolchain is now pinned, and nothing tries the NEXT stable —
  so the pin can age silently until a routine six-week bump becomes a sixty-site
  one"
status: resolved
type: tech-debt
area: [ci, build]
severity: low
related: [1447]
found: 2026-10-02
resolved_in: 2026-10-03
---

## The gap

Issue 1447 was resolved by pinning `rust-toolchain.toml` to `channel = "1.99.0"`
(PR #1556), after Rust 1.99.0 reached every checkout and CI at once through the
floating `stable` channel and took `main` red for everyone.

The pin trades one risk for another. Before, a toolchain move arrived with no
commit and broke everyone the same day. Now a toolchain move happens only when
someone chooses to bump — and **nothing tells anyone that a bump is due, or how
large it will be.** Rust ships a stable release every six weeks. A pin left
alone for three releases arrives at its next bump carrying three releases of new
lints at once.

The measurement that prices this: clearing ONE release (1.98 → 1.99) took nine
sites across three tools — 2 clippy, 5 rustdoc, 7 rustc future-incompat errors
(PR #1536). That is the per-release cost, and it does not shrink by waiting.

## What would close it

An **advisory** lane that builds and lints the workspace against the next stable
toolchain — and is **allowed to be red**. Its job is to report, not to gate:

- red means "the next bump will cost these sites", visible before anyone pays it
- green means "the bump is free today", which is the moment to take it

It must NOT be merge-gating. A gating lane on an unpinned toolchain would
re-create exactly the failure 1447 just fixed — every PR blocked by a compiler
nobody chose.

Shape worth pricing: a scheduled job (not per-PR) running `check fast`'s clippy
and rustdoc constituents under `RUSTUP_TOOLCHAIN=stable`, posting its delta
against the pinned version. The two halves of the measurement already exist —
#1536's A/B (`RUSTUP_TOOLCHAIN=1.98.1` vs `=1.99.0` on the same tree) is the
template.

## What this is NOT

- **Not a question of which lanes gate merges.** This asks for a lane that
  deliberately gates nothing. Conflating the two would put an unpinned
  toolchain in the merge path. (This bullet used to say "Not issue 1445. 1445
  asks which lanes gate merges." Issue 1445 is an unrelated linker-script bug,
  and no issue owns the gating question — corrected 2026-10-03; the full
  correction, with what actually gates today, is in archived issue 1447.)
- **Not a reason to un-pin.** The pin stands; this is what makes keeping it cheap.

## RESOLVED 2026-10-03 — `.github/workflows/next-stable.yml`

**Shape.** One job, `next-stable (advisory — gates nothing)`, in the same
`nano-ros-ci:humble` container `gate.yml`'s `check` job uses, with the same
CLI build (`setup-nros-cli`) and compile-tier `nros setup --source …` set. It
runs `scripts/ci/next-stable-delta.py --install`, which installs the candidate
as a NAMED toolchain (`rustup toolchain install stable --profile minimal`, the
default and the pin untouched) and then, per gate, runs
`RUSTUP_TOOLCHAIN=<pin> just check <gate>` and
`RUSTUP_TOOLCHAIN=<candidate> just check <gate>` on the same tree — #1536's
A/B — and reports the diagnostic SITES (gate, `file:line:col`, message) the
candidate raises that the pin does not, classified clippy / rustdoc / rustc.

**What it runs — corrected from the shape above.** This issue proposed "`check
fast`'s clippy and rustdoc constituents". `check fast` has none: it is
buildless by contract (RFC-0061). The lint gates a toolchain can move are in the
compile tier, and the ones that gate a merge are `test-targets` and
`workspace-embedded` (the two halves of `check workspace-all`, `pull_request` +
`merge_group`) and `rustdoc-links` / `rustdoc-workspace` (`pull_request`) —
precisely the five that went red on 2026-10-01. Those are the lane's gates,
because they are what a bump PR must clear, plus `cli-clippy` on its own row:
it is the last step of `test-targets`, which is `set -e`, so when the workspace
clippy fails the CLI is never linted (the replay below measured that). `--selftest` asserts gate.yml still
runs each, so a renamed step drifts the list loudly.

**Trigger.** `schedule` (Mondays 04:17 UTC) and `workflow_dispatch`, whose
`candidate` input also accepts `beta` or an exact version for a longer look
ahead. Weekly catches a new stable within seven days of its release.

**Three outcomes.** Exit 0 FREE (no new sites, or the candidate IS the pin — in
which case no gate runs at all); exit 1 PRICED (the sites are listed); exit 2
NO VERDICT (the pin arm is itself red, so nothing is attributable to the
toolchain, or a gate failed under the candidate with no parseable diagnostic).
The count is a LOWER BOUND, and the report says so: cargo aborts an invocation
at the first failing crate, and `--keep-going` has no config key or env
spelling (measured — `CARGO_BUILD_KEEP_GOING` and `--config build.keep-going`
are both ignored), so passing it would mean editing every recipe or the cargo
shim. `test-targets` lints each crate alone anyway, which is how #1536's clippy
sites surfaced.

**Why it cannot gate a merge.** The ruleset `main-rules` requires exactly one
context, `{"context":"CI"}` (read from the API on 2026-10-03), produced by
`gate.yml`'s `ci-ok` job, whose `needs:` can only name jobs in `gate.yml`. This
workflow (1) has no `pull_request` / `merge_group` / `push` trigger, so no check
run ever attaches to a PR head or a queue batch; (2) is a separate file, so it
cannot be in `ci-ok`'s `needs:`; (3) names no job `CI` — the required context is
matched by bare NAME, so that is the one way a job elsewhere could report into
it. All three are one-line edits that would fail nothing on the day they were
made, so `just check next-stable-advisory` (fast tier, pure text) refuses each,
with negative controls for all of them in its selftest.

**Where the report goes.** The job summary (the markdown table of gates and
sites), an `::error` / `::notice` annotation naming the verdict, and a
`next-stable-report` artifact with every log (30 days). GitHub mails a
scheduled run's failure to whoever last edited its `cron:` line. Whoever bumps
the pin reads `gh run list --workflow next-stable.yml` first, or runs the same
script locally.

**Measured locally — a replay of the 1.98 -> 1.99 bump.** On a temporary local revert of #1536's two fix commits (`dc5be4dbe2`,
`c958d943dd`; not committed), in the `ros2` box, `--pin 1.98.1 --candidate
1.99.0`. The pin arm was green on every gate, so the delta is attributable:

| #1536 measured | this lane found | gate |
| --- | --- | --- |
| 2 clippy `needless_borrows_for_generic_args` (nros-node `spin.rs`) | **2**, same lint, same two sites | `test-targets` |
| 7 rustc `semicolon_in_expressions_from_non_local_macros` (4 cargo-nano-ros + 3 nros-pkg-index) | **7**, same files | `cli-clippy` |
| 5 rustdoc `redundant_explicit_links` | **2** (lower bound — see below) | `rustdoc-links` |

Two things the replay corrected in the tool before it shipped. rustdoc reports
`redundant explicit link target` with NO `-->` span, so a parser keyed on
locations counted zero of them; span-less diagnostics now count one per
occurrence. And the first version had no `cli-clippy` row: `test-targets` died
at its workspace clippy on the nros-node errors and never reached the CLI, so
all 7 rustc errors were invisible. rustdoc's 2 of 5 is the documented lower
bound — `cargo doc` stops at the first crate that fails. The verdict counts
ERRORS; new warnings are listed as early notice but do not redden the run
(measured: the three `nros-pkg-index` sites are warnings where the root
workspace reaches that crate as a lint-capped dependency, and errors in
`cli-clippy`, where it is a member — the lane prices the latter once).

**What only the first scheduled run proves.** That the container's rustup
installs `stable` with clippy and the thumbv7em target; that the compile-tier
sources provision the way `gate.yml`'s do; that two full lint builds fit the
runner's disk and the 240-minute timeout; and that the summary renders. None
of that can be exercised from a workstation.

