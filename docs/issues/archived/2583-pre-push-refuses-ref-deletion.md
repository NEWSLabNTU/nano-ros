---
id: 2583
title: "The pre-push hook refused a branch deletion, reading its all-zero local sha as a submodule pin rewind"
status: resolved
type: bug
area: [ci]
severity: medium
found: 2026-11-15
resolved: 2026-11-15
related: [issue-0840, issue-1246]
---

# The pre-push hook refused a branch deletion

## Symptom

Deleting the release branch after the `0.6.0-nros9` tag existed was refused by
our own hook:

```
$ git push origin --delete release/nros9
submodule-pins: FAILED — could not read the gitlinks at 'dff3242405…' or
  '0000000000000000000000000000000000000000' within 60 s; nothing was compared.
pre-push: REFUSING to push — a submodule pin would move backward.
```

A deletion publishes no commit, so no pin can move. The refusal had no true
reading, and the release flow (cut a `release/<ver>` branch, delete it once the
tag exists) meets it on every release.

## Cause

git feeds the hook `<local-ref> <local-sha> <remote-ref> <remote-sha>` per ref,
and a deletion's `<local-sha>` is all zeros. The hook skipped an all-zero
REMOTE sha (a new branch) but treated every LOCAL sha as a commit:

- the pin loop passed it to `submodule-pins-check.sh`, which cannot read
  gitlinks at a zero sha and fails;
- the reachability loop did not skip it either;
- the branch-push guard counted a deletion, so a deletion-only push still ran
  the issue-id and diagnostic-code checks and the whole fast tier.

## Fix

One predicate, `nros_prepush_deletes`, which every loop asks before reading
`<local-sha>` as a commit. A deletion arms no check and reaches none. An
update, including a deletion and an update in the same push, is checked as
before.

`tests/pre-push-ref-tests.sh` (`just check pre-push-refs`) runs the real hook
in a scratch repository with its four checks stubbed. The stubs fail on a zero
sha, as the real script does. It has 11 checks:

- a deletion passes and arms no check;
- the negative control: an update whose pin check refuses is still refused,
  and says why;
- an ordinary update passes and asks both checks against the remote sha;
- a deletion beside an update lets only the update's shas reach a check;
- a tag push arms no check.

Against the old hook, 5 of the 11 fail, including the refusal measured above.
