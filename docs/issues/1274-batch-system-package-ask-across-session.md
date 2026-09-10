---
id: 1274
title: "The system-package ask is batched inside one `nros setup` call and not
  across a session, so one bootstrap prints three overlapping `apt install`
  lines and acts on none of them"
status: open
type: tech-debt
area: cli, build
severity: medium
found: 2026-09-10
related: [issue-1266, issue-1267, issue-1273, issue-0368]
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
