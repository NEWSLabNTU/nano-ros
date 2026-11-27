---
id: 2958
title: "Pushes skip the pre-push hook on hosts that never ran `just setup-hooks` (host 142: every checkout), and where it runs, `check fast` judges the working tree, not the pushed commit"
status: resolved
resolved: 2026-11-27
resolved_in: "b2c9fd08b4 (the hook judges the pushed commit); host 142 ran `just setup-hooks` 2026-11-27"
type: bug
area: [ci, gates]
severity: low
found: 2026-11-25
related: [0840, 0196, 2955, 1964, phase-489]
---

**Home: [phase-489](../../roadmap/phase-489-ci-build-speedup.md)**, which owns
the fast line's run time and its gate issues (2669, 2955 shape).

## What happened

PR #4821 archived issue 2955. The issue file was edited first (`status:
resolved`, a Resolution section), then moved with `git mv` into
`docs/issues/archived/`. `git mv` stages the file as it is in the INDEX, so
the edits stayed unstaged and the commit archived an issue that still said
`status: open`.

`just check fast` in that worktree was green (664/664), and the branch was
pushed. Hosted `check / fast` on the PR was red:

```
check-archived-issue-status: 2955-hook-gate-changelog-audit-probe-near-budget.md:
frontmatter says `status: open` — an archived issue is not open
```

The local run read the working tree, which had the edits; CI checked out
the commit, which did not. The fix was a follow-up commit (force-push is
not available to agents here).

In this instance the pre-push hook was not installed in that worktree
(`core.hooksPath` unset), so the green came from a manual
`just check fast`. The hook runs the same recipe in the same checkout
(`.githooks/pre-push`: `cd "$repo_root" && just check fast`), so it has the
same reach: it would have passed this push too.

## Why it is a class, not a slip

This is issue 0196's reach gap in a new place: the gate's SUBJECT (the
working tree) is wider than the thing the rule is about (the pushed commit).
Any difference between the two passes locally and fails on CI:

- an edit left unstaged by `git mv`, `git add -p`, or a forgotten path;
- an untracked new file a gate needs (an issue file, a fragment, a golden);
- a deleted file whose deletion was never staged.

The pre-push hook exists (issue 0840) to refuse exactly the reds that are
true in the author's own tree. These are not true in the tree; they are
true in the commit, which nothing local examines.

The committed tree IS examined in one lane: `gate::check::pristine-checkout`
in `just ci gate` clones the commit and runs the fast tier there
(`NROS_GATE_SCOPE=pristine`). That lane costs ~6 min, and `ci gate` is not
what anyone runs before pushing a docs-only change.

## The larger gap: on host 142 no checkout runs the hook at all

Checked 2026-11-25 after the incident above:

- **The hook is not installed anywhere on host 142.** The repository's
  shared config has none of the four settings in `config/git-settings.txt`
  (`core.hooksPath=.githooks` required; `diff.submodule`,
  `status.submoduleSummary`, `push.recurseSubmodules` advisory), and
  `.git/hooks/` holds only samples. So every push from this host, main
  checkout and every agent worktree alike, runs no pre-push hook: no
  `check fast`, no duplicate-issue-id refusal, no submodule-rewind refusal.
  The server-side gates still catch the last two on the PR (`check fast`
  in CI, `refs/issue-ids/*` reservation), so they are found later, not
  missed.
- **A linked worktree DOES inherit it once set.** There is no
  `extensions.worktreeConfig`, so `git config` writes the common config
  every worktree reads. One `just setup-hooks` in any checkout on a host
  covers all of them; the gap is per HOST, not per worktree.
- **`just setup-worktree` deliberately does not install it.** `setup-hooks`
  is opt-in, because pointing `core.hooksPath` at tracked scripts lets a
  checkout run repository code on push (the justfile's own comment). The
  provisioning check only prints `worktree-provisioning: NOTE git
  core.hooksPath=<unset> (want .githooks) -> just setup-hooks; until then
  pushes skip the pre-push gates`. That line is in every `just ci gate` log
  on this host today (the #4803 and #4808 runs, for two), and nobody acted on
  it. A note in a long log is not a refusal.

## Proposed fix (not built)

**First, and cheaper than anything below: get the hook installed on every
host that pushes.** Either an operator runs `just setup-hooks` once per
host (a decision for the host's owner, since it is the opt-in above; it
also makes every push pay `check fast`, ~4 min under load), or the
opt-in becomes a stated choice rather than a silent default: for example,
`setup-worktree` and `just doctor` report the missing hook as a FAIL with
both remedies (install it, or record `NROS_SKIP_PREPUSH_CHECKS=1` as the
host's choice) instead of a NOTE. That catches more than the tree-diff
refusal below, which only matters once a hook runs at all.

Then, for the working-tree reach:

Cheapest first; the first alone covers every case above.

1. **The pre-push hook refuses a push whose working tree differs from the
   commit it pushes, for tracked paths.** Before running `check fast`:
   `git diff --quiet HEAD --` (staged or unstaged edits to tracked files) and
   `git ls-files --others --exclude-standard` filtered to paths a gate
   reads. On a difference it names the paths and says the gates would judge
   the working tree, not the push, with `git stash push -u -m …` /
   commit / `NROS_SKIP_PREPUSH_CHECKS=1` as the ways forward. It costs a few
   milliseconds and changes no gate. Caveat to settle before building: the
   pushed ref is not always `HEAD` (`git push origin other-branch`); the hook
   receives the local SHA on stdin and should compare against that, refusing
   only when it equals `HEAD`.
2. **Narrower, if (1) is judged too strict:** refuse only when the
   difference touches `docs/issues/` or `changelog.d/`, the two trees whose
   gates read frontmatter and file presence and which are edited by hand
   right before a commit.
3. **Not proposed:** running `check fast` against a `git worktree` or clone
   of the pushed commit in the hook. That is what `pristine-checkout`
   already does, and it is minutes, not milliseconds; a hook that slow gets
   `--no-verify`'d (the hook's own comment).

Whichever is chosen, `check-hook-repo-side-effects` must still pass: the
check reads git state only and writes nothing.

## Done: the hook refuses a tree that differs from the push (2026-11-25)

Proposal (1) is built. When a pushed branch's local sha is `HEAD`, the
pre-push hook (`scripts/lib/prepush-tree.sh`, before `check fast`) compares
the working tree with it. Tracked differences come from
`git diff --name-status --ignore-submodules=dirty HEAD`: staged or not,
including a moved pin. Untracked files come from
`git ls-files --others --exclude-standard`. On any difference it refuses,
names the paths, and gives `git stash push -u` or
`NROS_SKIP_PREPUSH_CHECKS=1` as the ways forward.

It runs only when the fast gates would run. A push of another ref, a tag, or
nothing is not judged, since the working tree is not its subject. It is
read-only, and `check-hook-repo-side-effects` still passes.

`just check prepush-tree` holds ten cases in a throwaway repository.

- **Refused:** an unstaged edit, a staged-only edit, an untracked file, an
  unstaged deletion, and `git mv` over an unstaged edit (PR #4821's
  incident).
- **Passed:** a clean tree, an ignored file, a push of a non-HEAD commit, a
  tag push, and an empty push.

Disabling the refusal fails three of those cases.

Still open: the hook is opt-in (`just setup-hooks`) and absent on host 142.
Installing it there, or turning the provisioning NOTE into a FAIL, is the
host owner's decision and is not part of this change.

## Resolution (2026-11-27)

Both halves are closed.

- **The hook judges what is pushed:** `b2c9fd08b4` (2026-11-25, the section
  above). `.githooks/pre-push` refuses a push whose working tree differs from
  the pushed commit, so `check fast` reads exactly what it is judging.
- **Host 142 now runs the hook.** On 2026-11-27, nros 142 ran
  `just setup-hooks` in its main checkout (`/home/aeon/repos/nano-ros`):

  ```
  git config core.hooksPath .githooks
  git config diff.submodule log
  git config status.submoduleSummary true
  git config push.recurseSubmodules check
  hooks installed: core.hooksPath -> .githooks
  ```

  `core.hooksPath` is repository config, which every linked worktree shares,
  so pushes from all of 142's worktrees run `.githooks/pre-push`. It was
  checked in a worktree (w2984): `git config --get core.hooksPath` reads
  `.githooks`, and the hook resolves and is executable.

Per the manager's ruling, the provisioning check stays a NOTE, not a FAIL. A
new host that never ran `just setup-hooks` is told so by `just doctor` /
`setup-worktree`, not refused.

