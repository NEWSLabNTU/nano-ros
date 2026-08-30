---
id: 602
title: "`[source.threadx]` declares upstream at a commit we do not pin — inert to provisioning, but it is what set a clone's `origin` to upstream"
status: resolved
type: bug
area: build
related: [issue-0550, issue-0575, phase-363]
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


## RESOLVED 2026-08-19 — the better question, answered: the fields are gone

This issue's remaining acceptance was "a decision recorded on whether `git`/`ref`
belong on submodule-mode entries at all". They do not, and they have been
removed from all **14** of them — not the 6 the survey named, which were only
the ones that had visibly drifted.

### Measured first: the fields are unread

`SdkStore`'s submodule branch takes `src.submodule` (the path) and nothing else:

```
git submodule update --init <path>          # fast path
git ls-tree HEAD <path>                     # the gitlink sha, on fallback
git fetch --depth 1 origin <sha> && git checkout <sha>
```

The entry's `git`/`ref` are never consulted. So the claim in `SourcePackage`'s
own doc comment — that they "record the canonical pin (the SSOT — so
`.gitmodules` and the index can't drift)" — was false in both halves: not the
SSOT, and no barrier to drift. Six of fourteen had drifted.

### Why removal rather than a gate

`.gitmodules` plus the gitlink already hold provenance authoritatively, and git
ENFORCES them — a checkout cannot silently disagree with the gitlink the way a
TOML field can. Keeping the fields and adding a comparison gate would police a
third recording of one fact; deleting them removes the surface. That is also why
the gate written during the original investigation was right to be dropped.

Schema-safe: `SourcePackage.git` / `.git_ref` are already `Option<String>`
(clone-mode entries keep them, where they ARE the only pin). Verified by loading
the index through the CLI — `SourcePackage` carries `deny_unknown_fields` and
the whole file deserializes on load, so a successful `nros sdk-path` proves all
fourteen edited entries parse. `just check fast` and `check-cli-tests` green.

The doc comment now states the split: clone mode's `git`/`ref` are the pin;
submodule mode has none and resolves through git.

### Still open from the acceptance list

"What actually moved the checkout onto the upstream tag" is NOT identified. The
local clone's `origin` pointed at `eclipse-threadx` while `.gitmodules` declared
the fork, and nothing in this tree writes that remote. It is unreproducible from
the repository alone — most likely a manual `git remote` or a clone predating
the fork — and is closed here as unanswerable rather than left implying an
undiscovered mechanism. The harm it caused (a contributor pushing to upstream)
is what the removal addresses: the index no longer names a push target at all.
