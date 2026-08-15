---
id: 602
title: "`[source.threadx]` declares upstream at a commit we do not pin — inert to provisioning, but it is what set a clone's `origin` to upstream"
status: open
type: bug
area: build
related: [issue-0550, issue-0575, phase-360]
---

## What is true

`nros-sdk-index.toml` and git disagree about `third-party/threadx/kernel`:

| | URL | commit |
| --- | --- | --- |
| `.gitmodules` + superproject gitlink | `NEWSLabNTU/threadx.git` | `13d061a7` |
| `[source.threadx]` | `eclipse-threadx/threadx.git` | `4b6e8100` |

`13d061a7^` IS `4b6e8100`, and the single commit between them is our own
`fix(linux port): key LONG/ULONG and ALIGN_TYPE on the data model, not x86_64`.

Observed alongside it: the local clone's `origin` was
`eclipse-threadx/threadx.git` — upstream — while `.gitmodules` declares the
fork, and the checkout sat on the upstream tag `v6.4.5.202504_rel` (`4b6e8100`)
rather than the recorded `13d061a7`.

## What I first claimed, and why it was WRONG

Filed as "`nros setup --source threadx` provisions upstream at the parent of our
fix, silently reverting it". That mechanism does not exist.

`SourceProvision::provision()` returns `Submodule` whenever `submodule` is set,
and in that arm provisioning runs `git submodule update --init`, which checks out
the **gitlink**. `git_ref` is read only in the `Clone` arm
(`expect("clone mode has a ref")`); its one other use is a display string in
`setup.rs`. So for this entry — and every `[source.*]` naming a submodule — `git`
and `ref` are **never consulted**. `nros setup --source threadx` checks out
`13d061a7`, i.e. WITH the fix.

The evidence looked strong: the index says `4b6e8100`, the checkout was at
`4b6e8100`. That is a correlation, and I wrote a mechanism around it without
reading the consumer. Same mistake as #0575 the same day — trusting a
comparison instead of following the code that acts on it.

**So what moved the checkout is UNKNOWN.** Not established: whether a `just`
recipe, a historical `.gitmodules` URL, or a manual step did it. Whoever picks
this up should start there rather than from the index.

## What is still worth fixing

A data file that declares a repository we do not pin, at a commit we do not
pin. Two concrete harms, neither of them a revert:

* **It names the wrong push target.** The vendored-fork workflow says commit in
  the submodule, then push the fork. A contributor reading `[source.threadx]`,
  or working in a clone whose `origin` matches it, pushes to `eclipse-threadx`.
  That was the live configuration on this machine.
* **It reads as authoritative and is not.** The entry is the only place stating
  where ThreadX comes from; it disagrees with git and nothing says so.

Five sibling entries have the same shape — `ref` disagreeing with the gitlink
for `zenoh-pico`, `nuttx-libc`, `px4-rs`, `cyclonedds-src`, `nuttx-kernel`. All
are equally inert, and all are equally misleading to a reader.

## Fix

Point `[source.threadx]` at the fork and the pinned commit
(`NEWSLabNTU/threadx.git`, `13d061a7`). Applied.

A gate comparing every `submodule =`-bearing `[source.*]` against `.gitmodules`
and the gitlink was written and then **deliberately dropped**: with the fields
inert, it would enforce tidiness in data nothing reads, and the six failures it
produced would read as six bugs when none of them changes behaviour. Recorded
here so the next person does not rebuild it without first deciding whether these
fields should exist at all — which is the better question.

## The better question

If `git`/`ref` are unused for submodule-mode entries, they are three recordings
of one fact where one would do. Either drop them from those entries, or have
`provision()` use them and make the submodule path honour the index. Today the
index carries a third opinion nobody asks for, which is how it drifted six ways
without anyone noticing.

## Acceptance

* `[source.threadx]` names the same repo and commit git does (done);
* a decision recorded on whether `git`/`ref` belong on submodule-mode entries at
  all — and if they stay, one mechanism that keeps them true;
* what actually moved the checkout onto the upstream tag is identified, or the
  question is explicitly closed as unanswerable.
