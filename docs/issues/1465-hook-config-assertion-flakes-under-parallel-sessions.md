---
id: 1465
title: "`check-hook-repo-side-effects` asserts on a config every worktree SHARES, so a
  concurrent agent session makes it report issue 0986 — a red about the observer,
  not about the hook"
status: open
type: bug
area: [ci, tooling]
related: [0986, 0988, 1336, 1391]
found: 2026-09-23
---

## What happens

`check-hook-repo-side-effects` ends with the assertion that closes issue 0986:
run `.githooks/pre-push` under a hook-shaped environment and check that **this**
repository's own config did not change. Its failure text is

```
  FAIL  this repo's own config CHANGED across a hook run — that is
        0986's exact symptom (core.bare=true), and it is live right now.
```

Measured on `just check fast` in an agent worktree on 2026-09-23, two runs
about ten minutes apart on the same tree:

| run | verdict |
| --- | --- |
| first, concurrent with another agent's `just check fast` | **FAIL** (5 of 344) |
| solo, same commit | `ok this repo's own config (/home/aeon/repos/nano-ros/.git/config) is byte-identical across the hook runs` — gate OK |

## Why

The path in the OK message is the answer: `/home/aeon/repos/nano-ros/.git/config`
— the **main checkout's** config, not the worktree's. A linked worktree has no
config of its own (`.git` there is a file; issue 1336), so every agent session
in this repository shares one file, and the gate's before/after snapshot spans
whatever any *other* session does in that window. `just setup-hooks` writes
three git builtins into exactly that file, and several recipes touch it.

So the assertion is sound about the hook and unsound about attribution: it can
only distinguish "the hook wrote to this config" from "somebody wrote to this
config" when nothing else is running. In a repository whose own CLAUDE.md
describes parallel agent sessions as the normal way of working, that condition
does not hold.

## Why it matters more than an ordinary flake

The failure text names **issue 0986** and says *it is live right now*. 0986 is
the class where `pre-push` — the hook whose job is refusing bad submodule pins —
writes `core.bare=true` into the caller's config and stages an invalid gitlink
into its index. A reader who hits this red has every reason to stop and
investigate a serious repository-corruption class, and the thing to investigate
is a second terminal.

It is also on the **fast line**, which is the lane `pre-push` itself runs and
the lane CLAUDE.md tells every session to run before every push. A red there is
maximally expensive per occurrence.

## Fix direction (not decided)

The gate needs to attribute a change to the hook rather than observe one in a
shared file. Candidates, none measured:

- snapshot and compare a **copy**: run the hook against a `GIT_CONFIG_GLOBAL`/
  `GIT_CONFIG_SYSTEM`-isolated clone of this repo's config, so the file under
  observation is one only this gate can write;
- compare the config's **content minus keys other recipes legitimately set**,
  which is weaker and would have to enumerate them — the same
  authored-list drift the repo dislikes elsewhere;
- take an advisory lock around the assertion, which serialises sessions rather
  than fixing the attribution;
- report NOT VERIFIED (the `nros_check_skip` ledger, as `check-submodule-pins`
  does for a missing object store) when the config's mtime moves during the run
  without the hook having written it — three outcomes instead of two, which is
  the shape issue 1043 settled on for the same problem one gate over.

## Acceptance

`just check fast` run concurrently with another session's `just check fast`
gives this gate the same verdict as a solo run, and the 0986 assertion still
fails when the hook really does write to a config.
