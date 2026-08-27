---
id: 844
title: "`check-no-tracked-file-find` does not see `grep -r`, so a gate script
  walked 9.2 GB of gitignored Zephyr SDK and stalled the fixture build"
status: open
type: tech-debt
area: testing
related: [issue-0196]
---

## What happened

`just build-test-fixtures lane=native` sat for **37 minutes** with no output and
no compiler running. It was not hung. `core_only_predicate.sh` was running:

```sh
consumed="$(grep -rhoE '[a-z0-9-]+ +[a-z]+ +--core-only' just/ scripts/ 2>/dev/null | ...)"
```

`scripts/` is 9.2 GB on a provisioned host, because the Zephyr SDK lives there:

| path | size | tracked? |
| --- | ---: | --- |
| `scripts/zephyr/sdk/` | 7.8 GB | gitignored |
| `scripts/zephyr/downloads/` | 1.4 GB | gitignored |
| everything else under `scripts/` | ~1.2 MB | tracked |

So the recursive grep read the entire toolchain — compilers, sysroots, tarballs
— looking for a CLI flag that only ever appears in tracked shell and `just`
source. The index lookup returns the same answer:

```
git grep -hoE '...' -- just/ scripts/   →  linux, in 0.33s
grep -r  ...        just/ scripts/      →  37+ minutes, killed
```

Fixed in `core_only_predicate.sh`. An untracked match could only be inside the
SDK, which is not a caller, so scoping to the index loses nothing.

## The actual defect: the gate does not cover its own rule

`check-no-tracked-file-find` states the rule as *"tracked-file discovery goes
through `git ls-files`"* and prints the measured 7m36s → 0.8s comparison. But it
only detects:

- `find …` (line-wise and multi-line spellings), and
- Python `.glob(` / `os.walk`

`grep -r` is tracked-file discovery by walk, and is invisible to it. That is the
[issue 0196](0196-*) shape — a gate narrower than the rule it enforces — and
this gate has form: its own header records two prior escapes, an unquoted-only
pattern and a pattern-held-in-a-variable.

The walk is also load-bearing evidence that people keep hitting this and fixing
only their own site. Both of these are existing comments in the tree:

- `scripts/check-no-absolute-model-paths.sh` — *"`grep -r examples` instead
  walks each example's …"*
- `scripts/check-no-direct-kernel-alloc.sh` — *"The previous `grep -r
  --exclude-dir={target,…}` basename set …"*

Each author found the problem, fixed their script, and left the gate alone. So
the next one pays for it again — which is exactly what happened here.

## What to do

Extend `check-no-tracked-file-find.sh` to flag `grep -r` (and `-R`, and the
`-rn`/`-rh` letter soup) rooted at a repo directory, with the same
`git grep` remedy it already prints for `find`.

It needs care rather than a blanket rule — the remaining sites are mostly
narrowly scoped and legitimate:

| site | root | verdict |
| --- | --- | --- |
| `size_probe_verify.sh:52,57` | `packages/tooling/nros-build-helpers` | small, tracked-only |
| `build_root_derivation.sh:501` | needs checking | — |
| `scripts/build/prune-dirs.sh` | documents the antipattern | exempt |

A reasonable predicate is "flag `grep -r` whose root is a top-level directory
that can contain gitignored bulk (`scripts/`, `examples/`, `packages/`,
`build/`, `third-party/`) unless it is `git grep` or carries `--exclude-dir`".
Whatever shape it takes, it should ship with the same self-test the sibling
gates have, and be verified to FAIL against the pre-fix
`core_only_predicate.sh` line — a gate for this class that cannot reproduce this
class is the thing being complained about.

## Note for anyone timing CI on this host

The stall is worst here because CARLA/Autoware were saturating the disk at the
same time. On an idle host the same grep would still read 9.2 GB; it would just
be less obvious about it.
