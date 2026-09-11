---
id: 1274
title: "The system-package ask is batched inside one `nros setup` call and not
  across a session, so one bootstrap prints three overlapping `apt install`
  lines and acts on none of them"
status: resolved
type: tech-debt
area: cli, build
severity: medium
found: 2026-09-10
resolved: 2026-09-11
related: [rfc-0099, phase-447, issue-1266, issue-1267, issue-1273, issue-0368]
---

## What this is

Within one `nros setup` call the packages are already composed into a single
command — the batching people ask for exists:

```
sudo apt-get install -y aria2 doxygen graphviz libz3-dev make ninja-build parallel \
  || { sudo apt-get update && sudo apt-get install -y … }
```

What does not exist is batching ACROSS the several `nros setup` calls a
provisioning session makes. Measured on one contained-runner bootstrap
(`just setup base`, then `qemu`, `threadx_riscv64`, `zephyr`): that block was
printed THREE times with DIFFERENT subsets —

```
…install -y aria2 doxygen graphviz libz3-dev make ninja-build parallel
…install -y aria2 doxygen graphviz libz3-dev parallel
…install -y aria2 doxygen graphviz libz3-dev parallel
```

— so a contributor who runs the first command is still told about it twice more,
and one who reads only the last is told about a subset.

## And it prints rather than runs, on purpose

This is not a bug to fix by adding sudo. Phase-327 W2 / issue 0368 F1: no
provisioning step runs sudo, because one sudo failure used to cascade and abort
every sudo-less step after it. `nros setup --system --sudo` exists for an
operator who wants it executed.

The problem is WHEN the ask happens, not that it is an ask. It arrives in the
middle of a long run, is non-fatal by design, and therefore scrolls past. On a
host where nobody can act on it — a container is the honest case, since
`--cap-drop ALL --security-opt no-new-privileges` with a non-root user means
`sudo apt` can NEVER succeed inside it — the notice is pure noise, and the run
ends one package short of a label it claims.

That is not hypothetical: it is how the runner image shipped five packages short
(`aria2`, `doxygen`, `graphviz`, `libz3`, `gnu-parallel`), and separately how
`wget` reached a 1.4 GiB download before failing at exit 91 (issue filed and
fixed in PR #842).

## Fix

Resolve the union of every system package the WHOLE session needs, before any
download starts, and ask once. A preflight, not a running commentary.

Two properties to keep:

* still no sudo by default — one printed command, or `--sudo` to execute it;
* still per-manager, through the index (`prereq-packages.py` / `PrereqContext`),
  never a hand-written list beside it (RFC-0062).

## Batching is not only apt

DECIDED (2026-09-10): the same "gather, then do once" applies to other stages of
provisioning, and the survey belongs to whoever takes this issue rather than
being guessed at here. Candidates seen in one bootstrap: `rustup target add`
(invoked per platform verb, and `just workspace rust-targets` already has the
shared list), submodule provisioning (`nros setup --source` called repeatedly
with overlapping sets), and the repeated `nros setup --tool corrosion:
present (skip)` lines that re-resolve the same tools per verb.

Survey first, then batch what the survey justifies. A batch of one is a
refactor with no payoff, and a batch of the wrong things reorders work that had
an ordering reason (issue 0500).

## Resolution (phase-447 E1 + E2, RFC-0099 D7)

**The three lines differed because the FIRST one was wrong, not because it was
early.** Its extra `make ninja-build` were keys the same session went on to
provision from the store (`workspace setup` runs `nros setup --tool ninja` /
`--tool make`), after which the later asks correctly dropped them. Worse, on
Ubuntu 22.04 neither apt package meets its version floor (ninja 1.10 against
1.13, make 4.3 against 4.4), so that part of the first ask could never have
been satisfied. So the fix is two things, not one:

* **A key the store provides is never an OS ask.** A missing chain key
  (`providers = ["system", "sdk"]`) is offered as `nros setup --tool …`, and not
  asked at all when the plan installs that tool itself. `[prereq.make]` gained
  the chain `[prereq.ninja]` already had.
* **One ask per session.** Inside a process, every install path resolves a plan
  first and asks once for the union of its tools' `system = [..]`
  (`cmd/setup/session.rs`). Across processes, which RFC-0099 D6 keeps, the
  session's driver (`just setup`, `runner-provision.sh`) opens a ledger named by
  `NROS_SETUP_SESSION`, and a key asked for once is not asked again. Only
  repeats are removed, never a need: a key nobody has asked for yet is still
  asked for, wherever it is first met.

Both properties kept: no sudo by default (the print form still prints; `--sudo`
still runs, and never consults the ledger), and packages still come only from the
index's per-manager mapping through `PrereqContext`.

**Measured**, with the release `nros` over a synthetic index carrying this
issue's keys (every OS probe pointed at a command that cannot exist, so the
result does not depend on the host), in this issue's bootstrap order — `just
setup base` (every role, twice), then `qemu`, `threadx_riscv64`, `zephyr` (host
roles): ONE `sudo apt-get install -y aria2 doxygen graphviz libslirp0 libz3-dev
parallel` line, one sudo-less `nros setup --tool …` line for make and ninja, and
a one-line "not repeated" note from each of the four later steps. Not measured: a
real contained-runner bootstrap.

**The one case that still prints two lines, deliberately:** a session that asks
NARROW first and WIDE later — a `just setup <platform>` (host roles) whose own
setup later asks every role — prints a second line holding only the keys the
first never covered. It is disjoint, never overlapping, because the ledger
removes repeats, not needs. Widening the first ask to prevent it would undo
phase-422 W6's deliberate host-role narrowing for platform setups. The bootstrap
this issue measured starts with `just setup base`, whose first ask is already
the widest, so it is not affected.

Found on the way: `nros setup --system`'s print form ignored `--role` (it
re-read `index.prereqs()`) and ran each entry's bare probe instead of its
provider chain. It now shares one classification with `--check` and the install
paths. `_setup-common` asks through the print form, so the session's ask is a
command, not a doctor's `Error:`.

**The survey, and what it justified:**

* *Repeated `--tool` calls* — batched (E1). `--tool` repeats, and the two
  adjacent pairs in `workspace.just` are one plan each. The remaining separate
  calls in `cargo-tools` are separate on purpose: each is individually
  non-fatal, and one plan would make the set's exit status the only verdict.
  An already-present tool re-resolves to one `present (skip)` line; that cost
  is what RFC-0099 D6 decided to keep cheap rather than remove.
* *Submodule `--source` repeats* — not batched. B1's fast-skip already answers
  a provisioned source from `git submodule status`, so a repeat costs well
  under a second; batching would have removed very little.
* *`rustup target add`* — not batched here. `just workspace rust-targets`
  already reads the one shared list and is idempotent, and it is not an
  `nros setup` path at all. Not measured in this change, so no claim is made
  about its cost.
