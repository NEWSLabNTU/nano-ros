---
id: 1336
title: "`nros-cli-core/build.rs` watches `.git/index`, which does not exist in a
  linked worktree — the CLI source stamp never re-bakes on a commit, and the
  same file already solved this for submodules three lines below"
status: open
type: bug
area: cli, build, tooling
severity: medium
found: 2026-09-12
related: [issue-0419, issue-0466, issue-0561, issue-0627, issue-0921, phase-454]
---

## Measured

`packages/cli/nros-cli-core/build.rs:54`:

```rust
// …and when the index moves (commit, rebase, branch switch), since the
// stamp reads index blob SHAs.
let index = root.join(".git/index");
if index.exists() {
    println!("cargo:rerun-if-changed={}", index.display());
}
```

In a linked worktree `.git` is a **FILE**, not a directory:

```
$ cat .claude/worktrees/agent-XXXX/.git
gitdir: /home/aeon/repos/nano-ros/.git/worktrees/agent-XXXX

$ [ -f .claude/worktrees/agent-XXXX/.git/index ]   # what the code tests
NO

$ [ -f /home/aeon/repos/nano-ros/.git/worktrees/agent-XXXX/index ]   # the real one
YES
```

So the watch is **silently skipped**. `if index.exists()` fails OPEN: no
`rerun-if-changed`, no diagnostic, and a build that looks identical to one where
the watch was registered.

## Consequence

The stamp reads index blob SHAs (the comment says so), so a commit, rebase or
branch switch inside a worktree changes an input nothing watches. `build.rs` does
not re-run, `NROS_CLI_SOURCE_STAMP` keeps its old value, and `just setup-cli`
reports success while rebuilding nothing.

That stamp is load-bearing: issue 0466 established that a refresh re-arms the
in-tree CLI's source stamp and that **fixtures key on it**, which is why the
documented order is rebuild the CLI first, then fixtures. A stamp that cannot
move inverts that — the CLI reads fresh while its inputs have moved, and the
failure presents as a stale CLI rather than as a missing dependency edge, which
aims the reader at the wrong layer.

**Every agent worktree has this**, so it is the default condition for parallel
sessions rather than an edge case.

## Why this is the issue-0196 shape

The knowledge is already in this file. Forty lines below the defect, the same
`build.rs` handles precisely this hazard for submodules, and says so:

> *"A submodule checkout has a `.git` FILE, so gate on that"* (issue 0419)

and routes it through a shared helper because getting it wrong was easy:

> *"WHICH files must be watched is not obvious and got it wrong twice, so it
> lives in one shared helper (`build-support/submodule_watch.rs`, issue 0921)
> that this and the resolver's build.rs both `include!`."*

So the tree knows that `.git` may be a file and that resolving the real gitdir is
subtle enough to deserve one spelling. The worktree case was never swept in. A
guard narrower than the rule it enforces — the same finding the 2026-07-28 audit
made against four gates.

Issue 0561 is the adjacent precedent: the stamp came to watch something different
from what the build baked because one expression was spelled out twice.

## What a fix has to decide

* Resolve the gitdir rather than assuming `.git` is a directory — read the
  `gitdir:` line when `.git` is a file, then watch `<gitdir>/index`. `git
  rev-parse --git-dir` answers it directly and is what the submodule helper
  already reasons about.
* Whether this belongs in `build-support/submodule_watch.rs` beside
  `watch_submodule_commit`. The two are the same question (where does git really
  keep this for the checkout shape I am in?), and issue 0921's argument for one
  spelling applies unchanged.
* **The silent-skip is separately wrong.** `if index.exists()` cannot distinguish
  "no repository here" (legitimate — a tarball build) from "repository whose
  layout I did not model". The second should be loud; the first should not.
  Compare `source_stamp`'s own convention, where a MISSING list returns `None`
  (⇒ rebuild) rather than a stamp over a smaller closure (issue 0627).

## Reproduction

In any linked worktree: commit a change to a file under `packages/cli/`, run
`just setup-cli`, and observe `NROS_CLI_SOURCE_STAMP` unchanged. The main
checkout does not reproduce it — there `.git` is a directory and the watch
registers — which is why this survived.

Found during phase-454 W3, in a worktree, as an incidental observation while
chasing gates that appeared to need a rebuild that had already happened.
