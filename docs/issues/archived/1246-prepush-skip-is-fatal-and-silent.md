---
id: 1246
title: "`pre-push` refuses a push with NO OUTPUT when submodule reachability cannot be asked — the three-outcome handling was unreachable under `set -e`"
status: resolved
type: bug
area: ci, process
severity: high
found: 2026-09-09
related: [1043, 0986, 0988]
---

# A guard that fails closed, silently, and teaches `--no-verify`

`.githooks/pre-push` knows that "could not ask the remotes" is a third outcome,
not a failure. It says so, at the call site:

```sh
# 2 = could not ask the remotes. Say so and let the push through: a
# push is already a network operation, so if the network were truly
# gone this would not be running — and refusing on an unanswerable
# question is worse than reporting it.
if [ "$rc" -eq 2 ]; then
    echo "pre-push: submodule reachability NOT verified (no network)." >&2
    continue
fi
```

**That arm had never executed once.** The file runs under `set -euo pipefail`
(line 23), and the value it inspects is produced by a bare command
substitution assignment:

```sh
out="$("$reach" --changed "$remote_sha" 2>&1)"
…
rc=$?
```

When `$reach` exits 2, `set -e` takes the script at the ASSIGNMENT. `rc=$?` and
everything after it — the rc=2 arm, the rc≠0 arm that prints `$out` — are
unreachable. The hook dies where it stands, and because the only thing it had to
say was in the branches it never reached, it prints **nothing**. Git reports
`error: failed to push some refs` and no more.

## Measured

Identical input, same tree, hook driven by hand for a new branch whose head does
not descend from `origin/main` (which selects the full-scan arm):

| | exit | lines printed |
| --- | --- | --- |
| before | **2** | **0** |
| after | 0 | 1 — `pre-push: submodule reachability NOT verified (no network).` |

The underlying skip is real and reproducible here:

```
$ bash scripts/ci/submodule-commits-reachable.sh; echo $?
submodule-commits-reachable: COULD NOT ASK while checking third-party/esp32/qemu
  fatal: shallow file has changed since we read it
submodule-commits-reachable: SKIPPED — cannot ask the remotes.
2
```

The script is behaving correctly — it reports the skip and exits 2, exactly the
protocol issue 1043 established for `check-submodule-pins` (FAIL / NOT VERIFIED
/ OK). The consumer is what was wrong.

## Why this is severity high rather than an annoyance

The push is refused with no reason, so the only way forward a contributor can
find is `git push --no-verify` — which disables **every** guard in this hook at
once, not just the unanswerable one: the duplicate issue-id refusal, the
submodule-pin rewind refusal (whose whole argument is that a rewind is usually
INHERITED from a rebase rather than authored), and the `just check fast` tier.
A guard whose failure mode is "teach people to bypass all the guards" is worse
than no guard on that axis.

It was found exactly that way: an agent working issue 1230 hit it, could not
diagnose it from the empty output, verified the hook's checks by hand and pushed
with `--no-verify`.

## The class

Same shape as the `workspace-fixtures-build.sh` defect fixed the same day
(phase-433 W5): a command substitution whose non-zero exit is meant to be
INSPECTED, written as a bare assignment under `set -e`, so the inspection never
runs and the script dies with no message of its own. Two independent instances
in guard/build infrastructure in one day suggests the idiom, not the site.

The safe spelling, used now on all three arms:

```sh
rc=0
out="$("$reach" --changed "$remote_sha" 2>&1)" || rc=$?
```

An assignment whose failure you intend to read must never be a bare statement
under `set -e`. `if out="$(cmd)"; then` is equally exempt and equally fine.

## Resolution

Fixed in this commit: `rc=0` initialised before the `case`, `|| rc=$?` on each
of the three arms, and the now-dead `rc=$?` after `esac` removed. The comment at
the call site records that the arm below it had never run.

**Not fixed here:** the `fatal: shallow file has changed since we read it` on
`third-party/esp32/qemu` that makes the skip fire in the first place. That is a
real condition — a shallow submodule clone whose `.git/shallow` moved under a
concurrent operation — and it deserves its own look, but the reachability check
is entitled to be unable to answer, and the hook must survive that answer.
