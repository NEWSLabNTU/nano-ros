---
id: 988
title: "No gate runs a hook the way git runs it — with `GIT_DIR` set — so a
  script that corrupts the caller's repository passes every check"
status: open
type: tech-debt
area: tooling
related: [issue-0986, issue-0196]
---

## The gap

Issue 0986 found `.githooks/pre-push` writing into the repository it guards:
three scripts reachable from it build throwaway repos in their selftests, and an
inherited `GIT_DIR` overrides BOTH a path argument and `git -C`. So the
selftests ran against the caller's repo — setting `core.bare = true` in its
config, and staging a gitlink named `dep` carrying a deliberately invalid sha
into its index. The hook whose job is refusing bad submodule pins was staging
one.

That bug is FIXED (`unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE
GIT_OBJECT_DIRECTORY` at the top of each script). This issue is the reason it
was possible, which the fix does not close:

**Nothing in this repo runs a hook under a hook's environment and asserts the
repository is unchanged afterwards.** Verified, not assumed:

* `grep -rln "githooks/pre-push" scripts/ just/` → **no matches**. No gate
  invokes the hook at all.
* The only files mentioning `GIT_DIR` are the three offenders plus the hook
  itself — i.e. the bug and its fix. Nothing tests the property.

## Why this class is invisible to ordinary checking

Two properties, both recorded in 0986 and both measured there:

1. **It does not reproduce by hand.** `bash .githooks/pre-push` from an
   interactive shell has no `GIT_DIR`, so the hook passes. Only git running it
   does the damage. 0986's author reports drawing the wrong conclusion twice —
   first that the hook was innocent, then that the push transport was writing
   the config.
2. **It is self-masking.** Run 1 sets `core.bare=true`; from run 2 the hook dies
   at `rev-parse --show-toplevel` BEFORE reaching the selftests. The symptom
   presents as a broken repository rather than a bad script, and the cause stops
   being executed — so the evidence destroys itself.

A selftest cannot catch it either, because the selftests ARE the thing that
misbehaves, and they redirect to `/dev/null` and return a status.

## The rule that has no gate

> A script reachable from a git hook must not modify the invoking repository —
> not its config, not its index, not its object store — whatever git puts in the
> environment.

This is exactly issue 0196's shape: a rule everyone believes, enforced nowhere.
`check-gate-selftests` makes a gate exercise its own failure path, but a
selftest that runs in the WRONG REPOSITORY still passes its own assertions.

## Sketch of the gate

Buildless and fast — it stages a scratch repo and shells out; no cargo, no
cmake, no network.

1. `git init` a scratch repo; record `git config --list` and
   `git ls-files --stage` (and the config of a *submodule-shaped* repo too —
   separate gitdir, `core.worktree` set, no `bare` key — since that is the shape
   0986 measured the config damage on).
2. Export the variables git sets for a hook (`GIT_DIR`, and
   `GIT_INDEX_FILE`/`GIT_WORK_TREE` where applicable), pointed at that repo.
3. Run `.githooks/pre-push` (and each script reachable from it) with stdin shaped
   like git's — pre-push reads `<local-ref> <local-sha> <remote-ref>
   <remote-sha>` lines.
4. Assert config and index are byte-identical afterwards. A diff is the failure,
   and it should print WHICH key or path moved.

Discovering "each script reachable from it" should be derived rather than
hand-listed — a hand-list is what let three scripts share one defect. The hook
already names them; parsing its invocations is cheaper than maintaining a
parallel list, and a list that drifts is issue 0196 again one level down.

## Scope worth checking while implementing

`pre-push` is the hook this repo installs and the one 0986 hit, but the rule is
about hooks generally. `just setup-hooks` installs the set; whatever else it
installs deserves the same assertion, and if the answer is "only pre-push", the
gate should say so rather than leave it ambiguous.

## Not established

* Whether any script reachable from the hook STILL modifies the caller's repo
  after 0986's fix. 0986 verified its three by inspection and by re-running one
  under an exported `GIT_DIR`; the gate is what would make that a standing
  answer rather than a one-time check.
* Whether `GIT_INDEX_FILE` and `GIT_OBJECT_DIRECTORY` are actually set by git
  for `pre-push` specifically, or only for commit-family hooks. 0986's fix
  clears them defensively, which is right, but a gate asserting the wrong
  environment would be testing a fiction — read `githooks(5)` before fixing the
  variable set.
