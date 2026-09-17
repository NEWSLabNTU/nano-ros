---
id: 1336
title: "`nros-cli-core/build.rs` watches `.git/index`, which does not exist in a
  linked worktree — the CLI source stamp never re-bakes on a commit, and the
  same file already solved this for submodules three lines below"
status: resolved
type: bug
area: cli, build, tooling
severity: medium
found: 2026-09-12
resolved: 2026-09-18
resolved_in: "5d39ee50d, 321ccaa3d, 8b39d7195"
related: [issue-1306, issue-0419, issue-0466, issue-0561, issue-0627, issue-0921,
  issue-0986, issue-1043, issue-1280, phase-454]
---

> **Absorbed issue 1306** (filed 2026-09-11, a day earlier, by another session
> for the same defect; retired `wontfix` under this number). Two things came
> from there rather than from here: the fix spelling
> `git rev-parse --git-path index`, and an acceptance recipe, which this file
> did not have. See `archived/1306-*`.

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

## Resolution (2026-09-18) — `5d39ee50d`, `321ccaa3d`, `8b39d7195`

**The fix.** `build.rs` asks git:
`git rev-parse --path-format=absolute --git-path index`, run in `root`, through
`source_stamp.rs`'s existing `git()`/`git_program()` helper — one expression per
stamp input, shared with the runtime through the `include!` (issue 0561's rule),
beside the code that READS the index. Not the `--git-dir` + hand-read `gitdir:`
line this file proposed, and not `build-support/submodule_watch.rs`: that helper
answers a different question (which files move when a submodule's COMMIT moves)
and is deliberately pure, while this one has to consult git. `--path-format=absolute`
is load-bearing — without it git answers relative to its cwd, and cargo resolves
a relative `rerun-if-changed` against the PACKAGE root, not `root`.

**The silent skip**, which this file flagged separately, is now visible: a
`cargo:warning` naming the unwatched stamp when git says this IS a repository
but cannot resolve the path (realistically git < 2.31, which has no
`--path-format`). Silence only for "no repository here" — a tarball or vendored
tree, where `source_stamp()` returns `None` too and the freshness check skips
itself rather than guessing.

**Measured, in a linked worktree, both directions.** Build the CLI, commit one
new CLI source file, `just setup-cli`, `just check cli-fresh`:

*Before* (`fa6cf68d2`), and identically on every repeat:

```
[setup-cli] building nros CLI (packages/cli)…
    Finished `release` profile [optimized] target(s) in 1.24s
[setup-cli] built: …/packages/cli/target/release/nros
Error: source-stamp: STALE — built from 21d769f07ba67348, sources are now 5e650e717ff5cbb3.
```

*After*, same sequence, no `touch`:

```
[setup-cli] building nros CLI (packages/cli)…
    Finished `release` profile [optimized] target(s) in 28.82s
[setup-cli] built: …/packages/cli/target/release/nros
cli-fresh: OK — source-stamp: fresh (b680c14dc396b088)
```

and the watch is in the build script's own output, where it was absent before:

```
cargo:rerun-if-changed=/home/aeon/repos/nano-ros/.git/worktrees/agent-…/index
```

**One thing the reproduction section above got wrong, worth recording.** A
worktree whose `packages/cli/third-party/play_launch` submodule is
UNINITIALISED does not reproduce this at all: `watch_submodule_commit` then
watches a gitlink that does not exist, cargo treats a missing watched path as
"re-run when it appears", and the build script therefore re-ran on EVERY cargo
invocation — masking the defect completely and printing
`NROS_PLAY_LAUNCH_SHA=unknown` as the only clue. The submodule has to be
initialised before this is visible, which is one more reason it survived.

Also corrected: a commit of content-identical bytes does NOT move the stamp (by
design — issue 0561's same-encoding rule), so the commits that expose this are
the ones that ADD or REMOVE a CLI input, or change its mode. That is the common
case for an agent session and is what issue 1285's agent hit.

**Swept the class**, per CLAUDE.md, and it was not one site:

```
rg -n '"\.git/' --glob '*.rs' --glob '*.py' --glob '*.sh' --glob '*.just'
rg -n -e 'join\("\.git"\)' -e '\.git.*is_dir' -e 'isdir\(.*\.git' \
      -e '-d\s+"[^"]*\.git"' -e 'IS_DIRECTORY[^)]*\.git' \
      --glob '*.rs' --glob '*.py' --glob '*.sh' --glob '*.just' \
      --glob '*.cmake' --glob 'CMakeLists.txt' --glob '*.yml'
```

Six siblings, two of them live:

* `check-hook-repo-side-effects.sh` hashed `$(git rev-parse --git-dir)/config`
  either side of the hook run. In a worktree `--git-dir` is the per-worktree
  admin dir, which has **no `config`** (measured: HEAD, index, commondir,
  gitdir, logs, modules, nothing else), so `sha1sum` failed, `2>/dev/null` ate
  it, both sides were empty, they compared equal, and the `ok` printed having
  measured nothing — in every agent worktree. The assertion that exists to
  catch issue 0986 writing `core.bare=true` into the shared config was the one
  that went vacuous, in the gate whose subject is that hazard. Now
  `--git-path config`, and unresolvable is LOUD.
* `submodule-pins-check.sh::submodule_git_dir` derived a deinit'd submodule's
  store as `$(--git-common-dir)/modules/<name>`, on a written claim that a
  worktree gitdir "has no `modules/`" — measurably false: this worktree's has
  one, and `play_launch`'s gitlink points into it. Both stores exist here and
  they are DIFFERENT, so the common-dir-only spelling read the MAIN checkout's
  history, or found nothing and downgraded a verdict to a skip (the issue-1043
  outcome this function exists to avoid). Now per-worktree first, then common.
* `nros-pkg-index::detect_workspace_root`'s `.git` pass asked `is_dir()`, so it
  found nothing in a worktree AND on a submodule and walked past the real root.
  Now `exists()`, with `detects_workspace_root_via_either_git_shape` asserting
  both shapes.
* `runner-bootstrap.sh`, `esp_idf/setup.sh`, `can/build-zenohc-can.sh`,
  `zephyr/fetch-kconfig-trees.sh` tested `-d "<dir>/.git"` before reusing a
  checkout. Each guards a clone the script itself makes, so each was arguably
  right — but three sit on an operator-settable directory, where `-d` sends a
  good worktree or submodule into the clone branch, which then fails against a
  populated directory. `-e` is never worse for the question, so all four were
  fixed rather than waived.

Everything else the sweep found is correct and stays: directory-NAME skips in
tree walks (a `.git` FILE is skipped just as well), `find -name .git -prune`
groups, the `':!.git/*'` pathspec and `dep-closure.py`'s depfile filter (issue
0635), test fixtures that create the shape they read, `submodule_watch.rs`
(already parses `gitdir:`), and `nros-build-paths` /
`scripts/lib/checkout-paths.sh`, which deliberately do not use `.git` as a
checkout marker and say why (issue 1280).

**Gated:** `check-git-dir-layout-assumptions` (fast line, 1.27 s), forbidding
both spellings — a literal subpath under `.git`, and a `.git` existence test
that demands a directory. The rule is MEASURED, not merely grepped: the
self-test builds a real checkout AND a real linked worktree and looks, so if
`<root>/.git/index` ever existed in a worktree the gate would say to re-measure
rather than keep enforcing pedantry. Three declared exceptions, all test DATA,
and the self-test fails on a DEAD exception (the issue-0743 class). It clears
the inherited git environment first (issues 0986/0988) through
`scripts/lib/git_hook_env.py`.

**Tests:** `the_index_resolves_in_a_main_checkout_and_in_a_linked_worktree`
(both shapes, with the pre-fix literal as an inline negative control; verified
to FAIL against the old implementation) and
`outside_a_repository_there_is_no_index_and_no_checkout`.
